// VoxelMotion Studio - Production Exporter (Bedrock 1.8.0 / GeckoLib 4.4.4 / Timings / PNG Strip)

use crate::model::{
    optimize_track_points, sample_clip_pose, AnimatorProject, ClipDef, CurationStatus, WeaponRecord,
    BONE_NAMES,
};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Bedrock18,
    GeckoLib4,
    Both,
}

impl ExportFormat {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::GeckoLib4,
            2 => Self::Both,
            _ => Self::Bedrock18,
        }
    }
}

pub struct ExportSummary {
    pub clips_exported: usize,
    pub bc_overrides: usize,
    pub event_timings: usize,
    pub targets: Vec<PathBuf>,
}

fn round2(v: f32) -> f64 {
    ((v as f64) * 100.0).round() / 100.0
}

fn format_ts(t: f32) -> String {
    let s = format!("{:.3}", t);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn bake_clip_json(clip: &ClipDef, optimize: bool, geckolib: bool, include_events: bool) -> Value {
    let dur = clip.duration.max(0.08);
    let fps = 30.0f32;
    let steps = ((dur * fps).round() as usize).max(2);

    let mut sample_times: Vec<f32> = (0..=steps).map(|i| dur * (i as f32) / (steps as f32)).collect();
    for kf in &clip.keyframes {
        if kf.time >= 0.0 && kf.time <= dur {
            sample_times.push(kf.time);
        }
    }
    sample_times.sort_by(|a, b| a.total_cmp(b));
    sample_times.dedup_by(|a, b| (*a - *b).abs() < 0.002);

    let sampled: Vec<(f32, _)> = sample_times.iter().map(|&t| (t, sample_clip_pose(clip, t))).collect();

    let mut bones_map = Map::new();
    for bone in BONE_NAMES {
        let rot_raw: Vec<(f32, [f32; 3])> = sampled
            .iter()
            .map(|(t, p)| (*t, p.get(bone).map(|x| x.rot).unwrap_or([0.0; 3])))
            .collect();
        let pos_raw: Vec<(f32, [f32; 3])> = sampled
            .iter()
            .map(|(t, p)| (*t, p.get(bone).map(|x| x.pos).unwrap_or([0.0; 3])))
            .collect();
        let scl_raw: Vec<(f32, [f32; 3])> = sampled
            .iter()
            .map(|(t, p)| (*t, p.get(bone).map(|x| x.scale).unwrap_or([1.0; 3])))
            .collect();

        let has_rot = rot_raw.iter().any(|(_, v)| v.iter().any(|x| x.abs() > 0.05));
        let has_pos = pos_raw.iter().any(|(_, v)| v.iter().any(|x| x.abs() > 0.02));
        let has_scl = scl_raw
            .iter()
            .any(|(_, v)| v.iter().any(|x| (*x - 1.0).abs() > 0.015));
        if !has_rot && !has_pos && !has_scl {
            continue;
        }

        let mut bone_obj = Map::new();
        if has_rot {
            let pts = if optimize { optimize_track_points(&rot_raw, 0.35) } else { rot_raw };
            let mut m = Map::new();
            for (t, v) in pts {
                m.insert(format_ts(t), json!([round2(v[0]), round2(v[1]), round2(v[2])]));
            }
            bone_obj.insert("rotation".into(), Value::Object(m));
        }
        if has_pos {
            let pts = if optimize { optimize_track_points(&pos_raw, 0.04) } else { pos_raw };
            let mut m = Map::new();
            for (t, v) in pts {
                m.insert(format_ts(t), json!([round2(v[0]), round2(v[1]), round2(v[2])]));
            }
            bone_obj.insert("position".into(), Value::Object(m));
        }
        if has_scl {
            let pts = if optimize { optimize_track_points(&scl_raw, 0.015) } else { scl_raw };
            let mut m = Map::new();
            for (t, v) in pts {
                m.insert(format_ts(t), json!([round2(v[0]), round2(v[1]), round2(v[2])]));
            }
            bone_obj.insert("scale".into(), Value::Object(m));
        }
        bones_map.insert(bone.to_string(), Value::Object(bone_obj));
    }

    if bones_map.is_empty() {
        bones_map.insert(
            "right_arm".into(),
            json!({"rotation": {"0": [0.0, 0.0, 0.0], format_ts(dur): [0.0, 0.0, 0.0]}}),
        );
    }

    let mut obj = Map::new();
    obj.insert("loop".into(), Value::Bool(clip.looped));
    obj.insert("animation_length".into(), json!(round2(dur)));
    obj.insert("bones".into(), Value::Object(bones_map));

    if geckolib && include_events {
        if let Some(imp) = clip.impact_time {
            let mut pfx = Map::new();
            pfx.insert(
                format_ts(imp.clamp(0.01, dur)),
                json!({"effect": "voxelmotion:impact", "locator": "right_item"}),
            );
            obj.insert("particle_effects".into(), Value::Object(pfx));
        }
    }
    Value::Object(obj)
}

pub fn export_project(
    project: &AnimatorProject,
    workspace_root: &Path,
    custom_dest: Option<&Path>,
    format: ExportFormat,
    include_events: bool,
    optimize_keyframes: bool,
) -> Result<ExportSummary, String> {
    let mut bedrock_anims = Map::new();
    let mut geckolib_anims = Map::new();
    let mut bc_overrides = Map::new();
    let mut event_timings = Map::new();

    for clip in &project.clips {
        let slug: String = clip
            .id
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        let clean_slug = slug.trim_matches('_');
        let clip_key = format!("vm_{clean_slug}");
        let dur = clip.duration.max(0.08);
        let impact_time = clip.impact_time.map(|t| t.clamp(0.02, dur - 0.02));

        bedrock_anims.insert(
            clip_key.clone(),
            bake_clip_json(clip, optimize_keyframes, false, include_events),
        );
        geckolib_anims.insert(
            format!("animation.voxelmotion.{clean_slug}"),
            bake_clip_json(clip, optimize_keyframes, true, include_events),
        );

        for bc in &clip.bc_targets {
            if bc.contains(':') {
                let mut e = Map::new();
                e.insert("clip".into(), Value::String(clip_key.clone()));
                e.insert("transition_seconds".into(), json!(0.05));
                e.insert("locomotion_retain".into(), json!(0.16));
                if let Some(imp) = impact_time {
                    e.insert("impact_time".into(), json!(round2(imp)));
                }
                bc_overrides.insert(bc.clone(), Value::Object(e));
            }
        }

        if let Some(ev) = &clip.event_tag {
            let mut obj = Map::new();
            obj.insert("clip".into(), Value::String(clip_key.clone()));
            obj.insert("duration".into(), json!(round2(dur)));
            obj.insert("impact_time".into(), json!(round2(impact_time.unwrap_or(dur * 0.4))));
            obj.insert("loop".into(), Value::Bool(clip.looped));
            obj.insert("universal".into(), Value::Bool(clip.is_universal()));
            if !clip.allowed_categories.is_empty() {
                obj.insert(
                    "allowed_categories".into(),
                    Value::Array(
                        clip.allowed_categories
                            .iter()
                            .map(|c| Value::String(c.export_key().into()))
                            .collect(),
                    ),
                );
            }
            if !clip.allowed_weapons.is_empty() {
                obj.insert(
                    "allowed_weapons".into(),
                    Value::Array(clip.allowed_weapons.iter().cloned().map(Value::String).collect()),
                );
            }
            event_timings.insert(ev.clone(), Value::Object(obj));
        } else if let Some(imp) = impact_time {
            let mut obj = Map::new();
            obj.insert("clip".into(), Value::String(clip_key.clone()));
            obj.insert("duration".into(), json!(round2(dur)));
            obj.insert("impact_time".into(), json!(round2(imp)));
            obj.insert("loop".into(), Value::Bool(clip.looped));
            obj.insert("universal".into(), Value::Bool(clip.is_universal()));
            if !clip.allowed_categories.is_empty() {
                obj.insert(
                    "allowed_categories".into(),
                    Value::Array(
                        clip.allowed_categories
                            .iter()
                            .map(|c| Value::String(c.export_key().into()))
                            .collect(),
                    ),
                );
            }
            if !clip.allowed_weapons.is_empty() {
                obj.insert(
                    "allowed_weapons".into(),
                    Value::Array(clip.allowed_weapons.iter().cloned().map(Value::String).collect()),
                );
            }
            event_timings.insert(clip_key.clone(), Value::Object(obj));
        }
    }

    let bedrock_doc = json!({
        "format_version": "1.8.0",
        "animations": Value::Object(bedrock_anims.clone()),
    });
    let geckolib_doc = json!({
        "format_version": "1.8.0",
        "geckolib_format_version": 2,
        "animations": Value::Object(geckolib_anims),
    });
    let timings_doc = json!({
        "format_version": 1,
        "events": Value::Object(event_timings.clone()),
    });
    let compat_doc = json!({
        "format_version": 2,
        "defaults": {
            "better_combat": {
                "enabled": true,
                "transition_seconds": 0.06,
                "locomotion_retain": 0.16,
                "animations": Value::Object(bc_overrides.clone()),
            }
        }
    });

    // Save native project file in workspace root
    let project_path = workspace_root.join("project.json");
    std::fs::write(
        &project_path,
        serde_json::to_string_pretty(project).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    let out_dir = custom_dest
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.join("exports"));
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    if matches!(format, ExportFormat::Bedrock18 | ExportFormat::Both) {
        std::fs::write(
            out_dir.join("voxelmotion.animation.json"),
            serde_json::to_string_pretty(&bedrock_doc).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    if matches!(format, ExportFormat::GeckoLib4 | ExportFormat::Both) {
        std::fs::write(
            out_dir.join("voxelmotion_geckolib.animation.json"),
            serde_json::to_string_pretty(&geckolib_doc).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    if include_events {
        std::fs::write(
            out_dir.join("voxelmotion_timings.json"),
            serde_json::to_string_pretty(&timings_doc).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if !bc_overrides.is_empty() {
            std::fs::write(
                out_dir.join("voxelmotion_combat_compat.json"),
                serde_json::to_string_pretty(&compat_doc).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(ExportSummary {
        clips_exported: bedrock_anims.len(),
        bc_overrides: bc_overrides.len(),
        event_timings: event_timings.len(),
        targets: vec![out_dir],
    })
}

pub struct RellExportSummary {
    pub clips_exported: usize,
    pub bc_overrides: usize,
    pub vfx_events: usize,
    pub targets_written: usize,
}

#[inline]
fn round3(v: f32) -> f64 {
    ((v as f64) * 1000.0).round() / 1000.0
}

fn build_rell_timings_value(clips: &[&ClipDef], include_events: bool) -> serde_json::Value {
    let mut event_timings = Map::new();
    for clip in clips {
        if include_events {
            let mut ev = Map::new();
            ev.insert("clip".into(), Value::String(clip.id.clone()));
            ev.insert("duration".into(), serde_json::json!(round3(clip.duration)));
            if let Some(imp) = clip.impact_time {
                ev.insert("impact_time".into(), serde_json::json!(round3(imp)));
            }
            ev.insert("loop".into(), Value::Bool(clip.looped));
            if let Some(ref tag) = clip.event_tag {
                ev.insert("event_tag".into(), Value::String(tag.clone()));
            }
            ev.insert("universal".into(), Value::Bool(clip.is_universal()));
            if !clip.allowed_categories.is_empty() {
                ev.insert(
                    "allowed_categories".into(),
                    Value::Array(
                        clip.allowed_categories
                            .iter()
                            .map(|c| Value::String(c.export_key().into()))
                            .collect(),
                    ),
                );
            }
            if !clip.allowed_weapons.is_empty() {
                ev.insert(
                    "allowed_weapons".into(),
                    Value::Array(clip.allowed_weapons.iter().cloned().map(Value::String).collect()),
                );
            }
            if !clip.vfx_events.is_empty() {
                let vfx_arr: Vec<Value> = clip
                    .vfx_events
                    .iter()
                    .map(|vx| {
                        let r = (vx.color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
                        let g = (vx.color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
                        let b = (vx.color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
                        serde_json::json!({
                            "time": round3(vx.time),
                            "type": vx.kind.export_key(),
                            "color": format!("#{r:02X}{g:02X}{b:02X}"),
                            "scale": round3(vx.scale),
                            "duration": round3(vx.duration),
                            "intensity": round3(vx.intensity),
                            "texture": vx.texture_hint
                        })
                    })
                    .collect();
                ev.insert("vfx".into(), Value::Array(vfx_arr));
            }
            event_timings.insert(clip.id.clone(), Value::Object(ev));
        }
    }
    serde_json::json!({
        "schemaVersion": 2,
        "tps": 20,
        "events": event_timings
    })
}

fn build_punchy_compat_value(clips: &[&ClipDef]) -> (Value, usize) {
    let mut bc_overrides = Map::new();
    for clip in clips {
        if let Some(imp) = clip.impact_time {
            for tgt in &clip.bc_targets {
                let mut o = Map::new();
                o.insert("clip".into(), Value::String(clip.id.clone()));
                o.insert("impact_time".into(), serde_json::json!(round3(imp)));
                o.insert("locomotion_retain".into(), serde_json::json!(0.16));
                o.insert("transition_seconds".into(), serde_json::json!(0.05));
                bc_overrides.insert(tgt.clone(), Value::Object(o));
            }
        }
    }
    let count = bc_overrides.len();
    let doc = serde_json::json!({
        "format_version": 2,
        "defaults": {
            "better_combat": {
                "enabled": true,
                "locomotion_retain": 0.16,
                "transition_seconds": 0.06,
                "animations": bc_overrides
            }
        }
    });
    (doc, count)
}

/// Export ONLY approved (`CurationStatus::Aprobada`) combat clips to all 3 RELL Seas F3+T destinations
/// without overwriting ParCool or general Fluid & Foolish locomotion.
pub fn export_rell_seas_f3t(
    project: &AnimatorProject,
    server_root: &Path,
    include_events: bool,
    optimize_keyframes: bool,
) -> Result<RellExportSummary, String> {
    let approved: Vec<&ClipDef> = project
        .clips
        .iter()
        .filter(|c| c.status == CurationStatus::Aprobada)
        .collect();
    if approved.is_empty() {
        return Err("No hay clips marcados como 'Aprobada' para exportar a RELL Seas.".into());
    }

    let mut bedrock_anims = Map::new();
    let mut geckolib_anims = Map::new();
    let mut total_vfx = 0usize;
    for clip in &approved {
        bedrock_anims.insert(
            clip.id.clone(),
            bake_clip_json(clip, optimize_keyframes, false, include_events),
        );
        geckolib_anims.insert(
            clip.id.clone(),
            bake_clip_json(clip, optimize_keyframes, true, include_events),
        );
        total_vfx += clip.vfx_events.len();
    }

    let bedrock_str = serde_json::to_string_pretty(&serde_json::json!({
        "format_version": "1.8.0",
        "animations": bedrock_anims
    }))
    .map_err(|e| e.to_string())?;
    let geckolib_str = serde_json::to_string_pretty(&serde_json::json!({
        "format_version": "1.8.0",
        "animations": geckolib_anims
    }))
    .map_err(|e| e.to_string())?;
    let timings_str = serde_json::to_string_pretty(&build_rell_timings_value(&approved, include_events))
        .map_err(|e| e.to_string())?;
    let (compat_val, bc_count) = build_punchy_compat_value(&approved);
    let compat_str = serde_json::to_string_pretty(&compat_val).map_err(|e| e.to_string())?;

    let mod_combat_dir = server_root
        .join("src")
        .join("main")
        .join("resources")
        .join("assets")
        .join("rellseas")
        .join("animations")
        .join("combat");
    std::fs::create_dir_all(&mod_combat_dir).map_err(|e| format!("Error creando dir mod: {e}"))?;
    std::fs::write(mod_combat_dir.join("rellseas_combat.animation.json"), &bedrock_str)
        .map_err(|e| e.to_string())?;
    std::fs::write(mod_combat_dir.join("rellseas_geckolib.animation.json"), &geckolib_str)
        .map_err(|e| e.to_string())?;
    if include_events {
        std::fs::write(mod_combat_dir.join("rellseas_combat_timings.json"), &timings_str)
            .map_err(|e| e.to_string())?;
    }

    let pack_mcmeta = serde_json::json!({
        "pack": {
            "pack_format": 15,
            "description": "RELL Seas Combat Animations Override (VoxelMotion Studio)"
        }
    });
    let pack_mcmeta_str = serde_json::to_string_pretty(&pack_mcmeta).map_err(|e| e.to_string())?;

    let rp_roots = [
        server_root
            .join("sklauncher_modpack")
            .join("resourcepacks")
            .join("RELL_Seas_Combat_Animations"),
        server_root
            .join("dist")
            .join("sklauncher-client")
            .join("resourcepacks")
            .join("RELL_Seas_Combat_Animations"),
    ];
    for rp_root in &rp_roots {
        let pm_anim = rp_root.join("assets").join("punchy_moves").join("animations");
        let pm_compat = rp_root.join("assets").join("punchy_moves").join("compat");
        let rs_combat = rp_root
            .join("assets")
            .join("rellseas")
            .join("animations")
            .join("combat");
        std::fs::create_dir_all(&pm_anim).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&pm_compat).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&rs_combat).map_err(|e| e.to_string())?;

        if !rp_root.join("pack.mcmeta").is_file() {
            std::fs::write(rp_root.join("pack.mcmeta"), &pack_mcmeta_str).map_err(|e| e.to_string())?;
        }
        std::fs::write(pm_anim.join("rellseas_combat.animation.json"), &bedrock_str)
            .map_err(|e| e.to_string())?;
        std::fs::write(pm_compat.join("rellseas_combat.json"), &compat_str)
            .map_err(|e| e.to_string())?;
        std::fs::write(rs_combat.join("rellseas_combat.animation.json"), &bedrock_str)
            .map_err(|e| e.to_string())?;
        std::fs::write(rs_combat.join("rellseas_geckolib.animation.json"), &geckolib_str)
            .map_err(|e| e.to_string())?;
        if include_events {
            std::fs::write(rs_combat.join("rellseas_combat_timings.json"), &timings_str)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(RellExportSummary {
        clips_exported: approved.len(),
        bc_overrides: bc_count,
        vfx_events: total_vfx,
        targets_written: 3,
    })
}

/// Export the user's approved weapon shortlist (`curated_weapons_final.json`) inside `servernuevo/modpack/`.
pub fn export_curated_weapons_final(
    weapons: &[WeaponRecord],
    server_root: &Path,
) -> Result<(std::path::PathBuf, usize), String> {
    let approved: Vec<Value> = weapons
        .iter()
        .filter(|w| w.status == CurationStatus::Aprobada)
        .map(|w| {
            serde_json::json!({
                "id": w.id,
                "item_id": w.item_id,
                "name": w.name,
                "category": w.category.label(),
                "mod_source": w.mod_source,
                "weapon_class": w.weapon_class,
                "mesh_archetype": w.mesh_archetype,
                "bc_archetype": w.bc_archetype,
            })
        })
        .collect();
    if approved.is_empty() {
        return Err("Marcá al menos un arma como 'Aprobada' para exportar curated_weapons_final.json.".into());
    }
    let count = approved.len();
    let modpack_dir = server_root.join("modpack");
    std::fs::create_dir_all(&modpack_dir).map_err(|e| e.to_string())?;
    let out_path = modpack_dir.join("curated_weapons_final.json");
    let doc = serde_json::json!({
        "schemaVersion": 1,
        "totalWeapons": count,
        "weapons": approved
    });
    std::fs::write(&out_path, serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?)
        .map_err(|e| format!("No se pudo escribir curated_weapons_final.json: {e}"))?;
    Ok((out_path, count))
}

pub fn load_saved_project(workspace_root: &Path) -> Option<AnimatorProject> {
    let p = workspace_root.join("project.json");
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ClipSource, VfxEvent, VfxKind, WeaponCategory};

    #[test]
    fn exports_only_approved_clips_to_all_three_rell_targets_with_vfx_and_filter() {
        let tmp = std::env::temp_dir().join(format!("vm_rell_export_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        let mut c_ok = crate::model::create_blank_clip(1);
        c_ok.id = "rell_slash_test".into();
        c_ok.source = ClipSource::BetterCombat;
        c_ok.status = CurationStatus::Aprobada;
        c_ok.impact_time = Some(0.25);
        c_ok.bc_targets = vec!["bettercombat:one_handed_slash_horizontal_right".into()];
        c_ok.allowed_categories = vec![WeaponCategory::Espadas, WeaponCategory::Katanas];
        c_ok.vfx_events.push(VfxEvent {
            time: 0.25,
            kind: VfxKind::ParrySpark,
            color: [1.0, 0.8, 0.2],
            scale: 1.2,
            duration: 0.2,
            intensity: 1.0,
            texture_hint: "vfx_parry_star.png".into(),
        });
        let mut c_drop = crate::model::create_blank_clip(2);
        c_drop.id = "unwanted_clip".into();
        c_drop.status = CurationStatus::Descartada;

        let proj = AnimatorProject {
            version: 1,
            clips: vec![c_ok, c_drop],
        };

        let summary = export_rell_seas_f3t(&proj, &tmp, true, true).unwrap();
        assert_eq!(summary.clips_exported, 1);
        assert_eq!(summary.bc_overrides, 1);
        assert_eq!(summary.vfx_events, 1);

        let mod_timings = std::fs::read_to_string(
            tmp.join("src/main/resources/assets/rellseas/animations/combat/rellseas_combat_timings.json"),
        )
        .unwrap();
        assert!(mod_timings.contains("rell_slash_test"));
        assert!(!mod_timings.contains("unwanted_clip"));
        assert!(mod_timings.contains("parry_spark"));
        assert!(mod_timings.contains("allowed_categories"));
        assert!(mod_timings.contains("espadas"));

        let compat_rp1 = std::fs::read_to_string(
            tmp.join("sklauncher_modpack/resourcepacks/RELL_Seas_Combat_Animations/assets/punchy_moves/compat/rellseas_combat.json"),
        )
        .unwrap();
        let compat_rp2 = std::fs::read_to_string(
            tmp.join("dist/sklauncher-client/resourcepacks/RELL_Seas_Combat_Animations/assets/punchy_moves/compat/rellseas_combat.json"),
        )
        .unwrap();
        assert_eq!(compat_rp1, compat_rp2);
        assert!(compat_rp1.contains("bettercombat:one_handed_slash_horizontal_right"));

        let w_ok = WeaponRecord {
            id: "ss:katana".into(),
            name: "Katana".into(),
            mod_source: "Simply Swords".into(),
            item_id: "simplyswords:netherite_katana".into(),
            category: WeaponCategory::Katanas,
            weapon_class: "MEDIUM".into(),
            mesh_archetype: "katana".into(),
            status: CurationStatus::Aprobada,
            bc_archetype: "bettercombat:katana".into(),
            icon_rgba: None,
        };
        let (_, cnt) = export_curated_weapons_final(&[w_ok], &tmp).unwrap();
        assert_eq!(cnt, 1);
        assert!(tmp.join("modpack/curated_weapons_final.json").is_file());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
