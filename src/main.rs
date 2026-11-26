// VoxelMotion Studio - Native Rust Studio Entrypoint, GPU Loop, Self-Tests & Headless Render Verification
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod exporter;
mod math3d;
mod model;
mod renderer3d;
mod voxelizer;

use exporter::{export_project, load_saved_project, ExportFormat};
use math3d::{
    apply_gizmo_drag, camera_basis, cursor_target_plane_point, evaluate_rig_matrices,
    gizmo_axes_world, pick_bone_ray, pick_bone_ray_point, pick_gizmo_handle, GizmoAxis, Mat4,
    Vec3,
};
use model::{
    build_demo_presets, create_blank_clip, empty_pose, mirror_pose, next_keyframe_time,
    prev_keyframe_time, sample_clip_pose, AnimatorProject, BoneTransform, ClipDef, EasingMode,
    GalleryGroup, Keyframe, ToolMode, WeaponCategory, WeaponRecord, BONE_LABELS_ES, BONE_NAMES,
};
use renderer3d::{CameraState, GpuRenderer, RenderParams};
use slint::winit_030::WinitWindowAccessor;
use slint::{Color, ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

slint::include_modules!();

struct DragSession {
    active: bool,
    button: i32,
    start_x: f32,
    start_y: f32,
    last_x: f32,
    last_y: f32,
    undo_pushed: bool,
    active_gizmo: GizmoAxis,
}

impl Default for DragSession {
    fn default() -> Self {
        Self {
            active: false,
            button: 0,
            start_x: 0.0,
            start_y: 0.0,
            last_x: 0.0,
            last_y: 0.0,
            undo_pushed: false,
            active_gizmo: GizmoAxis::None,
        }
    }
}

struct AppState {
    workspace_root: PathBuf,
    project: AnimatorProject,
    weapons: Vec<WeaponRecord>,
    weapon_icons: Vec<Image>,
    filtered_indices: Vec<usize>,
    active_weapon_idx: usize,
    active_clip_idx: usize,
    active_bone_idx: usize,
    selected_category: usize,
    weapon_search: String,
    gallery_filter: i32,
    gallery_search: String,

    current_time: f32,
    playing: bool,
    world_playing: bool,
    speed: f32,
    tool_mode: ToolMode,
    solo_weapon: bool,
    shadows: bool,
    onion: bool,
    trail: bool,
    bg_mode: i32,
    auto_keyframe: bool,
    show_all_bones: bool,
    hidden_bones: [bool; 8],
    hovered_gizmo: GizmoAxis,
    main_camera: CameraState,
    main_target_dist: f32,
    main_target_pos: [f32; 3],
    world_camera_preset: usize,
    world_camera: CameraState,
    world_target_dist: f32,

    clipboard_pose: Option<HashMap<String, BoneTransform>>,
    undo_stack: Vec<ClipDef>,
    redo_stack: Vec<ClipDef>,

    vp_size: (u32, u32),
    world_size: (u32, u32),
    gpu: Option<GpuRenderer>,
    main_img: Option<Image>,
    world_img: Option<Image>,
    card_img: Option<Image>,
    gallery_thumbs: HashMap<String, Image>,
    dirty_main: bool,
    dirty_world: bool,
    dirty_card: bool,
    dirty_catalog: bool,
    dirty_gallery: bool,

    drag: DragSession,
    world_drag: DragSession,
    drag_kf_undo_pending: bool,
    held_nav_keys: [bool; 8], // 0:W, 1:S, 2:A, 3:D, 4:E, 5:Q, 6:+, 7:-
    last_tick: Instant,
    world_last_render: Instant,
    present_fps_accum: u32,
    present_fps_last: Instant,
    present_frames_total: u32,
    gpu_pass_us_accum: u64,
    gpu_pass_samples: u32,
    fps_value: i32,
    hit_stop_timer: f32,
    impact_flash: f32,
    hit_stop_triggered: bool,
    clock_sec: f32,
    custom_export_dest: Option<PathBuf>,
    toast_until: Option<Instant>,
}

fn detect_workspace_root() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            candidates.push(p.to_path_buf());
        }
    }
    for base in candidates {
        for anc in base.ancestors() {
            if anc.join("Cargo.toml").is_file() {
                return anc.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn rgba_to_slint_image(w: u32, h: u32, raw: &[u8]) -> Image {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w.max(1), h.max(1));
    let expected = (w.max(1) * h.max(1) * 4) as usize;
    if raw.len() >= expected {
        buf.make_mut_slice()
            .iter_mut()
            .zip(raw.chunks_exact(4))
            .for_each(|(dst, src)| {
                *dst = Rgba8Pixel { r: src[0], g: src[1], b: src[2], a: src[3] };
            });
    }
    Image::from_rgba8(buf)
}

fn bone_color(idx: usize) -> Color {
    match idx {
        3 => Color::from_rgb_u8(96, 165, 250),  // BrazoDerecho
        4 => Color::from_rgb_u8(167, 139, 250), // BrazoIzquierdo
        5 => Color::from_rgb_u8(251, 191, 36),  // Espada
        1 => Color::from_rgb_u8(52, 211, 153),  // Cuerpo
        2 => Color::from_rgb_u8(244, 114, 182), // Cabeza
        0 => Color::from_rgb_u8(148, 163, 184), // Cintura
        _ => Color::from_rgb_u8(129, 140, 248), // Legs
    }
}

impl AppState {
    fn new(workspace_root: PathBuf) -> Self {
        let project = load_saved_project(&workspace_root).unwrap_or_else(|| AnimatorProject {
            version: 1,
            clips: vec![create_blank_clip(1)],
        });
        let weapons = catalog::build_starter_weapons();
        let weapon_icons = weapons
            .iter()
            .map(|w| match &w.icon_rgba {
                Some((iw, ih, raw)) => rgba_to_slint_image(*iw, *ih, raw),
                None => Image::default(),
            })
            .collect();
        let filtered_indices = (0..weapons.len()).collect();

        let mut world_cam = CameraState::default();
        world_cam.yaw = 0.42;
        world_cam.pitch = 0.16;
        world_cam.dist = 3.25;
        world_cam.target = [0.0, 0.96, 0.0];

        Self {
            workspace_root,
            project,
            weapons,
            weapon_icons,
            filtered_indices,
            active_weapon_idx: 0,
            active_clip_idx: 0,
            active_bone_idx: 3, // BrazoDerecho
            selected_category: 0,
            weapon_search: String::new(),
            gallery_filter: 0,
            gallery_search: String::new(),
            current_time: 0.0,
            playing: false,
            world_playing: true,
            speed: 1.0,
            tool_mode: ToolMode::Rotate,
            solo_weapon: false,
            shadows: true,
            onion: true,
            trail: true,
            bg_mode: 0,
            auto_keyframe: true,
            show_all_bones: false,
            hidden_bones: [false; 8],
            hovered_gizmo: GizmoAxis::None,
            main_camera: CameraState {
                yaw: 0.58,
                pitch: 0.24,
                dist: 3.35,
                target: [0.0, 0.96, 0.0],
            },
            main_target_dist: 3.35,
            main_target_pos: [0.0, 0.96, 0.0],
            world_camera_preset: 0,
            world_camera: world_cam,
            world_target_dist: 3.25,
            clipboard_pose: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            vp_size: (900, 520),
            world_size: (540, 200),
            gpu: None,
            main_img: None,
            world_img: None,
            card_img: None,
            gallery_thumbs: HashMap::new(),
            dirty_main: true,
            dirty_world: true,
            dirty_card: true,
            dirty_catalog: true,
            dirty_gallery: true,
            drag: DragSession::default(),
            world_drag: DragSession::default(),
            drag_kf_undo_pending: false,
            held_nav_keys: [false; 8],
            last_tick: Instant::now(),
            world_last_render: Instant::now(),
            present_fps_accum: 0,
            present_fps_last: Instant::now(),
            present_frames_total: 0,
            gpu_pass_us_accum: 0,
            gpu_pass_samples: 0,
            fps_value: 60,
            hit_stop_timer: 0.0,
            impact_flash: 0.0,
            hit_stop_triggered: false,
            clock_sec: 0.0,
            custom_export_dest: None,
            toast_until: None,
        }
    }

    fn active_weapon(&self) -> &WeaponRecord {
        &self.weapons[self.active_weapon_idx.min(self.weapons.len().saturating_sub(1))]
    }

    fn active_clip(&self) -> &ClipDef {
        &self.project.clips[self.active_clip_idx.min(self.project.clips.len().saturating_sub(1))]
    }

    fn active_clip_mut(&mut self) -> &mut ClipDef {
        let idx = self.active_clip_idx.min(self.project.clips.len().saturating_sub(1));
        &mut self.project.clips[idx]
    }

    fn push_undo(&mut self) {
        let snap = self.active_clip().clone();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    fn ensure_kf_at_current(&mut self) -> usize {
        let cur_t = self.current_time;
        let pose = sample_clip_pose(self.active_clip(), cur_t);
        let clip = self.active_clip_mut();
        for (i, k) in clip.keyframes.iter().enumerate() {
            if (k.time - cur_t).abs() < 0.016 {
                return i;
            }
        }
        clip.keyframes.push(Keyframe {
            time: (cur_t * 1000.0).round() / 1000.0,
            easing: EasingMode::EaseOut,
            bones: pose,
        });
        clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        clip.keyframes
            .iter()
            .position(|k| (k.time - cur_t).abs() < 0.016)
            .unwrap_or(0)
    }

    /// Keyframe index targeted by direct viewport/inspector bone edits: creates if Auto-KF is on,
    /// otherwise snaps cleanly onto the nearest existing keyframe without polluting timeline.
    fn editable_kf_index(&mut self) -> usize {
        let cur_t = self.current_time;
        if let Some(exact) = self
            .active_clip()
            .keyframes
            .iter()
            .position(|k| (k.time - cur_t).abs() < 0.016)
        {
            return exact;
        }
        if self.auto_keyframe {
            return self.ensure_kf_at_current();
        }
        self.active_clip()
            .keyframes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.time - cur_t).abs().total_cmp(&(b.time - cur_t).abs()))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn reset_main_camera(&mut self) {
        self.main_target_pos = [0.0, 0.96, 0.0];
        self.main_target_dist = 3.35;
        self.main_camera = CameraState {
            yaw: 0.58,
            pitch: 0.24,
            dist: 3.35,
            target: [0.0, 0.96, 0.0],
        };
        self.dirty_main = true;
    }

    fn focus_active_bone(&mut self) {
        let (pivot, _, _) = self.active_gizmo_geom();
        // Offset toward mid-limb center when a limb is selected (arms/legs extend downward from shoulder/hip pivot)
        let center_offset = match self.active_bone_idx {
            3 | 4 => Vec3::new(0.0, -0.22, 0.0),
            6 | 7 => Vec3::new(0.0, -0.35, 0.0),
            2 => Vec3::new(0.0, 0.22, 0.0),
            1 => Vec3::new(0.0, 0.30, 0.0),
            _ => Vec3::ZERO,
        };
        let p = pivot.add(center_offset);
        self.main_target_pos = [p.x.clamp(-5.0, 5.0), p.y.clamp(-0.5, 4.0), p.z.clamp(-5.0, 5.0)];
        self.main_target_dist = 0.95;
        self.dirty_main = true;
    }

    fn queue_zoom_at_cursor(&mut self, mx: f32, my: f32, vw: f32, vh: f32, dy: f32) {
        if dy.abs() < 0.01 {
            return;
        }
        let notch = (dy / 60.0).clamp(-6.0, 6.0);
        let old_d = self.main_target_dist.max(0.18);
        let new_d = (old_d * (-notch * 0.28).exp()).clamp(0.18, 16.0);
        if vw > 1.0 && vh > 1.0 {
            let nx = (mx / vw) * 2.0 - 1.0;
            let ny = 1.0 - (my / vh) * 2.0;
            let (_vp, inv_vp, _) = self.main_camera.vp_and_inv(vw / vh);
            let pose = sample_clip_pose(self.active_clip(), self.current_time);
            let mats = evaluate_rig_matrices(&pose);
            let (_, _, cam_f) = camera_basis(self.main_camera.yaw, self.main_camera.pitch);
            let cur_tgt = Vec3::new(self.main_target_pos[0], self.main_target_pos[1], self.main_target_pos[2]);
            let anchor = pick_bone_ray_point(nx, ny, inv_vp, &mats, self.solo_weapon)
                .map(|(_, pt)| pt)
                .unwrap_or_else(|| cursor_target_plane_point(nx, ny, inv_vp, cur_tgt, cam_f));
            if dy > 0.0 {
                // Keep the hovered arm/weapon pixel pinned under the cursor as we zoom in!
                let ratio = (new_d / old_d).clamp(0.25, 1.0);
                let next_tgt = anchor.add(cur_tgt.sub(anchor).mul(ratio));
                self.main_target_pos = [
                    next_tgt.x.clamp(-5.0, 5.0),
                    next_tgt.y.clamp(-0.5, 4.0),
                    next_tgt.z.clamp(-5.0, 5.0),
                ];
            } else if new_d > 2.2 {
                // On zoom out past mid-range, gently re-center back toward character torso
                let relax = ((new_d - 2.2) / 3.0).clamp(0.0, 0.30);
                let def = Vec3::new(0.0, 0.96, 0.0);
                let next_tgt = cur_tgt.add(def.sub(cur_tgt).mul(relax));
                self.main_target_pos = [next_tgt.x, next_tgt.y, next_tgt.z];
            }
        }
        self.main_target_dist = new_d;
    }

    fn queue_smooth_zoom(target_dist: &mut f32, cam: &mut CameraState, dy: f32, min_d: f32, max_d: f32, allow_dolly_push: bool) {
        if dy.abs() < 0.01 {
            return;
        }
        // Slint 1.19 Winit LineDelta uses 60px per wheel notch
        let notch = (dy / 60.0).clamp(-6.0, 6.0);
        if allow_dolly_push && dy > 0.0 && *target_dist <= min_d + 0.04 {
            Self::move_camera_free(cam, 0.09 * notch, 0.0, 0.0);
            *target_dist = min_d;
            return;
        }
        *target_dist = (*target_dist * (-notch * 0.28).exp()).clamp(min_d, max_d);
    }

    fn advance_smooth_camera(cam: &mut CameraState, target_dist: f32, target_pos: [f32; 3], dt: f32) -> bool {
        let mut moved = Self::advance_smooth_zoom(cam, target_dist, dt);
        let dx = target_pos[0] - cam.target[0];
        let dy = target_pos[1] - cam.target[1];
        let dz = target_pos[2] - cam.target[2];
        if dx * dx + dy * dy + dz * dz > 1e-7 {
            let alpha = 1.0 - (-28.0 * dt.clamp(0.001, 0.05)).exp();
            cam.target[0] += dx * alpha;
            cam.target[1] += dy * alpha;
            cam.target[2] += dz * alpha;
            moved = true;
        } else if dx != 0.0 || dy != 0.0 || dz != 0.0 {
            cam.target = target_pos;
            moved = true;
        }
        moved
    }

    fn advance_smooth_zoom(cam: &mut CameraState, target_dist: f32, dt: f32) -> bool {
        let diff = target_dist - cam.dist;
        if diff.abs() <= 0.0006 {
            if diff != 0.0 {
                cam.dist = target_dist;
                return true;
            }
            return false;
        }
        let alpha = 1.0 - (-30.0 * dt.clamp(0.001, 0.05)).exp();
        cam.dist += diff * alpha;
        true
    }

    fn move_camera_free(cam: &mut CameraState, fwd_step: f32, right_step: f32, up_step: f32) {
        let (r, u, f) = camera_basis(cam.yaw, cam.pitch);
        let delta = f.mul(fwd_step).add(r.mul(right_step)).add(u.mul(up_step));
        cam.target[0] = (cam.target[0] + delta.x).clamp(-6.0, 6.0);
        cam.target[1] = (cam.target[1] + delta.y).clamp(-0.5, 4.0);
        cam.target[2] = (cam.target[2] + delta.z).clamp(-6.0, 6.0);
    }

    fn active_gizmo_geom(&self) -> (Vec3, [Vec3; 3], Mat4) {
        let aspect = (self.vp_size.0 as f32 / self.vp_size.1.max(1) as f32).max(0.5);
        let (vp, _, _) = self.main_camera.vp_and_inv(aspect);
        let pose = sample_clip_pose(self.active_clip(), self.current_time);
        let mats = evaluate_rig_matrices(&pose);
        let bone_idx = if self.solo_weapon { 5 } else { self.active_bone_idx.min(7) };
        let bone_m = if self.solo_weapon {
            let ri = pose.get("right_item").cloned().unwrap_or_default();
            crate::math3d::Mat4::translate(0.0, 0.96, 0.55)
                .mul(crate::math3d::Mat4::rot_zyx_deg(ri.rot[0], ri.rot[1], ri.rot[2]))
                .mul(crate::math3d::Mat4::rot_y(0.18))
                .mul(crate::math3d::Mat4::rot_x(-0.14))
                .mul(crate::math3d::Mat4::scale(1.0 / 16.0, 1.0 / 16.0, 1.0 / 16.0))
                .mul(crate::math3d::Mat4::rot_x(std::f32::consts::PI))
        } else {
            mats[bone_idx]
        };
        let (pivot, axes) = gizmo_axes_world(bone_m, self.solo_weapon);
        (pivot, axes, vp)
    }

    fn rebuild_weapon_filter(&mut self) {
        let cat = WeaponCategory::ALL_SIDEBAR
            .get(self.selected_category)
            .copied()
            .unwrap_or(WeaponCategory::Todas);
        let q = self.weapon_search.trim().to_ascii_lowercase();
        self.filtered_indices = self
            .weapons
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                let cat_ok = cat == WeaponCategory::Todas || w.category == cat;
                let q_ok = q.is_empty()
                    || w.name.to_ascii_lowercase().contains(&q)
                    || w.id.to_ascii_lowercase().contains(&q);
                cat_ok && q_ok
            })
            .map(|(i, _)| i)
            .collect();
    }

    fn ensure_active_weapon_gpu(&mut self) {
        let w = self.active_weapon().clone();
        let Some(gpu) = self.gpu.as_mut() else { return };
        if let Some((iw, ih, raw)) = &w.icon_rgba {
            gpu.ensure_weapon_mesh(&w.id, *iw, *ih, raw, &w.mesh_archetype);
        }
    }

    fn generate_gallery_thumbs_if_needed(&mut self) {
        if !self.gallery_thumbs.is_empty() {
            return;
        }
        self.ensure_active_weapon_gpu();
        let weapon_id = self.active_weapon().id.clone();
        let clips = self.project.clips.clone();
        let Some(gpu) = self.gpu.as_mut() else { return };
        let thumb_cam = CameraState {
            yaw: 0.45,
            pitch: 0.20,
            dist: 3.1,
            target: [0.0, 0.96, 0.0],
        };
        for c in &clips {
            let t = c.impact_time.unwrap_or(c.duration * 0.45);
            let pose = sample_clip_pose(c, t);
            gpu.render_to_surface(
                3,
                &RenderParams {
                    pose: &pose,
                    ghost_pose: None,
                    ghost_next_pose: None,
                    clip: c,
                    current_time: t,
                    weapon_id: &weapon_id,
                    selected_bone: 99,
                    solo_weapon: false,
                    hidden_bones: [false; 8],
                    hovered_gizmo: GizmoAxis::None,
                    active_gizmo: GizmoAxis::None,
                    show_shadows: true,
                    show_onion: false,
                    show_trail: true,
                    show_gizmo: false,
                    bg_mode: 0.0,
                    impact_flash: 0.0,
                    anim_clock: 0.0,
                    camera: &thumb_cam,
                    draw_world_backdrop: false,
                },
            );
            let (tw, th, raw) = gpu.readback_surface(3);
            self.gallery_thumbs.insert(c.id.clone(), rgba_to_slint_image(tw, th, &raw));
        }
    }

    fn sync_ui(&mut self, win: &MainWindow) {
        if self.dirty_catalog {
            // Categories & filtered catalog (rebuilt ONLY on startup or search/category filter!)
            let cats: Vec<CategoryEntry> = WeaponCategory::ALL_SIDEBAR
                .iter()
                .map(|c| {
                    let count = if *c == WeaponCategory::Todas {
                        self.weapons.len() as i32
                    } else {
                        self.weapons.iter().filter(|w| w.category == *c).count() as i32
                    };
                    CategoryEntry { name: c.label().into(), count }
                })
                .collect();
            win.set_categories(ModelRc::new(VecModel::from(cats)));
            win.set_total_weapon_count(self.weapons.len() as i32);

            let list: Vec<WeaponListItem> = self
                .filtered_indices
                .iter()
                .take(180)
                .map(|&idx| {
                    let w = &self.weapons[idx];
                    WeaponListItem {
                        index: idx as i32,
                        id: w.id.clone().into(),
                        name: w.name.clone().into(),
                        mod_name: w.mod_source.clone().into(),
                        category: w.category.label().into(),
                        icon: self.weapon_icons.get(idx).cloned().unwrap_or_default(),
                    }
                })
                .collect();
            win.set_filtered_weapons(ModelRc::new(VecModel::from(list)));
            self.dirty_catalog = false;
        }
        win.set_clips_count(self.project.clips.len() as i32);

        let w = self.active_weapon();
        win.set_active_weapon_index(self.active_weapon_idx as i32);
        win.set_active_weapon_name(w.name.clone().into());
        win.set_active_weapon_mod(w.mod_source.clone().into());
        win.set_active_weapon_class(w.weapon_class.clone().into());
        win.set_solo_weapon(self.solo_weapon);

        let clip = self.active_clip();
        let dur = clip.duration.max(0.05);
        let frames = (dur * 60.0).round().max(1.0) as i32;
        let cur_f = ((self.current_time / dur) * (frames as f32)).round() as i32;
        win.set_active_clip_name(clip.name.clone().into());
        win.set_playing(self.playing);
        win.set_world_playing(self.world_playing);
        win.set_current_frame(cur_f.clamp(0, frames));
        win.set_total_frames(frames);
        let tick_labels: Vec<slint::SharedString> = (0..=6)
            .map(|i| slint::SharedString::from(format!("{}f", ((frames as f32 * (i as f32 / 6.0)).round() as i32))))
            .collect();
        win.set_ruler_ticks(ModelRc::new(VecModel::from(tick_labels)));
        win.set_playhead_frac((self.current_time / dur).clamp(0.0, 1.0));
        win.set_has_impact(clip.impact_time.is_some());
        if let Some(imp) = clip.impact_time {
            win.set_impact_frac((imp / dur).clamp(0.0, 1.0));
            win.set_impact_time_text(format!("{imp:.2}s").into());
        } else {
            win.set_impact_frac(0.0);
            win.set_impact_time_text("Sin marca".into());
        }
        win.set_anim_loop(clip.looped);
        win.set_duration_val(dur);
        win.set_duration_text(format!("{dur:.2}").into());
        win.set_speed_label(format!("{:.2}x", self.speed).trim_end_matches('0').trim_end_matches('.').to_string().into());
        win.set_shadows_enabled(self.shadows);
        win.set_onion_enabled(self.onion);
        win.set_trail_enabled(self.trail);
        win.set_bg_idx(self.bg_mode);

        // Active Bone transform readout
        let pose = sample_clip_pose(clip, self.current_time);
        let bname = BONE_NAMES[self.active_bone_idx.min(7)];
        let tr = pose.get(bname).cloned().unwrap_or_default();
        win.set_active_bone_idx(self.active_bone_idx as i32);
        win.set_active_bone_name(BONE_LABELS_ES[self.active_bone_idx.min(7)].into());
        win.set_pos_x(format!("{:.2}", tr.pos[0]).into());
        win.set_pos_y(format!("{:.2}", tr.pos[1]).into());
        win.set_pos_z(format!("{:.2}", tr.pos[2]).into());
        win.set_rot_x(format!("{:.1}°", tr.rot[0]).into());
        win.set_rot_y(format!("{:.1}°", tr.rot[1]).into());
        win.set_rot_z(format!("{:.1}°", tr.rot[2]).into());
        win.set_scl_x(format!("{:.2}", tr.scale[0]).into());
        win.set_scl_y(format!("{:.2}", tr.scale[1]).into());
        win.set_scl_z(format!("{:.2}", tr.scale[2]).into());

        // Active Easing + KF badge + Status HUD
        let kf_count = clip.keyframes.len().max(1);
        let exact_kf = clip.keyframes.iter().position(|k| (k.time - self.current_time).abs() < 0.016);
        let nearest_kf_idx = exact_kf.unwrap_or_else(|| {
            clip.keyframes
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (a.time - self.current_time).abs().total_cmp(&(b.time - self.current_time).abs()))
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
        if let Some(nk) = clip.keyframes.get(nearest_kf_idx) {
            win.set_easing_idx(nk.easing.index());
        }
        win.set_selected_kf_idx(nearest_kf_idx as i32);
        win.set_auto_keyframe(self.auto_keyframe);
        win.set_show_all_bones(self.show_all_bones);
        if exact_kf.is_some() {
            win.set_kf_badge(format!("KF {}/{}", nearest_kf_idx + 1, kf_count).into());
        } else {
            win.set_kf_badge(format!("~{}/{} KF", nearest_kf_idx + 1, kf_count).into());
        }

        let tool_lbl = match self.tool_mode {
            ToolMode::Select => "Seleccionar",
            ToolMode::Rotate => "Rotar",
            ToolMode::Scale => "Escalar",
            ToolMode::Camera => "Cámara",
            ToolMode::Move => "Mover",
        };
        let hl_axis = if self.drag.active && self.drag.active_gizmo != GizmoAxis::None {
            self.drag.active_gizmo
        } else {
            self.hovered_gizmo
        };
        let axis_suffix = if hl_axis == GizmoAxis::None {
            String::new()
        } else {
            format!(" · {}", hl_axis.label_es())
        };
        win.set_status_pill(format!("{} · {}{}", BONE_LABELS_ES[self.active_bone_idx.min(7)], tool_lbl, axis_suffix).into());

        // All 8 bones always exposed in Huesos Inspector Tab; Timeline toggles between Primary 4 and Full 8
        let all_tracks: Vec<TimelineTrack> = (0..8usize)
            .map(|bi| TimelineTrack {
                bone_idx: bi as i32,
                name: BONE_LABELS_ES[bi].into(),
                dot_color: bone_color(bi),
                active: self.active_bone_idx == bi,
                visible: !self.hidden_bones[bi],
            })
            .collect();
        win.set_all_bones_tracks(ModelRc::new(VecModel::from(all_tracks)));

        let visible_bones: &[usize] = if self.show_all_bones {
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
                active: self.active_bone_idx == bi,
                visible: !self.hidden_bones[bi],
            })
            .collect();
        win.set_timeline_tracks(ModelRc::new(VecModel::from(tracks)));

        let mut diamonds = Vec::new();
        for (ki, kf) in clip.keyframes.iter().enumerate() {
            let frac = (kf.time / dur).clamp(0.0, 1.0);
            let is_imp = clip.impact_time.map(|imp| (kf.time - imp).abs() < 0.02).unwrap_or(false);
            let is_sel = (kf.time - self.current_time).abs() < 0.016;
            for (ti, &bi) in visible_bones.iter().enumerate() {
                let bone_key = BONE_NAMES[bi];
                let touched = ki == 0
                    || ki + 1 == clip.keyframes.len()
                    || kf
                        .bones
                        .get(bone_key)
                        .map(|t| {
                            t.rot.iter().any(|x| x.abs() > 0.1)
                                || t.pos.iter().any(|x| x.abs() > 0.05)
                                || t.scale.iter().any(|x| (x - 1.0).abs() > 0.02)
                        })
                        .unwrap_or(false);
                if touched {
                    diamonds.push(TimelineDiamond {
                        kf_idx: ki as i32,
                        track_idx: ti as i32,
                        frac,
                        is_impact: is_imp,
                        is_selected: is_sel && self.active_bone_idx == bi,
                        dot_color: bone_color(bi),
                    });
                }
            }
        }
        win.set_timeline_diamonds(ModelRc::new(VecModel::from(diamonds)));

        if self.dirty_gallery {
            // Animation Gallery Cards (rebuilt only on startup, thumb gen, filter/search, or clip switch!)
            let gq = self.gallery_search.trim().to_ascii_lowercase();
            let g_items: Vec<GalleryItem> = self
                .project
                .clips
                .iter()
                .filter(|c| {
                    let grp_ok = match self.gallery_filter {
                        1 => c.gallery_group == GalleryGroup::Combate,
                        2 => c.gallery_group == GalleryGroup::Movimiento,
                        3 => c.gallery_group == GalleryGroup::Defensa,
                        4 => c.gallery_group == GalleryGroup::Especiales,
                        5 => c.gallery_group == GalleryGroup::Interaccion,
                        _ => true,
                    };
                    let q_ok = gq.is_empty() || c.name.to_ascii_lowercase().contains(&gq);
                    grp_ok && q_ok
                })
                .map(|c| GalleryItem {
                    id: c.id.clone().into(),
                    name: c.name.clone().into(),
                    group: format!("{:?}", c.gallery_group).into(),
                    thumb: self.gallery_thumbs.get(&c.id).cloned().unwrap_or_default(),
                    active: c.id == clip.id,
                })
                .collect();
            win.set_gallery_items(ModelRc::new(VecModel::from(g_items)));
            win.set_featured_gallery_id(clip.id.clone().into());
            win.set_featured_gallery_name(clip.name.clone().into());
            win.set_featured_gallery_thumb(self.gallery_thumbs.get(&clip.id).cloned().unwrap_or_default());
            self.dirty_gallery = false;
        }

        // Pan-invariant Live 3D Triad Axis Projections using pure camera basis
        let (cam_r, cam_u, _) = camera_basis(self.main_camera.yaw, self.main_camera.pitch);
        let project_axis = |v: Vec3| -> (f32, f32) {
            (v.dot(cam_r) * 15.0, -v.dot(cam_u) * 15.0)
        };
        let (xx, xy) = project_axis(Vec3::new(1.0, 0.0, 0.0));
        let (yx, yy) = project_axis(Vec3::new(0.0, 1.0, 0.0));
        let (zx, zy) = project_axis(Vec3::new(0.0, 0.0, 1.0));
        win.set_triad_x_dx(xx);
        win.set_triad_x_dy(xy);
        win.set_triad_y_dx(yx);
        win.set_triad_y_dy(yy);
        win.set_triad_z_dx(zx);
        win.set_triad_z_dy(zy);
    }

    fn import_weapon_paths(&mut self, paths: &[PathBuf]) -> usize {
        let imported = catalog::import_weapons_from_paths(paths);
        let count = imported.len();
        if count == 0 {
            return 0;
        }
        let mut first_idx = None;
        for w in imported {
            let icon = match &w.icon_rgba {
                Some((iw, ih, raw)) => rgba_to_slint_image(*iw, *ih, raw),
                None => Image::default(),
            };
            let idx = if let Some(pos) = self.weapons.iter().position(|existing| existing.id == w.id) {
                self.weapons[pos] = w;
                self.weapon_icons[pos] = icon;
                pos
            } else {
                self.weapons.push(w);
                self.weapon_icons.push(icon);
                self.weapons.len() - 1
            };
            if first_idx.is_none() {
                first_idx = Some(idx);
            }
        }
        if let Some(idx) = first_idx {
            self.active_weapon_idx = idx;
        }
        self.selected_category = 0;
        self.weapon_search.clear();
        self.rebuild_weapon_filter();
        self.dirty_catalog = true;
        self.dirty_main = true;
        self.dirty_world = true;
        self.dirty_card = true;
        count
    }

    fn reset_to_blank_scene(&mut self) {
        self.project = AnimatorProject {
            version: 1,
            clips: vec![create_blank_clip(1)],
        };
        self.active_clip_idx = 0;
        self.current_time = 0.0;
        self.playing = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.gallery_thumbs.clear();
        self.dirty_main = true;
        self.dirty_world = true;
        self.dirty_gallery = true;
    }

    fn load_demo_presets(&mut self) -> usize {
        let demos = build_demo_presets();
        let mut added = 0usize;
        let first_added_idx = self.project.clips.len();
        for c in demos {
            if !self.project.clips.iter().any(|existing| existing.id == c.id) {
                self.project.clips.push(c);
                added += 1;
            }
        }
        if added > 0 {
            self.active_clip_idx = first_added_idx.min(self.project.clips.len().saturating_sub(1));
            self.current_time = 0.0;
            self.playing = false;
            self.gallery_thumbs.clear();
            self.dirty_main = true;
            self.dirty_world = true;
            self.dirty_gallery = true;
        }
        added
    }

    fn show_toast(&mut self, win: &MainWindow, msg: String) {
        win.set_toast_message(msg.into());
        win.set_toast_visible(true);
        self.toast_until = Some(Instant::now() + std::time::Duration::from_secs(3));
    }

    fn render_gpu_frames_only(
        &mut self,
        vp_size: (u32, u32),
        world_size: (u32, u32),
    ) -> (Option<Image>, Option<Image>, Option<Image>) {
        if self.vp_size != vp_size {
            self.vp_size = vp_size;
            self.dirty_main = true;
        }
        if self.world_size != world_size {
            self.world_size = world_size;
            self.dirty_world = true;
        }

        self.ensure_active_weapon_gpu();
        self.generate_gallery_thumbs_if_needed();

        let clip = self.active_clip().clone();
        let cur_t = self.current_time;
        let pose = sample_clip_pose(&clip, cur_t);
        let prev_kf = prev_keyframe_time(&clip, cur_t).map(|pt| sample_clip_pose(&clip, pt));
        let next_kf = next_keyframe_time(&clip, cur_t).map(|nt| sample_clip_pose(&clip, nt));
        let weapon_id = self.active_weapon().id.clone();

        let Some(gpu) = self.gpu.as_mut() else {
            return (None, None, None);
        };

        let mut new_main = None;
        let mut new_world = None;
        let mut new_card = None;

        if gpu.resize_main_if_needed(self.vp_size.0, self.vp_size.1) || self.main_img.is_none() {
            self.main_img = Image::try_from(gpu.main_surface.resolve_tex.clone()).ok();
            self.dirty_main = true;
            new_main = self.main_img.clone();
        }
        if gpu.resize_world_if_needed(self.world_size.0, self.world_size.1) || self.world_img.is_none() {
            self.world_img = Image::try_from(gpu.world_surface.resolve_tex.clone()).ok();
            self.dirty_world = true;
            new_world = self.world_img.clone();
        }
        if self.card_img.is_none() {
            self.card_img = Image::try_from(gpu.card_surface.resolve_tex.clone()).ok();
            self.dirty_card = true;
            new_card = self.card_img.clone();
        }

        if self.dirty_main {
            gpu.render_to_surface(
                0,
                &RenderParams {
                    pose: &pose,
                    ghost_pose: prev_kf.as_ref(),
                    ghost_next_pose: next_kf.as_ref(),
                    clip: &clip,
                    current_time: cur_t,
                    weapon_id: &weapon_id,
                    selected_bone: self.active_bone_idx,
                    solo_weapon: self.solo_weapon,
                    hidden_bones: self.hidden_bones,
                    hovered_gizmo: self.hovered_gizmo,
                    active_gizmo: self.drag.active_gizmo,
                    show_shadows: self.shadows,
                    show_onion: self.onion,
                    show_trail: self.trail,
                    show_gizmo: matches!(self.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move),
                    bg_mode: self.bg_mode as f32,
                    impact_flash: self.impact_flash,
                    anim_clock: self.clock_sec,
                    camera: &self.main_camera,
                    draw_world_backdrop: self.bg_mode == 1,
                },
            );
            self.dirty_main = false;
        }

        if self.dirty_world || self.world_playing {
            let world_t = if self.playing {
                self.current_time
            } else {
                self.clock_sec % clip.duration.max(0.1)
            };
            let world_pose = sample_clip_pose(&clip, world_t);
            gpu.render_to_surface(
                1,
                &RenderParams {
                    pose: &world_pose,
                    ghost_pose: None,
                    ghost_next_pose: None,
                    clip: &clip,
                    current_time: world_t,
                    weapon_id: &weapon_id,
                    selected_bone: 99,
                    solo_weapon: false,
                    hidden_bones: [false; 8],
                    hovered_gizmo: GizmoAxis::None,
                    active_gizmo: GizmoAxis::None,
                    show_shadows: true,
                    show_onion: false,
                    show_trail: true,
                    show_gizmo: false,
                    bg_mode: 1.0, // Always Sakura Temple Courtyard in "Vista Previa en Juego"!
                    impact_flash: self.impact_flash,
                    anim_clock: self.clock_sec,
                    camera: &self.world_camera,
                    draw_world_backdrop: true,
                },
            );
            self.dirty_world = false;
        }

        if self.dirty_card {
            let card_cam = CameraState {
                yaw: 1.48,
                pitch: 0.22,
                dist: 2.35,
                target: [0.0, 0.96, 0.55],
            };
            let zero_pose = empty_pose();
            gpu.render_to_surface(
                2,
                &RenderParams {
                    pose: &zero_pose,
                    ghost_pose: None,
                    ghost_next_pose: None,
                    clip: &clip,
                    current_time: 0.0,
                    weapon_id: &weapon_id,
                    selected_bone: 99,
                    solo_weapon: true,
                    hidden_bones: [false; 8],
                    hovered_gizmo: GizmoAxis::None,
                    active_gizmo: GizmoAxis::None,
                    show_shadows: false,
                    show_onion: false,
                    show_trail: false,
                    show_gizmo: false,
                    bg_mode: 0.0,
                    impact_flash: 0.0,
                    anim_clock: 0.0,
                    camera: &card_cam,
                    draw_world_backdrop: false,
                },
            );
            self.dirty_card = false;
        }
        (new_main, new_world, new_card)
    }
}

