// VoxelMotion Studio - Granular Slint Presenter & Event Wiring
// Keeps camera orbit/pan/zoom free of VecModel teardown and aligns simulation to BeforeRendering.

use crate::app_state::{bone_color, detect_workspace_root, export_storyboard_strip, AppState, DragSession};
use crate::design_spec::spec_for_theme;
use crate::exporter::{export_project, export_rell_seas_f3t, ExportFormat};
use crate::math3d::{
    apply_gizmo_drag, camera_basis, evaluate_rig_matrices, pick_bone_ray, pick_gizmo_handle,
    GizmoAxis,
};
use crate::model::{
    mirror_pose, next_keyframe_time, prev_keyframe_time, sample_clip_pose, ClipDef,
    CurationStatus, EasingMode, GalleryGroup, Keyframe, ToolMode, VfxEvent, VfxKind,
    WeaponCategory, BONE_COUNT, BONE_LABELS_ES, DISPLAY_BONE_ORDER,
};
use crate::renderer3d::GpuRenderer;
use crate::{
    CategoryEntry, FamilyChipRow, GalleryItem, GalleryRow, MainWindow, PaletteEntry, RulerTick,
    Theme, TimelineDiamond, TimelineTrack, VfxEventItem, WeaponListItem,
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
    win.set_auto_crouch_knees(state.auto_crouch_knees);
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
    let angle_suffix = if state.drag.active
        && state.tool_mode == ToolMode::Rotate
        && state.drag.drag_angle_deg.abs() > 0.05
    {
        format!(" ({:+.1}°)", state.drag.drag_angle_deg)
    } else {
        String::new()
    };
    let mod_suffix = if state.drag.active && state.drag.active_gizmo != GizmoAxis::None {
        if state.drag.fine_mode {
            " · Precisión"
        } else if state.drag.snap_mode {
            " · Snap"
        } else {
            ""
        }
    } else {
        ""
    };
    win.set_status_pill(
        format!(
            "{} · {}{}{}{}",
            BONE_LABELS_ES[state.active_bone_idx.min(BONE_COUNT - 1)],
            tool_lbl,
            axis_suffix,
            angle_suffix,
            mod_suffix
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
    let g_pairs: Vec<(GalleryGroup, GalleryItem)> = visible
        .into_iter()
        .map(|idx| &state.project.clips[idx])
        .map(|c| {
            (
                c.gallery_group,
                GalleryItem {
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
                },
            )
        })
        .collect();
    let g_items: Vec<GalleryItem> = g_pairs.iter().map(|(_, it)| it.clone()).collect();
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

    const GROUP_ORDER: [(i32, GalleryGroup, &str); 5] = [
        (0, GalleryGroup::Combate, "Ataques"),
        (1, GalleryGroup::Defensa, "Defensa"),
        (2, GalleryGroup::Movimiento, "Movimiento"),
        (3, GalleryGroup::Especiales, "Especiales"),
        (4, GalleryGroup::Interaccion, "Interacción"),
    ];
    let dummy = GalleryItem::default();
    let force_open = !state.gallery_search.trim().is_empty() || state.gallery_filter != 0;
    let mut rows: Vec<GalleryRow> = Vec::new();
    let mut cursor_y: f32 = 0.0;
    for &(g_idx, g_variant, g_title) in &GROUP_ORDER {
        let items_in_group: Vec<&GalleryItem> = g_pairs
            .iter()
            .filter_map(|(grp, item)| (*grp == g_variant).then_some(item))
            .collect();
        if items_in_group.is_empty() {
            continue;
        }
        let collapsed = !force_open && state.collapsed_gallery_groups[g_idx as usize];
        if cursor_y > 0.0 {
            cursor_y += 4.0;
        }
        rows.push(GalleryRow {
            is_header: true,
            y_px: cursor_y,
            h_px: 26.0,
            group_idx: g_idx,
            header_title: g_title.into(),
            group_count: items_in_group.len() as i32,
            collapsed,
            left: dummy.clone(),
            has_right: false,
            right: dummy.clone(),
        });
        cursor_y += 30.0;
        if !collapsed {
            for pair in items_in_group.chunks(2) {
                let left = pair[0].clone();
                let has_right = pair.len() > 1;
                let right = if has_right {
                    pair[1].clone()
                } else {
                    dummy.clone()
                };
                rows.push(GalleryRow {
                    is_header: false,
                    y_px: cursor_y,
                    h_px: 96.0,
                    group_idx: g_idx,
                    header_title: "".into(),
                    group_count: 0,
                    collapsed: false,
                    left,
                    has_right,
                    right,
                });
                cursor_y += 102.0;
            }
        }
    }
    win.set_gallery_rows(ModelRc::new(VecModel::from(rows)));
    win.set_gallery_content_h((cursor_y + 4.0).max(200.0));

    win.set_gallery_compat_only(state.gallery_compat_only);
    win.set_featured_gallery_id(clip.id.clone().into());
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
    win.set_active_weapon_status(w.status.index());
    win.set_solo_weapon(state.solo_weapon);

    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    win.set_active_clip_name(clip.name.clone().into());
    win.set_active_clip_status(clip.status.index());
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
    win.set_auto_crouch_knees(state.auto_crouch_knees);
    win.set_bg_idx(state.bg_mode);
    win.set_auto_keyframe(state.auto_keyframe);
    win.set_show_all_bones(state.show_all_bones);
    win.set_gizmo_world_space(state.gizmo_world_space);
    win.set_show_offhand_weapon(state.show_offhand_weapon);
    win.set_has_custom_skin(state.skin_pixels.is_some());
    let skin_lbl = if state.skin_pixels.is_some() && !state.skin_name.is_empty() {
        format!("Skin: {}", state.skin_name)
    } else {
        "Skin: Modelo base".to_string()
    };
    win.set_skin_label(skin_lbl.into());
    win.set_skin_slim(state.skin_slim);
    win.set_skin_outer_layer(state.skin_outer_layer);
    sync_transform_readout(state, win);
}

fn lane_w_logical(win: &MainWindow) -> f32 {
    let sf = win.window().scale_factor().max(1.0);
    let win_phys_w = win.window().size().width;
    let win_phys_wf = if win_phys_w > 100 {
        win_phys_w as f32
    } else {
        1440.0 * sf
    };
    let lane_phys_w = (win_phys_wf - 264.0 * sf).round().max(400.0);
    (lane_phys_w / sf).max(320.0)
}

#[inline]
fn frame_to_screen_frac(frame_pos: f32, scroll_frame: f32, visible_frames: f32) -> f32 {
    (frame_pos - scroll_frame) / visible_frames.max(1.0)
}

fn sync_timeline_meta(state: &AppState, win: &MainWindow) {
    let clip = state.active_clip();
    let dur = clip.duration.max(0.05);
    let frames = state.active_total_frames();
    let cur_f = ((state.current_time / dur) * (frames as f32)).round() as i32;
    let lane_w = lane_w_logical(win);
    let vis_f = state.timeline_visible_frames(lane_w);
    let max_scroll = state.max_scroll_frame(lane_w);
    let scroll_f = state.timeline_scroll_frame.clamp(0.0, max_scroll);
    let cur_frame_pos = (state.current_time / dur).clamp(0.0, 1.0) * (frames as f32);
    let playhead_screen = frame_to_screen_frac(cur_frame_pos, scroll_f, vis_f);
    let clip_end_screen = frame_to_screen_frac(frames as f32, scroll_f, vis_f);

    win.set_current_frame(cur_f.clamp(0, frames));
    win.set_total_frames(frames);
    win.set_timeline_fps(state.timeline_fps);
    win.set_playhead_frac(playhead_screen.clamp(0.0, 1.0));
    win.set_playhead_visible((-0.005..=1.005).contains(&playhead_screen));
    win.set_clip_end_frac(clip_end_screen.clamp(0.0, 1.2));
    win.set_timeline_zoom_label(format!("{:.1}x", state.timeline_zoom).into());
    win.set_timeline_zoomed((state.timeline_zoom - 1.0).abs() > 0.02);
    win.set_timeline_scrollable(max_scroll > 0.05 || scroll_f > 0.05);
    let bar_frames = (frames as f32).max(scroll_f + vis_f).max(1.0);
    win.set_timeline_view_start((scroll_f / bar_frames).clamp(0.0, 1.0));
    win.set_timeline_view_span((vis_f / bar_frames).clamp(0.05, 1.0));
    win.set_timeline_bone_scope(state.timeline_bone_scope);
    if let Some(ref cb) = state.clipboard_bone {
        win.set_bone_clipboard_count(cb.frames.len() as i32);
        let lbl = if cb.multi_bone {
            format!("Multi ({}k)", cb.frames.len())
        } else {
            format!(
                "{} ({}f)",
                BONE_LABELS_ES[cb.source_bone_idx.min(BONE_COUNT - 1)],
                cb.frames.len()
            )
        };
        win.set_bone_clipboard_label(lbl.into());
    } else {
        win.set_bone_clipboard_count(0);
        win.set_bone_clipboard_label("".into());
    }
    win.set_has_impact(clip.impact_time.is_some());
    if let Some(imp) = clip.impact_time {
        let imp_pos = (imp / dur).clamp(0.0, 1.0) * (frames as f32);
        let imp_screen = frame_to_screen_frac(imp_pos, scroll_f, vis_f);
        win.set_impact_frac(imp_screen.clamp(0.0, 1.0));
        win.set_impact_visible((-0.005..=1.005).contains(&imp_screen));
        win.set_impact_time_text(format!("{imp:.2}s").into());
    } else {
        win.set_impact_frac(0.0);
        win.set_impact_visible(false);
        win.set_impact_time_text("Sin marca".into());
    }
}

pub fn build_ruler_ticks_frames(
    total_frames: i32,
    scroll_frame: f32,
    visible_frames: f32,
    lane_width_px: f32,
) -> Vec<RulerTick> {
    let frames = total_frames.max(1);
    let vis_f = visible_frames.max(1.0);
    let scroll_f = scroll_frame.max(0.0);
    let px_per_frame = lane_width_px.max(320.0) / vis_f;

    let (label_step, major_step) = if px_per_frame >= 22.0 {
        (1, 5)
    } else if px_per_frame >= 12.0 {
        (2, 10)
    } else if px_per_frame >= 7.0 {
        (5, 10)
    } else {
        (10, 20)
    };
    let tick_step = if px_per_frame >= 5.0 { 1 } else { 2 };

    let first_f = (scroll_f.floor() as i32 - 1).max(0);
    let last_f = (scroll_f + vis_f).ceil() as i32 + 1;

    let mut ticks = Vec::with_capacity(((last_f - first_f) / tick_step + 4).max(2) as usize);
    let mut f = (first_f / tick_step) * tick_step;
    if f < 0 {
        f = 0;
    }
    let clip_end_visible = frame_to_screen_frac(frames as f32, scroll_f, vis_f) <= 1.005;
    while f <= last_f {
        let screen_frac = frame_to_screen_frac(f as f32, scroll_f, vis_f);
        if (-0.002..=1.002).contains(&screen_frac) {
            let in_clip = f <= frames;
            let is_end = f == frames;
            let dist_end_px = (frames - f).abs() as f32 * px_per_frame;
            let too_close_to_end =
                in_clip && !is_end && clip_end_visible && dist_end_px < 18.0;
            let is_major = f == 0 || is_end || (f % major_step == 0);
            let is_mid = !is_major && (f % (major_step / 2).max(1) == 0);
            let show_label = in_clip
                && ((is_major && !too_close_to_end)
                    || (!too_close_to_end && f % label_step == 0));
            ticks.push(RulerTick {
                frac: screen_frac.clamp(0.0, 1.0),
                label: if show_label {
                    format!("{f}").into()
                } else {
                    "".into()
                },
                major: is_major,
                mid: is_mid,
            });
        }
        f += tick_step;
    }
    let end_frac = frame_to_screen_frac(frames as f32, scroll_f, vis_f);
    if (-0.002..=1.002).contains(&end_frac)
        && ticks
            .iter()
            .all(|t| (t.frac - end_frac.clamp(0.0, 1.0)).abs() > 1e-3)
    {
        ticks.push(RulerTick {
            frac: end_frac.clamp(0.0, 1.0),
            label: format!("{frames}").into(),
            major: true,
            mid: false,
        });
        ticks.sort_by(|a, b| a.frac.total_cmp(&b.frac));
    }
    ticks
}

#[cfg(test)]
pub fn build_ruler_ticks_window(
    total_frames: i32,
    view_start: f32,
    view_span: f32,
    lane_width_px: f32,
) -> Vec<RulerTick> {
    let frames = total_frames.max(1) as f32;
    let span = view_span.clamp(0.05, 1.0);
    let start = view_start.clamp(0.0, (1.0 - span).max(0.0));
    build_ruler_ticks_frames(
        total_frames,
        start * frames,
        (span * frames).max(1.0),
        lane_width_px,
    )
}

#[cfg(test)]
pub fn build_ruler_ticks(total_frames: i32) -> Vec<RulerTick> {
    build_ruler_ticks_window(total_frames, 0.0, 1.0, 1160.0)
}

const DIGITS_5X7: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110], // 0
    [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110], // 1
    [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111], // 2
    [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110], // 3
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010], // 4
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110], // 5
    [0b01110, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b01110], // 6
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000], // 7
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110], // 8
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110], // 9
];

