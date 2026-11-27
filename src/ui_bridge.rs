// VoxelMotion Studio - Granular Slint Presenter & Event Wiring
// Keeps camera orbit/pan/zoom free of VecModel teardown and aligns simulation to BeforeRendering.

use crate::app_state::{bone_color, detect_workspace_root, export_storyboard_strip, AppState, DragSession};
use crate::design_spec::spec_for_theme;
use crate::exporter::{
    export_curated_weapons_final, export_project, export_rell_seas_f3t, ExportFormat,
};
use crate::math3d::{
    apply_gizmo_drag, camera_basis, evaluate_rig_matrices, pick_bone_ray, pick_gizmo_handle,
    GizmoAxis,
};
use crate::model::{
    mirror_pose, next_keyframe_time, prev_keyframe_time, sample_clip_pose, ClipDef,
    CurationStatus, EasingMode, Keyframe, ToolMode, VfxEvent, VfxKind, WeaponCategory,
    BONE_COUNT, BONE_LABELS_ES, BONE_NAMES,
};
use crate::renderer3d::GpuRenderer;
use crate::{
    CategoryEntry, CategoryRow, FamilyChipRow, GalleryItem, MainWindow, PaletteEntry, Theme,
    TimelineDiamond, TimelineTrack, VfxEventItem, WeaponListItem,
};
use slint::winit_030::winit::platform::windows::{CornerPreference, WindowExtWindows};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

thread_local! {
    static TOAST_WAKE: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static SAVE_WAKE: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
    static THUMB_WAKE: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}

fn wake_toast() {
    TOAST_WAKE.with(|c| {
        if let Some(ref f) = *c.borrow() {
            f();
        }
    });
}

fn wake_save() {
    SAVE_WAKE.with(|c| {
        if let Some(ref f) = *c.borrow() {
            f();
        }
    });
}

fn wake_thumb() {
    THUMB_WAKE.with(|c| {
        if let Some(ref f) = *c.borrow() {
            f();
        }
    });
}

pub struct StudioTimers {
    _toast: Rc<slint::Timer>,
    _save: Rc<slint::Timer>,
    _thumb: Rc<slint::Timer>,
    _capture: Option<slint::Timer>,
}

fn sync_theme_and_prefs(state: &AppState, win: &MainWindow) {
    let idx = state.theme_idx.clamp(0, 4);
    let th = win.global::<Theme>();
    spec_for_theme(idx).apply_to_slint(&th);
    th.set_motion_enabled(state.ui_motion_enabled);
    win.set_theme_idx(idx);
    win.set_ui_motion_enabled(state.ui_motion_enabled);
    win.set_gpu_low_power(state.gpu_low_power);
    win.set_focus_mode(state.focus_mode);
    win.set_show_gizmo(state.show_gizmo);
}

fn build_palette_entries(state: &AppState) -> Vec<PaletteEntry> {
    state
        .palette_matches()
        .into_iter()
        .map(|(kind, id, title, subtitle)| PaletteEntry {
            kind: kind.into(),
            id: id.into(),
            title: title.into(),
            subtitle: subtitle.into(),
        })
        .collect()
}

fn show_toast(state: &mut AppState, win: &MainWindow, msg: String) {
    win.set_toast_message(msg.into());
    win.set_toast_visible(true);
    state.toast_until = Some(Instant::now() + std::time::Duration::from_secs(3));
    wake_toast();
}

#[inline]
fn request_immediate_redraw(win: &MainWindow) {
    let mut sent = false;
    win.window().with_winit_window(|ww| {
        ww.request_redraw();
        sent = true;
    });
    if !sent {
        win.window().request_redraw();
    }
}

#[inline]
fn request_coalesced_redraw(win: &MainWindow, queued: &Cell<bool>) {
    if queued.replace(true) {
        return;
    }
    request_immediate_redraw(win);
}

fn sync_status_pill(state: &AppState, win: &MainWindow) {
    let tool_lbl = match state.tool_mode {
        ToolMode::Select => "Seleccionar",
        ToolMode::Rotate => "Rotar",
        ToolMode::Scale => "Escalar",
        ToolMode::Camera => "Cámara",
        ToolMode::Move => "Mover",
    };
    let hl_axis = if state.drag.active && state.drag.active_gizmo != GizmoAxis::None {
        state.drag.active_gizmo
    } else {
        state.hovered_gizmo
    };
    let axis_suffix = if hl_axis == GizmoAxis::None {
        String::new()
    } else {
        format!(" · {}", hl_axis.label_es())
    };
    win.set_status_pill(
        format!(
            "{} · {}{}",
            BONE_LABELS_ES[state.active_bone_idx.min(BONE_COUNT - 1)],
            tool_lbl,
            axis_suffix
        )
        .into(),
    );
}

fn sync_catalog(state: &AppState, win: &MainWindow) {
    let cats: Vec<CategoryEntry> = WeaponCategory::ALL_SIDEBAR
        .iter()
        .map(|c| {
            let count = if *c == WeaponCategory::Todas {
                state.weapons.len() as i32
            } else {
                state.weapons.iter().filter(|w| w.category == *c).count() as i32
            };
            CategoryEntry {
                name: c.label().into(),
                count,
            }
        })
        .collect();
    win.set_categories(ModelRc::new(VecModel::from(cats)));

    let rows: Vec<CategoryRow> = WeaponCategory::ALL_FAMILIES
        .chunks_exact(2)
        .enumerate()
        .map(|(pair_idx, pair)| {
            let left_cat = pair[0];
            let right_cat = pair[1];
            let left_count = state.weapons.iter().filter(|w| w.category == left_cat).count() as i32;
            let right_count = state.weapons.iter().filter(|w| w.category == right_cat).count() as i32;
            CategoryRow {
                left_idx: (pair_idx * 2 + 1) as i32,
                left_name: left_cat.short_label().into(),
                left_count,
                right_idx: (pair_idx * 2 + 2) as i32,
                right_name: right_cat.short_label().into(),
                right_count,
            }
        })
        .collect();
    win.set_category_rows(ModelRc::new(VecModel::from(rows)));
    win.set_total_weapon_count(state.weapons.len() as i32);

    let list: Vec<WeaponListItem> = state
        .filtered_indices
        .iter()
        .take(200)
        .map(|&idx| {
            let w = &state.weapons[idx];
            WeaponListItem {
                index: idx as i32,
                id: w.id.clone().into(),
                name: w.name.clone().into(),
                mod_name: w.mod_source.clone().into(),
                category: w.category.label().into(),
                status_tag: w.status.short_tag().into(),
                status_idx: w.status.index(),
                icon: state.weapon_icons.get(idx).cloned().unwrap_or_default(),
            }
        })
        .collect();
    win.set_filtered_weapons(ModelRc::new(VecModel::from(list)));
    win.set_weapon_status_filter(state.weapon_status_filter);
}

fn sync_gallery(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    let active_w = state.active_weapon();
    let visible = state.filtered_clip_indices();
    let g_items: Vec<GalleryItem> = visible
        .into_iter()
        .map(|idx| &state.project.clips[idx])
        .map(|c| GalleryItem {
            id: c.id.clone().into(),
            name: c.name.clone().into(),
            group: format!("{:?}", c.gallery_group).into(),
            source_label: c.source.short_label().into(),
            target_label: c.target_badge().into(),
            compatible: c.allows_weapon(active_w),
            status_tag: c.status.short_tag().into(),
            status_idx: c.status.index(),
            thumb: state.gallery_thumbs.get(&c.id).cloned().unwrap_or_default(),
            active: c.id == clip.id,
        })
        .collect();
    let existing = win.get_gallery_items();
    let mut patched_in_place = false;
    if let Some(vm) = existing.as_any().downcast_ref::<VecModel<GalleryItem>>() {
        if vm.row_count() == g_items.len()
            && (0..g_items.len()).all(|i| {
                vm.row_data(i)
                    .map(|old| old.id == g_items[i].id)
                    .unwrap_or(false)
            })
        {
            for (i, new_item) in g_items.iter().enumerate() {
                if let Some(old) = vm.row_data(i) {
                    let thumb_changed =
                        (old.thumb.size().width == 0) != (new_item.thumb.size().width == 0);
                    if old.active != new_item.active
                        || old.compatible != new_item.compatible
                        || old.target_label != new_item.target_label
                        || old.status_idx != new_item.status_idx
                        || old.status_tag != new_item.status_tag
                        || thumb_changed
                    {
                        vm.set_row_data(i, new_item.clone());
                    }
                }
            }
            patched_in_place = true;
        }
    }
    if !patched_in_place {
        win.set_gallery_items(ModelRc::new(VecModel::from(g_items)));
    }
    win.set_gallery_compat_only(state.gallery_compat_only);
    win.set_gallery_source_filter(state.gallery_source_filter);
    win.set_gallery_status_filter(state.gallery_status_filter);
    win.set_featured_gallery_id(clip.id.clone().into());
    win.set_featured_gallery_name(clip.name.clone().into());
    win.set_featured_gallery_thumb(state.gallery_thumbs.get(&clip.id).cloned().unwrap_or_default());
}

/// Lightweight per-frame / scrub / step readout sync: updates only active bone numbers & KF badge without rebuilding VFX or counting modpack items.
fn sync_transform_readout(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    win.set_playing(state.playing);

    let pose = sample_clip_pose(clip, state.current_time);
    let tr = pose.bone(state.active_bone_idx);
    win.set_active_bone_idx(state.active_bone_idx as i32);
    win.set_active_bone_name(BONE_LABELS_ES[state.active_bone_idx.min(BONE_COUNT - 1)].into());
    win.set_pos_x(format!("{:.2}", tr.pos[0]).into());
    win.set_pos_y(format!("{:.2}", tr.pos[1]).into());
    win.set_pos_z(format!("{:.2}", tr.pos[2]).into());
    win.set_rot_x(format!("{:.1}°", tr.rot[0]).into());
    win.set_rot_y(format!("{:.1}°", tr.rot[1]).into());
    win.set_rot_z(format!("{:.1}°", tr.rot[2]).into());
    win.set_scl_x(format!("{:.2}", tr.scale[0]).into());
    win.set_scl_y(format!("{:.2}", tr.scale[1]).into());
    win.set_scl_z(format!("{:.2}", tr.scale[2]).into());

    let kf_count = clip.keyframes.len().max(1);
    let exact_kf = clip
        .keyframes
        .iter()
        .position(|k| (k.time - state.current_time).abs() < 0.016);
    let nearest_kf_idx = exact_kf.unwrap_or_else(|| {
        clip.keyframes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.time - state.current_time)
                    .abs()
                    .total_cmp(&(b.time - state.current_time).abs())
            })
            .map(|(i, _)| i)
            .unwrap_or(0)
    });
    if let Some(nk) = clip.keyframes.get(nearest_kf_idx) {
        win.set_easing_idx(nk.easing.index());
    }
    win.set_selected_kf_idx(nearest_kf_idx as i32);
    if exact_kf.is_some() {
        win.set_kf_badge(format!("KF {}/{}", nearest_kf_idx + 1, kf_count).into());
    } else {
        win.set_kf_badge(format!("~{}/{} KF", nearest_kf_idx + 1, kf_count).into());
    }
}

