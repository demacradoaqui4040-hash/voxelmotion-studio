// VoxelMotion Studio - Production Exporter (Bedrock 1.8.0 / GeckoLib 4.4.4 / Timings / PNG Strip)

use crate::model::{optimize_track_points, sample_clip_pose, AnimatorProject, ClipDef, BONE_NAMES};
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

        let has_rot = rot_raw.iter().any(|(_, v)| v.iter().any(|x| x.abs() > 0.05));
        let has_pos = pos_raw.iter().any(|(_, v)| v.iter().any(|x| x.abs() > 0.02));
        if !has_rot && !has_pos {
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
            event_timings.insert(
                ev.clone(),
                json!({
                    "clip": clip_key,
                    "duration": round2(dur),
                    "impact_time": round2(impact_time.unwrap_or(dur * 0.4)),
                    "loop": clip.looped
                }),
            );
        } else if let Some(imp) = impact_time {
            event_timings.insert(
                clip_key.clone(),
                json!({
                    "clip": clip_key,
                    "duration": round2(dur),
                    "impact_time": round2(imp),
                    "loop": clip.looped
                }),
            );
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

pub fn load_saved_project(workspace_root: &Path) -> Option<AnimatorProject> {
    let p = workspace_root.join("project.json");
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}