fn export_storyboard_strip(state: &mut AppState) -> Result<PathBuf, String> {
    state.ensure_active_weapon_gpu();
    let clip = state.active_clip().clone();
    let weapon_id = state.active_weapon().id.clone();
    let Some(gpu) = state.gpu.as_mut() else {
        return Err("GPU no inicializada".into());
    };
    let cols = 6u32;
    let cw = 260u32;
    let ch = 260u32;
    gpu.resize_main_if_needed(cw, ch);
    let mut strip = image::RgbaImage::new(cols * cw, ch);
    let dur = clip.duration.max(0.08);

    for i in 0..cols {
        let mut t = dur * (i as f32) / ((cols - 1) as f32);
        if let Some(imp) = clip.impact_time {
            if ((t - imp).abs() < dur / 10.0) && i > 0 && i < cols - 1 {
                t = imp;
            }
        }
        let pose = sample_clip_pose(&clip, t);
        let is_imp = clip.impact_time.map(|imp| (t - imp).abs() < 0.01).unwrap_or(false);
        gpu.render_to_surface(
            0,
            &RenderParams {
                pose: &pose,
                ghost_pose: None,
                ghost_next_pose: None,
                clip: &clip,
                current_time: t,
                weapon_id: &weapon_id,
                selected_bone: 99,
                solo_weapon: false,
                hidden_bones: [false; 8],
                hovered_gizmo: GizmoAxis::None,
                active_gizmo: GizmoAxis::None,
                show_shadows: true,
                show_onion: false,
                show_trail: true,
                show_gizmo: false,
                bg_mode: 0.0,
                impact_flash: if is_imp { 0.9 } else { 0.0 },
                anim_clock: 0.0,
                camera: &state.main_camera,
                draw_world_backdrop: false,
            },
        );
        let (rw, rh, raw) = gpu.readback_surface(0);
        if let Some(cell_img) = image::RgbaImage::from_raw(rw, rh, raw) {
            image::imageops::overlay(&mut strip, &cell_img, (i * cw) as i64, 0);
        }
    }
    state.dirty_main = true;
    let out_dir = state.workspace_root.join("exports");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_file = out_dir.join(format!("{}_strip.png", clip.id));
    strip.save(&out_file).map_err(|e| e.to_string())?;
    Ok(out_file)
}