fn sync_inspector(state: &AppState, win: &MainWindow) {
    let w = state.active_weapon();
    win.set_has_rell_profile(state.servernuevo_root.is_some());
    let profile_lbl = if state.servernuevo_root.is_some() {
        let app_w = state
            .weapons
            .iter()
            .filter(|item| item.status == CurationStatus::Aprobada)
            .count();
        let app_c = state
            .project
            .clips
            .iter()
            .filter(|c| c.status == CurationStatus::Aprobada)
            .count();
        format!(
            "RELL Seas · {}/{} armas ({} OK) · {} clips ({} OK)",
            state.weapons.len(),
            state.curated_manifest_count,
            app_w,
            state.project.clips.len(),
            app_c
        )
    } else {
        "Modo Público (sin modpack)".to_string()
    };
    win.set_rell_profile_label(profile_lbl.into());
    win.set_clips_count(state.project.clips.len() as i32);
    win.set_active_weapon_index(state.active_weapon_idx as i32);
    win.set_active_weapon_name(w.name.clone().into());
    win.set_active_weapon_mod(w.mod_source.clone().into());
    win.set_active_weapon_family(w.category.label().into());
    win.set_active_weapon_class(w.weapon_class.clone().into());
    win.set_active_weapon_status(w.status.index());
    win.set_active_weapon_bc_arch(w.bc_archetype.clone().into());
    win.set_solo_weapon(state.solo_weapon);

    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    win.set_active_clip_name(clip.name.clone().into());
    win.set_active_clip_status(clip.status.index());
    win.set_active_clip_source(clip.source.label().into());
    win.set_active_clip_compatible(clip.allows_weapon(w));
    win.set_active_clip_targets(clip.target_summary(&state.weapons).into());
    win.set_active_clip_universal(clip.is_universal());
    win.set_active_clip_only_current(
        clip.allowed_categories.is_empty()
            && clip.allowed_weapons.len() == 1
            && clip.allowed_weapons[0] == w.id,
    );
    let fam_rows: Vec<FamilyChipRow> = WeaponCategory::ALL_FAMILIES
        .chunks_exact(2)
        .enumerate()
        .map(|(pair_idx, pair)| FamilyChipRow {
            left_idx: (pair_idx * 2) as i32,
            left_label: pair[0].short_label().into(),
            left_on: clip.allowed_categories.contains(&pair[0]),
            right_idx: (pair_idx * 2 + 1) as i32,
            right_label: pair[1].short_label().into(),
            right_on: clip.allowed_categories.contains(&pair[1]),
        })
        .collect();
    win.set_clip_family_rows(ModelRc::new(VecModel::from(fam_rows)));
    let vfx_rows: Vec<VfxEventItem> = clip
        .vfx_events
        .iter()
        .enumerate()
        .map(|(i, ev)| {
            let r = (ev.color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let g = (ev.color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            let b = (ev.color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            VfxEventItem {
                index: i as i32,
                time_text: format!("{:.2}s", ev.time).into(),
                kind_label: ev.kind.label().into(),
                color_hex: format!("#{r:02X}{g:02X}{b:02X}").into(),
                scale_text: format!("{:.1}x", ev.scale).into(),
            }
        })
        .collect();
    win.set_clip_vfx_events(ModelRc::new(VecModel::from(vfx_rows)));
    win.set_anim_loop(clip.looped);
    win.set_duration_val(dur);
    win.set_duration_text(format!("{dur:.2}").into());
    win.set_speed_label(
        format!("{:.2}x", state.speed)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
            .into(),
    );
    win.set_shadows_enabled(state.shadows);
    win.set_onion_enabled(state.onion);
    win.set_trail_enabled(state.trail);
    win.set_bg_idx(state.bg_mode);
    win.set_auto_keyframe(state.auto_keyframe);
    win.set_show_all_bones(state.show_all_bones);
    sync_transform_readout(state, win);
}

fn sync_timeline_meta(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    let frames = (dur * 60.0).round().max(1.0) as i32;
    let cur_f = ((state.current_time / dur) * (frames as f32)).round() as i32;
    win.set_current_frame(cur_f.clamp(0, frames));
    win.set_total_frames(frames);
    win.set_playhead_frac((state.current_time / dur).clamp(0.0, 1.0));
    win.set_timeline_bone_scope(state.timeline_bone_scope);
    if let Some(ref cb) = state.clipboard_bone {
        win.set_bone_clipboard_count(cb.frames.len() as i32);
        win.set_bone_clipboard_label(
            format!(
                "{} ({}f)",
                BONE_LABELS_ES[cb.source_bone_idx.min(BONE_COUNT - 1)],
                cb.frames.len()
            )
            .into(),
        );
    } else {
        win.set_bone_clipboard_count(0);
        win.set_bone_clipboard_label("".into());
    }
    win.set_has_impact(clip.impact_time.is_some());
    if let Some(imp) = clip.impact_time {
        win.set_impact_frac((imp / dur).clamp(0.0, 1.0));
        win.set_impact_time_text(format!("{imp:.2}s").into());
    } else {
        win.set_impact_frac(0.0);
        win.set_impact_time_text("Sin marca".into());
    }
}

fn sync_timeline_tracks(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    let frames = (dur * 60.0).round().max(1.0) as i32;
    let tick_labels: Vec<slint::SharedString> = (0..=6)
        .map(|i| {
            slint::SharedString::from(format!(
                "{}f",
                ((frames as f32 * (i as f32 / 6.0)).round() as i32)
            ))
        })
        .collect();
    win.set_ruler_ticks(ModelRc::new(VecModel::from(tick_labels)));

    let all_tracks: Vec<TimelineTrack> = (0..BONE_COUNT)
        .map(|bi| TimelineTrack {
            bone_idx: bi as i32,
            name: BONE_LABELS_ES[bi].into(),
            dot_color: bone_color(bi),
            active: state.active_bone_idx == bi,
            visible: !state.hidden_bones[bi],
        })
        .collect();
    win.set_all_bones_tracks(ModelRc::new(VecModel::from(all_tracks)));

    let visible_bones: &[usize] = if state.show_all_bones {
        &[0, 1, 2, 3, 4, 5, 6, 7]
    } else {
        &[3, 4, 5, 1]
    };
    let tracks: Vec<TimelineTrack> = visible_bones
        .iter()
        .map(|&bi| TimelineTrack {
            bone_idx: bi as i32,
            name: BONE_LABELS_ES[bi].into(),
            dot_color: bone_color(bi),
            active: state.active_bone_idx == bi,
            visible: !state.hidden_bones[bi],
        })
        .collect();
    win.set_timeline_tracks(ModelRc::new(VecModel::from(tracks)));
}

fn sync_timeline_diamonds(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    let visible_bones: &[usize] = if state.show_all_bones {
        &[0, 1, 2, 3, 4, 5, 6, 7]
    } else {
        &[3, 4, 5, 1]
    };
    let mut diamonds = Vec::new();
    for (ki, kf) in clip.keyframes.iter().enumerate() {
        let frac = (kf.time / dur).clamp(0.0, 1.0);
        let is_imp = clip
            .impact_time
            .map(|imp| (kf.time - imp).abs() < 0.02)
            .unwrap_or(false);
        let explicit_mask = kf.bone_mask != crate::model::ALL_BONES_MASK;
        for (ti, &bi) in visible_bones.iter().enumerate() {
            if !kf.has_bone(bi) {
                continue;
            }
            let t = kf.bones.bone(bi);
            let touched = explicit_mask
                || ki == 0
                || ki + 1 == clip.keyframes.len()
                || t.rot.iter().any(|x| x.abs() > 0.1)
                || t.pos.iter().any(|x| x.abs() > 0.05)
                || t.scale.iter().any(|x| (x - 1.0).abs() > 0.02);
            if touched {
                let in_range = state.selected_range_bone == Some(bi)
                    && state.selected_range_times.map_or(false, |(t0, t1)| {
                        kf.time >= t0.min(t1) - 0.016 && kf.time <= t0.max(t1) + 0.016
                    });
                let is_sel = in_range
                    || (state.selected_range_bone.is_none()
                        && (kf.time - state.current_time).abs() < 0.016
                        && state.active_bone_idx == bi);
                diamonds.push(TimelineDiamond {
                    kf_idx: ki as i32,
                    track_idx: ti as i32,
                    frac,
                    is_impact: is_imp,
                    is_selected: is_sel,
                    dot_color: bone_color(bi),
                });
            }
        }
    }
    if let (Some(rb), Some((t0, t1))) = (state.selected_range_bone, state.selected_range_times) {
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let count = clip
            .keyframes
            .iter()
            .filter(|k| k.has_bone(rb) && k.time >= lo - 0.016 && k.time <= hi + 0.016)
            .count() as i32;
        let track_i = visible_bones
            .iter()
            .position(|&b| b == rb)
            .map(|i| i as i32)
            .unwrap_or(-1);
        win.set_selected_range_count(count);
        win.set_selected_range_track_idx(track_i);
        win.set_selected_range_start_frac((lo / dur).clamp(0.0, 1.0));
        win.set_selected_range_end_frac((hi / dur).clamp(0.0, 1.0));
    } else {
        win.set_selected_range_count(0);
        win.set_selected_range_track_idx(-1);
        win.set_selected_range_start_frac(0.0);
        win.set_selected_range_end_frac(0.0);
    }
    let existing = win.get_timeline_diamonds();
    if let Some(vm) = existing.as_any().downcast_ref::<VecModel<TimelineDiamond>>() {
        if vm.row_count() == diamonds.len()
            && (0..diamonds.len()).all(|i| {
                vm.row_data(i)
                    .map(|old| {
                        old.kf_idx == diamonds[i].kf_idx
                            && old.track_idx == diamonds[i].track_idx
                            && old.is_impact == diamonds[i].is_impact
                            && old.dot_color == diamonds[i].dot_color
                    })
                    .unwrap_or(false)
            })
        {
            for (i, new_d) in diamonds.iter().enumerate() {
                if let Some(old) = vm.row_data(i) {
                    if old.is_selected != new_d.is_selected
                        || (old.frac - new_d.frac).abs() > 1e-5
                    {
                        vm.set_row_data(i, new_d.clone());
                    }
                }
            }
            return;
        }
    }
    win.set_timeline_diamonds(ModelRc::new(VecModel::from(diamonds)));
}

/// Synchronize only the Slint UI sections marked dirty, leaving untouched models intact.
pub fn sync_dirty_ui(state: &mut AppState, win: &MainWindow) {
    if !state.dirty.any_ui() {
        return;
    }
    if state.dirty.catalog {
        sync_catalog(state, win);
        state.dirty.catalog = false;
    }
    if state.dirty.gallery {
        sync_gallery(state, win);
        state.dirty.gallery = false;
    }
    state.dirty.triad = false;
    if state.dirty.status_pill {
        sync_status_pill(state, win);
        state.dirty.status_pill = false;
    }
    if state.dirty.inspector {
        sync_inspector(state, win);
        state.dirty.inspector = false;
        state.dirty.transform_readout = false;
    } else if state.dirty.transform_readout {
        sync_transform_readout(state, win);
        state.dirty.transform_readout = false;
    }
    if state.dirty.timeline_meta {
        sync_timeline_meta(state, win);
        state.dirty.timeline_meta = false;
    }
    if state.dirty.timeline_tracks {
        sync_timeline_tracks(state, win);
        state.dirty.timeline_tracks = false;
    }
    if state.dirty.timeline_diamonds {
        sync_timeline_diamonds(state, win);
        state.dirty.timeline_diamonds = false;
    }
    win.set_can_undo(!state.undo_stack.is_empty());
    win.set_can_redo(!state.redo_stack.is_empty());
    win.set_has_unsaved(state.profile_dirty);
    win.set_show_gizmo(state.show_gizmo);
    if state.profile_dirty {
        wake_save();
    }
}

pub fn wire_ui_and_gpu(
    win: &MainWindow,
    state: &Rc<RefCell<AppState>>,
    vp_size_cell: &Rc<Cell<(u32, u32)>>,
    auto_capture_path: Option<PathBuf>,
    bench_window: bool,
    bench_orbit_zoom: bool,
) -> StudioTimers {
    // Event-driven Zero-Idle Timers (self-stopping when idle)
    let toast_timer = Rc::new(slint::Timer::default());
    let save_timer = Rc::new(slint::Timer::default());
    let thumb_timer = Rc::new(slint::Timer::default());
    let redraw_queued = Rc::new(Cell::new(false));
    let any_bench = bench_window || bench_orbit_zoom;

    {
        let t_ref = toast_timer.clone();
        let st = state.clone();
        let w_weak = win.as_weak();
        TOAST_WAKE.with(|c| {
            *c.borrow_mut() = Some(Rc::new(move || {
                let st2 = st.clone();
                let w2 = w_weak.clone();
                t_ref.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_millis(3050),
                    move || {
                        let Some(w) = w2.upgrade() else { return };
                        let mut s = st2.borrow_mut();
                        if let Some(until) = s.toast_until {
                            if Instant::now() >= until {
                                s.toast_until = None;
                                w.set_toast_visible(false);
                            }
                        }
                    },
                );
            }));
        });
    }

    {
        let s_ref = save_timer.clone();
        let s_ref_inner = save_timer.clone();
        let st = state.clone();
        let w_weak = win.as_weak();
        SAVE_WAKE.with(|c| {
            *c.borrow_mut() = Some(Rc::new(move || {
                let st2 = st.clone();
                let w2 = w_weak.clone();
                let sr = s_ref_inner.clone();
                s_ref.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_millis(400),
                    move || {
                        let Some(w) = w2.upgrade() else {
                            return;
                        };
                        let mut s = st2.borrow_mut();
                        if !s.profile_dirty {
                            return;
                        }
                        if s.drag.active
                            || s.main_cam.is_actively_zooming()
                            || s.last_interaction_at.elapsed().as_millis() < 350
                        {
                            let _ = &sr;
                            wake_save();
                        } else {
                            s.flush_profile_if_dirty(false);
                            w.set_has_unsaved(s.profile_dirty);
                        }
                    },
                );
            }));
        });
    }
    {
        let th_ref = thumb_timer.clone();
        let w_weak = win.as_weak();
        let rq = redraw_queued.clone();
        THUMB_WAKE.with(|c| {
            *c.borrow_mut() = Some(Rc::new(move || {
                let w2 = w_weak.clone();
                let rq2 = rq.clone();
                th_ref.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_millis(270),
                    move || {
                        if let Some(w) = w2.upgrade() {
                            request_coalesced_redraw(&w, &rq2);
                        }
                    },
                );
            }));
        });
    }

    {
        let s = state.borrow();
        sync_theme_and_prefs(&s, win);
    }

    // Attach GPU rendering notifier (zero-copy WGPU30 device/queue shared with Slint, frame-locked simulation!)
    let bench_start = Rc::new(RefCell::new(Option::<Instant>::None));
    let bench_before_end = Rc::new(Cell::new(Instant::now()));
    let bench_after_end = Rc::new(Cell::new(Instant::now()));
    let bench_frame_start = Rc::new(Cell::new(Instant::now()));
    let bench_ui_us = Rc::new(Cell::new(0u64));
    let bench_wait_us = Rc::new(Cell::new(0u64));
    let bench_frames_us = Rc::new(RefCell::new(Vec::<u64>::with_capacity(64)));
    {
        let st = state.clone();
        let vp_c = vp_size_cell.clone();
        let w_weak = win.as_weak();
        let b_start = bench_start.clone();
        let b_before = bench_before_end.clone();
        let b_after = bench_after_end.clone();
        let b_fstart = bench_frame_start.clone();
        let b_ui = bench_ui_us.clone();
        let b_wait = bench_wait_us.clone();
        let b_hist = bench_frames_us.clone();
        let rq_notifier = redraw_queued.clone();
        let _ = win.window().set_rendering_notifier(move |render_state, api| {
            let (Some(win), slint::GraphicsAPI::WGPU30 { device, queue, .. }) =
                (w_weak.upgrade(), api)
            else {
                return;
            };
            match render_state {
                slint::RenderingState::RenderingSetup => {
                    win.window().with_winit_window(|ww| {
                        ww.set_corner_preference(CornerPreference::Round);
                    });
                    let (imgs, low_power) = {
                        let Ok(mut s) = st.try_borrow_mut() else { return };
                        if s.gpu.is_none() {
                            s.gpu = Some(GpuRenderer::new(device.clone(), queue.clone()));
                        }
                        s.dirty.main_viewport = true;
                        s.dirty.card_viewport = true;
                        (s.render_gpu_frames_only(vp_c.get()), s.gpu_low_power)
                    };
                    let adapter_lbl = if low_power {
                        "DirectX 12 · Ahorro Batería (iGPU)"
                    } else {
                        "DirectX 12 · Alto Rendimiento"
                    };
                    win.set_gpu_adapter_name(adapter_lbl.into());
                    if let Some(im) = imgs.0 { win.set_main_viewport(im); }
                    if let Some(im) = imgs.1 { win.set_weapon_card_img(im); }
                    if let Ok(mut s) = st.try_borrow_mut() {
                        sync_theme_and_prefs(&s, &win);
                        s.generate_gallery_thumbs_if_needed();
                        s.dirty.gallery = true;
                        s.last_render_tick = Instant::now();
                        if let Some(ref dest) = s.custom_export_dest {
                            win.set_export_dest(dest.to_string_lossy().to_string().into());
                        }
                        sync_dirty_ui(&mut s, &win);
                    }
                    if any_bench {
                        let now = Instant::now();
                        *b_start.borrow_mut() = Some(now);
                        b_before.set(now);
                        b_after.set(now);
                        b_fstart.set(now);
                    }
                }
                slint::RenderingState::BeforeRendering => {
                    rq_notifier.set(false);
                    let t0 = Instant::now();
                    if any_bench {
                        b_wait.set(b_wait.get() + (t0 - b_after.get()).as_micros() as u64);
                        b_hist.borrow_mut().push((t0 - b_fstart.get()).as_micros() as u64);
                        b_fstart.set(t0);
                    }
                    let (imgs, did_main) = {
                        let Ok(mut s) = st.try_borrow_mut() else { return };
                        let now = Instant::now();
                        let raw_dt = (now - s.last_render_tick).as_secs_f32();
                        // Give idle wakeup frames a crisp initial step for wheel detents while avoiding >50ms playback jumps.
                        let dt = if raw_dt > 0.05 {
                            if s.main_cam.is_animating() {
                                0.016
                            } else {
                                0.008
                            }
                        } else {
                            raw_dt.clamp(0.001, 0.05)
                        };
                        s.last_render_tick = now;
                        s.step_frame_simulation(dt);
                        let dm = s.dirty.main_viewport;
                        let imgs = s.render_gpu_frames_only(vp_c.get());
                        let quiet_idle = !s.drag.active
                            && !s.playing
                            && !s.main_cam.is_animating()
                            && !s.main_cam.is_actively_zooming()
                            && s.last_interaction_at.elapsed().as_millis() >= 250;
                        if quiet_idle && s.has_pending_visible_thumbs() {
                            if s.generate_gallery_thumbs_batch(2) {
                                s.dirty.gallery = true;
                            }
                        }
                        (imgs, dm)
                    };
                    if did_main {
                        let elapsed_us = t0.elapsed().as_micros() as u64;
                        if let Ok(mut s) = st.try_borrow_mut() {
                            s.gpu_pass_us_accum += elapsed_us;
                            s.gpu_pass_samples += 1;
                        }
                    }
                    if let Some(im) = imgs.0 { win.set_main_viewport(im); }
                    if let Some(im) = imgs.1 { win.set_weapon_card_img(im); }
                    if let Ok(mut s) = st.try_borrow_mut() {
                        if s.dirty.any_ui() {
                            sync_dirty_ui(&mut s, &win);
                        }
                    }
                    if any_bench {
                        b_before.set(Instant::now());
                    }
                }
                slint::RenderingState::AfterRendering => {
                    let t_after = Instant::now();
                    if any_bench {
                        b_ui.set(b_ui.get() + (t_after - b_before.get()).as_micros() as u64);
                    }
                    let mut bench_done: Option<String> = None;
                    let mut keep_redrawing = false;
                    let mut schedule_thumb_wake = false;
                    if let Ok(mut s) = st.try_borrow_mut() {
                        s.present_frames_total += 1;
                        if bench_orbit_zoom {
                            if s.present_frames_total <= 60 {
                                for _ in 0..4 {
                                    s.main_cam.orbit(3.5, 1.2, 0.009, 0.008, -1.25, 1.48);
                                }
                                s.note_interaction();
                                s.dirty.main_viewport = true;
                                keep_redrawing = true;
                            } else if s.present_frames_total == 61 {
                                s.note_interaction();
                                s.dirty.main_viewport = true;
                                keep_redrawing = true;
                            } else if s.present_frames_total <= 91 {
                                s.note_interaction();
                                s.queue_zoom_at_cursor(
                                    390.0,
                                    260.0,
                                    vp_c.get().0 as f32,
                                    vp_c.get().1 as f32,
                                    28.0,
                                );
                                s.main_cam.step_damping(0.016);
                                s.dirty.main_viewport = true;
                                keep_redrawing = true;
                            } else if let Some(t0) = *b_start.borrow() {
                                let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
                                let n = s.present_frames_total.max(1) as f64;
                                let avg_gpu_ms = if s.gpu_pass_samples > 0 {
                                    (s.gpu_pass_us_accum as f64 / s.gpu_pass_samples as f64)
                                        / 1000.0
                                } else {
                                    0.0
                                };
                                let raw_hist = b_hist.borrow().clone();
                                let mut hist = raw_hist;
                                if hist.len() > 2 {
                                    hist.drain(0..2);
                                }
                                hist.sort_unstable();
                                let p50 = hist.get(hist.len() / 2).copied().unwrap_or(0) as f64 / 1000.0;
                                let p95 = hist.get((hist.len() * 95) / 100).copied().unwrap_or(0) as f64 / 1000.0;
                                let pmax = hist.last().copied().unwrap_or(0) as f64 / 1000.0;
                                let steady_avg = if hist.is_empty() { 0.0 } else { (hist.iter().sum::<u64>() as f64 / hist.len() as f64) / 1000.0 };
                                let avg_ui_ms = (b_ui.get() as f64 / n) / 1000.0;
                                let avg_wait_ms = (b_wait.get() as f64 / n) / 1000.0;
                                bench_done = Some(format!(
                                    "BENCH_ORBIT_ZOOM_OK: frames={} total={:.1}ms steady_avg={:.2}ms(steady_fps={:.1}) p50={:.2}ms p95={:.2}ms max={:.2}ms gpu3d={:.2}ms ui={:.2}ms wait={:.2}ms",
                                    s.present_frames_total, total_ms, steady_avg, 1000.0 / steady_avg.max(0.01), p50, p95, pmax, avg_gpu_ms, avg_ui_ms, avg_wait_ms
                                ));
                            }
                        } else if bench_window {
                            if s.present_frames_total <= 60 {
                                s.queue_zoom_at_cursor(
                                    390.0,
                                    260.0,
                                    vp_c.get().0 as f32,
                                    vp_c.get().1 as f32,
                                    30.0,
                                );
                                s.main_cam.step_damping(0.016);
                                s.dirty.main_viewport = true;
                                keep_redrawing = true;
                            } else if let Some(t0) = *b_start.borrow() {
                                let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
                                let n = s.present_frames_total.max(1) as f64;
                                let avg_gpu_ms = if s.gpu_pass_samples > 0 {
                                    (s.gpu_pass_us_accum as f64 / s.gpu_pass_samples as f64)
                                        / 1000.0
                                } else {
                                    0.0
                                };
                                let avg_ui_ms = (b_ui.get() as f64 / n) / 1000.0;
                                let avg_wait_ms = (b_wait.get() as f64 / n) / 1000.0;
                                let raw_hist = b_hist.borrow().clone();
                                let first_ms = raw_hist.get(1).copied().unwrap_or(0) as f64 / 1000.0;
                                let mut hist = raw_hist;
                                if hist.len() > 2 {
                                    hist.drain(0..2);
                                }
                                hist.sort_unstable();
                                let pmin = hist.first().copied().unwrap_or(0) as f64 / 1000.0;
                                let p50 = hist.get(hist.len() / 2).copied().unwrap_or(0) as f64 / 1000.0;
                                let p95 = hist.get((hist.len() * 95) / 100).copied().unwrap_or(0) as f64 / 1000.0;
                                let pmax = hist.last().copied().unwrap_or(0) as f64 / 1000.0;
                                let steady_avg = if hist.is_empty() { 0.0 } else { (hist.iter().sum::<u64>() as f64 / hist.len() as f64) / 1000.0 };
                                let phys = win.window().size();
                                let sf = win.window().scale_factor();
                                bench_done = Some(format!(
                                    "BENCH_GUI_OK: frames={} total={:.1}ms f1={:.1}ms steady_avg={:.2}ms(steady_fps={:.1}) min/p50/p95/max={:.2}/{:.2}/{:.2}/{:.2}ms gpu3d={:.2}ms ui={:.2}ms wait={:.2}ms phys={}x{} sf={:.2} vp={}x{}",
                                    s.present_frames_total, total_ms, first_ms, steady_avg, 1000.0 / steady_avg.max(0.01), pmin, p50, p95, pmax, avg_gpu_ms, avg_ui_ms, avg_wait_ms, phys.width, phys.height, sf, vp_c.get().0, vp_c.get().1
                                ));
                            }
                        } else {
                            keep_redrawing = s.needs_continuous_redraw();
                            if !keep_redrawing && s.has_pending_visible_thumbs() {
                                if s.last_interaction_at.elapsed().as_millis() >= 250 {
                                    keep_redrawing = true;
                                } else {
                                    schedule_thumb_wake = true;
                                }
                            }
                        }
                    }
                    if any_bench {
                        b_after.set(Instant::now());
                    }
                    if keep_redrawing {
                        request_coalesced_redraw(&win, &rq_notifier);
                    } else if schedule_thumb_wake {
                        wake_thumb();
                    }
                    if let Some(report) = bench_done {
                        let exports_dir = detect_workspace_root().join("exports");
                        let _ = std::fs::create_dir_all(&exports_dir);
                        let _ = std::fs::write(exports_dir.join("bench_gui.txt"), &report);
                        println!("{report}");
                        let _ = win.hide();
                        let _ = slint::quit_event_loop();
                    }
                }
                _ => {}
            }
        });
    }

    // Window chrome handlers
    {
        let w_weak = win.as_weak();
        win.on_start_drag(move || {
            if let Some(w) = w_weak.upgrade() {
                w.window().with_winit_window(|ww| {
                    let _ = ww.drag_window();
                });
            }
        });
    }
    {
        let w_weak = win.as_weak();
        win.on_win_minimize(move || {
            if let Some(w) = w_weak.upgrade() {
                w.window().set_minimized(true);
            }
        });
    }
    {
        let w_weak = win.as_weak();
        win.on_win_maximize(move || {
            if let Some(w) = w_weak.upgrade() {
                let next = !w.window().is_maximized();
                w.window().set_maximized(next);
                w.set_is_maximized(next);
            }
        });
    }
    win.on_win_close({
        let w_weak = win.as_weak();
        move || {
            if let Some(w) = w_weak.upgrade() {
                let _ = w.hide();
            }
            let _ = slint::quit_event_loop();
        }
    });

    // Library Callbacks
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_tool(move |ti| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.tool_mode = match ti {
                    0 => ToolMode::Select,
                    1 => ToolMode::Rotate,
                    2 => ToolMode::Scale,
                    3 => ToolMode::Camera,
                    _ => ToolMode::Move,
                };
                s.dirty.main_viewport = true;
                s.dirty.status_pill = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_category(move |c| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.selected_category = c.max(0) as usize;
                s.rebuild_weapon_filter();
                s.dirty.catalog = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_search_weapons(move |q| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.weapon_search = q.to_string();
                s.rebuild_weapon_filter();
                s.dirty.catalog = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_weapon(move |idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if (idx as usize) < s.weapons.len() {
                    s.active_weapon_idx = idx as usize;
                    if !s.is_active_clip_compatible() {
                        s.playing = false;
                    }
                    s.dirty.main_viewport = true;
                    s.dirty.card_viewport = true;
                    s.dirty.gallery = true;
                    s.dirty.inspector = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_cycle_weapon(move |delta| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.filtered_indices.is_empty() {
                    let cur_pos = s
                        .filtered_indices
                        .iter()
                        .position(|&x| x == s.active_weapon_idx)
                        .unwrap_or(0) as i32;
                    let n = s.filtered_indices.len() as i32;
                    let next_pos = ((cur_pos + delta) % n + n) % n;
                    s.active_weapon_idx = s.filtered_indices[next_pos as usize];
                    if !s.is_active_clip_compatible() {
                        s.playing = false;
                    }
                    s.dirty.main_viewport = true;
                    s.dirty.card_viewport = true;
                    s.dirty.gallery = true;
                    s.dirty.inspector = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }

    // Viewport Resize & 3D Interaction (zero-VecModel-rebuild on move/orbit/pan/zoom!)
    {
        let vp_c = vp_size_cell.clone();
        win.on_viewport_resized(move |vw, vh| {
            vp_c.set((vw.max(200.0) as u32, vh.max(150.0) as u32));
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_vp_pointer_down(move |x, y, btn, mod_pan, mod_ctrl, vw, vh| {
            let Some(w) = w_weak.upgrade() else { return };
            let mut s = st.borrow_mut();
            let mut active_gizmo = GizmoAxis::None;
            if btn == 0 && !mod_pan && !mod_ctrl && vw > 1.0 && vh > 1.0 {
                let (pivot, axes, vp) = s.active_gizmo_geom();
                let (cam_r, _, _) =
                    camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
                let gizmo_shown =
                    s.show_gizmo && matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move);
                if gizmo_shown {
                    active_gizmo =
                        pick_gizmo_handle(x, y, vp, vw, vh, pivot, axes, cam_r, 0.42, 0.425);
                }
            }
            s.drag = DragSession {
                active: true,
                button: btn,
                start_x: x,
                start_y: y,
                last_x: x,
                last_y: y,
                undo_pushed: false,
                active_gizmo,
            };
            sync_dirty_ui(&mut s, &w);
            w.window().request_redraw();
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_vp_pointer_hover(move |x, y, vw, vh| {
            if vw <= 1.0 || vh <= 1.0 {
                return;
            }
            let Some(w) = w_weak.upgrade() else { return };
            let mut s = st.borrow_mut();
            if s.drag.active
                || !matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move)
            {
                if s.hovered_gizmo != GizmoAxis::None {
                    s.hovered_gizmo = GizmoAxis::None;
                    s.dirty.main_viewport = true;
                    s.dirty.status_pill = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
                return;
            }
            let (pivot, axes, vp) = s.active_gizmo_geom();
            let (cam_r, _, _) = camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
            let next_hl = pick_gizmo_handle(x, y, vp, vw, vh, pivot, axes, cam_r, 0.42, 0.425);
            if next_hl != s.hovered_gizmo {
                s.hovered_gizmo = next_hl;
                s.dirty.main_viewport = true;
                s.dirty.status_pill = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        let rq = redraw_queued.clone();
        win.on_vp_pointer_move(move |x, y, mod_pan, mod_ctrl, vw, vh| {
            let Some(w) = w_weak.upgrade() else { return };
            let mut s = st.borrow_mut();
            if !s.drag.active {
                return;
            }
            let prev_x = s.drag.last_x;
            let prev_y = s.drag.last_y;
            let dx = x - prev_x;
            let dy = y - prev_y;
            s.drag.last_x = x;
            s.drag.last_y = y;

            let is_bone_edit = s.drag.button == 0
                && s.drag.active_gizmo != GizmoAxis::None
                && s.show_gizmo
                && matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move)
                && !mod_pan
                && !mod_ctrl;

            if mod_ctrl {
                s.note_interaction();
                s.main_cam.dolly_drag(dy);
                s.dirty.main_viewport = true;
                request_coalesced_redraw(&w, &rq);
                return;
            }

            if is_bone_edit && ((x - s.drag.start_x).hypot(y - s.drag.start_y) > 2.0) {
                if !s.drag.undo_pushed {
                    s.push_undo();
                    s.drag.undo_pushed = true;
                }
                s.playing = false;
                let (pivot, axes, vp) = s.active_gizmo_geom();
                let (cam_r, cam_u, cam_f) =
                    camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
                let axis = s.drag.active_gizmo;
                let tmode = s.tool_mode;
                let ki = s.editable_kf_index();
                let b_idx = if s.solo_weapon {
                    5
                } else {
                    s.active_bone_idx.min(BONE_COUNT - 1)
                };
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                apply_gizmo_drag(
                    tr,
                    tmode,
                    axis,
                    axes,
                    cam_r,
                    cam_u,
                    cam_f,
                    vp,
                    vw.max(1.0),
                    vh.max(1.0),
                    pivot,
                    prev_x,
                    prev_y,
                    x,
                    y,
                );
                s.note_interaction();
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                sync_dirty_ui(&mut s, &w);
                request_coalesced_redraw(&w, &rq);
            } else if s.drag.button == 2
                || ((s.drag.button == 1 || s.drag.button == 0) && mod_pan)
            {
                s.note_interaction();
                s.main_cam.pan(dx, dy, 0.0018);
                s.dirty.main_viewport = true;
                request_coalesced_redraw(&w, &rq);
            } else {
                s.note_interaction();
                s.main_cam.orbit(dx, dy, 0.009, 0.008, -1.25, 1.48);
                s.dirty.main_viewport = true;
                request_coalesced_redraw(&w, &rq);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_vp_pointer_up(move |x, y, _shift, vw, vh| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let dist = (x - s.drag.start_x).hypot(y - s.drag.start_y);
                let btn = s.drag.button;
                let edited_bone = s.drag.undo_pushed;
                s.drag.active = false;
                s.drag.active_gizmo = GizmoAxis::None;
                if dist < 4.5 && btn == 0 && vw > 1.0 && vh > 1.0 {
                    let nx = (x / vw) * 2.0 - 1.0;
                    let ny = 1.0 - (y / vh) * 2.0;
                    let (_vp, inv_vp, _) = s.main_cam.current.vp_and_inv(vw / vh);
                    let pose = sample_clip_pose(s.active_clip(), s.current_time);
                    let mats = evaluate_rig_matrices(&pose);
                    if let Some(picked) = pick_bone_ray(nx, ny, inv_vp, &mats, s.solo_weapon) {
                        if picked != s.active_bone_idx {
                            s.active_bone_idx = picked;
                            s.dirty.inspector = true;
                            s.dirty.timeline_tracks = true;
                            s.dirty.timeline_diamonds = true;
                            s.dirty.status_pill = true;
                        }
                    }
                }
                if edited_bone {
                    s.mark_active_clip_modified();
                    s.dirty.timeline_diamonds = true;
                }
                s.dirty.main_viewport = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        let rq = redraw_queued.clone();
        win.on_vp_wheel(move |mx, my, vw, vh, dy| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.queue_zoom_at_cursor(mx, my, vw, vh, dy);
                request_coalesced_redraw(&w, &rq);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_focus_bone(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.focus_active_bone();
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_camera(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.main_cam.reset_main_smooth();
                s.dirty.main_viewport = true;
                s.dirty.triad = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_camera_preset(move |p| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.main_cam.set_main_preset(p);
                s.dirty.main_viewport = true;
                s.dirty.triad = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_solo_weapon(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.solo_weapon = !s.solo_weapon;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }

    // Playback & Timeline Controls
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_play(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.playing && !s.is_active_clip_compatible() {
                    let msg = format!(
                        "Bloqueada para {} ({}) · Permitida: {}",
                        s.active_weapon().name,
                        s.active_weapon().category.label(),
                        s.active_clip().target_summary(&s.weapons)
                    );
                    show_toast(&mut s, &w, msg);
                    return;
                }
                s.playing = !s.playing;
                if s.playing && s.current_time >= s.active_clip().duration - 0.005 {
                    s.current_time = 0.0;
                    s.hit_stop_triggered = false;
                }
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                if s.playing {
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_jump_edge(move |edge| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                s.current_time = if edge == 0 { 0.0 } else { s.active_clip().duration };
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_meta = true;
                s.update_exact_kf_dirty();
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_cycle_speed(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.speed = if (s.speed - 1.0).abs() < 0.05 {
                    0.5
                } else if (s.speed - 0.5).abs() < 0.05 {
                    0.25
                } else {
                    1.0
                };
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_scrub_timeline(move |frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                s.current_time = (frac.clamp(0.0, 1.0)) * s.active_clip().duration;
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_meta = true;
                s.update_exact_kf_dirty();
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_step_frame(move |delta| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let dur = s.active_clip().duration.max(0.05);
                let frames = (dur * 60.0).round().max(1.0);
                let cur_f = ((s.current_time / dur) * frames).round() as i32;
                let next_f = (cur_f + delta).clamp(0, frames as i32);
                s.current_time = (next_f as f32 / frames) * dur;
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_meta = true;
                s.update_exact_kf_dirty();
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_jump_kf(move |dir| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let target_t = if dir < 0 {
                    prev_keyframe_time(s.active_clip(), s.current_time)
                } else {
                    next_keyframe_time(s.active_clip(), s.current_time)
                };
                if let Some(nt) = target_t {
                    s.current_time = nt;
                    s.dirty.main_viewport = true;
                    s.dirty.transform_readout = true;
                    s.dirty.timeline_meta = true;
                    s.update_exact_kf_dirty();
                    sync_dirty_ui(&mut s, &w);
                    request_immediate_redraw(&w);
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_click_diamond(move |kf_idx, track_idx, shift| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                s.drag_kf_undo_pending = true;
                let visible_bones: &[usize] = if s.show_all_bones {
                    &[0, 1, 2, 3, 4, 5, 6, 7]
                } else {
                    &[3, 4, 5, 1]
                };
                let prev_time = s.current_time;
                let prev_bone = s.active_bone_idx;
                let bone_idx = visible_bones
                    .get(track_idx.max(0) as usize)
                    .copied()
                    .unwrap_or(s.active_bone_idx);
                s.active_bone_idx = bone_idx;
                if let Some(kf) = s.active_clip().keyframes.get(kf_idx as usize) {
                    let kf_time = kf.time;
                    if shift {
                        let anchor = if s.selected_range_bone == Some(bone_idx) {
                            s.selected_range_times.map(|(a, _)| a).unwrap_or(prev_time)
                        } else if prev_bone == bone_idx {
                            prev_time
                        } else {
                            kf_time
                        };
                        s.selected_range_bone = Some(bone_idx);
                        s.selected_range_times = Some((anchor, kf_time));
                    } else {
                        s.clear_bone_selection();
                    }
                    s.current_time = kf_time;
                }
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_diamonds = true;
                s.dirty.status_pill = true;
                s.update_exact_kf_dirty();
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_drag_diamond(move |kf_idx, new_frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                let ki = kf_idx as usize;
                let n = s.active_clip().keyframes.len();
                if ki >= n || n < 2 {
                    return;
                }
                if s.drag_kf_undo_pending {
                    s.push_undo();
                    s.drag_kf_undo_pending = false;
                }
                s.playing = false;
                let dur = s.active_clip().duration.max(0.05);
                let min_t = if ki == 0 { 0.0 } else { s.active_clip().keyframes[ki - 1].time + 0.017 };
                let max_t = if ki + 1 == n { dur } else { s.active_clip().keyframes[ki + 1].time - 0.017 };
                let snapped = ((new_frac.clamp(0.0, 1.0) * dur) * 60.0).round() / 60.0;
                let final_t = snapped.clamp(min_t.min(max_t), max_t.max(min_t));
                let old_t = s.active_clip().keyframes[ki].time;
                let clip = s.active_clip_mut();
                clip.keyframes[ki].time = final_t;
                if let Some(imp) = clip.impact_time {
                    if (imp - old_t).abs() < 0.02 {
                        clip.impact_time = Some(final_t);
                    }
                }
                s.current_time = final_t;
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_dup_kf_next(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if s.timeline_bone_scope {
                    let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                    let rounded = s.duplicate_bone_frame_next(b_idx);
                    sync_dirty_ui(&mut s, &w);
                    show_toast(
                        &mut s,
                        &w,
                        format!("{} duplicado en {rounded:.2}s", BONE_LABELS_ES[b_idx]),
                    );
                    w.window().request_redraw();
                    return;
                }
                s.push_undo();
                let dur = s.active_clip().duration.max(0.1);
                let pose = sample_clip_pose(s.active_clip(), s.current_time);
                let target_t = (s.current_time + 0.10).min(dur);
                let rounded = (target_t * 60.0).round() / 60.0;
                let clip = s.active_clip_mut();
                if let Some(ex) = clip.keyframes.iter_mut().find(|k| (k.time - rounded).abs() < 0.016) {
                    ex.bones = pose;
                    ex.bone_mask = crate::model::ALL_BONES_MASK;
                } else {
                    clip.keyframes.push(Keyframe {
                        time: rounded,
                        easing: EasingMode::EaseOut,
                        bones: pose,
                        bone_mask: crate::model::ALL_BONES_MASK,
                    });
                    clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
                }
                s.current_time = rounded;
                s.clear_bone_selection();
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, format!("Pose duplicada en {rounded:.2}s"));
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_auto_kf(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.auto_keyframe = v;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                show_toast(
                    &mut s,
                    &w,
                    if v {
                        "Auto-KF activo".into()
                    } else {
                        "Auto-KF manual (apretá K para guardar)".into()
                    },
                );
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_all_bones(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.show_all_bones = !s.show_all_bones;
                s.dirty.inspector = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_bone_visible(move |bi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let idx = (bi as usize).min(BONE_COUNT - 1);
                s.hidden_bones[idx] = !s.hidden_bones[idx];
                s.dirty.main_viewport = true;
                s.dirty.timeline_tracks = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_show_all_bones_reset(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.hidden_bones = [false; BONE_COUNT];
                s.dirty.main_viewport = true;
                s.dirty.timeline_tracks = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_add_keyframe_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                if s.timeline_bone_scope {
                    let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                    let cur_t = s.current_time;
                    s.ensure_bone_kf_at(cur_t, b_idx, EasingMode::EaseOut);
                    s.mark_active_clip_modified();
                    sync_dirty_ui(&mut s, &w);
                    let msg = format!(
                        "{} guardado en {:.2}s",
                        BONE_LABELS_ES[b_idx], s.current_time
                    );
                    show_toast(&mut s, &w, msg);
                } else {
                    s.ensure_kf_at_current();
                    s.mark_active_clip_modified();
                    s.dirty.inspector = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    let msg = format!("Keyframe completo en {:.2}s", s.current_time);
                    show_toast(&mut s, &w, msg);
                }
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_delete_keyframe_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if s.timeline_bone_scope {
                    let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                    let removed = s.delete_bone_frames(b_idx, false);
                    if removed > 0 {
                        sync_dirty_ui(&mut s, &w);
                        let msg = if removed == 1 {
                            format!("Frame borrado en {}", BONE_LABELS_ES[b_idx])
                        } else {
                            format!("{removed} frames borrados en {}", BONE_LABELS_ES[b_idx])
                        };
                        show_toast(&mut s, &w, msg);
                        w.window().request_redraw();
                    } else {
                        show_toast(
                            &mut s,
                            &w,
                            format!("No hay keyframe de {} aquí", BONE_LABELS_ES[b_idx]),
                        );
                    }
                    return;
                }
                let t = s.current_time;
                if s.active_clip().keyframes.len() <= 2 {
                    show_toast(&mut s, &w, "Se necesitan al menos 2 keyframes".into());
                    return;
                }
                if let Some(idx) = s.active_clip().keyframes.iter().position(|k| (k.time - t).abs() < 0.025) {
                    s.push_undo();
                    s.active_clip_mut().keyframes.remove(idx);
                    s.clear_bone_selection();
                    s.mark_active_clip_modified();
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, "Frame completo borrado".into());
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_delete_full_keyframe_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let t = s.current_time;
                if s.active_clip().keyframes.len() <= 2 {
                    show_toast(&mut s, &w, "Se necesitan al menos 2 keyframes".into());
                    return;
                }
                if let Some(idx) = s.active_clip().keyframes.iter().position(|k| (k.time - t).abs() < 0.025) {
                    s.push_undo();
                    s.active_clip_mut().keyframes.remove(idx);
                    s.clear_bone_selection();
                    s.mark_active_clip_modified();
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, "Frame completo (8h) borrado".into());
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_timeline_bone_scope(move |is_bone| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.timeline_bone_scope = is_bone;
                if !is_bone {
                    s.clear_bone_selection();
                }
                s.dirty.timeline_meta = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_copy_bone_action(move |full_track| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let count = s.copy_bone_frames(b_idx, full_track);
                s.dirty.timeline_meta = true;
                sync_dirty_ui(&mut s, &w);
                let msg = if full_track {
                    format!("Pista de {} copiada ({count}f)", BONE_LABELS_ES[b_idx])
                } else if count > 1 {
                    format!("{count} frames de {} copiados", BONE_LABELS_ES[b_idx])
                } else {
                    format!("Frame de {} copiado", BONE_LABELS_ES[b_idx])
                };
                show_toast(&mut s, &w, msg);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_paste_bone_action(move |mirror_lr| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let count = s.paste_bone_frames(b_idx, mirror_lr);
                if count == 0 {
                    show_toast(&mut s, &w, "Primero copiá frame(s) de un hueso".into());
                    return;
                }
                sync_dirty_ui(&mut s, &w);
                let msg = if mirror_lr {
                    format!("{count}f pegados espejados en {}", BONE_LABELS_ES[b_idx])
                } else if count > 1 {
                    format!("{count}f pegados en {}", BONE_LABELS_ES[b_idx])
                } else {
                    format!("Frame pegado en {}", BONE_LABELS_ES[b_idx])
                };
                show_toast(&mut s, &w, msg);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_delete_bone_action(move |full_track| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let removed = s.delete_bone_frames(b_idx, full_track);
                if removed == 0 {
                    show_toast(
                        &mut s,
                        &w,
                        format!("No hay keyframes de {} para borrar", BONE_LABELS_ES[b_idx]),
                    );
                    return;
                }
                sync_dirty_ui(&mut s, &w);
                let msg = if full_track {
                    format!("Pista de {} limpiada ({removed}f)", BONE_LABELS_ES[b_idx])
                } else if removed > 1 {
                    format!("{removed} frames borrados en {}", BONE_LABELS_ES[b_idx])
                } else {
                    format!("Frame borrado en {}", BONE_LABELS_ES[b_idx])
                };
                show_toast(&mut s, &w, msg);
                w.window().request_redraw();
            }
        });
    }
    {
        let w_weak = win.as_weak();
        win.on_copy_timeline_action(move || {
            if let Some(w) = w_weak.upgrade() {
                if w.get_timeline_bone_scope() {
                    w.invoke_copy_bone_action(false);
                } else {
                    w.invoke_copy_pose_action();
                }
            }
        });
    }
    {
        let w_weak = win.as_weak();
        win.on_paste_timeline_action(move || {
            if let Some(w) = w_weak.upgrade() {
                if w.get_timeline_bone_scope() {
                    w.invoke_paste_bone_action(false);
                } else {
                    w.invoke_paste_pose_action();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_mark_impact_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let t = s.current_time.clamp(0.02, (s.active_clip().duration - 0.02).max(0.03));
                s.active_clip_mut().impact_time = Some(t);
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, format!("Impacto marcado en {t:.2}s"));
            }
        });
    }

    // Inspector Bone Transform & Options
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_step_bone_axis(move |chan, axis, delta| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                s.playing = false;
                let ki = s.editable_kf_index();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = (tr.pos[ax] + delta * 0.25).clamp(-24.0, 24.0),
                    1 => tr.rot[ax] = (tr.rot[ax] + delta * 3.0).clamp(-180.0, 180.0),
                    _ => tr.scale[ax] = (tr.scale[ax] + delta * 0.05).clamp(0.1, 3.0),
                }
                s.note_interaction();
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        win.on_scrub_axis_start(move || {
            st.borrow_mut().push_undo();
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_scrub_bone_axis(move |chan, axis, px_dx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let ki = s.editable_kf_index();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = (tr.pos[ax] + px_dx * 0.06).clamp(-24.0, 24.0),
                    1 => tr.rot[ax] = (tr.rot[ax] + px_dx * 0.75).clamp(-180.0, 180.0),
                    _ => tr.scale[ax] = (tr.scale[ax] + px_dx * 0.01).clamp(0.1, 3.0),
                }
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_bone_axis(move |chan, axis| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.push_undo();
                let ki = s.editable_kf_index();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = 0.0,
                    1 => tr.rot[ax] = 0.0,
                    _ => tr.scale[ax] = 1.0,
                }
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                request_immediate_redraw(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_bone_idx(move |bi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.active_bone_idx = (bi as usize).min(BONE_COUNT - 1);
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_diamonds = true;
                s.dirty.status_pill = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_loop(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                s.active_clip_mut().looped = v;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_change_duration(move |d| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let next_d = d.clamp(0.12, 3.0);
                s.push_undo();
                let old_d = s.active_clip().duration.max(0.05);
                let ratio = next_d / old_d;
                let clip = s.active_clip_mut();
                clip.duration = next_d;
                for k in &mut clip.keyframes {
                    k.time = ((k.time * ratio) * 1000.0).round() / 1000.0;
                }
                if let Some(imp) = clip.impact_time {
                    clip.impact_time = Some(((imp * ratio) * 1000.0).round() / 1000.0);
                }
                s.current_time = s.current_time.min(next_d);
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_easing(move |ei| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let ki = s.ensure_kf_at_current();
                s.active_clip_mut().keyframes[ki].easing = EasingMode::from_index(ei);
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_shadows(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.shadows = v;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_onion(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.onion = v;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_trail(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.trail = v;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_bg_mode(move |bi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.bg_mode = bi;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }

    // Undo / Redo / Pose Actions
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_undo_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(prev) = s.undo_stack.pop() {
                    let cur = s.active_clip().clone();
                    s.redo_stack.push(cur);
                    *s.active_clip_mut() = prev;
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_redo_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(next) = s.redo_stack.pop() {
                    let cur = s.active_clip().clone();
                    s.undo_stack.push(cur);
                    *s.active_clip_mut() = next;
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_pose_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let ki = s.ensure_kf_at_current();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                *s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx) = Default::default();
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_copy_pose_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.clipboard_pose = Some(sample_clip_pose(s.active_clip(), s.current_time));
                show_toast(&mut s, &w, "Pose copiada".into());
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_paste_pose_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let Some(cp) = s.clipboard_pose else {
                    show_toast(&mut s, &w, "Primero copiá una pose".into());
                    return;
                };
                s.push_undo();
                let ki = s.ensure_kf_at_current();
                s.active_clip_mut().keyframes[ki].bones = cp;
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_mirror_pose_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let ki = s.ensure_kf_at_current();
                let mirrored = mirror_pose(&s.active_clip().keyframes[ki].bones);
                s.active_clip_mut().keyframes[ki].bones = mirrored;
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }

    // Gallery Controls
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_filter_gallery(move |fi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_filter = fi;
                s.dirty.gallery = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_search_gallery(move |q| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_search = q.to_string();
                s.dirty.gallery = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_gallery_item(move |cid| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(pos) = s.project.clips.iter().position(|c| c.id == cid.as_str()) {
                    s.active_clip_idx = pos;
                    s.current_time = 0.0;
                    s.playing = false;
                    s.last_exact_kf = None;
                    s.clear_bone_selection();
                    s.dirty.main_viewport = true;
                    s.dirty.gallery = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_tracks = true;
                    s.dirty.timeline_diamonds = true;
                    s.dirty.status_pill = true;
                    sync_dirty_ui(&mut s, &w);
                    if !s.is_active_clip_compatible() {
                        let msg = format!(
                            "Bloqueada para {} · Permitida: {}",
                            s.active_weapon().name,
                            s.active_clip().target_summary(&s.weapons)
                        );
                        show_toast(&mut s, &w, msg);
                    }
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_add_gallery_clip(move |cid| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(src) = s.project.clips.iter().find(|c| c.id == cid.as_str()).cloned() {
                    let next_num = s.project.clips.len() + 1;
                    let mut copy: ClipDef = src;
                    copy.id = format!("{}_v{}", copy.id, next_num);
                    copy.name = format!("{} #{}", copy.name, next_num);
                    copy.bc_targets.clear();
                    s.project.clips.push(copy);
                    s.active_clip_idx = s.project.clips.len() - 1;
                    s.current_time = 0.0;
                    s.clear_bone_selection();
                    s.gallery_thumbs.clear();
                    s.dirty.main_viewport = true;
                    s.dirty.gallery = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_tracks = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, "Variación agregada al proyecto".into());
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_create_blank_clip(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let seq = s.project.clips.len() + 1;
                let mut clip = crate::model::create_blank_clip(seq);
                let active_cat = s.active_weapon().category;
                if !matches!(active_cat, WeaponCategory::Todas | WeaponCategory::Otros) {
                    clip.allowed_categories = vec![active_cat];
                }
                let cname = clip.name.clone();
                s.project.clips.push(clip);
                s.active_clip_idx = s.project.clips.len() - 1;
                s.current_time = 0.0;
                s.playing = false;
                s.clear_bone_selection();
                s.gallery_thumbs.clear();
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.gallery = true;
                s.dirty.inspector = true;
                s.dirty.timeline_meta = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, format!("{cname} listo para animar frame a frame"));
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_blank_scene(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.reset_to_blank_scene();
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, "Escena en blanco lista (modo Blender)".into());
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_load_demo_presets(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let added = s.load_demo_presets();
                sync_dirty_ui(&mut s, &w);
                if added > 0 {
                    show_toast(
                        &mut s,
                        &w,
                        format!("{added} plantillas demo agregadas a la galería"),
                    );
                } else {
                    show_toast(
                        &mut s,
                        &w,
                        "Todas las plantillas demo ya están en el proyecto".into(),
                    );
                }
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_import_custom_weapons(move || {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Importar sprites .png o paquetes .jar / .zip")
                .add_filter("Sprites y Paquetes", &["png", "jar", "zip"])
                .pick_files()
            {
                if let Some(w) = w_weak.upgrade() {
                    let mut s = st.borrow_mut();
                    let count = s.import_weapon_paths(&paths);
                    sync_dirty_ui(&mut s, &w);
                    if count > 0 {
                        show_toast(&mut s, &w, format!("{count} ítem(s) voxelizados e importados"));
                    } else {
                        show_toast(
                            &mut s,
                            &w,
                            "No se encontraron sprites válidos en los archivos".into(),
                        );
                    }
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_studio_key(move |tag, ctrl, shift, _alt| {
            let Some(w) = w_weak.upgrade() else { return };
            let low = tag.to_ascii_lowercase();
            match low.as_str() {
                "space" => w.invoke_toggle_play(),
                "left" if shift => w.invoke_jump_kf(-1),
                "right" if shift => w.invoke_jump_kf(1),
                "left" | "," => w.invoke_step_frame(-1),
                "right" | "." => w.invoke_step_frame(1),
                "[" => w.invoke_jump_kf(-1),
                "]" => w.invoke_jump_kf(1),
                "k" => w.invoke_add_keyframe_action(),
                "del" if shift => w.invoke_delete_full_keyframe_action(),
                "del" => w.invoke_delete_keyframe_action(),
                "m" => w.invoke_mirror_pose_action(),
                "f" => w.invoke_focus_bone(),
                "r" => w.invoke_reset_camera(),
                "z" if ctrl && !shift => w.invoke_undo_action(),
                "y" | "z" if ctrl => w.invoke_redo_action(),
                "c" if ctrl && shift => w.invoke_copy_pose_action(),
                "v" if ctrl && shift => w.invoke_paste_pose_action(),
                "c" if ctrl => w.invoke_copy_timeline_action(),
                "v" if ctrl => w.invoke_paste_timeline_action(),
                "1" => { w.set_active_tool(0); w.invoke_select_tool(0); }
                "2" => { w.set_active_tool(1); w.invoke_select_tool(1); }
                "3" => { w.set_active_tool(2); w.invoke_select_tool(2); }
                "4" => { w.set_active_tool(3); w.invoke_select_tool(3); }
                "5" => { w.set_active_tool(4); w.invoke_select_tool(4); }
                "w" => { st.borrow_mut().held_nav_keys[0] = true; w.window().request_redraw(); }
                "s" => { st.borrow_mut().held_nav_keys[1] = true; w.window().request_redraw(); }
                "a" => { st.borrow_mut().held_nav_keys[2] = true; w.window().request_redraw(); }
                "d" => { st.borrow_mut().held_nav_keys[3] = true; w.window().request_redraw(); }
                "e" => { st.borrow_mut().held_nav_keys[4] = true; w.window().request_redraw(); }
                "q" => { st.borrow_mut().held_nav_keys[5] = true; w.window().request_redraw(); }
                "=" | "+" => { st.borrow_mut().held_nav_keys[6] = true; w.window().request_redraw(); }
                "-" | "_" => { st.borrow_mut().held_nav_keys[7] = true; w.window().request_redraw(); }
                _ => {}
            }
        });
    }
    {
        let st = state.clone();
        win.on_studio_key_up(move |tag| {
            let mut s = st.borrow_mut();
            let low = tag.to_ascii_lowercase();
            match low.as_str() {
                "*" => s.held_nav_keys = [false; 8],
                "w" => s.held_nav_keys[0] = false,
                "s" => s.held_nav_keys[1] = false,
                "a" => s.held_nav_keys[2] = false,
                "d" => s.held_nav_keys[3] = false,
                "e" => s.held_nav_keys[4] = false,
                "q" => s.held_nav_keys[5] = false,
                "=" | "+" => s.held_nav_keys[6] = false,
                "-" | "_" => s.held_nav_keys[7] = false,
                _ => {}
            }
        });
    }

    // RELL Seas Curation & Timeline VFX Controls
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_filter_weapon_status(move |sf| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.weapon_status_filter = sf;
                s.rebuild_weapon_filter();
                s.dirty.catalog = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_weapon_status(move |widx, st_idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let status = CurationStatus::from_index(st_idx);
                s.set_weapon_curation_status(widx.max(0) as usize, status);
                s.flush_profile_if_dirty(true);
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, format!("Arma marcada como {}", status.label()));
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_export_curated_weapons_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.flush_profile_if_dirty(true);
                let Some(ref sr) = s.servernuevo_root else {
                    show_toast(&mut s, &w, "Sin ruta local servernuevo".into());
                    return;
                };
                match export_curated_weapons_final(&s.weapons, sr) {
                    Ok((_, cnt)) => {
                        show_toast(
                            &mut s,
                            &w,
                            format!("curated_weapons_final.json guardado ({cnt} armas aprobadas)"),
                        );
                    }
                    Err(e) => show_toast(&mut s, &w, e),
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_filter_gallery_source(move |sf| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_source_filter = sf;
                s.dirty.gallery = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_filter_gallery_status(move |sf| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_status_filter = sf;
                s.dirty.gallery = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_gallery_compat_only(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_compat_only = !s.gallery_compat_only;
                s.dirty.gallery = true;
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_equip_compatible_weapon(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(idx) = s.first_compatible_weapon_index() {
                    s.active_weapon_idx = idx;
                    s.selected_category = 0;
                    s.weapon_status_filter = 0;
                    s.weapon_search.clear();
                    s.rebuild_weapon_filter();
                    s.dirty.main_viewport = true;
                    s.dirty.card_viewport = true;
                    s.dirty.catalog = true;
                    s.dirty.gallery = true;
                    s.dirty.inspector = true;
                    sync_dirty_ui(&mut s, &w);
                    let name = s.active_weapon().name.clone();
                    show_toast(&mut s, &w, format!("Equipado: {name}"));
                    w.window().request_redraw();
                } else {
                    show_toast(&mut s, &w, "Sin ítem compatible en el catálogo actual".into());
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_allow_active_weapon_family(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let cat = s.active_weapon().category;
                if !s.active_clip().allowed_categories.contains(&cat) {
                    s.toggle_active_clip_family(cat);
                    s.flush_profile_if_dirty(true);
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, format!("Familia {} habilitada para este clip", cat.label()));
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_clip_family(move |idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(cat) = WeaponCategory::from_family_index(idx) {
                    s.toggle_active_clip_family(cat);
                    s.flush_profile_if_dirty(true);
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_clip_universal(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.set_active_clip_universal();
                s.flush_profile_if_dirty(true);
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, "Clip configurado como Universal".into());
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_clip_only_current_weapon(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.toggle_active_clip_only_current_weapon();
                s.flush_profile_if_dirty(true);
                sync_dirty_ui(&mut s, &w);
                let msg = format!("Permitido ahora: {}", s.active_clip().target_summary(&s.weapons));
                show_toast(&mut s, &w, msg);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_clip_status(move |cid, st_idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(pos) = s.project.clips.iter().position(|c| c.id == cid.as_str()) {
                    let status = CurationStatus::from_index(st_idx);
                    s.set_clip_curation_status(pos, status);
                    s.flush_profile_if_dirty(true);
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, format!("Clip marcado como {}", status.label()));
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_add_vfx_event(move |kind_i, col_i| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let t = s.current_time;
                let kind = VfxKind::from_index(kind_i);
                let color = match col_i {
                    1 => [1.0, 0.84, 0.30], // Dorado
                    2 => [0.22, 0.88, 1.0], // Cian Geppo
                    3 => [0.96, 0.34, 0.34], // Carmesi
                    _ => [0.58, 0.45, 1.0], // Violeta
                };
                let (dur, hint) = match kind {
                    VfxKind::SlashTrail => (0.16, String::new()),
                    VfxKind::ParrySpark => (0.22, "vfx_parry_star.png".to_string()),
                    VfxKind::SonarRing => (
                        0.28,
                        if col_i == 2 {
                            "anime_geppo_cyan.png".to_string()
                        } else {
                            "vfx_pixel_ring.png".to_string()
                        },
                    ),
                    VfxKind::HitStop => (0.07, String::new()),
                    VfxKind::ScreenShake => (0.16, String::new()),
                };
                s.active_clip_mut().vfx_events.push(VfxEvent {
                    time: (t * 100.0).round() / 100.0,
                    kind,
                    color,
                    scale: 1.15,
                    duration: dur,
                    intensity: 1.0,
                    texture_hint: hint,
                });
                s.active_clip_mut()
                    .vfx_events
                    .sort_by(|a, b| a.time.total_cmp(&b.time));
                s.mark_active_clip_modified();
                s.flush_profile_if_dirty(true);
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
                show_toast(&mut s, &w, format!("VFX '{}' añadido en {t:.2}s", kind.label()));
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_remove_vfx_event(move |idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let ui = idx as usize;
                if ui < s.active_clip().vfx_events.len() {
                    s.push_undo();
                    s.active_clip_mut().vfx_events.remove(ui);
                    s.mark_active_clip_modified();
                    s.flush_profile_if_dirty(true);
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_jump_to_vfx_event(move |idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if let Some(ev) = s.active_clip().vfx_events.get(idx as usize).cloned() {
                    s.playing = false;
                    s.current_time = ev.time;
                    if matches!(ev.kind, VfxKind::ParrySpark | VfxKind::HitStop) {
                        s.impact_flash = 0.85;
                    }
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    sync_dirty_ui(&mut s, &w);
                    w.window().request_redraw();
                }
            }
        });
    }

    // Export Actions
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_browse_export_dest(move || {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Seleccionar carpeta destino de animaciones")
                .pick_folder()
            {
                if let Some(w) = w_weak.upgrade() {
                    let disp = folder.to_string_lossy().to_string();
                    st.borrow_mut().custom_export_dest = Some(folder);
                    w.set_export_dest(disp.into());
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_export_project_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let fmt = ExportFormat::from_index(w.get_export_format_idx());
                let inc_ev = w.get_include_events();
                let opt = w.get_optimize_keyframes();
                let mut s = st.borrow_mut();
                match export_project(
                    &s.project,
                    &s.workspace_root,
                    s.custom_export_dest.as_deref(),
                    fmt,
                    inc_ev,
                    opt,
                ) {
                    Ok(res) => {
                        show_toast(
                            &mut s,
                            &w,
                            format!(
                                "Exportado ({} clips · {} eventos) · Listo para Minecraft F3+T",
                                res.clips_exported, res.event_timings
                            ),
                        );
                    }
                    Err(e) => show_toast(&mut s, &w, format!("Error exportando: {e}")),
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_export_strip_png_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                match export_storyboard_strip(&mut s) {
                    Ok(p) => {
                        let name = p.file_name().and_then(|x| x.to_str()).unwrap_or("strip.png");
                        show_toast(&mut s, &w, format!("Tira 6 frames guardada: exports/{name}"));
                    }
                    Err(e) => show_toast(&mut s, &w, format!("Error PNG: {e}")),
                }
            }
        });
    }

    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_export_rell_seas_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let inc_ev = w.get_include_events();
                let opt = w.get_optimize_keyframes();
                let mut s = st.borrow_mut();
                s.flush_profile_if_dirty(true);
                let Some(sr) = s.servernuevo_root.clone() else {
                    show_toast(&mut s, &w, "Sin carpeta local servernuevo conectada".into());
                    return;
                };
                match export_rell_seas_f3t(&s.project, &sr, inc_ev, opt) {
                    Ok(res) => {
                        show_toast(
                            &mut s,
                            &w,
                            format!(
                                "RELL Seas F3+T exportado ({} clips · {} BC overrides · {} VFX en {} rutas)",
                                res.clips_exported, res.bc_overrides, res.vfx_events, res.targets_written
                            ),
                        );
                    }
                    Err(e) => show_toast(&mut s, &w, format!("Export RELL Seas: {e}")),
                }
            }
        });
    }

    // Theme, Motion, GPU Power, Focus Mode & Command Palette (Ctrl+K)
    let apply_theme_idx = {
        let st = state.clone();
        let w_weak = win.as_weak();
        move |idx: i32| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.theme_idx = idx.clamp(0, 4);
                s.profile.theme_idx = s.theme_idx;
                s.profile_dirty = true;
                s.gallery_thumbs.clear();
                s.thumb_lru.clear();
                s.dirty.main_viewport = true;
                s.dirty.card_viewport = true;
                s.dirty.gallery = true;
                sync_theme_and_prefs(&s, &w);
                sync_dirty_ui(&mut s, &w);
                let label = format!("Estilo: {}", spec_for_theme(s.theme_idx).name);
                show_toast(&mut s, &w, label);
                w.window().request_redraw();
            }
        }
    };
    {
        let apply = apply_theme_idx.clone();
        win.on_select_theme(move |idx| apply(idx));
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_ui_motion(move |on| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.ui_motion_enabled = on;
                s.profile_dirty = true;
                sync_theme_and_prefs(&s, &w);
                wake_save();
                show_toast(
                    &mut s,
                    &w,
                    if on {
                        "Animaciones de interfaz activadas"
                    } else {
                        "Animaciones de interfaz en modo instantáneo (0ms)"
                    }
                    .into(),
                );
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_gpu_power_mode(move |low_power| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gpu_low_power = low_power;
                s.profile_dirty = true;
                sync_theme_and_prefs(&s, &w);
                wake_save();
                show_toast(
                    &mut s,
                    &w,
                    if low_power {
                        "Perfil Ahorro Batería (iGPU) guardado · Aplica al reiniciar"
                    } else {
                        "Perfil Alto Rendimiento (dGPU) guardado · Aplica al reiniciar"
                    }
                    .into(),
                );
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_focus_mode(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.focus_mode = !s.focus_mode;
                w.set_focus_mode(s.focus_mode);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_open_palette(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.palette_open = true;
                s.palette_query.clear();
                let items = build_palette_entries(&s);
                w.set_palette_query("".into());
                w.set_palette_items(ModelRc::new(VecModel::from(items)));
                w.set_palette_open(true);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_close_palette(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.palette_open = false;
                w.set_palette_open(false);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_search_palette(move |q| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.palette_query = q.to_string();
                let items = build_palette_entries(&s);
                w.set_palette_items(ModelRc::new(VecModel::from(items)));
            }
        });
    }
    let run_palette_id = {
        let st = state.clone();
        let w_weak = win.as_weak();
        move |raw_id: String| {
            let Some(w) = w_weak.upgrade() else { return };
            {
                let mut s = st.borrow_mut();
                s.palette_open = false;
                w.set_palette_open(false);
                match raw_id.as_str() {
                    "focus" => {
                        s.focus_mode = !s.focus_mode;
                        w.set_focus_mode(s.focus_mode);
                    }
                    "compat" => {
                        s.gallery_compat_only = !s.gallery_compat_only;
                        s.dirty.gallery = true;
                        sync_dirty_ui(&mut s, &w);
                    }
                    "cam:reset" => {
                        s.main_cam.reset_main_smooth();
                        s.dirty.main_viewport = true;
                        s.dirty.triad = true;
                        sync_dirty_ui(&mut s, &w);
                        w.window().request_redraw();
                    }
                    "theme:0" => {
                        drop(s);
                        apply_theme_idx(0);
                        return;
                    }
                    "theme:1" => {
                        drop(s);
                        apply_theme_idx(1);
                        return;
                    }
                    "theme:2" => {
                        drop(s);
                        apply_theme_idx(2);
                        return;
                    }
                    "theme:3" => {
                        drop(s);
                        apply_theme_idx(3);
                        return;
                    }
                    "theme:4" => {
                        drop(s);
                        apply_theme_idx(4);
                        return;
                    }
                    id if id.starts_with("clip:") => {
                        let cid = &id[5..];
                        if let Some(pos) = s.project.clips.iter().position(|c| c.id == cid) {
                            s.active_clip_idx = pos;
                            s.current_time = 0.0;
                            s.playing = false;
                            s.last_exact_kf = None;
                            s.clear_bone_selection();
                            s.dirty.main_viewport = true;
                            s.dirty.gallery = true;
                            s.dirty.inspector = true;
                            s.dirty.timeline_meta = true;
                            s.dirty.timeline_tracks = true;
                            s.dirty.timeline_diamonds = true;
                            s.dirty.status_pill = true;
                            sync_dirty_ui(&mut s, &w);
                            w.window().request_redraw();
                        }
                    }
                    id if id.starts_with("weapon:") => {
                        if let Ok(idx) = id[7..].parse::<usize>() {
                            if idx < s.weapons.len() {
                                s.active_weapon_idx = idx;
                                if !s.is_active_clip_compatible() {
                                    s.playing = false;
                                }
                                s.dirty.main_viewport = true;
                                s.dirty.card_viewport = true;
                                s.dirty.gallery = true;
                                s.dirty.inspector = true;
                                sync_dirty_ui(&mut s, &w);
                                w.window().request_redraw();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    };
    {
        let exec = run_palette_id.clone();
        win.on_exec_palette_item(move |id| exec(id.to_string()));
    }
    {
        let st = state.clone();
        let exec = run_palette_id;
        win.on_exec_first_palette_item(move || {
            let first = {
                let s = st.borrow();
                build_palette_entries(&s)
                    .into_iter()
                    .next()
                    .map(|p| p.id.to_string())
            };
            if let Some(id) = first {
                exec(id);
            }
        });
    }

    // Optional headless snapshot timer (ONLY runs when --capture-window is passed)
    let capture_timer = if let Some(cap_path) = auto_capture_path {
        let t = slint::Timer::default();
        let w_weak = win.as_weak();
        let mut frame_ticks = 0u32;
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(16), move || {
            let Some(w) = w_weak.upgrade() else { return };
            frame_ticks += 1;
            if frame_ticks == 16 {
                if let Ok(snap) = w.window().take_snapshot() {
                    let width = snap.width();
                    let height = snap.height();
                    if let Some(img) =
                        image::RgbaImage::from_raw(width, height, snap.as_bytes().to_vec())
                    {
                        let _ = img.save(&cap_path);
                    }
                }
                let _ = w.hide();
                let _ = slint::quit_event_loop();
            }
        });
        Some(t)
    } else {
        None
    };

    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_gizmo(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.show_gizmo = !s.show_gizmo;
                s.dirty.main_viewport = true;
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_rename_clip(move |name| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if s.rename_active_clip(name.as_str()) {
                    sync_dirty_ui(&mut s, &w);
                }
            }
        });
    }

    let _ = BONE_NAMES;
    StudioTimers {
        _toast: toast_timer,
        _save: save_timer,
        _thumb: thumb_timer,
        _capture: capture_timer,
    }
}
