// VoxelMotion Studio - Core Animation Data Model, Easing Engine & Keyframe Optimizer

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const BONE_NAMES: [&str; 8] = [
    "waist",
    "body",
    "head",
    "right_arm",
    "left_arm",
    "right_item",
    "right_leg",
    "left_leg",
];

pub const BONE_LABELS_ES: [&str; 8] = [
    "Cintura",
    "Cuerpo",
    "Cabeza",
    "BrazoDerecho",
    "BrazoIzquierdo",
    "Espada",
    "PiernaDerecha",
    "PiernaIzquierda",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeaponCategory {
    Todas,
    Espadas,
    Katanas,
    Lanzas,
    Dagas,
    Mandobles,
    Arcos,
    Otros,
}

impl WeaponCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Todas => "Todas",
            Self::Espadas => "Espadas",
            Self::Katanas => "Katanas",
            Self::Lanzas => "Lanzas",
            Self::Dagas => "Dagas",
            Self::Mandobles => "Mandobles",
            Self::Arcos => "Arcos",
            Self::Otros => "Otros",
        }
    }

    pub const ALL_SIDEBAR: [Self; 8] = [
        Self::Todas,
        Self::Espadas,
        Self::Katanas,
        Self::Lanzas,
        Self::Dagas,
        Self::Mandobles,
        Self::Arcos,
        Self::Otros,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EasingMode {
    Snap,
    EaseOut,
    EaseInOut,
    Linear,
}

impl EasingMode {
    pub fn index(self) -> i32 {
        match self {
            Self::Snap => 0,
            Self::EaseOut => 1,
            Self::EaseInOut => 2,
            Self::Linear => 3,
        }
    }
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => Self::Snap,
            1 => Self::EaseOut,
            2 => Self::EaseInOut,
            _ => Self::Linear,
        }
    }
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Snap => {
                if t < 0.35 {
                    1.8 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::Linear => t,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BoneTransform {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl Default for BoneTransform {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f32,
    pub easing: EasingMode,
    pub bones: HashMap<String, BoneTransform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GalleryGroup {
    Combate,
    Movimiento,
    Defensa,
    Especiales,
    Interaccion,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipDef {
    pub id: String,
    pub name: String,
    pub gallery_group: GalleryGroup,
    pub duration: f32,
    pub impact_time: Option<f32>,
    pub looped: bool,
    pub bc_targets: Vec<String>,
    #[serde(default, alias = "rell_event")]
    pub event_tag: Option<String>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeaponRecord {
    pub id: String,
    pub name: String,
    pub mod_source: String,
    pub item_id: String,
    pub category: WeaponCategory,
    pub weapon_class: String, // MEDIUM | HEAVY | LIGHT | RANGED | UNARMED
    pub mesh_archetype: String,
    #[serde(skip)]
    pub icon_rgba: Option<(u32, u32, Vec<u8>)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimatorProject {
    pub version: u32,
    pub clips: Vec<ClipDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMode {
    Select,
    Rotate,
    Scale,
    Camera,
    Move,
}

pub fn empty_pose() -> HashMap<String, BoneTransform> {
    BONE_NAMES
        .iter()
        .map(|b| ((*b).to_string(), BoneTransform::default()))
        .collect()
}

/// Create a Blender-clean neutral resting clip ready for frame-by-frame keyframe authoring.
pub fn create_blank_clip(seq: usize) -> ClipDef {
    let neutral = empty_pose();
    ClipDef {
        id: format!("action_{seq}"),
        name: if seq == 1 { "Nueva Acción".into() } else { format!("Nueva Acción {seq}") },
        gallery_group: GalleryGroup::Combate,
        duration: 0.60,
        impact_time: None,
        looped: false,
        bc_targets: Vec::new(),
        event_tag: None,
        keyframes: vec![
            Keyframe { time: 0.0, easing: EasingMode::EaseInOut, bones: neutral.clone() },
            Keyframe { time: 0.60, easing: EasingMode::EaseInOut, bones: neutral },
        ],
    }
}

pub fn prev_keyframe_time(clip: &ClipDef, t: f32) -> Option<f32> {
    clip.keyframes
        .iter()
        .filter(|k| k.time < t - 0.012)
        .max_by(|a, b| a.time.total_cmp(&b.time))
        .map(|k| k.time)
}

pub fn next_keyframe_time(clip: &ClipDef, t: f32) -> Option<f32> {
    clip.keyframes
        .iter()
        .filter(|k| k.time > t + 0.012)
        .min_by(|a, b| a.time.total_cmp(&b.time))
        .map(|k| k.time)
}

pub fn sample_clip_pose(clip: &ClipDef, time: f32) -> HashMap<String, BoneTransform> {
    let mut kfs: Vec<&Keyframe> = clip.keyframes.iter().collect();
    kfs.sort_by(|a, b| a.time.total_cmp(&b.time));

    if kfs.is_empty() {
        return empty_pose();
    }
    let dur = clip.duration.max(0.05);
    let t = if clip.looped {
        ((time % dur) + dur) % dur
    } else {
        time.clamp(0.0, dur)
    };

    if kfs.len() == 1 || t <= kfs[0].time {
        return normalized_pose(&kfs[0].bones);
    }
    let last = kfs[kfs.len() - 1];
    if t >= last.time {
        return normalized_pose(&last.bones);
    }

    let mut a = kfs[0];
    let mut b = last;
    for w in kfs.windows(2) {
        if t >= w[0].time && t <= w[1].time {
            a = w[0];
            b = w[1];
            break;
        }
    }
    let span = (b.time - a.time).max(1e-5);
    let u = a.easing.eval((t - a.time) / span);

    let mut out = empty_pose();
    for bone in BONE_NAMES {
        let ba = a.bones.get(bone).cloned().unwrap_or_default();
        let bb = b.bones.get(bone).cloned().unwrap_or_default();
        let mut res = BoneTransform::default();
        for i in 0..3 {
            res.pos[i] = ba.pos[i] + (bb.pos[i] - ba.pos[i]) * u;
            res.rot[i] = ba.rot[i] + (bb.rot[i] - ba.rot[i]) * u;
            res.scale[i] = ba.scale[i] + (bb.scale[i] - ba.scale[i]) * u;
        }
        out.insert(bone.to_string(), res);
    }
    out
}

fn normalized_pose(src: &HashMap<String, BoneTransform>) -> HashMap<String, BoneTransform> {
    let mut out = empty_pose();
    for (k, v) in src {
        if out.contains_key(k) {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

pub fn mirror_pose(src: &HashMap<String, BoneTransform>) -> HashMap<String, BoneTransform> {
    let mut out = normalized_pose(src);
    let flip = |b: &BoneTransform| BoneTransform {
        pos: [-b.pos[0], b.pos[1], b.pos[2]],
        rot: [b.rot[0], -b.rot[1], -b.rot[2]],
        scale: b.scale,
    };
    for center in ["waist", "body", "head", "right_item"] {
        if let Some(v) = src.get(center) {
            out.insert(center.to_string(), flip(v));
        }
    }
    for (a, b) in [("right_arm", "left_arm"), ("right_leg", "left_leg")] {
        let ta = src.get(a).cloned().unwrap_or_default();
        let tb = src.get(b).cloned().unwrap_or_default();
        out.insert(a.to_string(), flip(&tb));
        out.insert(b.to_string(), flip(&ta));
    }
    out
}

/// Remove collinear intermediate points on baked tracks when "Optimizar keyframes" is enabled.
pub fn optimize_track_points(samples: &[(f32, [f32; 3])], eps: f32) -> Vec<(f32, [f32; 3])> {
    if samples.len() <= 2 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;

    let mut stack = vec![(0usize, samples.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        if e <= s + 1 {
            continue;
        }
        let (t0, v0) = samples[s];
        let (t1, v1) = samples[e];
        let dt = (t1 - t0).max(1e-5);
        let mut max_err = 0.0f32;
        let mut split = s;
        for i in (s + 1)..e {
            let u = (samples[i].0 - t0) / dt;
            let mut err = 0.0f32;
            for c in 0..3 {
                let interp = v0[c] + (v1[c] - v0[c]) * u;
                err = err.max((samples[i].1[c] - interp).abs());
            }
            if err > max_err {
                max_err = err;
                split = i;
            }
        }
        if max_err > eps && split > s && split < e {
            keep[split] = true;
            stack.push((s, split));
            stack.push((split, e));
        }
    }
    samples
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, v)| *v)
        .collect()
}

fn bt_rot(rx: f32, ry: f32, rz: f32) -> BoneTransform {
    BoneTransform {
        rot: [rx, ry, rz],
        ..Default::default()
    }
}

fn bt_pr(px: f32, py: f32, pz: f32, rx: f32, ry: f32, rz: f32) -> BoneTransform {
    BoneTransform {
        pos: [px, py, pz],
        rot: [rx, ry, rz],
        scale: [1.0, 1.0, 1.0],
    }
}

fn kf(time: f32, easing: EasingMode, pairs: Vec<(&str, BoneTransform)>) -> Keyframe {
    let mut bones = empty_pose();
    for (name, tr) in pairs {
        bones.insert(name.to_string(), tr);
    }
    Keyframe { time, easing, bones }
}

pub fn build_demo_presets() -> Vec<ClipDef> {
    vec![
        ClipDef {
            id: "attack_1".into(),
            name: "Attack 1".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.48,
            impact_time: Some(0.19),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_slash_horizontal_right".into(),
                "bettercombat:two_handed_slash_horizontal_right".into(),
            ],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -32.0, 0.0)),
                    ("right_arm", bt_rot(-48.0, -42.0, 34.0)),
                    ("left_arm", bt_rot(-44.0, 38.0, -14.0)),
                    ("right_item", bt_rot(-20.0, -14.0, 44.0)),
                ]),
                kf(0.09, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.7, 0.8, 12.0, -52.0, -5.0)),
                    ("head", bt_rot(-6.0, 36.0, 0.0)),
                    ("right_arm", bt_rot(-88.0, -56.0, 60.0)),
                    ("left_arm", bt_rot(-68.0, 46.0, -22.0)),
                    ("right_item", bt_rot(-38.0, -16.0, 60.0)),
                    ("right_leg", bt_rot(22.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-22.0, 0.0, -5.0)),
                ]),
                kf(0.19, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.6, -2.0, 16.0, 44.0, 6.0)),
                    ("head", bt_rot(-10.0, -30.0, 0.0)),
                    ("right_arm", bt_rot(-56.0, 56.0, -52.0)),
                    ("left_arm", bt_rot(-42.0, 18.0, -32.0)),
                    ("right_item", bt_rot(52.0, 18.0, -48.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(26.0, 0.0, -6.0)),
                ]),
                kf(0.35, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.7, -0.8, 8.0, 46.0, 3.0)),
                    ("right_arm", bt_rot(-34.0, 52.0, -44.0)),
                    ("left_arm", bt_rot(-30.0, 14.0, -24.0)),
                    ("right_item", bt_rot(42.0, 12.0, -40.0)),
                ]),
                kf(0.48, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "attack_2".into(),
            name: "Attack 2".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.48,
            impact_time: Some(0.19),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_slash_horizontal_left".into(),
                "bettercombat:two_handed_slash_horizontal_left".into(),
            ],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 10.0, 40.0, 4.0)),
                    ("right_arm", bt_rot(-50.0, 52.0, -48.0)),
                    ("left_arm", bt_rot(-44.0, 18.0, -28.0)),
                    ("right_item", bt_rot(40.0, 10.0, -44.0)),
                ]),
                kf(0.09, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.5, 0.6, 12.0, 54.0, 6.0)),
                    ("head", bt_rot(-6.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-68.0, 68.0, -62.0)),
                    ("left_arm", bt_rot(-56.0, 12.0, -34.0)),
                    ("right_item", bt_rot(56.0, 16.0, -54.0)),
                ]),
                kf(0.19, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.6, -1.8, 14.0, -42.0, -6.0)),
                    ("head", bt_rot(-8.0, 32.0, 0.0)),
                    ("right_arm", bt_rot(-90.0, -48.0, 54.0)),
                    ("left_arm", bt_rot(-72.0, 44.0, -16.0)),
                    ("right_item", bt_rot(-46.0, -18.0, 52.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-24.0, 0.0, -5.0)),
                ]),
                kf(0.35, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(6.0, -44.0, -4.0)),
                    ("right_arm", bt_rot(-58.0, -52.0, 56.0)),
                    ("right_item", bt_rot(-40.0, -12.0, 44.0)),
                ]),
                kf(0.48, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "attack_3".into(),
            name: "Attack 3".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.54,
            impact_time: Some(0.22),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_stab".into(),
                "bettercombat:two_handed_slash_vertical_right".into(),
                "simplyswords:halberd_stab_right".into(),
            ],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_rot(-6.0, -16.0, 0.0)),
                    ("right_arm", bt_rot(-136.0, -16.0, 12.0)),
                    ("left_arm", bt_rot(-128.0, 24.0, -10.0)),
                    ("right_item", bt_rot(-30.0, 0.0, 0.0)),
                ]),
                kf(0.11, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -0.6, 1.2, -14.0, -22.0, 0.0)),
                    ("head", bt_rot(8.0, 16.0, 0.0)),
                    ("right_arm", bt_rot(-166.0, -18.0, 14.0)),
                    ("left_arm", bt_rot(-160.0, 26.0, -12.0)),
                    ("right_item", bt_rot(-52.0, 0.0, 0.0)),
                ]),
                kf(0.22, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.2, -2.4, 24.0, 18.0, 0.0)),
                    ("head", bt_rot(-14.0, -12.0, 0.0)),
                    ("right_arm", bt_rot(-20.0, 18.0, -12.0)),
                    ("left_arm", bt_rot(-26.0, 26.0, 10.0)),
                    ("right_item", bt_rot(68.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(-26.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(28.0, 0.0, -4.0)),
                ]),
                kf(0.38, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.0, -1.0, 14.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-12.0, 12.0, -8.0)),
                    ("left_arm", bt_rot(-16.0, 20.0, 8.0)),
                    ("right_item", bt_rot(56.0, 0.0, 0.0)),
                ]),
                kf(0.54, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "heavy_attack".into(),
            name: "Heavy Attack".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.72,
            impact_time: Some(0.34),
            looped: false,
            bc_targets: vec![
                "bettercombat:two_handed_slam".into(),
                "bettercombat:two_handed_slam_heavy".into(),
                "bettercombat:two_handed_spin".into(),
            ],
            event_tag: Some("MEDIUM:STRONG_STRIKE".into()),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 6.0, -28.0, 0.0)),
                    ("right_arm", bt_rot(-105.0, -26.0, 24.0)),
                    ("left_arm", bt_rot(-96.0, 38.0, -12.0)),
                    ("right_item", bt_rot(-34.0, 0.0, 16.0)),
                ]),
                kf(0.19, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, 0.4, 1.8, -18.0, -42.0, -6.0)),
                    ("head", bt_rot(12.0, 32.0, 0.0)),
                    ("right_arm", bt_rot(-172.0, -34.0, 36.0)),
                    ("left_arm", bt_rot(-164.0, 40.0, -18.0)),
                    ("right_item", bt_rot(-58.0, 0.0, 20.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-26.0, 0.0, -6.0)),
                ]),
                kf(0.34, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.8, -3.0, 30.0, 38.0, 8.0)),
                    ("head", bt_rot(-18.0, -26.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 44.0, -36.0)),
                    ("left_arm", bt_rot(-24.0, 22.0, 12.0)),
                    ("right_item", bt_rot(74.0, 14.0, -20.0)),
                    ("right_leg", bt_rot(-30.0, 0.0, 8.0)),
                    ("left_leg", bt_rot(34.0, 0.0, -8.0)),
                ]),
                kf(0.56, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.4, -1.2, 16.0, 28.0, 4.0)),
                    ("right_arm", bt_rot(-12.0, 34.0, -24.0)),
                    ("right_item", bt_rot(60.0, 8.0, -14.0)),
                ]),
                kf(0.72, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "idle".into(),
            name: "Idle".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 1.00,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.5, 0.0, 4.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-32.0, -22.0, 14.0)),
                    ("left_arm", bt_rot(-18.0, 18.0, -10.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                    ("right_leg", bt_rot(8.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-10.0, 0.0, -4.0)),
                ]),
                kf(0.50, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.9, 0.0, 5.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-36.0, -24.0, 16.0)),
                    ("left_arm", bt_rot(-21.0, 20.0, -11.0)),
                    ("right_item", bt_rot(13.0, -8.0, 19.0)),
                    ("right_leg", bt_rot(9.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-11.0, 0.0, -4.0)),
                ]),
                kf(1.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.5, 0.0, 4.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-32.0, -22.0, 14.0)),
                    ("left_arm", bt_rot(-18.0, 18.0, -10.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                    ("right_leg", bt_rot(8.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-10.0, 0.0, -4.0)),
                ]),
            ],
        },
        ClipDef {
            id: "block".into(),
            name: "Block".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.44,
            impact_time: Some(0.14),
            looped: true,
            bc_targets: vec![],
            event_tag: Some("MEDIUM:GUARD".into()),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-66.0, -40.0, 20.0)),
                    ("left_arm", bt_rot(-48.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -26.0, 56.0)),
                    ("right_leg", bt_rot(15.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-17.0, 0.0, -5.0)),
                ]),
                kf(0.22, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.3, 0.0, 9.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-69.0, -42.0, 22.0)),
                    ("left_arm", bt_rot(-50.0, 32.0, -13.0)),
                    ("right_item", bt_rot(-24.0, -26.0, 58.0)),
                    ("right_leg", bt_rot(16.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-18.0, 0.0, -5.0)),
                ]),
                kf(0.44, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-66.0, -40.0, 20.0)),
                    ("left_arm", bt_rot(-48.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -26.0, 56.0)),
                    ("right_leg", bt_rot(15.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-17.0, 0.0, -5.0)),
                ]),
            ],
        },
        ClipDef {
            id: "parry".into(),
            name: "Parry".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.42,
            impact_time: Some(0.11),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("MEDIUM:PARRY_SUCCESS".into()),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 6.0, -20.0, 0.0)),
                    ("right_arm", bt_rot(-70.0, -42.0, 22.0)),
                    ("left_arm", bt_rot(-50.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -24.0, 54.0)),
                ]),
                kf(0.11, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.4, -0.8, -6.0, 28.0, -6.0)),
                    ("head", bt_rot(2.0, -20.0, 0.0)),
                    ("right_arm", bt_rot(-94.0, 36.0, 44.0)),
                    ("left_arm", bt_rot(-24.0, -22.0, -32.0)),
                    ("right_item", bt_rot(-38.0, 28.0, -44.0)),
                    ("right_leg", bt_rot(-16.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(18.0, 0.0, -6.0)),
                ]),
                kf(0.28, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(-2.0, 18.0, -3.0)),
                    ("right_arm", bt_rot(-78.0, 24.0, 34.0)),
                    ("right_item", bt_rot(-26.0, 18.0, -30.0)),
                ]),
                kf(0.42, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "dodge".into(),
            name: "Dodge".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.40,
            impact_time: Some(0.12),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:EVADE_DASH".into()),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.8, 0.0, 18.0, 0.0, 0.0)),
                    ("right_arm", bt_rot(38.0, -12.0, 18.0)),
                    ("left_arm", bt_rot(38.0, 12.0, -18.0)),
                ]),
                kf(0.14, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.6, -2.5, 28.0, -18.0, -10.0)),
                    ("head", bt_rot(-20.0, 14.0, 6.0)),
                    ("right_arm", bt_rot(56.0, -16.0, 26.0)),
                    ("left_arm", bt_rot(56.0, 16.0, -26.0)),
                    ("right_leg", bt_rot(-32.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(38.0, 0.0, -4.0)),
                ]),
                kf(0.40, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "equip".into(),
            name: "Equip".into(),
            gallery_group: GalleryGroup::Interaccion,
            duration: 0.44,
            impact_time: Some(0.22),
            looped: false,
            bc_targets: vec![],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(6.0, 18.0, 0.0)),
                    ("right_arm", bt_rot(-12.0, 32.0, 22.0)),
                    ("left_arm", bt_rot(-28.0, 24.0, 0.0)),
                    ("right_item", bt_rot(48.0, 0.0, 28.0)),
                ]),
                kf(0.18, EasingMode::Snap, vec![
                    ("waist", bt_rot(4.0, -14.0, 0.0)),
                    ("right_arm", bt_rot(-74.0, -32.0, 24.0)),
                    ("right_item", bt_rot(-16.0, -10.0, 32.0)),
                ]),
                kf(0.44, EasingMode::EaseInOut, vec![
                    ("waist", bt_rot(4.0, -14.0, 0.0)),
                    ("right_arm", bt_rot(-34.0, -20.0, 14.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                ]),
            ],
        },
        ClipDef {
            id: "unequip".into(),
            name: "Unequip".into(),
            gallery_group: GalleryGroup::Interaccion,
            duration: 0.40,
            impact_time: None,
            looped: false,
            bc_targets: vec![],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("right_arm", bt_rot(-34.0, -20.0, 14.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                ]),
                kf(0.20, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(5.0, 16.0, 0.0)),
                    ("right_arm", bt_rot(-4.0, 28.0, 18.0)),
                    ("right_item", bt_rot(44.0, 0.0, 24.0)),
                ]),
                kf(0.40, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "run".into(),
            name: "Run".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 0.56,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, -10.0, 0.0)),
                    ("head", bt_rot(-12.0, 8.0, 0.0)),
                    ("right_arm", bt_rot(34.0, -8.0, 12.0)),
                    ("left_arm", bt_rot(-46.0, 10.0, -10.0)),
                    ("right_leg", bt_rot(-44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(28.0, 0.0, 12.0)),
                ]),
                kf(0.28, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, 10.0, 0.0)),
                    ("head", bt_rot(-12.0, -8.0, 0.0)),
                    ("right_arm", bt_rot(44.0, 10.0, 14.0)),
                    ("left_arm", bt_rot(38.0, -10.0, -10.0)),
                    ("right_leg", bt_rot(44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(-44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(34.0, 0.0, 14.0)),
                ]),
                kf(0.56, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, -10.0, 0.0)),
                    ("head", bt_rot(-12.0, 8.0, 0.0)),
                    ("right_arm", bt_rot(34.0, -8.0, 12.0)),
                    ("left_arm", bt_rot(-46.0, 10.0, -10.0)),
                    ("right_leg", bt_rot(-44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(28.0, 0.0, 12.0)),
                ]),
            ],
        },
        ClipDef {
            id: "walk".into(),
            name: "Walk".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 0.76,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, -6.0, 0.0)),
                    ("right_arm", bt_rot(18.0, -8.0, 8.0)),
                    ("left_arm", bt_rot(-22.0, 8.0, -6.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(14.0, 0.0, 10.0)),
                ]),
                kf(0.38, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, 6.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 8.0, 8.0)),
                    ("left_arm", bt_rot(22.0, -8.0, -6.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(-24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(18.0, 0.0, 10.0)),
                ]),
                kf(0.76, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, -6.0, 0.0)),
                    ("right_arm", bt_rot(18.0, -8.0, 8.0)),
                    ("left_arm", bt_rot(-22.0, 8.0, -6.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(14.0, 0.0, 10.0)),
                ]),
            ],
        },
        ClipDef {
            id: "guard_break".into(),
            name: "Guard Break".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.72,
            impact_time: Some(0.18),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:GUARD_BREAK".into()),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_rot(6.0, 0.0, 0.0)),
                    ("right_arm", bt_rot(-60.0, -28.0, 14.0)),
                    ("left_arm", bt_rot(-60.0, 28.0, -14.0)),
                ]),
                kf(0.18, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.6, 2.4, -26.0, 0.0, 0.0)),
                    ("head", bt_rot(-24.0, 14.0, 10.0)),
                    ("right_arm", bt_rot(-42.0, -24.0, 68.0)),
                    ("left_arm", bt_rot(-42.0, 24.0, -68.0)),
                    ("right_leg", bt_rot(18.0, 0.0, 10.0)),
                    ("left_leg", bt_rot(-16.0, 0.0, -10.0)),
                ]),
                kf(0.46, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.2, 1.4, -14.0, 0.0, 0.0)),
                    ("head", bt_rot(-10.0, -10.0, -6.0)),
                    ("right_arm", bt_rot(-24.0, -14.0, 48.0)),
                    ("left_arm", bt_rot(-24.0, 14.0, -48.0)),
                ]),
                kf(0.72, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "air_kick".into(),
            name: "Air Kick".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.42,
            impact_time: Some(0.14),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:AIR_KICK".into()),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 12.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(-56.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(22.0, 0.0, -4.0)),
                    ("right_arm", bt_rot(-28.0, 0.0, 22.0)),
                    ("left_arm", bt_rot(-28.0, 0.0, -22.0)),
                ]),
                kf(0.14, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, 2.8, -0.6, -14.0, 10.0, 0.0)),
                    ("head", bt_rot(12.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(62.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-64.0, 0.0, -6.0)),
                    ("right_arm", bt_rot(24.0, 0.0, 46.0)),
                    ("left_arm", bt_rot(24.0, 0.0, -46.0)),
                ]),
                kf(0.42, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "execution".into(),
            name: "Execution".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.62,
            impact_time: Some(0.28),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:EXECUTION".into()),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.8, 0.8, 14.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-126.0, -30.0, 32.0)),
                    ("left_arm", bt_rot(-74.0, 38.0, -20.0)),
                    ("right_item", bt_rot(-36.0, 0.0, 22.0)),
                ]),
                kf(0.16, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, 1.6, 1.6, -12.0, -52.0, -6.0)),
                    ("head", bt_rot(8.0, 36.0, 0.0)),
                    ("right_arm", bt_rot(-168.0, -32.0, 34.0)),
                    ("left_arm", bt_rot(-110.0, 42.0, -24.0)),
                    ("right_item", bt_rot(-54.0, 0.0, 24.0)),
                    ("right_leg", bt_rot(32.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-34.0, 0.0, -6.0)),
                ]),
                kf(0.28, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.6, -3.2, 28.0, 48.0, 8.0)),
                    ("head", bt_rot(-18.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 56.0, -48.0)),
                    ("left_arm", bt_rot(42.0, -26.0, -38.0)),
                    ("right_item", bt_rot(72.0, 18.0, -38.0)),
                    ("right_leg", bt_rot(-32.0, 0.0, 8.0)),
                    ("left_leg", bt_rot(36.0, 0.0, -8.0)),
                ]),
                kf(0.62, EasingMode::Linear, vec![]),
            ],
        },
    ]
}
