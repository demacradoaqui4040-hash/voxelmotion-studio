// VoxelMotion Studio - PlayerAnimator / EmoteCraft -> Bedrock 1.8.0 / ClipDef Converter
// Calibrated against player-animation-lib-forge-1.0.2-rc1 bytecode:
//   - PlayerRendererMixin.applyBodyTransforms (`torso` in version < 3 -> entity-space `body`, before scale(-1,-1,1))
//   - PlayerModelMixin.setEmote (`head`, `rightArm`, `leftArm`, `rightLeg`, `leftLeg` ModelPart space)
//   - HeldItemMixin.changeItemLocation (`rightItem` inside Vanilla `RotX(-90) * RotY(180)` held-item space)

use crate::model::{
    BoneTransform, ClipDef, ClipSource, CurationStatus, EasingMode, GalleryGroup, Keyframe,
    RigPose, BONE_COUNT,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Default Minecraft ModelPart rest pose offsets in MC pixel space (+X left, +Y down, -Z forward).
#[inline]
fn mc_part_base_pose(pa_bone: &str) -> Option<(usize, [f32; 3])> {
    match pa_bone {
        // Virtual `waist` holds PlayerRendererMixin whole-body transform (v < 3 `torso` / `body`).
        "waist_body" => Some((0, [0.0, 0.0, 0.0])),
        "body_part" => Some((1, [0.0, 0.0, 0.0])),
        "head" => Some((2, [0.0, 0.0, 0.0])),
        "rightArm" | "right_arm" => Some((3, [-5.0, 2.0, 0.0])),
        "leftArm" | "left_arm" => Some((4, [5.0, 2.0, 0.0])),
        "rightItem" | "right_item" => Some((5, [0.0, 0.0, 0.0])),
        "rightLeg" | "right_leg" => Some((6, [-1.9, 12.0, 0.0])),
        "leftLeg" | "left_leg" => Some((7, [1.9, 12.0, 0.0])),
        "root" => Some((8, [0.0, 0.0, 0.0])),
        "leftItem" | "left_item" => Some((9, [0.0, 0.0, 0.0])),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct PaChannelState {
    pos: [Option<f32>; 3],
    rot: [Option<f32>; 3],
    easing: EasingMode,
}

impl Default for PaChannelState {
    fn default() -> Self {
        Self {
            pos: [None; 3],
            rot: [None; 3],
            easing: EasingMode::EaseInOut,
        }
    }
}

fn map_pa_easing(raw: &str) -> EasingMode {
    let up = raw.to_ascii_uppercase();
    if up.contains("INOUT") {
        EasingMode::EaseInOut
    } else if up.contains("OUT") {
        EasingMode::EaseOut
    } else if up.contains("IN") {
        EasingMode::Snap
    } else if up == "LINEAR" || up == "CONSTANT" {
        EasingMode::Linear
    } else {
        EasingMode::EaseInOut
    }
}

#[inline]
fn wrap_deg_180(deg: f32) -> f32 {
    let mut d = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if d <= -180.0 {
        d += 360.0;
    }
    d
}

/// Preserve shortest continuous angular delta across consecutive keyframes (critical for 360-degree rolls/spins).
#[inline]
fn unwrap_deg_near(prev: f32, next_raw: f32) -> f32 {
    prev + wrap_deg_180(next_raw - prev)
}

#[inline]
fn rot3_x(rad: f32) -> [[f32; 3]; 3] {
    let (s, c) = rad.sin_cos();
    [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
}

#[inline]
fn rot3_y(rad: f32) -> [[f32; 3]; 3] {
    let (s, c) = rad.sin_cos();
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

#[inline]
fn rot3_z(rad: f32) -> [[f32; 3]; 3] {
    let (s, c) = rad.sin_cos();
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

#[inline]
fn mul3(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

#[inline]
fn mul3_vec(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[inline]
fn transpose3(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

/// Intrinsic Z * Y * X rotation matrix from degrees (matches Minecraft ModelPart & VoxelMotion `rot_zyx_deg`).
#[inline]
fn rot_zyx_deg3(deg: [f32; 3]) -> [[f32; 3]; 3] {
    let rx = deg[0].to_radians();
    let ry = deg[1].to_radians();
    let rz = deg[2].to_radians();
    mul3(rot3_z(rz), mul3(rot3_y(ry), rot3_x(rx)))
}

/// Extract intrinsic Z * Y * X Euler angles in degrees from a 3x3 rotation matrix.
#[inline]
fn euler_zyx_deg_from_mat3(m: [[f32; 3]; 3]) -> [f32; 3] {
    let sy = (-m[2][0]).clamp(-1.0, 1.0);
    let ry = sy.asin();
    let (rx, rz) = if sy.abs() < 0.99985 {
        (m[2][1].atan2(m[2][2]), m[1][0].atan2(m[0][0]))
    } else {
        ((-m[1][2]).atan2(m[1][1]), 0.0)
    };
    [rx.to_degrees(), ry.to_degrees(), rz.to_degrees()]
}

/// HeldItemMixin runs inside vanilla `ItemInHandLayer` right after `XP(-90) * YP(180)`.
/// Pre-multiplying / conjugating by `R_hand` brings `rightItem` into VoxelMotion `right_item` (`right_arm`) space.
#[inline]
fn vanilla_hand_to_arm_mat3() -> [[f32; 3]; 3] {
    let rx = rot3_x(-std::f32::consts::FRAC_PI_2);
    let ry = rot3_y(std::f32::consts::PI);
    mul3(rx, ry)
}

/// Convert a single PlayerAnimator / EmoteCraft JSON file into a studio `ClipDef`.
pub fn parse_player_animator_json(
    raw_json: &str,
    clip_id: &str,
    source_tag: &str,
    bc_targets: Vec<String>,
) -> Result<ClipDef, String> {
    let root: Value = serde_json::from_str(raw_json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let version = root.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
    let emote = root
        .get("emote")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Missing `emote` object in PlayerAnimator JSON".to_string())?;

    let is_loop = emote
        .get("isLoop")
        .map(|v| match v {
            Value::Bool(b) => *b,
            Value::String(s) => s.eq_ignore_ascii_case("true"),
            _ => false,
        })
        .unwrap_or(false);
    let in_degrees = emote
        .get("degrees")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let begin_tick = emote.get("beginTick").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let end_tick = emote
        .get("endTick")
        .and_then(|v| v.as_f64())
        .unwrap_or((begin_tick + 10.0) as f64) as f32;
    let stop_tick = emote
        .get("stopTick")
        .and_then(|v| v.as_f64())
        .unwrap_or((end_tick + 6.0) as f64) as f32;

    let moves = emote
        .get("moves")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing `emote.moves` array".to_string())?;

    let map_pa_slot = |key: &str| -> Option<usize> {
        match key {
            // AnimationJson.addBodyPartIfExists renames `torso` -> `body` when version < 3.
            "torso" if version < 3 => Some(0),
            "body" => Some(0),
            "torso" => Some(1),
            _ => mc_part_base_pose(key).map(|(idx, _)| idx),
        }
    };

    let mut per_tick: BTreeMap<i32, [Option<PaChannelState>; BONE_COUNT]> = BTreeMap::new();
    for mv in moves {
        let Some(mobj) = mv.as_object() else { continue };
        let tick = mobj.get("tick").and_then(|v| v.as_f64()).unwrap_or(0.0).round() as i32;
        let easing = mobj
            .get("easing")
            .and_then(|v| v.as_str())
            .map(map_pa_easing)
            .unwrap_or(EasingMode::EaseInOut);

        let entry = per_tick.entry(tick).or_insert([None; BONE_COUNT]);
        for (key, val) in mobj {
            if key == "tick" || key == "easing" || key == "turn" {
                continue;
            }
            let Some(bone_idx) = map_pa_slot(key) else { continue };
            let Some(bobj) = val.as_object() else { continue };
            let ch = entry[bone_idx].get_or_insert(PaChannelState::default());
            ch.easing = easing;
            if let Some(x) = bobj.get("x").and_then(|v| v.as_f64()) {
                ch.pos[0] = Some(x as f32);
            }
            if let Some(y) = bobj.get("y").and_then(|v| v.as_f64()) {
                ch.pos[1] = Some(y as f32);
            }
            if let Some(z) = bobj.get("z").and_then(|v| v.as_f64()) {
                ch.pos[2] = Some(z as f32);
            }
            if let Some(p) = bobj.get("pitch").and_then(|v| v.as_f64()) {
                ch.rot[0] = Some(p as f32);
            }
            if let Some(y) = bobj.get("yaw").and_then(|v| v.as_f64()) {
                ch.rot[1] = Some(y as f32);
            }
            if let Some(r) = bobj.get("roll").and_then(|v| v.as_f64()) {
                ch.rot[2] = Some(r as f32);
            }
        }
    }

    if per_tick.is_empty() {
        return Err("No valid bone keyframes parsed from `emote.moves`".into());
    }

    let rad_to_deg = 180.0 / std::f32::consts::PI;
    let first_tick = *per_tick.keys().next().unwrap_or(&0) as f32;
    let last_tick = *per_tick.keys().next_back().unwrap_or(&1) as f32;
    let total_ticks = stop_tick.max(last_tick + 2.0).max(4.0);
    let duration = (total_ticks / 20.0).max(0.15);

    // Track continuous raw PlayerAnimator values per channel in MC / PA coordinate conventions.
    let mut pa_pos: [[f32; 3]; BONE_COUNT] = [
        [0.0, 0.0, 0.0],   // 0: waist (`body` whole-entity transform in blocks)
        [0.0, 0.0, 0.0],   // 1: body (`torso` when version >= 3)
        [0.0, 0.0, 0.0],   // 2: head
        [-5.0, 2.0, 0.0],  // 3: rightArm
        [5.0, 2.0, 0.0],   // 4: leftArm
        [0.0, 0.0, 0.0],   // 5: rightItem
        [-1.9, 12.0, 0.0], // 6: rightLeg
        [1.9, 12.0, 0.0],  // 7: leftLeg
        [0.0, 0.0, 0.0],   // 8: root
        [0.0, 0.0, 0.0],   // 9: leftItem
    ];
    let mut pa_rot_deg: [[f32; 3]; BONE_COUNT] = [[0.0; 3]; BONE_COUNT];
    let mut prev_out_pose = RigPose::ZERO;
    let mut keyframes = Vec::new();

    if first_tick > 0.0 {
        keyframes.push(Keyframe {
            time: 0.0,
            easing: EasingMode::EaseInOut,
            bones: RigPose::ZERO,
            bone_mask: crate::model::ALL_BONES_MASK,
        });
    }

    let r_hand = vanilla_hand_to_arm_mat3();
    let r_hand_inv = transpose3(r_hand);

    for (tick, slots) in &per_tick {
        let mut frame_easing = EasingMode::EaseInOut;
        for idx in 0..BONE_COUNT {
            if let Some(ch) = slots[idx] {
                frame_easing = ch.easing;
                for ax in 0..3 {
                    if let Some(v) = ch.pos[ax] {
                        pa_pos[idx][ax] = v;
                    }
                    if let Some(ang) = ch.rot[ax] {
                        let deg = if in_degrees { ang } else { ang * rad_to_deg };
                        pa_rot_deg[idx][ax] = unwrap_deg_near(pa_rot_deg[idx][ax], deg);
                    }
                }
            }
        }

        let mut out_pose = RigPose::ZERO;

        // 0: `waist` (`body` in PlayerRendererMixin before scale(-1, -1, 1)):
        //   - translation is in blocks (`1 block = 16 px`), `diag(-1, +1_up, +1_z)` in Bedrock
        //   - rotation conjugated by `diag(-1, -1, +1)` -> `(-pitch, -yaw, +roll)`
        let waist_tr = BoneTransform {
            pos: [
                -pa_pos[0][0] * 16.0,
                pa_pos[0][1] * 16.0,
                pa_pos[0][2] * 16.0,
            ],
            rot: [
                unwrap_deg_near(prev_out_pose.bone(0).rot[0], -pa_rot_deg[0][0]),
                unwrap_deg_near(prev_out_pose.bone(0).rot[1], -pa_rot_deg[0][1]),
                unwrap_deg_near(prev_out_pose.bone(0).rot[2], pa_rot_deg[0][2]),
            ],
            ..BoneTransform::IDENTITY
        };
        *out_pose.bone_mut(0) = waist_tr;
        let r_waist_mc = rot_zyx_deg3(waist_tr.rot);
        let waist_pos_mc = [waist_tr.pos[0], -waist_tr.pos[1], waist_tr.pos[2]];

        // 1..=4: `body`, `head`, `rightArm`, `leftArm` (direct ModelPart space, already children of `waist`).
        let bases: [[f32; 3]; 4] = [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [-5.0, 2.0, 0.0],
            [5.0, 2.0, 0.0],
        ];
        for i in 1..=4 {
            let b = bases[i - 1];
            *out_pose.bone_mut(i) = BoneTransform {
                pos: [
                    pa_pos[i][0] - b[0],
                    -(pa_pos[i][1] - b[1]),
                    pa_pos[i][2] - b[2],
                ],
                rot: [
                    unwrap_deg_near(prev_out_pose.bone(i).rot[0], pa_rot_deg[i][0]),
                    unwrap_deg_near(prev_out_pose.bone(i).rot[1], pa_rot_deg[i][1]),
                    unwrap_deg_near(prev_out_pose.bone(i).rot[2], pa_rot_deg[i][2]),
                ],
                ..BoneTransform::IDENTITY
            };
        }

        // 5 & 9: `rightItem` & `leftItem` (HeldItemMixin inside Vanilla `RotX(-90) * RotY(180)`):
        //   - `pos` mapped via `R_hand`: `(-hx, +hz, -hy)` in Bedrock item space
        //   - `rot` conjugated `R_hand * R_pa * R_hand^-1`
        for item_idx in [5usize, 9usize] {
            let item_r_conj = mul3(r_hand, mul3(rot_zyx_deg3(pa_rot_deg[item_idx]), r_hand_inv));
            let item_euler = euler_zyx_deg_from_mat3(item_r_conj);
            *out_pose.bone_mut(item_idx) = BoneTransform {
                pos: [
                    -pa_pos[item_idx][0],
                    pa_pos[item_idx][2],
                    -pa_pos[item_idx][1],
                ],
                rot: [
                    unwrap_deg_near(prev_out_pose.bone(item_idx).rot[0], item_euler[0]),
                    unwrap_deg_near(prev_out_pose.bone(item_idx).rot[1], item_euler[1]),
                    unwrap_deg_near(prev_out_pose.bone(item_idx).rot[2], item_euler[2]),
                ],
                ..BoneTransform::IDENTITY
            };
        }

        // 6 & 7: `rightLeg` & `leftLeg`:
        //   PlayerRendererMixin rotates legs together with `body`, while Bedrock 1.8.0 / CombatPoseAnimator
        //   evaluates legs independently of `waist`. Compose `M_waist * M_leg_pa` so stances match 1:1.
        for (idx, base_x) in [(6usize, -1.9f32), (7usize, 1.9f32)] {
            let rel_mc = [pa_pos[idx][0], pa_pos[idx][1] - 12.0, pa_pos[idx][2]];
            let rot_rel = mul3_vec(r_waist_mc, rel_mc);
            let leg_mc = [
                waist_pos_mc[0] + rot_rel[0],
                waist_pos_mc[1] + rot_rel[1],
                waist_pos_mc[2] + rot_rel[2],
            ];
            let leg_r_final = mul3(r_waist_mc, rot_zyx_deg3(pa_rot_deg[idx]));
            let leg_euler = euler_zyx_deg_from_mat3(leg_r_final);
            *out_pose.bone_mut(idx) = BoneTransform {
                pos: [leg_mc[0] - base_x, -leg_mc[1], leg_mc[2]],
                rot: [
                    unwrap_deg_near(prev_out_pose.bone(idx).rot[0], leg_euler[0]),
                    unwrap_deg_near(prev_out_pose.bone(idx).rot[1], leg_euler[1]),
                    unwrap_deg_near(prev_out_pose.bone(idx).rot[2], leg_euler[2]),
                ],
                ..BoneTransform::IDENTITY
            };
        }

        prev_out_pose = out_pose;
        keyframes.push(Keyframe {
            time: ((*tick as f32) / 20.0).clamp(0.0, duration),
            easing: frame_easing,
            bones: out_pose,
            bone_mask: crate::model::ALL_BONES_MASK,
        });
    }

    if !is_loop && (last_tick < total_ticks - 0.5) {
        // Settle rotations to the nearest full 360-degree turn so 360 somersaults/spins finish smoothly forward.
        let mut settle_pose = RigPose::ZERO;
        for idx in 0..BONE_COUNT {
            for ax in 0..3 {
                let last_a = prev_out_pose.bone(idx).rot[ax];
                settle_pose.bone_mut(idx).rot[ax] = (last_a / 360.0).round() * 360.0;
            }
        }
        keyframes.push(Keyframe {
            time: duration,
            easing: EasingMode::EaseOut,
            bones: settle_pose,
            bone_mask: crate::model::ALL_BONES_MASK,
        });
    }

    // Approximate strike peak right after windup (`beginTick` -> peak strike before `endTick`)
    let strike_tick = if end_tick > begin_tick {
        begin_tick + (end_tick - begin_tick) * 0.35
    } else {
        (first_tick + last_tick) * 0.5
    };
    let impact_time = Some((strike_tick / 20.0).clamp(0.04, duration - 0.04));

    let raw_name = root
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(clip_id);
    let pretty = raw_name
        .replace('_', " ")
        .split_whitespace()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(first) => first.to_uppercase().to_string() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let group = if clip_id.contains("roll") || source_tag.eq_ignore_ascii_case("Combat Roll") {
        GalleryGroup::Movimiento
    } else {
        GalleryGroup::Combate
    };

    let source = match source_tag.to_ascii_lowercase().as_str() {
        s if s.contains("simply") => ClipSource::SimplySwords,
        s if s.contains("roll") => ClipSource::CombatRoll,
        s if s.contains("better") => ClipSource::BetterCombat,
        _ => ClipSource::Personalizado,
    };

    Ok(ClipDef {
        id: clip_id.to_string(),
        name: pretty,
        gallery_group: group,
        duration,
        impact_time,
        looped: is_loop,
        bc_targets,
        event_tag: None,
        source,
        status: CurationStatus::Pendiente,
        vfx_events: Vec::new(),
        allowed_categories: Vec::new(),
        allowed_weapons: Vec::new(),
        keyframes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_torso_to_waist_with_leg_composition_and_conjugates_held_item() {
        let sample = r#"{
            "name": "calib_test",
            "emote": {
                "isLoop": false,
                "beginTick": 7,
                "endTick": 20,
                "stopTick": 25,
                "degrees": true,
                "moves": [
                    {
                        "tick": 10,
                        "easing": "EASEOUTQUAD",
                        "torso": { "y": -0.25, "z": -0.5, "pitch": -60.0, "yaw": 20.0, "roll": 5.0 },
                        "rightItem": { "x": 1.0, "y": 2.0, "z": -3.0, "pitch": -75.0, "yaw": 0.0, "roll": -90.0 },
                        "leftLeg": { "x": 1.9, "y": 12.0, "z": 0.0, "pitch": 60.0, "yaw": 0.0, "roll": 0.0 }
                    }
                ]
            }
        }"#;
        let clip = parse_player_animator_json(sample, "calib_test", "Better Combat", vec![]).unwrap();
        let kf = clip.keyframes.iter().find(|k| (k.time - 0.5).abs() < 0.01).unwrap();
        let waist = kf.bones.bone(0);
        assert!((waist.rot[0] - 60.0).abs() < 0.05, "waist pitch flips sign");
        assert!((waist.rot[1] - (-20.0)).abs() < 0.05, "waist yaw flips sign");
        assert!((waist.rot[2] - 5.0).abs() < 0.05, "waist roll keeps sign");
        assert!((waist.pos[1] - (-4.0)).abs() < 0.05, "waist y converts blocks -> 16px");
        assert!((waist.pos[2] - (-8.0)).abs() < 0.05, "waist z converts blocks -> 16px");

        let item = kf.bones.bone(5);
        assert!((item.pos[0] - (-1.0)).abs() < 0.01);
        assert!((item.pos[1] - (-3.0)).abs() < 0.01);
        assert!((item.pos[2] - (-2.0)).abs() < 0.01);
        // After conjugation `R_hand * R_item * R_hand^-1`, rest sword blade `(0, +0.707, -0.707)`
        // maps along `+Y` (forward along extended -90deg arm) rather than pointing up.
        let r_item = rot_zyx_deg3(item.rot);
        let blade = mul3_vec(r_item, [0.0, 0.70710677, -0.70710677]);
        assert!(blade[1] > 0.65, "blade should extend outward along +Y arm axis, got {blade:?}");
    }
}