fn draw_number_5x7(
    buf: &mut [slint::Rgba8Pixel],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    scale: usize,
    text: &str,
    col: slint::Rgba8Pixel,
) {
    let mut cx = x0;
    for b in text.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            continue;
        }
        let glyph = &DIGITS_5X7[(b - b'0') as usize];
        for (row, &bits) in glyph.iter().enumerate() {
            for col_bit in 0..5 {
                if (bits & (1 << (4 - col_bit))) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let py = y0 + row * scale + sy;
                            let px = cx + col_bit * scale + sx;
                            if py < h && px < w {
                                buf[py * w + px] = col;
                            }
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
    }
}

fn build_ruler_grid_images(
    ticks: &[RulerTick],
    theme_idx: i32,
    lane_phys_w: u32,
    ruler_phys_h: u32,
) -> (slint::Image, slint::Image) {
    let w = lane_phys_w.clamp(320, 3840);
    let h_ruler = ruler_phys_h.clamp(22, 64);
    let spec = spec_for_theme(theme_idx);
    let (nr, ng, nb) = (spec.text_dim.red(), spec.text_dim.green(), spec.text_dim.blue());
    let (gr, gg, gb) = (
        spec.border_control.red(),
        spec.border_control.green(),
        spec.border_control.blue(),
    );
    let col_major = slint::Rgba8Pixel {
        r: spec.text_primary.red(),
        g: spec.text_primary.green(),
        b: spec.text_primary.blue(),
        a: 245,
    };
    let col_inter = slint::Rgba8Pixel {
        r: spec.text_secondary.red(),
        g: spec.text_secondary.green(),
        b: spec.text_secondary.blue(),
        a: 185,
    };

    let mut notches_buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h_ruler);
    let mut grid_buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, 1);
    let notches_px = notches_buf.make_mut_slice();
    let grid_px = grid_buf.make_mut_slice();
    let w_us = w as usize;
    let h_us = h_ruler as usize;
    let scale = if h_us >= 36 { 2usize } else { 1usize };
    let y_num = ((h_us.saturating_sub(7 * scale + (h_us * 40 / 100))) / 2).max(2);

    let notch_h_major = (h_us * 38 / 100).max(6);
    let notch_h_mid = (h_us * 25 / 100).max(4);
    let notch_h_minor = (h_us * 16 / 100).max(3);

    for tick in ticks {
        let px = ((tick.frac.clamp(0.0, 1.0)) * (w - 1) as f32).round() as usize;
        let (nh, n_alpha, g_alpha) = if tick.major {
            (notch_h_major, 224u8, 142u8)
        } else if tick.mid {
            (notch_h_mid, 154u8, 92u8)
        } else {
            (notch_h_minor, 90u8, 50u8)
        };
        let y_start = h_us.saturating_sub(nh);
        for y in y_start..h_us {
            let slot = &mut notches_px[y * w_us + px];
            if n_alpha > slot.a {
                *slot = slint::Rgba8Pixel { r: nr, g: ng, b: nb, a: n_alpha };
            }
        }
        let g_slot = &mut grid_px[px];
        if g_alpha > g_slot.a {
            *g_slot = slint::Rgba8Pixel { r: gr, g: gg, b: gb, a: g_alpha };
        }
    }

    let mut last_right = 0usize;
    let mut any_drawn = false;
    for tick in ticks {
        let lbl = tick.label.as_str();
        if lbl.is_empty() {
            continue;
        }
        let px = ((tick.frac.clamp(0.0, 1.0)) * (w - 1) as f32).round() as usize;
        let text_w = (lbl.len() * 6 * scale).saturating_sub(scale);
        let x0 = if tick.frac <= 0.001 {
            px + 2
        } else if tick.frac >= 0.96 {
            px.saturating_sub(text_w + 1)
        } else {
            px.saturating_sub(text_w / 2).min(w_us.saturating_sub(text_w + 1))
        };
        if any_drawn && x0 < last_right + 4 * scale {
            continue;
        }
        let col = if tick.major { col_major } else { col_inter };
        draw_number_5x7(notches_px, w_us, h_us, x0, y_num, scale, lbl, col);
        last_right = x0 + text_w;
        any_drawn = true;
    }

    (
        slint::Image::from_rgba8(notches_buf),
        slint::Image::from_rgba8(grid_buf),
    )
}