fn run_headless_smoke(out_png: Option<&Path>) -> Result<(), String> {
    let root = detect_workspace_root();
    let mut state = AppState::new(root.clone());

    // 1. Verify Blender blank mode & procedural starter weapon catalog
    if state.weapons.is_empty() {
        return Err("No starter weapons available!".into());
    }
    if state.project.clips.is_empty() {
        return Err("Startup project missing blank clip!".into());
    }

    // 2. Verify timeline & pose edits + Undo/Redo + Demo preset loading
    let added_demos = state.load_demo_presets();
    if added_demos == 0 && state.project.clips.len() < 10 {
        return Err("Failed to load demo preset library!".into());
    }
    let orig_rot = state.active_clip().keyframes[0].bones["right_arm"].rot;
    state.push_undo();
    state.ensure_active_weapon_gpu();
    let ki = state.ensure_kf_at_current();
    state.active_clip_mut().keyframes[ki].bones.get_mut("right_arm").unwrap().rot[0] += 15.0;
    assert_ne!(state.active_clip().keyframes[ki].bones["right_arm"].rot, orig_rot);
    if let Some(prev) = state.undo_stack.pop() {
        state.active_clip_mut().clone_from(&prev);
    }

    // 3. Exercise search, category filter, mirror, easing, duration, keyframe edits
    state.selected_category = 2; // Katanas
    state.rebuild_weapon_filter();
    assert!(!state.filtered_indices.is_empty());
    state.weapon_search = "katana".into();
    state.rebuild_weapon_filter();
    assert!(!state.filtered_indices.is_empty());
    state.selected_category = 0;
    state.weapon_search.clear();
    state.rebuild_weapon_filter();

    let clip_snapshot = state.active_clip().clone();
    let p_mir = mirror_pose(&clip_snapshot.keyframes[0].bones);
    assert_eq!(p_mir["left_arm"].rot[1], -clip_snapshot.keyframes[0].bones["right_arm"].rot[1]);

    // 4. Verify real GPU wgpu adapter, 120-frame 4x MSAA performance benchmark & offscreen render
    let instance = slint::wgpu_30::wgpu::Instance::default();
    let adapter = pollster_block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("No GPU adapter: {e:?}"))?;
    let (device, queue) = pollster_block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("Device failed: {e:?}"))?;
    let mut gpu = GpuRenderer::new(device, queue);
    if let Some((w, h, raw)) = &state.weapons[0].icon_rgba {
        gpu.ensure_weapon_mesh(&state.weapons[0].id, *w, *h, raw, &state.weapons[0].mesh_archetype);
    }
    let clip = state.active_clip().clone();
    let t_start = Instant::now();
    for f in 0..120 {
        let t = (f as f32 / 120.0) * clip.duration;
        let pose = sample_clip_pose(&clip, t);
        gpu.render_to_surface(
            0,
            &RenderParams {
                pose: &pose,
                ghost_pose: None,
                ghost_next_pose: None,
                clip: &clip,
                current_time: t,
                weapon_id: &state.weapons[0].id,
                selected_bone: 3,
                solo_weapon: false,
                hidden_bones: [false; 8],
                hovered_gizmo: GizmoAxis::None,
                active_gizmo: GizmoAxis::None,
                show_shadows: true,
                show_onion: false,
                show_trail: true,
                show_gizmo: true,
                bg_mode: 1.0,
                impact_flash: if (t - clip.impact_time.unwrap_or(0.2)).abs() < 0.04 { 0.8 } else { 0.0 },
                anim_clock: t,
                camera: &state.main_camera,
                draw_world_backdrop: true,
            },
        );
    }
    let _ = gpu.device.poll(slint::wgpu_30::wgpu::PollType::wait_indefinitely());
    let gpu_120_ms = t_start.elapsed().as_secs_f64() * 1000.0;
    let avg_fps = 120.0 / (gpu_120_ms / 1000.0);

    if let Some(path) = out_png {
        let (rw, rh, raw) = gpu.readback_surface(0);
        if let Some(img) = image::RgbaImage::from_raw(rw, rh, raw) {
            img.save(path).map_err(|e| e.to_string())?;
        }
    }

    state.gpu = Some(gpu);
    let strip_path = export_storyboard_strip(&mut state)?;
    assert!(strip_path.is_file());

    // Explicitly tear down GPU resources BEFORE adapter/instance drop to avoid Windows DX12 COM teardown crash
    state.gpu = None;
    drop(adapter);
    drop(instance);

    // 5. Verify export across Bedrock 1.8.0 & GeckoLib 4.4.4 + Timings JSON
    let smoke_out = root.join("exports");
    let summary = export_project(&state.project, &root, Some(&smoke_out), ExportFormat::Both, true, true)?;
    if summary.clips_exported < 10 || summary.targets.is_empty() {
        return Err(format!("Export verification failed: {} clips", summary.clips_exported));
    }

    let report = format!(
        "VoxelMotion Studio SELF-TEST OK: {} weapons, {} clips exported, {} BC overrides, {} timing events | GPU 120x 4x-MSAA frames in {:.2} ms ({:.0} FPS) | strip={}\n",
        state.weapons.len(),
        summary.clips_exported,
        summary.bc_overrides,
        summary.event_timings,
        gpu_120_ms,
        avg_fps,
        strip_path.display()
    );
    let _ = std::fs::write(root.join("exports/self_test_report.txt"), &report);
    println!("{report}");
    Ok(())
}

/// Minimal zero-dep pollster-free Future executor using thread parking (safe for adapter/device init).
fn pollster_block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn clone_fn(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &VTABLE)
    }
    fn wake_fn(ptr: *const ()) {
        let t = unsafe { &*(ptr as *const std::thread::Thread) };
        t.unpark();
    }
    fn drop_fn(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_fn, wake_fn, wake_fn, drop_fn);
    let current = std::thread::current();
    let waker = unsafe { Waker::from_raw(RawWaker::new(&current as *const _ as *const (), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut pinned = std::pin::pin!(fut);
    loop {
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(windows)]
#[link(name = "winmm")]
extern "system" {
    fn timeBeginPeriod(u_period: u32) -> u32;
    fn timeEndPeriod(u_period: u32) -> u32;
}

fn main() {
    #[cfg(windows)]
    unsafe {
        timeBeginPeriod(1);
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-test") {
        let out_png = args
            .windows(2)
            .find(|w| w[0] == "--render-3d")
            .map(|w| PathBuf::from(&w[1]));
        if let Err(e) = run_headless_smoke(out_png.as_deref()) {
            eprintln!("SELF-TEST ERROR: {e}");
            std::process::exit(1);
        }
        #[cfg(windows)]
        unsafe {
            timeEndPeriod(1);
        }
        return;
    }

    let bench_window = args.iter().any(|a| a == "--bench-window");

    let mut wgpu_settings = slint::wgpu_30::WGPUSettings::default();
    wgpu_settings.power_preference = slint::wgpu_30::wgpu::PowerPreference::HighPerformance;
    slint::BackendSelector::new()
        .require_wgpu_30(slint::wgpu_30::WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Failed to initialize WGPU 30 + FemtoVG GPU backend");

    let win = MainWindow::new().expect("Failed to build MainWindow");
    let root = detect_workspace_root();
    let state = Rc::new(RefCell::new(AppState::new(root)));

    // Optional --capture-window <png> [+ --capture-tab N | --capture-time sec] mode: auto-takes live window snapshot & exits cleanly!
    let auto_capture_path: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--capture-window")
        .map(|w| PathBuf::from(&w[1]));
    let capture_tab: Option<i32> = args
        .windows(2)
        .find(|w| w[0] == "--capture-tab")
        .and_then(|w| w[1].parse().ok());
    let capture_time: Option<f32> = args
        .windows(2)
        .find(|w| w[0] == "--capture-time")
        .and_then(|w| w[1].parse().ok());

    if let Some(tab) = capture_tab {
        win.set_main_tab(tab);
    }
    if let Some(t) = capture_time {
        let mut s = state.borrow_mut();
        let dur = s.active_clip().duration.max(0.05);
        s.current_time = t.clamp(0.0, dur);
        s.solo_weapon = false;
        s.playing = false;
        s.impact_flash = s
            .active_clip()
            .impact_time
            .map(|imp| if (s.current_time - imp).abs() < 0.04 { 0.85 } else { 0.0 })
            .unwrap_or(0.0);
        s.dirty_main = true;
        s.dirty_world = true;
    }

    state.borrow_mut().sync_ui(&win);
    if capture_time.is_some() {
        win.set_playing(true); // Exercise vector Pause glyph when mid-swing capture runs
    }

    let vp_size_cell = Rc::new(Cell::new((900u32, 520u32)));
    let world_size_cell = Rc::new(Cell::new((540u32, 200u32)));

    // Attach GPU rendering notifier (zero-copy WGPU30 device/queue shared with Slint, zero RefCell re-entrancy!)
    let bench_start = Rc::new(RefCell::new(Option::<Instant>::None));
    {
        let st = state.clone();
        let vp_c = vp_size_cell.clone();
        let wd_c = world_size_cell.clone();
        let w_weak = win.as_weak();
        let b_start = bench_start.clone();
        let _ = win.window().set_rendering_notifier(move |render_state, api| {
            let (Some(win), slint::GraphicsAPI::WGPU30 { device, queue, .. }) = (w_weak.upgrade(), api) else {
                return;
            };
            match render_state {
                slint::RenderingState::RenderingSetup => {
                    let imgs = {
                        let Ok(mut s) = st.try_borrow_mut() else { return };
                        if s.gpu.is_none() {
                            s.gpu = Some(GpuRenderer::new(device.clone(), queue.clone()));
                        }
                        s.dirty_main = true;
                        s.dirty_world = true;
                        s.dirty_card = true;
                        s.render_gpu_frames_only(vp_c.get(), wd_c.get())
                    };
                    // Drop RefMut BEFORE mutating Slint window properties (prevents re-entrant RefCell panic!)
                    win.set_gpu_adapter_name("DirectX 12 / Vulkan · Alto Rendimiento".into());
                    if let Some(im) = imgs.0 { win.set_main_viewport(im); }
                    if let Some(im) = imgs.1 { win.set_world_viewport(im); }
                    if let Some(im) = imgs.2 { win.set_weapon_card_img(im); }
                    if let Ok(mut s) = st.try_borrow_mut() {
                        s.dirty_gallery = true;
                        s.present_fps_last = Instant::now();
                        s.present_fps_accum = 0;
                        s.sync_ui(&win);
                    }
                    if bench_window {
                        *b_start.borrow_mut() = Some(Instant::now());
                    }
                }
                slint::RenderingState::BeforeRendering => {
                    let t0 = Instant::now();
                    let (imgs, did_main) = {
                        let Ok(mut s) = st.try_borrow_mut() else { return };
                        let dm = s.dirty_main;
                        (s.render_gpu_frames_only(vp_c.get(), wd_c.get()), dm)
                    };
                    if did_main {
                        if let Ok(mut s) = st.try_borrow_mut() {
                            s.gpu_pass_us_accum += t0.elapsed().as_micros() as u64;
                            s.gpu_pass_samples += 1;
                        }
                    }
                    if let Some(im) = imgs.0 { win.set_main_viewport(im); }
                    if let Some(im) = imgs.1 { win.set_world_viewport(im); }
                    if let Some(im) = imgs.2 { win.set_weapon_card_img(im); }
                }
                slint::RenderingState::AfterRendering => {
                    let mut new_fps: Option<i32> = None;
                    let mut bench_done: Option<String> = None;
                    if let Ok(mut s) = st.try_borrow_mut() {
                        let now = Instant::now();
                        s.present_fps_accum += 1;
                        s.present_frames_total += 1;
                        let el = (now - s.present_fps_last).as_secs_f32();
                        if el >= 0.45 {
                            let f = (s.present_fps_accum as f32 / el).round() as i32;
                            s.fps_value = f.max(1);
                            s.present_fps_accum = 0;
                            s.present_fps_last = now;
                            new_fps = Some(s.fps_value);
                        }
                        if bench_window {
                            if s.present_frames_total <= 60 {
                                s.queue_zoom_at_cursor(390.0, 260.0, vp_c.get().0 as f32, vp_c.get().1 as f32, 30.0);
                                let td = s.main_target_dist;
                                let tp = s.main_target_pos;
                                AppState::advance_smooth_camera(&mut s.main_camera, td, tp, 0.016);
                                s.dirty_main = true;
                            } else if let Some(t0) = *b_start.borrow() {
                                let total_ms = t0.elapsed().as_secs_f64() * 1000.0;
                                let avg_frame_ms = total_ms / (s.present_frames_total as f64);
                                let real_fps = 1000.0 / avg_frame_ms.max(0.01);
                                let avg_gpu_ms = if s.gpu_pass_samples > 0 {
                                    (s.gpu_pass_us_accum as f64 / s.gpu_pass_samples as f64) / 1000.0
                                } else {
                                    0.0
                                };
                                bench_done = Some(format!(
                                    "BENCH_GUI_OK: frames={} total={:.1}ms frame_avg={:.2}ms real_fps={:.1} gpu3d_avg={:.2}ms",
                                    s.present_frames_total, total_ms, avg_frame_ms, real_fps, avg_gpu_ms
                                ));
                            }
                        }
                    }
                    if let Some(fps) = new_fps {
                        win.set_fps_display(fps);
                    }
                    if bench_window {
                        win.window().request_redraw();
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

    // Window chrome handlers (drag, minimize, maximize, close)
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
    {
        let w_weak = win.as_weak();
        win.on_win_close(move || {
            if let Some(w) = w_weak.upgrade() {
                let _ = w.hide();
            }
            let _ = slint::quit_event_loop();
        });
    }

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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_catalog = true;
                s.sync_ui(&w);
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
                s.dirty_catalog = true;
                s.sync_ui(&w);
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
                    s.dirty_main = true;
                    s.dirty_world = true;
                    s.dirty_card = true;
                    s.sync_ui(&w);
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
                    let cur_pos = s.filtered_indices.iter().position(|&x| x == s.active_weapon_idx).unwrap_or(0) as i32;
                    let n = s.filtered_indices.len() as i32;
                    let next_pos = ((cur_pos + delta) % n + n) % n;
                    s.active_weapon_idx = s.filtered_indices[next_pos as usize];
                    s.dirty_main = true;
                    s.dirty_world = true;
                    s.dirty_card = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
                }
            }
        });
    }

    // Viewport Resize & 3D Interaction (lock-free via Cell!)
    {
        let vp_c = vp_size_cell.clone();
        win.on_viewport_resized(move |vw, vh| {
            vp_c.set((vw.max(200.0) as u32, vh.max(150.0) as u32));
        });
    }
    {
        let wd_c = world_size_cell.clone();
        win.on_world_resized(move |vw, vh| {
            wd_c.set((vw.max(180.0) as u32, vh.max(100.0) as u32));
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
                let (cam_r, _, _) = camera_basis(s.main_camera.yaw, s.main_camera.pitch);
                let gizmo_shown = matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move);
                if gizmo_shown {
                    active_gizmo = pick_gizmo_handle(x, y, vp, vw, vh, pivot, axes, cam_r, 0.42, 0.425);
                }
                if active_gizmo == GizmoAxis::None && matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move) {
                    let nx = (x / vw) * 2.0 - 1.0;
                    let ny = 1.0 - (y / vh) * 2.0;
                    let (_vp2, inv_vp, _) = s.main_camera.vp_and_inv(vw / vh);
                    let pose = sample_clip_pose(s.active_clip(), s.current_time);
                    let mats = evaluate_rig_matrices(&pose);
                    if let Some(picked) = pick_bone_ray(nx, ny, inv_vp, &mats, s.solo_weapon) {
                        if !s.hidden_bones[picked] {
                            s.active_bone_idx = picked;
                            active_gizmo = GizmoAxis::Free;
                            s.dirty_main = true;
                        }
                    }
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
            s.sync_ui(&w);
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
            if s.drag.active || !matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move) {
                if s.hovered_gizmo != GizmoAxis::None {
                    s.hovered_gizmo = GizmoAxis::None;
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
                }
                return;
            }
            let (pivot, axes, vp) = s.active_gizmo_geom();
            let (cam_r, _, _) = camera_basis(s.main_camera.yaw, s.main_camera.pitch);
            let next_hl = pick_gizmo_handle(x, y, vp, vw, vh, pivot, axes, cam_r, 0.42, 0.425);
            if next_hl != s.hovered_gizmo {
                s.hovered_gizmo = next_hl;
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_vp_pointer_move(move |x, y, mod_pan, mod_ctrl, vw, vh| {
            if let Some(w) = w_weak.upgrade() {
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
                    && matches!(s.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move)
                    && !mod_pan
                    && !mod_ctrl;

                if mod_ctrl {
                    // Ultra-smooth pixel-level analog dolly zoom (Ctrl + vertical drag!)
                    let next_d = (s.main_camera.dist * (dy * 0.009).exp()).clamp(0.22, 16.0);
                    s.main_camera.dist = next_d;
                    s.main_target_dist = next_d;
                    s.dirty_main = true;
                    w.window().request_redraw();
                    return;
                }

                if is_bone_edit && ((x - s.drag.start_x).hypot(y - s.drag.start_y) > 2.0) {
                    if !s.drag.undo_pushed {
                        s.push_undo();
                        s.drag.undo_pushed = true;
                    }
                    s.playing = false;
                    let (pivot, axes, vp) = s.active_gizmo_geom();
                    let (cam_r, cam_u, cam_f) = camera_basis(s.main_camera.yaw, s.main_camera.pitch);
                    let axis = s.drag.active_gizmo;
                    let tmode = s.tool_mode;
                    let ki = s.editable_kf_index();
                    let bkey = BONE_NAMES[if s.solo_weapon { 5 } else { s.active_bone_idx.min(7) }].to_string();
                    let tr = s.active_clip_mut().keyframes[ki].bones.entry(bkey).or_default();
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
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
                } else if s.drag.button == 2 || ((s.drag.button == 1 || s.drag.button == 0) && mod_pan) {
                    // Free 3D Pan along camera screen right & up axes
                    let (cam_r, cam_u, _) = camera_basis(s.main_camera.yaw, s.main_camera.pitch);
                    let sens = s.main_camera.dist * 0.0018;
                    let pan = cam_r.mul(-dx * sens).add(cam_u.mul(dy * sens));
                    s.main_camera.target[0] = (s.main_camera.target[0] + pan.x).clamp(-6.0, 6.0);
                    s.main_camera.target[1] = (s.main_camera.target[1] + pan.y).clamp(-0.5, 4.0);
                    s.main_camera.target[2] = (s.main_camera.target[2] + pan.z).clamp(-6.0, 6.0);
                    s.main_target_pos = s.main_camera.target;
                    s.dirty_main = true;
                    w.window().request_redraw();
                } else {
                    // Free 3D Orbit (Right-drag anywhere OR Left-drag on empty backdrop / Camera tool)
                    s.main_camera.yaw -= dx * 0.009;
                    s.main_camera.pitch = (s.main_camera.pitch + dy * 0.008).clamp(-1.25, 1.48);
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
                }
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
                s.drag.active = false;
                s.drag.active_gizmo = GizmoAxis::None;
                if dist < 4.5 && btn == 0 && vw > 1.0 && vh > 1.0 {
                    let nx = (x / vw) * 2.0 - 1.0;
                    let ny = 1.0 - (y / vh) * 2.0;
                    let (_vp, inv_vp, _) = s.main_camera.vp_and_inv(vw / vh);
                    let pose = sample_clip_pose(s.active_clip(), s.current_time);
                    let mats = evaluate_rig_matrices(&pose);
                    if let Some(picked) = pick_bone_ray(nx, ny, inv_vp, &mats, s.solo_weapon) {
                        s.active_bone_idx = picked;
                    }
                }
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_vp_wheel(move |mx, my, vw, vh, dy| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.queue_zoom_at_cursor(mx, my, vw, vh, dy);
                let tgt = s.main_target_dist;
                let pos = s.main_target_pos;
                AppState::advance_smooth_camera(&mut s.main_camera, tgt, pos, 0.014);
                s.dirty_main = true;
                w.window().request_redraw();
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
                s.reset_main_camera();
                s.sync_ui(&w);
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
                s.main_target_pos = [0.0, 0.96, 0.0];
                match p {
                    0 => { s.main_camera.yaw = 0.58; s.main_camera.pitch = 0.24; s.main_target_dist = 3.35; }
                    1 => { s.main_camera.yaw = 0.0; s.main_camera.pitch = 0.14; s.main_target_dist = 3.2; }
                    2 => { s.main_camera.yaw = 1.57; s.main_camera.pitch = 0.14; s.main_target_dist = 3.3; }
                    _ => { s.main_camera.yaw = 3.14; s.main_camera.pitch = 0.18; s.main_target_dist = 3.3; }
                }
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.playing = !s.playing;
                if s.playing && s.current_time >= s.active_clip().duration - 0.005 {
                    s.current_time = 0.0;
                    s.hit_stop_triggered = false;
                }
                s.sync_ui(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_jump_edge(move |edge| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                s.current_time = if edge == 0 { 0.0 } else { s.active_clip().duration };
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_cycle_speed(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.speed = if (s.speed - 1.0).abs() < 0.05 { 0.5 } else if (s.speed - 0.5).abs() < 0.05 { 0.25 } else { 1.0 };
                s.sync_ui(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_scrub_timeline(move |frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                s.current_time = (frac.clamp(0.0, 1.0)) * s.active_clip().duration;
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_step_frame(move |delta| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                let dur = s.active_clip().duration.max(0.05);
                let frames = (dur * 60.0).round().max(1.0);
                let cur_f = ((s.current_time / dur) * frames).round() as i32;
                let next_f = (cur_f + delta).clamp(0, frames as i32);
                s.current_time = (next_f as f32 / frames) * dur;
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_jump_kf(move |dir| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                let target_t = if dir < 0 {
                    prev_keyframe_time(s.active_clip(), s.current_time)
                } else {
                    next_keyframe_time(s.active_clip(), s.current_time)
                };
                if let Some(nt) = target_t {
                    s.current_time = nt;
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_click_diamond(move |kf_idx, _track_idx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                s.drag_kf_undo_pending = true;
                if let Some(kf) = s.active_clip().keyframes.get(kf_idx as usize) {
                    s.current_time = kf.time;
                }
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_drag_diamond(move |kf_idx, new_frac| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
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
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_dup_kf_next(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let dur = s.active_clip().duration.max(0.1);
                let pose = sample_clip_pose(s.active_clip(), s.current_time);
                let target_t = (s.current_time + 0.10).min(dur);
                let rounded = (target_t * 60.0).round() / 60.0;
                let clip = s.active_clip_mut();
                if let Some(ex) = clip.keyframes.iter_mut().find(|k| (k.time - rounded).abs() < 0.016) {
                    ex.bones = pose;
                } else {
                    clip.keyframes.push(Keyframe {
                        time: rounded,
                        easing: EasingMode::EaseOut,
                        bones: pose,
                    });
                    clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
                }
                s.current_time = rounded;
                s.dirty_main = true;
                s.sync_ui(&w);
                s.show_toast(&w, format!("Keyframe duplicado en {rounded:.2}s"));
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
                s.sync_ui(&w);
                s.show_toast(&w, if v { "Auto-KF activo".into() } else { "Auto-KF manual (apretá K para guardar)".into() });
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
                s.sync_ui(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_bone_visible(move |bi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let idx = (bi as usize).min(7);
                s.hidden_bones[idx] = !s.hidden_bones[idx];
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.hidden_bones = [false; 8];
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.ensure_kf_at_current();
                s.sync_ui(&w);
                let msg = format!("Keyframe guardado en {:.2}s", s.current_time);
                s.show_toast(&w, msg);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_delete_keyframe_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let t = s.current_time;
                if s.active_clip().keyframes.len() <= 2 {
                    s.show_toast(&w, "Se necesitan al menos 2 keyframes".into());
                    return;
                }
                if let Some(idx) = s.active_clip().keyframes.iter().position(|k| (k.time - t).abs() < 0.025) {
                    s.push_undo();
                    s.active_clip_mut().keyframes.remove(idx);
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    w.window().request_redraw();
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
                s.dirty_main = true;
                s.sync_ui(&w);
                s.show_toast(&w, format!("Impacto marcado en {t:.2}s"));
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
                let bkey = BONE_NAMES[s.active_bone_idx.min(7)].to_string();
                let tr = s.active_clip_mut().keyframes[ki].bones.entry(bkey).or_default();
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = (tr.pos[ax] + delta * 0.25).clamp(-24.0, 24.0),
                    1 => tr.rot[ax] = (tr.rot[ax] + delta * 3.0).clamp(-180.0, 180.0),
                    _ => tr.scale[ax] = (tr.scale[ax] + delta * 0.05).clamp(0.1, 3.0),
                }
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        win.on_scrub_axis_start(move || {
            let mut s = st.borrow_mut();
            s.push_undo();
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_scrub_bone_axis(move |chan, axis, px_dx| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.playing = false;
                let ki = s.editable_kf_index();
                let bkey = BONE_NAMES[s.active_bone_idx.min(7)].to_string();
                let tr = s.active_clip_mut().keyframes[ki].bones.entry(bkey).or_default();
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = (tr.pos[ax] + px_dx * 0.06).clamp(-24.0, 24.0),
                    1 => tr.rot[ax] = (tr.rot[ax] + px_dx * 0.75).clamp(-180.0, 180.0),
                    _ => tr.scale[ax] = (tr.scale[ax] + px_dx * 0.01).clamp(0.1, 3.0),
                }
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_reset_bone_axis(move |chan, axis| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.push_undo();
                let ki = s.editable_kf_index();
                let bkey = BONE_NAMES[s.active_bone_idx.min(7)].to_string();
                let tr = s.active_clip_mut().keyframes[ki].bones.entry(bkey).or_default();
                let ax = axis.clamp(0, 2) as usize;
                match chan {
                    0 => tr.pos[ax] = 0.0,
                    1 => tr.rot[ax] = 0.0,
                    _ => tr.scale[ax] = 1.0,
                }
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_select_bone_idx(move |bi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.active_bone_idx = (bi as usize).min(7);
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }

    // Undo/Redo/Pose Actions
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
                    s.dirty_main = true;
                    s.sync_ui(&w);
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
                    s.dirty_main = true;
                    s.sync_ui(&w);
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
                let bkey = BONE_NAMES[s.active_bone_idx.min(7)].to_string();
                s.active_clip_mut().keyframes[ki].bones.insert(bkey, BoneTransform::default());
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.show_toast(&w, "Pose copiada".into());
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_paste_pose_action(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let Some(cp) = s.clipboard_pose.clone() else {
                    s.show_toast(&w, "Primero copiá una pose".into());
                    return;
                };
                s.push_undo();
                let ki = s.ensure_kf_at_current();
                s.active_clip_mut().keyframes[ki].bones = cp;
                s.dirty_main = true;
                s.sync_ui(&w);
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
                s.dirty_main = true;
                s.sync_ui(&w);
                w.window().request_redraw();
            }
        });
    }

    // Gallery & World Controls
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_filter_gallery(move |fi| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.gallery_filter = fi;
                s.dirty_gallery = true;
                s.sync_ui(&w);
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
                s.dirty_gallery = true;
                s.sync_ui(&w);
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
                    s.dirty_main = true;
                    s.dirty_world = true;
                    s.dirty_gallery = true;
                    s.sync_ui(&w);
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
                    let mut copy = src;
                    copy.id = format!("{}_v{}", copy.id, next_num);
                    copy.name = format!("{} #{}", copy.name, next_num);
                    copy.bc_targets.clear();
                    s.project.clips.push(copy);
                    s.active_clip_idx = s.project.clips.len() - 1;
                    s.current_time = 0.0;
                    s.gallery_thumbs.clear();
                    s.dirty_main = true;
                    s.dirty_gallery = true;
                    s.sync_ui(&w);
                    s.show_toast(&w, "Variación agregada al proyecto".into());
                }
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_toggle_world_play(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.world_playing = !s.world_playing;
                s.sync_ui(&w);
            }
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_cycle_world_camera(move || {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                s.world_camera_preset = (s.world_camera_preset + 1) % 3;
                match s.world_camera_preset {
                    0 => { s.world_camera.yaw = 0.42; s.world_camera.pitch = 0.16; s.world_target_dist = 3.25; }
                    1 => { s.world_camera.yaw = -0.55; s.world_camera.pitch = 0.16; s.world_target_dist = 3.45; }
                    _ => { s.world_camera.yaw = 0.0; s.world_camera.pitch = 0.08; s.world_target_dist = 3.25; }
                }
                s.dirty_world = true;
                w.window().request_redraw();
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
                let clip = create_blank_clip(seq);
                let cname = clip.name.clone();
                s.project.clips.push(clip);
                s.active_clip_idx = s.project.clips.len() - 1;
                s.current_time = 0.0;
                s.playing = false;
                s.gallery_thumbs.clear();
                s.dirty_main = true;
                s.dirty_world = true;
                s.dirty_gallery = true;
                s.sync_ui(&w);
                s.show_toast(&w, format!("{cname} listo para animar frame a frame"));
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
                s.sync_ui(&w);
                s.show_toast(&w, "Escena en blanco lista (modo Blender)".into());
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
                s.sync_ui(&w);
                if added > 0 {
                    s.show_toast(&w, format!("{added} plantillas demo agregadas a la galería"));
                } else {
                    s.show_toast(&w, "Todas las plantillas demo ya están en el proyecto".into());
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
                    s.sync_ui(&w);
                    if count > 0 {
                        s.show_toast(&w, format!("{count} ítem(s) voxelizados e importados"));
                    } else {
                        s.show_toast(&w, "No se encontraron sprites válidos en los archivos".into());
                    }
                    w.window().request_redraw();
                }
            }
        });
    }
    {
        let st = state.clone();
        win.on_world_pointer_down(move |x, y, btn| {
            let mut s = st.borrow_mut();
            s.world_drag = DragSession {
                active: true,
                button: btn,
                start_x: x,
                start_y: y,
                last_x: x,
                last_y: y,
                undo_pushed: false,
                active_gizmo: GizmoAxis::None,
            };
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_world_pointer_move(move |x, y, mod_pan| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                if !s.world_drag.active {
                    return;
                }
                let dx = x - s.world_drag.last_x;
                let dy = y - s.world_drag.last_y;
                s.world_drag.last_x = x;
                s.world_drag.last_y = y;
                if s.world_drag.button == 2 || mod_pan {
                    let sens = s.world_camera.dist * 0.0022;
                    AppState::move_camera_free(&mut s.world_camera, 0.0, -dx * sens, dy * sens);
                } else {
                    s.world_camera.yaw -= dx * 0.011;
                    s.world_camera.pitch = (s.world_camera.pitch + dy * 0.009).clamp(-0.35, 1.35);
                }
                s.dirty_world = true;
                w.window().request_redraw();
            }
        });
    }
    {
        let st = state.clone();
        win.on_world_pointer_up(move || {
            st.borrow_mut().world_drag.active = false;
        });
    }
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        win.on_world_wheel(move |dy| {
            if let Some(w) = w_weak.upgrade() {
                let mut s = st.borrow_mut();
                let mut tgt = s.world_target_dist;
                let mut cam = s.world_camera.clone();
                AppState::queue_smooth_zoom(&mut tgt, &mut cam, dy, 0.75, 12.0, false);
                s.world_camera = cam;
                s.world_target_dist = tgt;
                AppState::advance_smooth_zoom(&mut s.world_camera, tgt, 0.014);
                s.dirty_world = true;
                w.window().request_redraw();
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
                "del" => w.invoke_delete_keyframe_action(),
                "m" => w.invoke_mirror_pose_action(),
                "f" => w.invoke_focus_bone(),
                "r" => w.invoke_reset_camera(),
                "z" if ctrl && !shift => w.invoke_undo_action(),
                "y" | "z" if ctrl => w.invoke_redo_action(),
                "c" if ctrl => w.invoke_copy_pose_action(),
                "v" if ctrl => w.invoke_paste_pose_action(),
                "1" => { w.set_active_tool(0); w.invoke_select_tool(0); }
                "2" => { w.set_active_tool(1); w.invoke_select_tool(1); }
                "3" => { w.set_active_tool(2); w.invoke_select_tool(2); }
                "4" => { w.set_active_tool(3); w.invoke_select_tool(3); }
                "5" => { w.set_active_tool(4); w.invoke_select_tool(4); }
                "w" => st.borrow_mut().held_nav_keys[0] = true,
                "s" => st.borrow_mut().held_nav_keys[1] = true,
                "a" => st.borrow_mut().held_nav_keys[2] = true,
                "d" => st.borrow_mut().held_nav_keys[3] = true,
                "e" => st.borrow_mut().held_nav_keys[4] = true,
                "q" => st.borrow_mut().held_nav_keys[5] = true,
                "=" | "+" => st.borrow_mut().held_nav_keys[6] = true,
                "-" | "_" => st.borrow_mut().held_nav_keys[7] = true,
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
                match export_project(&s.project, &s.workspace_root, s.custom_export_dest.as_deref(), fmt, inc_ev, opt) {
                    Ok(res) => {
                        s.show_toast(
                            &w,
                            format!("Exportado ({} clips · {} eventos) · Listo para Minecraft F3+T", res.clips_exported, res.event_timings),
                        );
                    }
                    Err(e) => s.show_toast(&w, format!("Error exportando: {e}")),
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
                        s.show_toast(&w, format!("Tira 6 frames guardada: exports/{name}"));
                    }
                    Err(e) => s.show_toast(&w, format!("Error PNG: {e}")),
                }
            }
        });
    }

    // 144Hz GPU Frame Timer (zero CPU spin when idle!)
    let timer = slint::Timer::default();
    {
        let st = state.clone();
        let w_weak = win.as_weak();
        let cap_path = auto_capture_path.clone();
        let mut frame_ticks = 0u32;
        timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(7), move || {
            let Some(w) = w_weak.upgrade() else { return };
            let mut s = st.borrow_mut();
            let now = Instant::now();
            let dt = (now - s.last_tick).as_secs_f32().min(0.05);
            s.last_tick = now;
            s.clock_sec += dt;

            if let Some(until) = s.toast_until {
                if now >= until {
                    s.toast_until = None;
                    w.set_toast_visible(false);
                }
            }

            if s.impact_flash > 0.0 {
                s.impact_flash = (s.impact_flash - dt * 4.5).max(0.0);
                s.dirty_main = true;
            }

            let mut need_redraw = false;
            // Smooth 144Hz continuous WASD/QE/+/- navigation
            let fwd = (s.held_nav_keys[0] as i32 - s.held_nav_keys[1] as i32) as f32;
            let rgt = (s.held_nav_keys[3] as i32 - s.held_nav_keys[2] as i32) as f32;
            let upk = (s.held_nav_keys[4] as i32 - s.held_nav_keys[5] as i32) as f32;
            if fwd != 0.0 || rgt != 0.0 || upk != 0.0 {
                let spd = (s.main_camera.dist * 0.95).clamp(1.2, 4.0) * dt;
                AppState::move_camera_free(&mut s.main_camera, fwd * spd, rgt * spd, upk * spd);
                s.main_target_pos = s.main_camera.target;
                s.dirty_main = true;
                need_redraw = true;
            }
            if s.held_nav_keys[6] || s.held_nav_keys[7] {
                let dir = if s.held_nav_keys[6] { 1.0 } else { -1.0 };
                let mut tgt = s.main_target_dist;
                tgt = (tgt * (-dir * 1.9 * dt).exp()).clamp(0.18, 16.0);
                s.main_target_dist = tgt;
            }

            let m_tgt = s.main_target_dist;
            let m_pos = s.main_target_pos;
            if AppState::advance_smooth_camera(&mut s.main_camera, m_tgt, m_pos, dt) {
                s.dirty_main = true;
                need_redraw = true;
            }
            let w_tgt = s.world_target_dist;
            if AppState::advance_smooth_zoom(&mut s.world_camera, w_tgt, dt) {
                s.dirty_world = true;
                need_redraw = true;
            }

            if s.playing {
                if s.hit_stop_timer > 0.0 {
                    s.hit_stop_timer -= dt;
                } else {
                    let prev_t = s.current_time;
                    let dur = s.active_clip().duration.max(0.05);
                    s.current_time += dt * s.speed;
                    if let Some(imp) = s.active_clip().impact_time {
                        if !s.hit_stop_triggered && prev_t < imp && s.current_time >= imp {
                            s.current_time = imp;
                            s.hit_stop_timer = 0.06;
                            s.impact_flash = 1.0;
                            s.hit_stop_triggered = true;
                        }
                    }
                    if s.current_time >= dur {
                        if s.active_clip().looped {
                            s.current_time = 0.0;
                            s.hit_stop_triggered = false;
                        } else {
                            s.current_time = dur;
                            s.playing = false;
                        }
                    }
                    s.dirty_main = true;
                    s.sync_ui(&w);
                    need_redraw = true;
                }
            } else if s.world_playing && !s.dirty_main && !s.drag.active {
                // Throttle background world preview to ~30Hz and yield completely while zooming/orbiting main view
                if (now - s.world_last_render).as_secs_f32() >= 0.032 {
                    s.world_last_render = now;
                    s.dirty_world = true;
                    need_redraw = true;
                }
            }

            if need_redraw || s.dirty_main {
                w.window().request_redraw();
            }

            if let Some(ref p) = cap_path {
                frame_ticks += 1;
                if frame_ticks == 34 {
                    if let Ok(snap) = w.window().take_snapshot() {
                        let width = snap.width();
                        let height = snap.height();
                        if let Some(img) = image::RgbaImage::from_raw(width, height, snap.as_bytes().to_vec()) {
                            let _ = img.save(p);
                        }
                    }
                    let _ = w.hide();
                    let _ = slint::quit_event_loop();
                }
            }
        });
    }

    win.run().expect("Event loop error");
    #[cfg(windows)]
    unsafe {
        timeEndPeriod(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_clean_in_blender_blank_mode_and_loads_opt_in_presets() {
        let tmp = std::env::temp_dir().join("vm_blank_startup_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut s = AppState::new(tmp.clone());
        assert_eq!(s.project.clips.len(), 1, "Clean startup must have 1 blank clip like Blender");
        assert_eq!(s.project.clips[0].id, "action_1");
        assert!(s.project.clips[0].impact_time.is_none());

        let added = s.load_demo_presets();
        assert!(added >= 10, "Opt-in demo preset pack must append all sample clips");
        assert!(s.project.clips.len() >= 11);

        s.reset_to_blank_scene();
        assert_eq!(s.project.clips.len(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn samples_virtual_waist_and_optimizes_tracks() {
        let clips = build_demo_presets();
        assert!(clips.len() >= 15);
        for clip in &clips {
            let pose = sample_clip_pose(clip, clip.impact_time.unwrap_or(0.15));
            let mats = evaluate_rig_matrices(&pose);
            for m in mats {
                assert!(m.m.iter().all(|v| v.is_finite()));
            }
        }
        let samples = vec![(0.0, [0.0, 0.0, 0.0]), (0.25, [25.0, 0.0, 0.0]), (0.5, [50.0, 0.0, 0.0])];
        let opt = model::optimize_track_points(&samples, 0.5);
        assert_eq!(opt.len(), 2, "Collinear midpoint must be pruned cleanly");
    }

    #[test]
    fn generates_starter_weapons_imports_custom_png_and_exports_both_formats() {
        let weapons = catalog::build_starter_weapons();
        assert!(weapons.len() >= 8, "Should provide starter procedural voxel weapons");
        assert!(weapons[0].icon_rgba.is_some());

        let (iw, ih, raw) = weapons[0].icon_rgba.as_ref().unwrap();
        let mesh = voxelizer::voxelize_weapon_sprite(*iw, *ih, raw, &weapons[0].mesh_archetype);
        assert!(!mesh.indices.is_empty());

        let tmp = std::env::temp_dir().join("vm_import_export_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // Test user custom PNG weapon import + automatic voxelization selection
        let custom_png = tmp.join("cobalt_katana.png");
        let (_, _, katana_px) = catalog::make_archetype_sprite("katana", [60, 140, 240]);
        image::RgbaImage::from_raw(16, 16, katana_px).unwrap().save(&custom_png).unwrap();

        let mut st = AppState::new(tmp.clone());
        let before = st.weapons.len();
        let imported = st.import_weapon_paths(&[custom_png]);
        assert_eq!(imported, 1);
        assert_eq!(st.weapons.len(), before + 1);
        assert_eq!(st.active_weapon().name, "Cobalt Katana");

        let export_dir = tmp.join("out");
        let proj = AnimatorProject { version: 1, clips: build_demo_presets() };
        let res = export_project(&proj, &tmp, Some(&export_dir), ExportFormat::Both, true, true).unwrap();
        assert_eq!(res.clips_exported, proj.clips.len());
        assert!(export_dir.join("voxelmotion.animation.json").is_file());
        assert!(export_dir.join("voxelmotion_geckolib.animation.json").is_file());
        assert!(export_dir.join("voxelmotion_timings.json").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn filters_library_gallery_and_edits_keyframes_cleanly() {
        let root = detect_workspace_root();
        let mut s = AppState::new(root);
        let total = s.weapons.len();
        assert!(total >= 8);

        // Category + search filter narrowing
        s.selected_category = 2; // Katanas
        s.rebuild_weapon_filter();
        assert!(s.filtered_indices.len() <= total);
        assert!(!s.filtered_indices.is_empty());

        s.weapon_search = "katana".into();
        s.rebuild_weapon_filter();
        assert!(!s.filtered_indices.is_empty());

        s.selected_category = 0;
        s.weapon_search.clear();
        s.rebuild_weapon_filter();
        assert_eq!(s.filtered_indices.len(), total);

        // Keyframe insert + mirror pose symmetry across L/R arms
        s.current_time = 0.12;
        let ki = s.ensure_kf_at_current();
        assert!(ki < s.active_clip().keyframes.len());
        let p = sample_clip_pose(s.active_clip(), 0.12);
        let mir = mirror_pose(&p);
        assert!((mir["left_arm"].rot[1] + p["right_arm"].rot[1]).abs() < 1e-4);
        assert!((mir["right_arm"].rot[2] + p["left_arm"].rot[2]).abs() < 1e-4);

        // All easing curves produce valid finite transforms across endpoints
        let snap_clip = s.project.clips[0].clone();
        let p0 = sample_clip_pose(&snap_clip, 0.0);
        let pend = sample_clip_pose(&snap_clip, snap_clip.duration);
        assert!(p0["right_arm"].rot[0].is_finite());
        assert!(pend["right_arm"].rot[0].is_finite());
    }

    #[test]
    fn gizmo_handles_free_camera_and_auto_kf_work_accurately() {
        let root = detect_workspace_root();
        let mut s = AppState::new(root);

        // Blank clip creation + next/prev KF jump
        let blank = create_blank_clip(99);
        assert_eq!(blank.keyframes.len(), 2);
        s.project.clips.push(blank);
        s.active_clip_idx = s.project.clips.len() - 1;
        assert!(next_keyframe_time(s.active_clip(), 0.0).is_some());
        assert!(prev_keyframe_time(s.active_clip(), 0.6).is_some());

        // Auto-KF ON creates new keyframe at frame; Auto-KF OFF updates nearest without adding
        let orig_len = s.active_clip().keyframes.len();
        s.current_time = 0.25;
        s.auto_keyframe = false;
        let _idx_manual = s.editable_kf_index();
        assert_eq!(s.active_clip().keyframes.len(), orig_len);
        s.auto_keyframe = true;
        let _idx_auto = s.editable_kf_index();
        assert_eq!(s.active_clip().keyframes.len(), orig_len + 1);

        // Gizmo screen projection & X/Y/Z/Roll/Free drag transform
        let (pivot, axes, vp) = s.active_gizmo_geom();
        let (cam_r, cam_u, cam_f) = camera_basis(s.main_camera.yaw, s.main_camera.pitch);
        let center_px = math3d::project_to_screen(vp, pivot, 900.0, 520.0).unwrap();
        let pick_c = pick_gizmo_handle(center_px.0, center_px.1, vp, 900.0, 520.0, pivot, axes, cam_r, 0.42, 0.425);
        assert_eq!(pick_c, GizmoAxis::Free);

        let y_tip = math3d::project_to_screen(vp, pivot.add(axes[1].mul(0.35)), 900.0, 520.0).unwrap();
        let pick_y = pick_gizmo_handle(y_tip.0, y_tip.1, vp, 900.0, 520.0, pivot, axes, cam_r, 0.42, 0.425);
        assert_eq!(pick_y, GizmoAxis::Y);

        let mut bt = BoneTransform::default();
        apply_gizmo_drag(&mut bt, ToolMode::Rotate, GizmoAxis::Y, axes, cam_r, cam_u, cam_f, vp, 900.0, 520.0, pivot, 400.0, 260.0, 440.0, 260.0);
        assert!(bt.rot[1].abs() > 1.0 && bt.rot[0] == 0.0 && bt.rot[2] == 0.0);

        apply_gizmo_drag(&mut bt, ToolMode::Move, GizmoAxis::Free, axes, cam_r, cam_u, cam_f, vp, 900.0, 520.0, pivot, 400.0, 260.0, 420.0, 240.0);
        assert!(bt.pos.iter().any(|v| v.abs() > 0.01));

        // Free camera move & reset
        let before = s.main_camera.target;
        AppState::move_camera_free(&mut s.main_camera, 0.5, 0.2, 0.1);
        assert_ne!(s.main_camera.target, before);
        s.reset_main_camera();
        assert_eq!(s.main_camera.target, [0.0, 0.96, 0.0]);

        // Smooth Zoom-to-Cursor onto limb + Focus active bone + 144Hz camera damping
        s.active_bone_idx = 3; // BrazoDerecho
        let arm_px = {
            let (pivot, _, vp) = s.active_gizmo_geom();
            math3d::project_to_screen(vp, pivot, 900.0, 520.0).unwrap()
        };
        let orig_tgt = s.main_target_pos;
        s.queue_zoom_at_cursor(arm_px.0, arm_px.1, 900.0, 520.0, 60.0);
        assert!(s.main_target_dist < 3.35 && s.main_target_pos != orig_tgt);

        s.focus_active_bone();
        assert_eq!(s.main_target_dist, 0.95);
        for _ in 0..50 {
            let td = s.main_target_dist;
            let tp = s.main_target_pos;
            AppState::advance_smooth_camera(&mut s.main_camera, td, tp, 0.016);
        }
        assert!((s.main_camera.dist - 0.95).abs() < 0.002);

        // Smooth exponential wheel zoom (touchpad micro delta vs 60px mouse notch + monotonic convergence)
        let mut tgt = 3.35f32;
        let mut zcam = s.main_camera.clone();
        AppState::queue_smooth_zoom(&mut tgt, &mut zcam, 12.0, 0.22, 16.0, true);
        assert!(tgt < 3.35 && tgt > 3.10, "Touchpad delta should produce fine micro zoom");
        AppState::queue_smooth_zoom(&mut tgt, &mut zcam, 60.0, 0.22, 16.0, true);
        let target_final = tgt;
        for _ in 0..40 {
            AppState::advance_smooth_zoom(&mut zcam, target_final, 0.016);
        }
        assert!((zcam.dist - target_final).abs() < 0.002);
    }
}