fn sync_timeline_tracks(state: &AppState, win: &MainWindow) {
    let frames = state.active_total_frames();
    let sf = win.window().scale_factor().max(1.0);
    let win_phys_w = win.window().size().width;
    let win_phys_wf = if win_phys_w > 100 {
        win_phys_w as f32
    } else {
        1440.0 * sf
    };
    let lane_phys_w = (win_phys_wf - 264.0 * sf).round().max(400.0) as u32;
    let ruler_phys_h = (24.0 * sf).round().max(24.0) as u32;
    let lane_w_logical = (lane_phys_w as f32 / sf).max(320.0);
    let vis_f = state.timeline_visible_frames(lane_w_logical);
    let scroll_f = state
        .timeline_scroll_frame
        .clamp(0.0, state.max_scroll_frame(lane_w_logical));

    let all_ticks = build_ruler_ticks_frames(frames, scroll_f, vis_f, lane_w_logical);
    let (notches_img, grid_img) =
        build_ruler_grid_images(&all_ticks, state.theme_idx, lane_phys_w, ruler_phys_h);
    win.set_ruler_notches_img(notches_img);
    win.set_lanes_grid_img(grid_img);

    let all_tracks: Vec<TimelineTrack> = DISPLAY_BONE_ORDER
        .iter()
        .map(|&bi| TimelineTrack {
            bone_idx: bi as i32,
            name: BONE_LABELS_ES[bi].into(),
            dot_color: bone_color(bi),
            active: state.active_bone_idx == bi,
            visible: !state.hidden_bones[bi],
        })
        .collect();
    win.set_all_bones_tracks(ModelRc::new(VecModel::from(all_tracks)));

    let visible_bones: &[usize] = if state.show_all_bones {
        &DISPLAY_BONE_ORDER
    } else {
        &[8, 3, 4, 5]
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
    let total_frames = state.active_total_frames() as f32;
    let lane_w = lane_w_logical(win);
    let vis_f = state.timeline_visible_frames(lane_w);
    let scroll_f = state
        .timeline_scroll_frame
        .clamp(0.0, state.max_scroll_frame(lane_w));
    let sel_keys = state.effective_selected_keys();
    let visible_bones: &[usize] = state.visible_timeline_bones();
    let mut diamonds = Vec::new();
    for (ki, kf) in clip.keyframes.iter().enumerate() {
        let kf_pos = (kf.time / dur).clamp(0.0, 1.0) * total_frames;
        let kf_frame = kf_pos.round() as i32;
        let screen_frac = frame_to_screen_frac(kf_pos, scroll_f, vis_f);
        if !(-0.025..=1.025).contains(&screen_frac) {
            continue;
        }
        let frac = screen_frac.clamp(0.0, 1.0);
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
                let is_sel = if !sel_keys.is_empty() {
                    sel_keys.contains(&(kf_frame, bi))
                } else {
                    (kf.time - state.current_time).abs() < 0.016 && state.active_bone_idx == bi
                };
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
    win.set_selected_keys_count(sel_keys.len() as i32);
    if let (Some(rb), Some((t0, t1))) = (state.selected_range_bone, state.selected_range_times) {
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let count = clip
            .keyframes
            .iter()
            .filter(|k| k.has_bone(rb) && k.time >= lo - 0.016 && k.time <= hi + 0.016)
            .count() as i32;
        let lo_v = frame_to_screen_frac((lo / dur).clamp(0.0, 1.0) * total_frames, scroll_f, vis_f);
        let hi_v = frame_to_screen_frac((hi / dur).clamp(0.0, 1.0) * total_frames, scroll_f, vis_f);
        let track_i = if hi_v < 0.0 || lo_v > 1.0 {
            -1
        } else {
            visible_bones
                .iter()
                .position(|&b| b == rb)
                .map(|i| i as i32)
                .unwrap_or(-1)
        };
        win.set_selected_range_count(count);
        win.set_selected_range_track_idx(track_i);
        win.set_selected_range_start_frac(lo_v.clamp(0.0, 1.0));
        win.set_selected_range_end_frac(hi_v.clamp(0.0, 1.0));
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
    let is_interactive_startup = !any_bench && auto_capture_path.is_none();

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
        let st = state.clone();
        let w_weak = win.as_weak();
        SAVE_WAKE.with(|c| {
            *c.borrow_mut() = Some(Rc::new(move || {
                let st2 = st.clone();
                let w2 = w_weak.clone();
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
                        if is_interactive_startup {
                            ww.set_minimized(false);
                            ww.set_visible(true);
                            if let Some(mon) = ww.current_monitor().or_else(|| ww.primary_monitor()) {
                                let sf = mon.scale_factor().max(0.5);
                                let mon_pos = mon.position();
                                let mon_sz = mon.size();
                                let mw = mon_sz.width as f64 / sf;
                                let mh = mon_sz.height as f64 / sf;
                                if mw <= 1560.0 || mh <= 920.0 {
                                    ww.set_maximized(true);
                                } else {
                                    let target_w = 1440.0_f64;
                                    let target_h = 890.0_f64;
                                    let _ = ww.request_inner_size(
                                        slint::winit_030::winit::dpi::LogicalSize::new(target_w, target_h),
                                    );
                                    let phys_w = (target_w * sf).round() as i32;
                                    let phys_h = (target_h * sf).round() as i32;
                                    let ox = mon_pos.x + ((mon_sz.width as i32 - phys_w) / 2).max(0);
                                    let oy = mon_pos.y + ((mon_sz.height as i32 - phys_h) / 2).max(0);
                                    ww.set_outer_position(
                                        slint::winit_030::winit::dpi::PhysicalPosition::new(ox, oy),
                                    );
                                }
                            }
                            ww.focus_window();
                        }
                    });
                    win.set_is_maximized(win.window().is_maximized());
                    let (imgs, low_power) = {
                        let Ok(mut s) = st.try_borrow_mut() else { return };
                        if s.gpu.is_none() {
                            s.gpu = Some(GpuRenderer::new(device.clone(), queue.clone()));
                            s.sync_character_mesh_gpu();
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
        win.on_vp_pointer_down(move |x, y, btn, shift, alt, ctrl, vw, vh| {
            let Some(w) = w_weak.upgrade() else { return };
            let mut s = st.borrow_mut();
            let mut active_gizmo = GizmoAxis::None;
            let mod_cam = shift || alt || ctrl;
            if btn == 0 && !mod_cam && vw > 1.0 && vh > 1.0 {
                let (pivot, axes, _, _, vp, inv_vp, eye) = s.active_gizmo_geom_for_size(vw, vh);
                let (cam_r, _, _) =
                    camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
                let gizmo_shown =
                    s.show_gizmo && matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move);
                if gizmo_shown {
                    active_gizmo =
                        pick_gizmo_handle(s.tool_mode, x, y, vp, inv_vp, vw, vh, pivot, axes, cam_r, eye);
                }
            }
            let b_idx = if s.solo_weapon { 5 } else { s.active_bone_idx.min(BONE_COUNT - 1) };
            let raw_tr = sample_clip_pose(s.active_clip(), s.current_time).0[b_idx];
            s.drag = DragSession {
                active: true,
                button: btn,
                start_x: x,
                start_y: y,
                last_x: x,
                last_y: y,
                undo_pushed: false,
                active_gizmo,
                drag_angle_deg: 0.0,
                smooth_x: x,
                smooth_y: y,
                raw_tr,
                fine_mode: shift,
                snap_mode: ctrl,
            };
            s.dirty.status_pill = true;
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
            let (pivot, axes, _, _, vp, inv_vp, eye) = s.active_gizmo_geom_for_size(vw, vh);
            let (cam_r, _, _) = camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
            let next_hl =
                pick_gizmo_handle(s.tool_mode, x, y, vp, inv_vp, vw, vh, pivot, axes, cam_r, eye);
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
        win.on_vp_pointer_move(move |x, y, shift, alt, ctrl, vw, vh| {
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
                && !alt;

            if !is_bone_edit && ctrl {
                s.note_interaction();
                s.main_cam.dolly_smooth(dy);
                s.dirty.main_viewport = true;
                request_coalesced_redraw(&w, &rq);
                return;
            }

            if is_bone_edit && ((x - s.drag.start_x).hypot(y - s.drag.start_y) > 2.0) {
                let axis = s.drag.active_gizmo;
                let tmode = s.tool_mode;
                let b_idx = if s.solo_weapon {
                    5
                } else {
                    s.active_bone_idx.min(BONE_COUNT - 1)
                };
                if !s.drag.undo_pushed {
                    s.push_undo();
                    s.drag.undo_pushed = true;
                    let cur_kf = s.editable_kf_index();
                    s.drag.raw_tr = *s.active_clip().keyframes[cur_kf].bones.bone(b_idx);
                }
                s.playing = false;
                s.drag.fine_mode = shift;
                s.drag.snap_mode = ctrl;

                // Adaptive low-pass filter on raw pointer input + Shift 0.25x micro-precision
                let alpha = if shift { 0.36 } else { 0.62 };
                let prev_sx = s.drag.smooth_x;
                let prev_sy = s.drag.smooth_y;
                let next_sx = prev_sx + (x - prev_sx) * alpha;
                let next_sy = prev_sy + (y - prev_sy) * alpha;
                s.drag.smooth_x = next_sx;
                s.drag.smooth_y = next_sy;
                let fine_mul = if shift { 0.25 } else { 1.0 };
                let eff_x = prev_sx + (next_sx - prev_sx) * fine_mul;
                let eff_y = prev_sy + (next_sy - prev_sy) * fine_mul;

                let (pivot, axes, parent_axes, _, vp, inv_vp, eye) =
                    s.active_gizmo_geom_for_size(vw, vh);
                let (cam_r, cam_u, cam_f) =
                    camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
                let delta_deg = apply_gizmo_drag(
                    &mut s.drag.raw_tr,
                    tmode,
                    axis,
                    axes,
                    parent_axes,
                    cam_r,
                    cam_u,
                    cam_f,
                    vp,
                    inv_vp,
                    eye,
                    vw.max(1.0),
                    vh.max(1.0),
                    pivot,
                    prev_sx,
                    prev_sy,
                    eff_x,
                    eff_y,
                );
                let mut applied = s.drag.raw_tr;
                if ctrl {
                    match tmode {
                        ToolMode::Rotate | ToolMode::Select => {
                            for v in &mut applied.rot {
                                *v = (*v / 15.0).round() * 15.0;
                            }
                        }
                        ToolMode::Move => {
                            for v in &mut applied.pos {
                                *v = v.round();
                            }
                        }
                        ToolMode::Scale => {
                            for v in &mut applied.scale {
                                *v = (*v * 10.0).round() / 10.0;
                            }
                        }
                        _ => {}
                    }
                }
                let ki = s.editable_kf_index();
                *s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx) = applied;
                s.sync_crouch_knees_if_enabled(ki, b_idx);
                if s.clamp_edited_bone_above_floor(ki, b_idx) {
                    s.drag.raw_tr = *s.active_clip().keyframes[ki].bones.bone(b_idx);
                }
                s.drag.drag_angle_deg += delta_deg;
                s.note_interaction();
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.status_pill = true;
                sync_dirty_ui(&mut s, &w);
                request_coalesced_redraw(&w, &rq);
            } else if s.drag.button == 2
                || ((s.drag.button == 1 || s.drag.button == 0) && (shift || alt))
            {
                s.note_interaction();
                s.main_cam.pan_smooth(dx, dy, 0.0018);
                s.dirty.main_viewport = true;
                request_coalesced_redraw(&w, &rq);
            } else {
                s.note_interaction();
                s.main_cam.orbit_smooth(dx, dy, 0.008, 0.0072, -1.25, 1.48);
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
        win.on_vp_wheel(move |mx, my, vw, vh, dx, dy, ctrl, shift, alt| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.queue_viewport_scroll(mx, my, vw, vh, dx, dy, ctrl, shift, alt);
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
        let rq = redraw_queued.clone();
        win.on_scrub_timeline(move |screen_frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let dur = s.active_clip().duration.max(0.05);
                let frames = s.active_total_frames() as f32;
                let lane_w = lane_w_logical(&w);
                let vis_f = s.timeline_visible_frames(lane_w);
                let frame_pos =
                    s.timeline_scroll_frame + screen_frac.clamp(0.0, 1.0) * vis_f;
                let f_idx = frame_pos.round().clamp(0.0, frames);
                s.current_time = (f_idx / frames.max(1.0)) * dur;
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_meta = true;
                s.update_exact_kf_dirty();
                sync_dirty_ui(&mut s, &w);
                request_coalesced_redraw(&w, &rq);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_zoom_timeline(move |delta, anchor_screen_frac, is_button| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let factor = if is_button {
                    1.28_f32.powf(delta.signum())
                } else {
                    (delta * 0.0022).clamp(-0.32, 0.32).exp()
                };
                let mut next_zoom = (s.timeline_zoom * factor).clamp(0.35, 4.0);
                if (next_zoom - 1.0).abs() < 0.03 {
                    next_zoom = 1.0;
                }
                let lane_w = lane_w_logical(&w);
                s.set_timeline_zoom_at(next_zoom, anchor_screen_frac, lane_w);
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_timeline_zoom(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let lane_w = lane_w_logical(&w);
                s.set_timeline_zoom_at(1.0, 0.0, lane_w);
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_pan_timeline(move |delta_screen_frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let lane_w = lane_w_logical(&w);
                s.pan_timeline_frames(delta_screen_frac, lane_w);
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_set_timeline_view_start(move |new_start| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let lane_w = lane_w_logical(&w);
                let total_frames = s.active_total_frames() as f32;
                let vis_f = s.timeline_visible_frames(lane_w);
                let max_scroll = s.max_scroll_frame(lane_w);
                let bar_frames = total_frames.max(s.timeline_scroll_frame + vis_f).max(1.0);
                let max_start = (max_scroll / bar_frames).max(0.0);
                s.timeline_scroll_frame =
                    (new_start.clamp(0.0, max_start) * bar_frames).clamp(0.0, max_scroll);
                s.dirty.timeline_meta = true;
                s.dirty.timeline_tracks = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
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
                let frames_i = s.active_total_frames();
                let frames = frames_i as f32;
                let cur_f = ((s.current_time / dur) * frames).round() as i32;
                let next_f = (cur_f + delta).clamp(0, frames_i);
                s.current_time = (next_f as f32 / frames.max(1.0)) * dur;
                let lane_w = lane_w_logical(&w);
                s.ensure_playhead_in_view(lane_w);
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
                    let lane_w = lane_w_logical(&w);
                    s.ensure_playhead_in_view(lane_w);
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
        win.on_click_diamond(move |kf_idx, track_idx, shift, ctrl| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                s.drag_kf_undo_pending = true;
                let prev_time = s.current_time;
                let prev_bone = s.active_bone_idx;
                let bone_idx = s
                    .visible_timeline_bones()
                    .get(track_idx.max(0) as usize)
                    .copied()
                    .unwrap_or(s.active_bone_idx);
                s.active_bone_idx = bone_idx;
                if let Some(kf_time) = s.active_clip().keyframes.get(kf_idx as usize).map(|k| k.time) {
                    let kf_frame = s.frame_idx_for_time(kf_time);
                    if ctrl {
                        let key = (kf_frame, bone_idx);
                        if !s.selected_keys.remove(&key) {
                            s.selected_keys.insert(key);
                            s.selection_anchor = Some(key);
                        }
                        s.drag_group_origin_frame = None;
                        s.drag_group_moved = false;
                        s.sync_range_from_selected_keys();
                    } else if shift {
                        let anchor = s
                            .selection_anchor
                            .unwrap_or((s.frame_idx_for_time(prev_time), prev_bone));
                        s.select_rect_range(anchor, (kf_frame, bone_idx), false);
                        s.drag_group_origin_frame = None;
                        s.drag_group_moved = false;
                    } else if s.selected_keys.len() > 1 && s.selected_keys.contains(&(kf_frame, bone_idx)) {
                        // Preserve multi-selection while dragging any selected diamond in the group.
                        s.drag_group_origin_frame = Some(kf_frame);
                        s.drag_group_moved = false;
                    } else {
                        s.selected_keys.clear();
                        if s.timeline_bone_scope {
                            s.selected_keys.insert((kf_frame, bone_idx));
                        } else if let Some(mask) = s.active_clip().keyframes.get(kf_idx as usize).map(|kf| kf.bone_mask) {
                            for &bi in s.visible_timeline_bones() {
                                if (mask & (1u16 << bi)) != 0 {
                                    s.selected_keys.insert((kf_frame, bi));
                                }
                            }
                        }
                        s.selection_anchor = Some((kf_frame, bone_idx));
                        s.drag_group_origin_frame = Some(kf_frame);
                        s.drag_group_moved = false;
                        s.sync_range_from_selected_keys();
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
        win.on_end_drag_diamond(move |kf_idx, track_idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.drag_group_moved && s.selected_keys.len() > 1 {
                    let bone_idx = s
                        .visible_timeline_bones()
                        .get(track_idx.max(0) as usize)
                        .copied()
                        .unwrap_or(s.active_bone_idx);
                    if let Some(kf_time) = s.active_clip().keyframes.get(kf_idx as usize).map(|k| k.time) {
                        let kf_frame = s.frame_idx_for_time(kf_time);
                        s.selected_keys.clear();
                        if s.timeline_bone_scope {
                            s.selected_keys.insert((kf_frame, bone_idx));
                        } else if let Some(mask) = s.active_clip().keyframes.get(kf_idx as usize).map(|kf| kf.bone_mask) {
                            for &bi in s.visible_timeline_bones() {
                                if (mask & (1u16 << bi)) != 0 {
                                    s.selected_keys.insert((kf_frame, bi));
                                }
                            }
                        }
                        s.selection_anchor = Some((kf_frame, bone_idx));
                        s.sync_range_from_selected_keys();
                        s.dirty.timeline_meta = true;
                        s.dirty.timeline_diamonds = true;
                        sync_dirty_ui(&mut s, &w);
                    }
                }
                s.drag_group_origin_frame = None;
                s.drag_group_moved = false;
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_box_select_keyframes(move |f0, f1, y0, y1, additive, _commit| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let lane_w = lane_w_logical(&w);
                let vis_f = s.timeline_visible_frames(lane_w);
                let min_frame = s.timeline_scroll_frame + f0.clamp(0.0, 1.0) * vis_f;
                let max_frame = s.timeline_scroll_frame + f1.clamp(0.0, 1.0) * vis_f;
                let min_track = ((y0 - 2.0) / 24.0).floor() as i32;
                let max_track = ((y1 - 2.0) / 24.0).floor() as i32;
                s.select_keyframes_in_box(min_frame, max_frame, min_track, max_track, additive);
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_all_keyframes_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let n = s.select_all_visible_keyframes();
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, format!("{n} keyframes seleccionados"));
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_clear_keyframe_selection_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.selected_keys.is_empty() || s.selected_range_times.is_some() {
                    s.clear_bone_selection();
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_tracks = true;
                    s.dirty.timeline_diamonds = true;
                    sync_dirty_ui(&mut s, &w);
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_duplicate_selected_keyframes_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let n = s.duplicate_selected_keyframes();
                sync_dirty_ui(&mut s, &w);
                if n > 0 {
                    show_toast(&mut s, &w, format!("{n} keyframe(s) duplicados"));
                    w.window().request_redraw();
                }
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
                let n = s.active_clip().keyframes.len();
                if n < 2 {
                    return;
                }
                s.playing = false;
                let frames = s.active_total_frames() as f32;
                let lane_w = lane_w_logical(&w);
                let vis_f = s.timeline_visible_frames(lane_w);
                let frame_pos = s.timeline_scroll_frame + new_frac.clamp(0.0, 1.0) * vis_f;
                let target_f = frame_pos.round().clamp(0.0, frames) as i32;
                let origin_f = s.drag_group_origin_frame.unwrap_or_else(|| {
                    s.active_clip()
                        .keyframes
                        .get(kf_idx as usize)
                        .map(|k| s.frame_idx_for_time(k.time))
                        .unwrap_or(target_f)
                });
                let delta = target_f - origin_f;
                if delta == 0 {
                    return;
                }
                let push_undo = s.drag_kf_undo_pending;
                let moved = s.move_selected_keys_by_frames(delta, push_undo);
                if moved > 0 {
                    s.drag_kf_undo_pending = false;
                    s.drag_group_moved = true;
                    sync_dirty_ui(&mut s, &w);
                    request_immediate_redraw(&w);
                }
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
                if !s.effective_selected_keys().is_empty() {
                    let removed = s.delete_selected_keyframes();
                    if removed > 0 {
                        sync_dirty_ui(&mut s, &w);
                        show_toast(&mut s, &w, format!("{removed} keyframe(s) borrados"));
                        w.window().request_redraw();
                    }
                    return;
                }
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
                    show_toast(&mut s, &w, "Frame completo (10h) borrado".into());
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
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_copy_timeline_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.effective_selected_keys().is_empty() {
                    let count = s.copy_selected_keyframes();
                    if count > 0 {
                        s.dirty.timeline_meta = true;
                        sync_dirty_ui(&mut s, &w);
                        show_toast(&mut s, &w, format!("{count} keyframe(s) copiados"));
                        return;
                    }
                }
                drop(s);
                if w.get_timeline_bone_scope() {
                    w.invoke_copy_bone_action(false);
                } else {
                    w.invoke_copy_pose_action();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_paste_timeline_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let multi = {
                    let s = st.borrow();
                    s.clipboard_bone.as_ref().map_or(false, |cb| cb.multi_bone)
                };
                if multi || w.get_timeline_bone_scope() {
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
                    0 => tr.pos[ax] = (tr.pos[ax] + delta * 0.25).clamp(-64.0, 64.0),
                    1 => tr.rot[ax] = (tr.rot[ax] + delta * 3.0).clamp(-720.0, 720.0),
                    _ => tr.scale[ax] = (tr.scale[ax] + delta * 0.05).clamp(0.10, 4.0),
                }
                s.sync_crouch_knees_if_enabled(ki, b_idx);
                s.clamp_edited_bone_above_floor(ki, b_idx);
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
        let rq = redraw_queued.clone();
        win.on_scrub_bone_axis(move |chan, axis, px_dx, shift, ctrl| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.note_interaction();
                s.playing = false;
                let ki = s.editable_kf_index();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                let ax = axis.clamp(0, 2) as usize;
                let mul = if shift { 0.22 } else { 1.0 };
                match chan {
                    0 => {
                        let mut v = (tr.pos[ax] + px_dx * 0.04 * mul).clamp(-64.0, 64.0);
                        if ctrl { v = (v * 2.0).round() * 0.5; }
                        tr.pos[ax] = v;
                    }
                    1 => {
                        let mut v = (tr.rot[ax] + px_dx * 0.45 * mul).clamp(-720.0, 720.0);
                        if ctrl { v = (v / 5.0).round() * 5.0; }
                        tr.rot[ax] = v;
                    }
                    _ => {
                        let mut v = (tr.scale[ax] + px_dx * 0.006 * mul).clamp(0.10, 4.0);
                        if ctrl { v = (v * 20.0).round() / 20.0; }
                        tr.scale[ax] = v;
                    }
                }
                s.sync_crouch_knees_if_enabled(ki, b_idx);
                s.clamp_edited_bone_above_floor(ki, b_idx);
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                sync_dirty_ui(&mut s, &w);
                request_coalesced_redraw(&w, &rq);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        let rq = redraw_queued.clone();
        win.on_wheel_bone_axis(move |chan, axis, dy_px, shift, ctrl| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.drag.undo_pushed {
                    s.push_undo();
                }
                s.note_interaction();
                s.playing = false;
                let ki = s.editable_kf_index();
                let b_idx = s.active_bone_idx.min(BONE_COUNT - 1);
                let tr = s.active_clip_mut().keyframes[ki].bones.bone_mut(b_idx);
                let ax = axis.clamp(0, 2) as usize;
                let steps = (dy_px / 60.0).clamp(-3.0, 3.0);
                let mul = if shift { 0.25 } else { 1.0 };
                match chan {
                    0 => {
                        let mut v = (tr.pos[ax] + steps * 0.25 * mul).clamp(-64.0, 64.0);
                        if ctrl { v = v.round(); }
                        tr.pos[ax] = v;
                    }
                    1 => {
                        let mut v = (tr.rot[ax] + steps * 2.5 * mul).clamp(-720.0, 720.0);
                        if ctrl { v = (v / 15.0).round() * 15.0; }
                        tr.rot[ax] = v;
                    }
                    _ => {
                        let mut v = (tr.scale[ax] + steps * 0.04 * mul).clamp(0.10, 4.0);
                        if ctrl { v = (v * 10.0).round() / 10.0; }
                        tr.scale[ax] = v;
                    }
                }
                s.sync_crouch_knees_if_enabled(ki, b_idx);
                s.clamp_edited_bone_above_floor(ki, b_idx);
                s.mark_active_clip_modified();
                s.dirty.main_viewport = true;
                s.dirty.transform_readout = true;
                s.dirty.timeline_diamonds = true;
                sync_dirty_ui(&mut s, &w);
                request_coalesced_redraw(&w, &rq);
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
                s.sync_crouch_knees_if_enabled(ki, b_idx);
                s.clamp_edited_bone_above_floor(ki, b_idx);
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
        let st2 = state.clone();
        let w_weak2 = win.as_weak();
        win.on_set_clip_frames(move |target_frames| {
            if let Some(w) = w_weak2.upgrade() {
                let mut s = st2.borrow_mut();
                let clamped = target_frames.clamp(4, 180);
                s.set_clip_frames_preserve_timing(clamped);
                let lane_w = lane_w_logical(&w);
                s.ensure_playhead_in_view(lane_w);
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });

        let st_fps = state.clone();
        let w_fps = win.as_weak();
        win.on_set_timeline_fps(move |fps| {
            if let Some(w) = w_fps.upgrade() {
                let mut s = st_fps.borrow_mut();
                let valid = match fps {
                    20 | 30 | 60 => fps,
                    _ => 20,
                };
                if s.timeline_fps != valid {
                    s.push_undo();
                    s.timeline_fps = valid;
                    s.profile.timeline_fps = valid;
                    s.profile_dirty = true;
                    crate::model::snap_clip_to_fps(s.active_clip_mut(), valid as f32);
                    s.timeline_scroll_frame = 0.0;
                    s.dirty.main_viewport = true;
                    s.dirty.inspector = true;
                    s.dirty.timeline_meta = true;
                    s.dirty.timeline_tracks = true;
                    s.dirty.timeline_diamonds = true;
                    s.update_exact_kf_dirty();
                    sync_dirty_ui(&mut s, &w);
                    show_toast(&mut s, &w, format!("Grilla ajustada a {valid} FPS"));
                    w.window().request_redraw();
                }
            }
        });

        let st_pk = state.clone();
        let w_pk = win.as_weak();
        win.on_pack_keyframes_action(move || {
            if let Some(w) = w_pk.upgrade() {
                let mut s = st_pk.borrow_mut();
                let bone_only = s.timeline_bone_scope;
                let moved = s.pack_or_spread_keyframes(true, bone_only);
                sync_dirty_ui(&mut s, &w);
                let msg = if moved > 0 {
                    format!("Frames juntados ({moved} movidos)")
                } else {
                    "Los frames ya están juntos al mínimo (1f)".into()
                };
                show_toast(&mut s, &w, msg);
                w.window().request_redraw();
            }
        });

        let st_sp = state.clone();
        let w_sp = win.as_weak();
        win.on_spread_keyframes_action(move || {
            if let Some(w) = w_sp.upgrade() {
                let mut s = st_sp.borrow_mut();
                let bone_only = s.timeline_bone_scope;
                let moved = s.pack_or_spread_keyframes(false, bone_only);
                sync_dirty_ui(&mut s, &w);
                let msg = if moved > 0 {
                    format!("Frames separados (+1f, {moved} movidos)")
                } else {
                    "Sin espacio suficiente: aumenta duración (+f) para separar más".into()
                };
                show_toast(&mut s, &w, msg);
                w.window().request_redraw();
            }
        });

        let st_ng = state.clone();
        let w_ng = win.as_weak();
        win.on_nudge_keyframes_action(move |delta| {
            if let Some(w) = w_ng.upgrade() {
                let mut s = st_ng.borrow_mut();
                let bone_only = s.timeline_bone_scope;
                let moved = s.nudge_keyframes(delta, bone_only);
                sync_dirty_ui(&mut s, &w);
                let dir_lbl = if delta < 0 { "-1f" } else { "+1f" };
                let msg = if moved > 0 {
                    format!("Timing movido {dir_lbl} ({moved} frames)")
                } else {
                    format!("No hay margen para mover {dir_lbl}")
                };
                show_toast(&mut s, &w, msg);
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
                let n = s.set_easing_for_selection(EasingMode::from_index(ei));
                sync_dirty_ui(&mut s, &w);
                if n > 1 {
                    show_toast(&mut s, &w, format!("Curva aplicada a {n} frames"));
                }
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
        win.on_toggle_crouch_knees(move |v| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.auto_crouch_knees = v;
                if v {
                    let ki = s.editable_kf_index();
                    let b_idx = s.active_bone_idx;
                    if b_idx == 0 || b_idx == 1 {
                        s.sync_crouch_knees_if_enabled(ki, b_idx);
                    }
                }
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
                    s.clear_bone_selection();
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
                    s.clear_bone_selection();
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
        win.on_toggle_gallery_group(move |gi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.toggle_gallery_group(gi);
                sync_dirty_ui(&mut s, &w);
            }
        });
    }
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
                    s.crouch_legs_driven = false;
                    s.current_time = 0.0;
                    s.playing = false;
                    s.timeline_zoom = 1.0;
                    s.timeline_scroll_frame = 0.0;
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
                    s.crouch_legs_driven = false;
                    s.current_time = 0.0;
                    s.timeline_zoom = 1.0;
                    s.timeline_scroll_frame = 0.0;
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
                s.crouch_legs_driven = false;
                s.current_time = 0.0;
                s.playing = false;
                s.timeline_zoom = 1.0;
                s.timeline_scroll_frame = 0.0;
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
        win.on_studio_key(move |tag, ctrl, shift, alt| {
            let Some(w) = w_weak.upgrade() else { return };
            let low = tag.to_ascii_lowercase();
            match low.as_str() {
                "space" => w.invoke_toggle_play(),
                "left" if alt => w.invoke_nudge_keyframes_action(-1),
                "right" if alt => w.invoke_nudge_keyframes_action(1),
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
                "esc" => w.invoke_clear_keyframe_selection_action(),
                "a" if ctrl => w.invoke_select_all_keyframes_action(),
                "d" if ctrl => w.invoke_duplicate_selected_keyframes_action(),
                "c" if ctrl && shift => w.invoke_copy_pose_action(),
                "v" if ctrl && shift => w.invoke_paste_pose_action(),
                "c" if ctrl => w.invoke_copy_timeline_action(),
                "v" if ctrl => w.invoke_paste_timeline_action(),
                "1" => { w.set_active_tool(0); w.invoke_select_tool(0); }
                "2" | "t" => { w.set_active_tool(1); w.invoke_select_tool(1); }
                "3" | "x" => { w.set_active_tool(2); w.invoke_select_tool(2); }
                "4" => { w.set_active_tool(3); w.invoke_select_tool(3); }
                "5" | "g" => { w.set_active_tool(4); w.invoke_select_tool(4); }
                "l" => w.invoke_toggle_gizmo_space(),
                "w" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[0] = true; w.window().request_redraw(); }
                "s" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[1] = true; w.window().request_redraw(); }
                "a" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[2] = true; w.window().request_redraw(); }
                "d" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[3] = true; w.window().request_redraw(); }
                "e" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[4] = true; w.window().request_redraw(); }
                "q" if !ctrl && !alt => { st.borrow_mut().held_nav_keys[5] = true; w.window().request_redraw(); }
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
                s.dirty.timeline_tracks = true;
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
                            s.crouch_legs_driven = false;
                            s.current_time = 0.0;
                            s.playing = false;
                            s.timeline_zoom = 1.0;
                            s.timeline_scroll_frame = 0.0;
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
        win.on_toggle_gizmo_space(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gizmo_world_space = !s.gizmo_world_space;
                let msg = if s.gizmo_world_space {
                    "Espacio del Gizmo: Mundo (ejes fijos)"
                } else {
                    "Espacio del Gizmo: Local (ejes del hueso)"
                };
                s.dirty.main_viewport = true;
                s.dirty.inspector = true;
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, msg.into());
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_offhand_weapon(move |on| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.show_offhand_weapon = on;
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
        win.on_rename_clip(move |name| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if s.rename_active_clip(name.as_str()) {
                    sync_dirty_ui(&mut s, &w);
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_load_skin_action(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Seleccionar skin de Minecraft (.png)")
                .add_filter("Minecraft Skin PNG", &["png"])
                .pick_file()
            {
                if let Some(w) = w_weak.upgrade() {
                    let mut s = st.borrow_mut();
                    match s.load_skin_from_path(&path) {
                        Ok(msg) => {
                            sync_dirty_ui(&mut s, &w);
                            show_toast(&mut s, &w, msg);
                            w.window().request_redraw();
                        }
                        Err(err) => {
                            show_toast(&mut s, &w, err);
                        }
                    }
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_skin_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.clear_custom_skin();
                sync_dirty_ui(&mut s, &w);
                show_toast(&mut s, &w, "Skin restablecida al modelo base".into());
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_skin_slim(move |on| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.set_skin_slim(on);
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_skin_outer_layer(move |on| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.set_skin_outer_layer(on);
                sync_dirty_ui(&mut s, &w);
                w.window().request_redraw();
            }
        });
    }

    StudioTimers {
        _toast: toast_timer,
        _save: save_timer,
        _thumb: thumb_timer,
        _capture: capture_timer,
    }
}
