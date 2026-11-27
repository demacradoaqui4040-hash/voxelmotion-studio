// VoxelMotion Studio - Application Domain State, Dirty Tracking & Undo History

use crate::camera::ViewportCameraController;
use crate::catalog;
use crate::exporter::load_saved_project;
use crate::math3d::{evaluate_rig_matrices, gizmo_axes_world, GizmoAxis, Mat4, Vec3};
use crate::model::{
    build_demo_presets, create_blank_clip, sample_clip_pose, snap_clip_to_frame_grid,
    snap_time_to_frame, AnimatorProject, BoneTransform, ClipDef, ClipSource, CurationStatus,
    EasingMode, GalleryGroup, Keyframe, RigPose, SpritePixels, ToolMode, VfxKind, WeaponCategory,
    WeaponRecord, ALL_BONES_MASK, BONE_COUNT,
};
use crate::modpack::{
    discover_servernuevo_root, load_all_combat_animations, load_curated_weapons, load_local_profile,
    save_local_profile, LocalProfile,
};
use crate::renderer3d::{CameraState, GpuRenderer, RenderParams};
use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

const MAX_UNDO_STEPS: usize = 50;
const VISIBLE_THUMB_WINDOW: usize = 20;
const MAX_THUMB_CACHE: usize = 48;

#[derive(Clone, Copy, Debug)]
pub struct BoneClipFrame {
    pub dt: f32,
    pub easing: EasingMode,
    pub transform: BoneTransform,
}

#[derive(Clone, Debug)]
pub struct BoneClipboard {
    pub source_bone_idx: usize,
    pub frames: Vec<BoneClipFrame>,
    pub is_full_track: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DragSession {
    pub active: bool,
    pub button: i32,
    pub start_x: f32,
    pub start_y: f32,
    pub last_x: f32,
    pub last_y: f32,
    pub undo_pushed: bool,
    pub active_gizmo: GizmoAxis,
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

/// Granular dirty flags preventing Slint UI model rebuilds during camera orbit, pan, and zoom.
#[derive(Clone, Copy, Debug, Default)]
pub struct DirtyFlags {
    pub main_viewport: bool,
    pub card_viewport: bool,
    pub catalog: bool,
    pub gallery: bool,
    pub triad: bool,
    pub inspector: bool,
    pub transform_readout: bool,
    pub timeline_tracks: bool,
    pub timeline_diamonds: bool,
    pub timeline_meta: bool,
    pub status_pill: bool,
}

impl DirtyFlags {
    pub fn all() -> Self {
        Self {
            main_viewport: true,
            card_viewport: true,
            catalog: true,
            gallery: true,
            triad: true,
            inspector: true,
            transform_readout: true,
            timeline_tracks: true,
            timeline_diamonds: true,
            timeline_meta: true,
            status_pill: true,
        }
    }

    pub fn any_ui(&self) -> bool {
        self.catalog
            || self.gallery
            || self.triad
            || self.inspector
            || self.transform_readout
            || self.timeline_tracks
            || self.timeline_diamonds
            || self.timeline_meta
            || self.status_pill
    }
}

pub struct AppState {
    pub workspace_root: PathBuf,
    pub profile: LocalProfile,
    pub servernuevo_root: Option<PathBuf>,
    pub curated_manifest_count: usize,
    pub profile_dirty: bool,
    pub last_profile_save: Instant,
    pub project: AnimatorProject,
    pub weapons: Vec<WeaponRecord>,
    pub weapon_icons: Vec<Image>,
    pub filtered_indices: Vec<usize>,
    pub active_weapon_idx: usize,
    pub active_clip_idx: usize,
    pub active_bone_idx: usize,
    pub selected_category: usize,
    pub weapon_status_filter: i32,
    pub weapon_search: String,
    pub gallery_filter: i32,
    pub gallery_source_filter: i32,
    pub gallery_status_filter: i32,
    pub gallery_compat_only: bool,
    pub gallery_search: String,

    pub current_time: f32,
    pub playing: bool,
    pub speed: f32,
    pub tool_mode: ToolMode,
    pub show_gizmo: bool,
    pub solo_weapon: bool,
    pub shadows: bool,
    pub onion: bool,
    pub trail: bool,
    pub bg_mode: i32,
    pub auto_keyframe: bool,
    pub show_all_bones: bool,
    pub timeline_bone_scope: bool,
    pub timeline_zoom: f32,
    pub timeline_view_start: f32,
    pub hidden_bones: [bool; BONE_COUNT],
    pub hovered_gizmo: GizmoAxis,

    pub main_cam: ViewportCameraController,

    pub clipboard_pose: Option<RigPose>,
    pub clipboard_bone: Option<BoneClipboard>,
    pub selected_range_bone: Option<usize>,
    pub selected_range_times: Option<(f32, f32)>,
    pub undo_stack: Vec<ClipDef>,
    pub redo_stack: Vec<ClipDef>,

    pub vp_size: (u32, u32),
    pub gpu: Option<GpuRenderer>,
    pub main_img: Option<Image>,
    pub card_img: Option<Image>,
    pub gallery_thumbs: HashMap<String, Image>,
    pub thumb_lru: VecDeque<String>,
    pub theme_idx: i32,
    pub ui_motion_enabled: bool,
    pub gpu_low_power: bool,
    pub focus_mode: bool,
    pub palette_open: bool,
    pub palette_query: String,
    pub dirty: DirtyFlags,
    pub last_exact_kf: Option<usize>,

    pub drag: DragSession,
    pub drag_kf_undo_pending: bool,
    pub held_nav_keys: [bool; 8], // 0:W, 1:S, 2:A, 3:D, 4:E, 5:Q, 6:+, 7:-
    pub last_render_tick: Instant,
    pub last_interaction_at: Instant,
    pub present_frames_total: u32,
    pub gpu_pass_us_accum: u64,
    pub gpu_pass_samples: u32,
    pub hit_stop_timer: f32,
    pub impact_flash: f32,
    pub hit_stop_triggered: bool,
    pub last_vfx_trigger_t: f32,
    pub clock_sec: f32,
    pub custom_export_dest: Option<PathBuf>,
    pub toast_until: Option<Instant>,
}

pub fn detect_workspace_root() -> PathBuf {
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

pub fn rgba_to_slint_image(w: u32, h: u32, raw: &[u8]) -> Image {
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

pub fn sprite_to_slint_image(sprite: &SpritePixels) -> Image {
    rgba_to_slint_image(sprite.width, sprite.height, &sprite.rgba)
}

pub fn bone_color(idx: usize) -> Color {
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
    pub fn new(workspace_root: PathBuf) -> Self {
        let mut profile = load_local_profile(&workspace_root);
        let servernuevo_root = discover_servernuevo_root(&workspace_root, &profile);
        if let Some(ref sr) = servernuevo_root {
            if profile.servernuevo_root.is_none() {
                profile.servernuevo_root = Some(sr.to_string_lossy().to_string());
            }
        }

        let (weapons, curated_manifest_count, mut project) = if let Some(ref sr) = servernuevo_root {
            let (ws, raw_cnt) = load_curated_weapons(sr, &profile)
                .unwrap_or_else(|_| {
                    let s = catalog::build_starter_weapons();
                    let l = s.len();
                    (s, l)
                });
            let clips = load_all_combat_animations(sr, &profile);
            let final_clips = if clips.is_empty() {
                vec![create_blank_clip(1)]
            } else {
                clips
            };
            (ws, raw_cnt, AnimatorProject { version: 1, clips: final_clips })
        } else {
            let proj = load_saved_project(&workspace_root).unwrap_or_else(|| AnimatorProject {
                version: 1,
                clips: vec![create_blank_clip(1)],
            });
            let ws = catalog::build_starter_weapons();
            let raw_cnt = ws.len();
            (ws, raw_cnt, proj)
        };
        for c in &mut project.clips {
            snap_clip_to_frame_grid(c);
        }

        let weapon_icons = weapons
            .iter()
            .map(|w| match &w.icon_rgba {
                Some(sp) => sprite_to_slint_image(sp),
                None => Image::default(),
            })
            .collect();
        let filtered_indices = (0..weapons.len()).collect();
        let custom_export_dest = servernuevo_root.as_ref().map(|sr| {
            sr.join("src")
                .join("main")
                .join("resources")
                .join("assets")
                .join("rellseas")
                .join("animations")
                .join("combat")
        });

        let theme_idx = profile.theme_idx.clamp(0, 4);
        let ui_motion_enabled = profile.ui_motion_enabled;
        let gpu_low_power = profile.gpu_low_power;

        Self {
            workspace_root,
            profile,
            servernuevo_root,
            curated_manifest_count,
            profile_dirty: false,
            last_profile_save: Instant::now(),
            project,
            weapons,
            weapon_icons,
            filtered_indices,
            active_weapon_idx: 0,
            active_clip_idx: 0,
            active_bone_idx: 3, // BrazoDerecho
            selected_category: 0,
            weapon_status_filter: 0,
            weapon_search: String::new(),
            gallery_filter: 0,
            gallery_source_filter: 0,
            gallery_status_filter: 0,
            gallery_compat_only: false,
            gallery_search: String::new(),
            current_time: 0.0,
            playing: false,
            speed: 1.0,
            tool_mode: ToolMode::Rotate,
            show_gizmo: true,
            solo_weapon: false,
            shadows: true,
            onion: true,
            trail: true,
            bg_mode: 0,
            auto_keyframe: true,
            show_all_bones: false,
            timeline_bone_scope: true,
            timeline_zoom: 1.0,
            timeline_view_start: 0.0,
            hidden_bones: [false; BONE_COUNT],
            hovered_gizmo: GizmoAxis::None,
            main_cam: ViewportCameraController::new_main(),
            clipboard_pose: None,
            clipboard_bone: None,
            selected_range_bone: None,
            selected_range_times: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            vp_size: (900, 520),
            gpu: None,
            main_img: None,
            card_img: None,
            gallery_thumbs: HashMap::new(),
            thumb_lru: VecDeque::new(),
            theme_idx,
            ui_motion_enabled,
            gpu_low_power,
            focus_mode: false,
            palette_open: false,
            palette_query: String::new(),
            dirty: DirtyFlags::all(),
            last_exact_kf: None,
            drag: DragSession::default(),
            drag_kf_undo_pending: false,
            held_nav_keys: [false; 8],
            last_render_tick: Instant::now(),
            last_interaction_at: Instant::now(),
            present_frames_total: 0,
            gpu_pass_us_accum: 0,
            gpu_pass_samples: 0,
            hit_stop_timer: 0.0,
            impact_flash: 0.0,
            hit_stop_triggered: false,
            last_vfx_trigger_t: -1.0,
            clock_sec: 0.0,
            custom_export_dest,
            toast_until: None,
        }
    }

    #[inline]
    pub fn active_weapon(&self) -> &WeaponRecord {
        &self.weapons[self.active_weapon_idx.min(self.weapons.len().saturating_sub(1))]
    }

    #[inline]
    pub fn active_clip(&self) -> &ClipDef {
        &self.project.clips[self.active_clip_idx.min(self.project.clips.len().saturating_sub(1))]
    }

    #[inline]
    pub fn active_clip_mut(&mut self) -> &mut ClipDef {
        let idx = self.active_clip_idx.min(self.project.clips.len().saturating_sub(1));
        &mut self.project.clips[idx]
    }

    #[inline]
    pub fn any_nav_key_held(&self) -> bool {
        self.held_nav_keys.iter().any(|&k| k)
    }

    #[inline]
    pub fn note_interaction(&mut self) {
        self.last_interaction_at = Instant::now();
    }

    /// Only mark timeline diamonds dirty when playhead moves onto or off an exact keyframe.
    #[inline]
    pub fn update_exact_kf_dirty(&mut self) {
        let cur_exact = self
            .active_clip()
            .keyframes
            .iter()
            .position(|k| (k.time - self.current_time).abs() < 0.016);
        if cur_exact != self.last_exact_kf {
            self.last_exact_kf = cur_exact;
            self.dirty.timeline_diamonds = true;
        }
    }

    #[inline]
    pub fn needs_continuous_redraw(&self) -> bool {
        self.playing
            || self.main_cam.is_animating()
            || self.any_nav_key_held()
            || self.impact_flash > 0.0
    }

    pub fn push_undo(&mut self) {
        let snap = self.active_clip().clone();
        self.undo_stack.push(snap);
        if self.undo_stack.len() > MAX_UNDO_STEPS {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn timeline_view_span(&self) -> f32 {
        (1.0 / self.timeline_zoom.clamp(1.0, 8.0)).clamp(0.125, 1.0)
    }

    pub fn set_timeline_zoom(&mut self, next_zoom: f32, anchor_screen_frac: f32) {
        let old_span = self.timeline_view_span();
        let anchor_s = anchor_screen_frac.clamp(0.0, 1.0);
        let anchor_norm = (self.timeline_view_start + anchor_s * old_span).clamp(0.0, 1.0);
        self.timeline_zoom = next_zoom.clamp(1.0, 8.0);
        let new_span = self.timeline_view_span();
        let max_start = (1.0 - new_span).max(0.0);
        self.timeline_view_start = (anchor_norm - anchor_s * new_span).clamp(0.0, max_start);
        self.dirty.timeline_meta = true;
        self.dirty.timeline_tracks = true;
        self.dirty.timeline_diamonds = true;
    }

    pub fn pan_timeline_view(&mut self, delta_screen_frac: f32) {
        let span = self.timeline_view_span();
        let max_start = (1.0 - span).max(0.0);
        self.timeline_view_start =
            (self.timeline_view_start + delta_screen_frac * span).clamp(0.0, max_start);
        self.dirty.timeline_meta = true;
        self.dirty.timeline_tracks = true;
        self.dirty.timeline_diamonds = true;
    }

    pub fn ensure_playhead_in_view(&mut self) {
        if self.timeline_zoom <= 1.01 {
            return;
        }
        let dur = self.active_clip().duration.max(0.05);
        let p = (self.current_time / dur).clamp(0.0, 1.0);
        let span = self.timeline_view_span();
        let max_start = (1.0 - span).max(0.0);
        if p < self.timeline_view_start {
            self.timeline_view_start = (p - span * 0.1).clamp(0.0, max_start);
            self.dirty.timeline_meta = true;
            self.dirty.timeline_tracks = true;
            self.dirty.timeline_diamonds = true;
        } else if p > self.timeline_view_start + span {
            self.timeline_view_start = (p - span * 0.9).clamp(0.0, max_start);
            self.dirty.timeline_meta = true;
            self.dirty.timeline_tracks = true;
            self.dirty.timeline_diamonds = true;
        }
    }

    pub fn clear_bone_selection(&mut self) {
        self.selected_range_bone = None;
        self.selected_range_times = None;
    }

    pub fn ensure_bone_kf_at(&mut self, time: f32, bone_idx: usize, easing: EasingMode) -> usize {
        let b_idx = bone_idx.min(BONE_COUNT - 1);
        let dur = self.active_clip().duration.max(0.05);
        let frames = (dur * 60.0).round().max(1.0);
        let tol = (0.45 * (dur / frames)).min(0.014);
        let target_t = snap_time_to_frame(time, dur);
        let sampled_pose = sample_clip_pose(self.active_clip(), target_t);
        let clip = self.active_clip_mut();
        for (i, k) in clip.keyframes.iter_mut().enumerate() {
            if (k.time - target_t).abs() < tol {
                if !k.has_bone(b_idx) {
                    k.bones.0[b_idx] = sampled_pose.0[b_idx];
                    k.set_bone(b_idx, true);
                }
                return i;
            }
        }
        clip.keyframes.push(Keyframe {
            time: target_t,
            easing,
            bones: sampled_pose,
            bone_mask: 1u8 << b_idx,
        });
        clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        let idx = clip
            .keyframes
            .iter()
            .position(|k| (k.time - target_t).abs() < tol)
            .unwrap_or(0);
        self.dirty.timeline_diamonds = true;
        idx
    }

    pub fn ensure_kf_at_current(&mut self) -> usize {
        let dur = self.active_clip().duration.max(0.05);
        let frames = (dur * 60.0).round().max(1.0);
        let tol = (0.45 * (dur / frames)).min(0.014);
        let cur_t = snap_time_to_frame(self.current_time, dur);
        let pose = sample_clip_pose(self.active_clip(), cur_t);
        let clip = self.active_clip_mut();
        for (i, k) in clip.keyframes.iter_mut().enumerate() {
            if (k.time - cur_t).abs() < tol {
                if k.bone_mask != ALL_BONES_MASK {
                    for bi in 0..BONE_COUNT {
                        if !k.has_bone(bi) {
                            k.bones.0[bi] = pose.0[bi];
                        }
                    }
                    k.bone_mask = ALL_BONES_MASK;
                }
                return i;
            }
        }
        clip.keyframes.push(Keyframe {
            time: cur_t,
            easing: EasingMode::EaseOut,
            bones: pose,
            bone_mask: ALL_BONES_MASK,
        });
        clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
        let idx = clip
            .keyframes
            .iter()
            .position(|k| (k.time - cur_t).abs() < tol)
            .unwrap_or(0);
        self.dirty.timeline_diamonds = true;
        idx
    }

    /// Keyframe index targeted by direct viewport/inspector bone edits: creates per-bone KF if Auto-KF is on,
    /// otherwise snaps cleanly onto the nearest existing keyframe without polluting other bone lanes.
    pub fn editable_kf_index(&mut self) -> usize {
        let cur_t = self.current_time;
        let b_idx = self.active_bone_idx.min(BONE_COUNT - 1);
        let exact = self
            .active_clip()
            .keyframes
            .iter()
            .position(|k| (k.time - cur_t).abs() < 0.016);
        if let Some(ei) = exact {
            let kf_t = self.active_clip().keyframes[ei].time;
            if !self.active_clip().keyframes[ei].has_bone(b_idx) {
                let sampled = sample_clip_pose(self.active_clip(), kf_t).0[b_idx];
                let kf = &mut self.active_clip_mut().keyframes[ei];
                kf.bones.0[b_idx] = sampled;
                kf.set_bone(b_idx, true);
                self.dirty.timeline_diamonds = true;
            }
            return ei;
        }
        if self.auto_keyframe {
            return self.ensure_bone_kf_at(cur_t, b_idx, EasingMode::EaseOut);
        }
        let nearest = self
            .active_clip()
            .keyframes
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.time - cur_t).abs().total_cmp(&(b.time - cur_t).abs()))
            .map(|(i, _)| i)
            .unwrap_or(0);
        if let Some(kf_t) = self.active_clip().keyframes.get(nearest).map(|k| k.time) {
            if !self.active_clip().keyframes[nearest].has_bone(b_idx) {
                let sampled = sample_clip_pose(self.active_clip(), kf_t).0[b_idx];
                let kf = &mut self.active_clip_mut().keyframes[nearest];
                kf.bones.0[b_idx] = sampled;
                kf.set_bone(b_idx, true);
                self.dirty.timeline_diamonds = true;
            }
        }
        nearest
    }

    /// Copy active bone keyframe(s): full track, Shift-selected time range, or single sampled frame at playhead.
    pub fn copy_bone_frames(&mut self, bone_idx: usize, full_track: bool) -> usize {
        let b_idx = bone_idx.min(BONE_COUNT - 1);
        let clip = self.active_clip();
        let mut frames = Vec::new();
        if full_track {
            let mut kfs: Vec<&Keyframe> = clip.keyframes.iter().filter(|k| k.has_bone(b_idx)).collect();
            kfs.sort_by(|a, b| a.time.total_cmp(&b.time));
            if let Some(first) = kfs.first() {
                let base = first.time;
                for k in kfs {
                    frames.push(BoneClipFrame {
                        dt: (k.time - base).max(0.0),
                        easing: k.easing,
                        transform: k.bones.0[b_idx],
                    });
                }
            } else {
                let pose = sample_clip_pose(clip, self.current_time);
                frames.push(BoneClipFrame {
                    dt: 0.0,
                    easing: EasingMode::EaseOut,
                    transform: pose.0[b_idx],
                });
            }
        } else if self.selected_range_bone == Some(b_idx) {
            if let Some((t0, t1)) = self.selected_range_times {
                let lo = t0.min(t1);
                let hi = t0.max(t1);
                let mut kfs: Vec<&Keyframe> = clip
                    .keyframes
                    .iter()
                    .filter(|k| k.has_bone(b_idx) && k.time >= lo - 0.016 && k.time <= hi + 0.016)
                    .collect();
                kfs.sort_by(|a, b| a.time.total_cmp(&b.time));
                if let Some(first) = kfs.first() {
                    let base = first.time;
                    for k in kfs {
                        frames.push(BoneClipFrame {
                            dt: (k.time - base).max(0.0),
                            easing: k.easing,
                            transform: k.bones.0[b_idx],
                        });
                    }
                }
            }
        }

        if frames.is_empty() {
            let cur_t = self.current_time;
            let pose = sample_clip_pose(clip, cur_t);
            let easing = clip
                .keyframes
                .iter()
                .filter(|k| k.has_bone(b_idx))
                .min_by(|a, b| (a.time - cur_t).abs().total_cmp(&(b.time - cur_t).abs()))
                .map(|k| k.easing)
                .unwrap_or(EasingMode::EaseOut);
            frames.push(BoneClipFrame {
                dt: 0.0,
                easing,
                transform: pose.0[b_idx],
            });
        }

        let count = frames.len();
        self.clipboard_bone = Some(BoneClipboard {
            source_bone_idx: b_idx,
            frames,
            is_full_track: full_track,
        });
        count
    }

    /// Paste copied bone frame(s) onto `target_bone_idx` starting at playhead (0.0s for full track near start).
    pub fn paste_bone_frames(&mut self, target_bone_idx: usize, mirror_lr: bool) -> usize {
        let Some(cb) = self.clipboard_bone.clone() else {
            return 0;
        };
        let b_idx = target_bone_idx.min(BONE_COUNT - 1);
        self.push_undo();
        let dur = self.active_clip().duration.max(0.05);
        let base_t = if cb.is_full_track && self.current_time < 0.016 {
            0.0
        } else {
            self.current_time
        };
        let mut first_dst: Option<f32> = None;
        let mut last_dst: Option<f32> = None;
        for f in &cb.frames {
            let dst_t = snap_time_to_frame(base_t + f.dt, dur);
            let mut tr = f.transform;
            if mirror_lr {
                tr.pos[0] = -tr.pos[0];
                tr.rot[1] = -tr.rot[1];
                tr.rot[2] = -tr.rot[2];
            }
            let ki = self.ensure_bone_kf_at(dst_t, b_idx, f.easing);
            let kf = &mut self.active_clip_mut().keyframes[ki];
            kf.bones.0[b_idx] = tr;
            kf.easing = f.easing;
            kf.set_bone(b_idx, true);
            first_dst.get_or_insert(dst_t);
            last_dst = Some(dst_t);
        }
        if cb.frames.len() > 1 {
            if let (Some(a), Some(b)) = (first_dst, last_dst) {
                self.selected_range_bone = Some(b_idx);
                self.selected_range_times = Some((a.min(b), a.max(b)));
            }
        } else {
            self.clear_bone_selection();
        }
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
        self.dirty.timeline_diamonds = true;
        cb.frames.len()
    }

    /// Delete active bone keyframe(s): full bone track, Shift-selected range, or frame at playhead.
    pub fn delete_bone_frames(&mut self, bone_idx: usize, full_track: bool) -> usize {
        let b_idx = bone_idx.min(BONE_COUNT - 1);
        let cur_t = self.current_time;
        let range = if self.selected_range_bone == Some(b_idx) {
            self.selected_range_times
        } else {
            None
        };
        let matching_indices: Vec<usize> = self
            .active_clip()
            .keyframes
            .iter()
            .enumerate()
            .filter_map(|(i, k)| {
                if !k.has_bone(b_idx) {
                    return None;
                }
                if full_track {
                    Some(i)
                } else if let Some((t0, t1)) = range {
                    let (lo, hi) = (t0.min(t1), t0.max(t1));
                    (k.time >= lo - 0.016 && k.time <= hi + 0.016).then_some(i)
                } else {
                    ((k.time - cur_t).abs() < 0.025).then_some(i)
                }
            })
            .collect();
        if matching_indices.is_empty() {
            return 0;
        }
        self.push_undo();
        let clip = self.active_clip_mut();
        for &i in &matching_indices {
            if let Some(k) = clip.keyframes.get_mut(i) {
                k.set_bone(b_idx, false);
                k.bones.0[b_idx] = BoneTransform::IDENTITY;
            }
        }
        let mut idx = clip.keyframes.len();
        while idx > 0 && clip.keyframes.len() > 2 {
            idx -= 1;
            if clip.keyframes[idx].bone_mask == 0 {
                clip.keyframes.remove(idx);
            }
        }
        self.clear_bone_selection();
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
        self.dirty.timeline_diamonds = true;
        matching_indices.len()
    }

    /// Duplicate only the active bone's sampled transform +0.10s forward without affecting other bones.
    pub fn duplicate_bone_frame_next(&mut self, bone_idx: usize) -> f32 {
        let b_idx = bone_idx.min(BONE_COUNT - 1);
        self.push_undo();
        let dur = self.active_clip().duration.max(0.1);
        let tr = sample_clip_pose(self.active_clip(), self.current_time).0[b_idx];
        let rounded = snap_time_to_frame(self.current_time + 0.10, dur);
        let ki = self.ensure_bone_kf_at(rounded, b_idx, EasingMode::EaseOut);
        self.active_clip_mut().keyframes[ki].bones.0[b_idx] = tr;
        self.current_time = rounded;
        self.clear_bone_selection();
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
        self.dirty.timeline_diamonds = true;
        rounded
    }

    pub fn focus_active_bone(&mut self) {
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
        self.main_cam.focus_point(p, 0.95);
        self.dirty.main_viewport = true;
    }

    pub fn queue_zoom_at_cursor(&mut self, mx: f32, my: f32, vw: f32, vh: f32, dy: f32) {
        let pose = sample_clip_pose(self.active_clip(), self.current_time);
        self.main_cam
            .queue_zoom_at_cursor(mx, my, vw, vh, dy, Some(&pose), self.solo_weapon);
        self.dirty.main_viewport = true;
    }

    pub fn active_gizmo_geom(&self) -> (Vec3, [Vec3; 3], Mat4) {
        let aspect = (self.vp_size.0 as f32 / self.vp_size.1.max(1) as f32).max(0.5);
        let (vp, _, _) = self.main_cam.current.vp_and_inv(aspect);
        let pose = if self.is_active_clip_compatible() {
            sample_clip_pose(self.active_clip(), self.current_time)
        } else {
            RigPose::ZERO
        };
        let mats = evaluate_rig_matrices(&pose);
        let bone_idx = if self.solo_weapon { 5 } else { self.active_bone_idx.min(7) };
        let bone_m = if self.solo_weapon {
            let ri = pose.bone(5);
            Mat4::translate(0.0, 0.96, 0.55)
                .mul(Mat4::rot_zyx_deg(ri.rot[0], ri.rot[1], ri.rot[2]))
                .mul(Mat4::rot_y(0.18))
                .mul(Mat4::rot_x(-0.14))
                .mul(Mat4::scale(1.0 / 16.0, 1.0 / 16.0, 1.0 / 16.0))
                .mul(Mat4::rot_x(std::f32::consts::PI))
        } else {
            mats[bone_idx]
        };
        let (pivot, axes) = gizmo_axes_world(bone_m, self.solo_weapon);
        (pivot, axes, vp)
    }

    pub fn rebuild_weapon_filter(&mut self) {
        let cat = WeaponCategory::ALL_SIDEBAR
            .get(self.selected_category)
            .copied()
            .unwrap_or(WeaponCategory::Todas);
        let q = self.weapon_search.trim().to_ascii_lowercase();
        let st_filter = self.weapon_status_filter;
        self.filtered_indices = self
            .weapons
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                let cat_ok = cat == WeaponCategory::Todas || w.category == cat;
                let st_ok = match st_filter {
                    1 => w.status == CurationStatus::Pendiente,
                    2 => w.status == CurationStatus::Aprobada,
                    3 => w.status == CurationStatus::Mejorar,
                    4 => w.status == CurationStatus::Descartada,
                    _ => true,
                };
                let q_ok = q.is_empty()
                    || w.name.to_ascii_lowercase().contains(&q)
                    || w.id.to_ascii_lowercase().contains(&q)
                    || w.mod_source.to_ascii_lowercase().contains(&q);
                cat_ok && st_ok && q_ok
            })
            .map(|(i, _)| i)
            .collect();
    }

    pub fn is_active_clip_compatible(&self) -> bool {
        self.active_clip().allows_weapon(self.active_weapon())
    }

    pub fn first_compatible_weapon_index(&self) -> Option<usize> {
        let clip = self.active_clip();
        self.filtered_indices
            .iter()
            .copied()
            .find(|&idx| self.weapons.get(idx).is_some_and(|w| clip.allows_weapon(w)))
            .or_else(|| self.weapons.iter().position(|w| clip.allows_weapon(w)))
    }

    pub fn toggle_active_clip_family(&mut self, cat: WeaponCategory) {
        if matches!(cat, WeaponCategory::Todas | WeaponCategory::Otros) {
            return;
        }
        self.push_undo();
        {
            let clip = self.active_clip_mut();
            if let Some(pos) = clip.allowed_categories.iter().position(|&c| c == cat) {
                clip.allowed_categories.remove(pos);
            } else {
                clip.allowed_categories.push(cat);
            }
            clip.allowed_categories.sort();
            clip.allowed_categories.dedup();
        }
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.gallery = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
    }

    pub fn set_active_clip_universal(&mut self) {
        self.push_undo();
        {
            let clip = self.active_clip_mut();
            clip.allowed_categories.clear();
            clip.allowed_weapons.clear();
        }
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.gallery = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
    }

    pub fn toggle_active_clip_only_current_weapon(&mut self) {
        let wid = self.active_weapon().id.clone();
        let wcat = self.active_weapon().category;
        self.push_undo();
        {
            let clip = self.active_clip_mut();
            let already_only = clip.allowed_categories.is_empty()
                && clip.allowed_weapons.len() == 1
                && clip.allowed_weapons[0] == wid;
            if already_only {
                clip.allowed_categories = vec![wcat];
                clip.allowed_weapons.clear();
            } else {
                clip.allowed_categories.clear();
                clip.allowed_weapons = vec![wid];
            }
        }
        self.mark_active_clip_modified();
        self.dirty.main_viewport = true;
        self.dirty.gallery = true;
        self.dirty.inspector = true;
        self.dirty.timeline_meta = true;
    }

    pub fn filtered_clip_indices(&self) -> Vec<usize> {
        let gq = self.gallery_search.trim().to_ascii_lowercase();
        let active_w = self.active_weapon();
        self.project
            .clips
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                let grp_ok = match self.gallery_filter {
                    1 => c.gallery_group == GalleryGroup::Combate,
                    2 => c.gallery_group == GalleryGroup::Movimiento,
                    3 => c.gallery_group == GalleryGroup::Defensa,
                    4 => c.gallery_group == GalleryGroup::Especiales,
                    5 => c.gallery_group == GalleryGroup::Interaccion,
                    _ => true,
                };
                let src_ok = match self.gallery_source_filter {
                    1 => c.source == ClipSource::RellSeas,
                    2 => c.source == ClipSource::BetterCombat,
                    3 => c.source == ClipSource::SimplySwords,
                    4 => c.source == ClipSource::CombatRoll,
                    5 => c.source == ClipSource::FluidFoolish,
                    6 => c.source == ClipSource::Personalizado,
                    _ => true,
                };
                let st_ok = match self.gallery_status_filter {
                    1 => c.status == CurationStatus::Pendiente,
                    2 => c.status == CurationStatus::Aprobada,
                    3 => c.status == CurationStatus::Mejorar,
                    4 => c.status == CurationStatus::Descartada,
                    _ => true,
                };
                let compat_ok = !self.gallery_compat_only || c.allows_weapon(active_w);
                let q_ok = gq.is_empty()
                    || c.name.to_ascii_lowercase().contains(&gq)
                    || c.id.to_ascii_lowercase().contains(&gq)
                    || c.target_badge().to_ascii_lowercase().contains(&gq);
                grp_ok && src_ok && st_ok && compat_ok && q_ok
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn mark_active_clip_modified(&mut self) {
        let clip = self.active_clip().clone();
        self.profile.clip_status.insert(clip.id.clone(), clip.status);
        self.profile.clip_vfx.insert(clip.id.clone(), clip.vfx_events.clone());
        self.profile
            .clip_allowed_categories
            .insert(clip.id.clone(), clip.allowed_categories.clone());
        self.profile
            .clip_allowed_weapons
            .insert(clip.id.clone(), clip.allowed_weapons.clone());
        if let Some(imp) = clip.impact_time {
            self.profile.clip_impact_overrides.insert(clip.id.clone(), imp);
        }
        self.profile.clip_modified_clips.insert(clip.id.clone(), clip);
        self.profile_dirty = true;
    }

    pub fn set_weapon_curation_status(&mut self, idx: usize, status: CurationStatus) {
        if let Some(w) = self.weapons.get_mut(idx) {
            w.status = status;
            self.profile.weapon_status.insert(w.id.clone(), status);
            self.profile_dirty = true;
            self.rebuild_weapon_filter();
            self.dirty.catalog = true;
            self.dirty.inspector = true;
        }
    }

    pub fn set_clip_curation_status(&mut self, clip_idx: usize, status: CurationStatus) {
        if let Some(c) = self.project.clips.get_mut(clip_idx) {
            c.status = status;
            let cid = c.id.clone();
            self.profile.clip_status.insert(cid.clone(), status);
            if self.profile.clip_modified_clips.contains_key(&cid) {
                self.profile.clip_modified_clips.insert(cid, c.clone());
            }
            self.profile_dirty = true;
            self.dirty.gallery = true;
            self.dirty.inspector = true;
        }
    }

    pub fn flush_profile_if_dirty(&mut self, force: bool) {
        if !self.profile_dirty {
            return;
        }
        if force {
            if save_local_profile(&self.workspace_root, &self.profile).is_ok() {
                self.profile_dirty = false;
                self.last_profile_save = Instant::now();
            }
        } else if self.last_profile_save.elapsed().as_secs_f32() >= 0.45 {
            self.profile_dirty = false;
            self.last_profile_save = Instant::now();
            let root = self.workspace_root.clone();
            let prof = self.profile.clone();
            std::thread::spawn(move || {
                let _ = save_local_profile(&root, &prof);
            });
        }
    }

    /// Upload the active weapon mesh to GPU without cloning the WeaponRecord or pixel buffer.
    pub fn ensure_active_weapon_gpu(&mut self) {
        let idx = self.active_weapon_idx.min(self.weapons.len().saturating_sub(1));
        let Some(gpu) = self.gpu.as_mut() else { return };
        let w = &self.weapons[idx];
        if let Some(sp) = &w.icon_rgba {
            gpu.ensure_weapon_mesh(&w.id, sp.width, sp.height, &sp.rgba, &w.mesh_archetype);
        }
    }

    pub fn has_pending_visible_thumbs(&self) -> bool {
        let active_id = &self.active_clip().id;
        if !self.gallery_thumbs.contains_key(active_id) {
            return true;
        }
        self.filtered_clip_indices()
            .into_iter()
            .take(VISIBLE_THUMB_WINDOW)
            .any(|idx| !self.gallery_thumbs.contains_key(&self.project.clips[idx].id))
    }

    pub fn generate_gallery_thumbs_batch(&mut self, max_batch: usize) -> bool {
        let active_id = self.active_clip().id.clone();
        let visible = self.filtered_clip_indices();
        let mut todo: Vec<usize> = Vec::new();
        if !self.gallery_thumbs.contains_key(&active_id) {
            todo.push(self.active_clip_idx.min(self.project.clips.len().saturating_sub(1)));
        }
        for idx in visible.into_iter().take(VISIBLE_THUMB_WINDOW) {
            if todo.len() >= max_batch {
                break;
            }
            let cid = &self.project.clips[idx].id;
            if !self.gallery_thumbs.contains_key(cid) && !todo.contains(&idx) {
                todo.push(idx);
            }
        }
        if todo.is_empty() {
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.release_thumb_surface();
            }
            return false;
        }
        self.ensure_active_weapon_gpu();
        let weapon_id = self.active_weapon().id.clone();
        let theme_idx = self.theme_idx as f32;
        let Some(mut gpu) = self.gpu.take() else { return false };
        let thumb_cam = CameraState {
            yaw: 0.45,
            pitch: 0.20,
            dist: 3.1,
            target: [0.0, 0.96, 0.0],
        };
        for idx in todo {
            let c = &self.project.clips[idx];
            let t = c.impact_time.unwrap_or(c.duration * 0.45);
            let pose = sample_clip_pose(c, t);
            let tex = gpu.render_thumb_texture(&RenderParams {
                pose: &pose,
                ghost_pose: None,
                ghost_next_pose: None,
                clip: c,
                current_time: t,
                weapon_id: &weapon_id,
                selected_bone: 99,
                solo_weapon: false,
                hidden_bones: [false; BONE_COUNT],
                hovered_gizmo: GizmoAxis::None,
                active_gizmo: GizmoAxis::None,
                show_shadows: true,
                show_onion: false,
                show_trail: true,
                show_gizmo: false,
                bg_mode: 0.0,
                theme_idx,
                impact_flash: 0.0,
                anim_clock: 0.0,
                camera: &thumb_cam,
                draw_world_backdrop: false,
            });
            let cid = c.id.clone();
            self.gallery_thumbs.insert(cid.clone(), Image::try_from(tex).unwrap_or_default());
            self.thumb_lru.retain(|k| k != &cid);
            self.thumb_lru.push_back(cid);
            while self.gallery_thumbs.len() > MAX_THUMB_CACHE {
                let Some(old) = self.thumb_lru.pop_front() else { break };
                if old == active_id {
                    self.thumb_lru.push_back(old);
                    if self.thumb_lru.len() <= 1 {
                        break;
                    }
                    continue;
                }
                self.gallery_thumbs.remove(&old);
            }
        }
        self.gpu = Some(gpu);
        if !self.has_pending_visible_thumbs() {
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.release_thumb_surface();
            }
        }
        true
    }

    pub fn palette_matches(&self) -> Vec<(String, String, String, String)> {
        let q = self.palette_query.trim().to_ascii_lowercase();
        let mut out: Vec<(String, String, String, String)> = Vec::new();
        let active_w = self.active_weapon();
        for c in &self.project.clips {
            if out.len() >= 18 {
                break;
            }
            if q.is_empty()
                || c.name.to_ascii_lowercase().contains(&q)
                || c.id.to_ascii_lowercase().contains(&q)
                || c.target_badge().to_ascii_lowercase().contains(&q)
            {
                let tag = if c.allows_weapon(active_w) { "OK" } else { "Bloq." };
                out.push((
                    "CLIP".into(),
                    format!("clip:{}", c.id),
                    c.name.clone(),
                    format!("{} · {} ({})", c.source.short_label(), c.target_badge(), tag),
                ));
            }
        }
        for (i, w) in self.weapons.iter().enumerate() {
            if out.len() >= 18 {
                break;
            }
            if q.is_empty()
                || w.name.to_ascii_lowercase().contains(&q)
                || w.category.label().to_ascii_lowercase().contains(&q)
                || w.mod_source.to_ascii_lowercase().contains(&q)
            {
                out.push((
                    "ÍTEM".into(),
                    format!("weapon:{i}"),
                    w.name.clone(),
                    format!("{} · {}", w.category.label(), w.mod_source),
                ));
            }
        }
        let commands = [
            ("cam:reset", "Centrar Cámara", "Atajo: R"),
            ("focus", "Expandir / Enfocar Visor 3D", "Atajo: Tab"),
            ("compat", "Filtrar Solo Clips Compatibles", "Navegador de Clips"),
            ("theme:0", "Estilo: Pro Studio Neutral", "Grafito neutro · Estándar"),
            ("theme:1", "Estilo: Field Hardware", "Aluminio mate · Naranja señal · Mono"),
            ("theme:2", "Estilo: Daylight Studio", "Porcelana clara · 3D neutro"),
            ("theme:3", "Estilo: CAD Blueprint", "Ortogonal 0px · Pizarra cobalto"),
            ("theme:4", "Estilo: Studio Clay", "Piedra cálida · Terracota"),
        ];
        for (id, title, sub) in commands {
            if out.len() >= 18 {
                break;
            }
            if q.is_empty()
                || title.to_ascii_lowercase().contains(&q)
                || sub.to_ascii_lowercase().contains(&q)
            {
                out.push(("CMD".into(), id.into(), title.into(), sub.into()));
            }
        }
        out.truncate(18);
        out
    }

    pub fn generate_gallery_thumbs_if_needed(&mut self) {
        if self.gallery_thumbs.is_empty() {
            self.generate_gallery_thumbs_batch(20);
        }
    }

    pub fn import_weapon_paths(&mut self, paths: &[PathBuf]) -> usize {
        let imported = catalog::import_weapons_from_paths(paths);
        let count = imported.len();
        if count == 0 {
            return 0;
        }
        let mut first_idx = None;
        for w in imported {
            let icon = match &w.icon_rgba {
                Some(sp) => sprite_to_slint_image(sp),
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
        self.dirty.catalog = true;
        self.dirty.main_viewport = true;
        self.dirty.card_viewport = true;
        self.dirty.inspector = true;
        count
    }

    pub fn rename_active_clip(&mut self, raw_name: &str) -> bool {
        let trimmed = raw_name.trim();
        if trimmed.is_empty() || self.active_clip().name == trimmed {
            return false;
        }
        self.active_clip_mut().name = trimmed.to_string();
        self.mark_active_clip_modified();
        self.dirty.gallery = true;
        self.dirty.inspector = true;
        true
    }

    pub fn reset_to_blank_scene(&mut self) {
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
        self.thumb_lru.clear();
        self.last_exact_kf = None;
        self.dirty = DirtyFlags::all();
        self.dirty.catalog = false;
        self.dirty.card_viewport = false;
    }

    pub fn load_demo_presets(&mut self) -> usize {
        let demos = build_demo_presets();
        let mut added = 0usize;
        let first_added_idx = self.project.clips.len();
        for mut c in demos {
            if !self.project.clips.iter().any(|existing| existing.id == c.id) {
                snap_clip_to_frame_grid(&mut c);
                self.project.clips.push(c);
                added += 1;
            }
        }
        if added > 0 {
            self.active_clip_idx = first_added_idx.min(self.project.clips.len().saturating_sub(1));
            self.current_time = 0.0;
            self.playing = false;
            self.gallery_thumbs.clear();
            self.thumb_lru.clear();
            self.last_exact_kf = None;
            self.dirty.main_viewport = true;
            self.dirty.gallery = true;
            self.dirty.inspector = true;
            self.dirty.timeline_meta = true;
            self.dirty.timeline_tracks = true;
            self.dirty.timeline_diamonds = true;
            self.dirty.status_pill = true;
        }
        added
    }

    /// Advance camera smoothing, held navigation keys, and clip playback once per rendered frame.
    pub fn step_frame_simulation(&mut self, dt: f32) {
        self.clock_sec += dt;

        if self.impact_flash > 0.0 {
            self.impact_flash = (self.impact_flash - dt * 4.5).max(0.0);
            self.dirty.main_viewport = true;
        }

        let fwd = (self.held_nav_keys[0] as i32 - self.held_nav_keys[1] as i32) as f32;
        let rgt = (self.held_nav_keys[3] as i32 - self.held_nav_keys[2] as i32) as f32;
        let upk = (self.held_nav_keys[4] as i32 - self.held_nav_keys[5] as i32) as f32;
        if fwd != 0.0 || rgt != 0.0 || upk != 0.0 {
            let spd = (self.main_cam.current.dist * 0.95).clamp(1.2, 4.0) * dt;
            self.main_cam.move_free(fwd * spd, rgt * spd, upk * spd);
            self.dirty.main_viewport = true;
        }

        if self.held_nav_keys[6] || self.held_nav_keys[7] {
            let dir = if self.held_nav_keys[6] { 1.0 } else { -1.0 };
            self.main_cam.target_dist =
                (self.main_cam.target_dist * (-dir * 1.9 * dt).exp()).clamp(self.main_cam.min_dist, self.main_cam.max_dist);
            self.main_cam.last_zoom_at = Some(Instant::now());
        }

        if self.main_cam.step_damping(dt) {
            self.dirty.main_viewport = true;
            self.dirty.triad = true;
        }

        if self.playing {
            if !self.is_active_clip_compatible() {
                self.playing = false;
                self.dirty.transform_readout = true;
                return;
            }
            if self.hit_stop_timer > 0.0 {
                self.hit_stop_timer -= dt;
            } else {
                let prev_t = self.current_time;
                let dur = self.active_clip().duration.max(0.05);
                let impact_t = self.active_clip().impact_time;
                let looped = self.active_clip().looped;
                self.current_time += dt * self.speed;
                if let Some(imp) = impact_t {
                    if !self.hit_stop_triggered && prev_t < imp && self.current_time >= imp {
                        self.current_time = imp;
                        self.hit_stop_timer = 0.06;
                        self.impact_flash = 1.0;
                        self.hit_stop_triggered = true;
                    }
                }
                let mut hitstop_vfx_dur: Option<(f32, f32)> = None;
                for ev in &self.active_clip().vfx_events {
                    if prev_t < ev.time && self.current_time >= ev.time && self.last_vfx_trigger_t < ev.time - 0.01 {
                        if matches!(ev.kind, VfxKind::HitStop | VfxKind::ParrySpark) {
                            hitstop_vfx_dur = Some((ev.time, ev.duration.clamp(0.03, 0.16)));
                            break;
                        }
                    }
                }
                if let Some((et, hdur)) = hitstop_vfx_dur {
                    self.last_vfx_trigger_t = et;
                    self.hit_stop_timer = self.hit_stop_timer.max(hdur);
                    self.impact_flash = 1.0;
                }
                if self.current_time >= dur {
                    if looped {
                        self.current_time = 0.0;
                        self.hit_stop_triggered = false;
                        self.last_vfx_trigger_t = -1.0;
                    } else {
                        self.current_time = dur;
                        self.playing = false;
                    }
                }
                self.dirty.main_viewport = true;
                self.dirty.transform_readout = true;
                self.dirty.timeline_meta = true;
                self.update_exact_kf_dirty();
            }
        }
    }

    /// Render only viewports whose dirty flag is set — zero ClipDef cloning and zero HashMap heap churn.
    pub fn render_gpu_frames_only(
        &mut self,
        vp_size: (u32, u32),
    ) -> (Option<Image>, Option<Image>) {
        if self.vp_size != vp_size {
            self.vp_size = vp_size;
            self.dirty.main_viewport = true;
        }

        self.ensure_active_weapon_gpu();

        let clip_idx = self.active_clip_idx.min(self.project.clips.len().saturating_sub(1));
        let weapon_idx = self.active_weapon_idx.min(self.weapons.len().saturating_sub(1));
        let cur_t = self.current_time;

        let Some(mut gpu) = self.gpu.take() else {
            return (None, None);
        };

        let mut new_main = None;
        let mut new_card = None;

        if gpu.resize_main_if_needed(self.vp_size.0, self.vp_size.1) || self.main_img.is_none() {
            self.main_img = Image::try_from(gpu.main_surface.resolve_tex.clone()).ok();
            self.dirty.main_viewport = true;
            new_main = self.main_img.clone();
        }
        if self.card_img.is_none() {
            self.card_img = Image::try_from(gpu.card_surface.resolve_tex.clone()).ok();
            self.dirty.card_viewport = true;
            new_card = self.card_img.clone();
        }

        let clip = &self.project.clips[clip_idx];
        let weapon = &self.weapons[weapon_idx];
        let weapon_id = weapon.id.as_str();
        let compatible = clip.allows_weapon(weapon);

        if self.dirty.main_viewport {
            let pose = if compatible {
                sample_clip_pose(clip, cur_t)
            } else {
                RigPose::ZERO
            };
            let prev_kf = if compatible {
                crate::model::prev_keyframe_time(clip, cur_t).map(|pt| sample_clip_pose(clip, pt))
            } else {
                None
            };
            let next_kf = if compatible {
                crate::model::next_keyframe_time(clip, cur_t).map(|nt| sample_clip_pose(clip, nt))
            } else {
                None
            };
            gpu.render_to_surface(
                0,
                &RenderParams {
                    pose: &pose,
                    ghost_pose: prev_kf.as_ref(),
                    ghost_next_pose: next_kf.as_ref(),
                    clip,
                    current_time: cur_t,
                    weapon_id,
                    selected_bone: self.active_bone_idx,
                    solo_weapon: self.solo_weapon,
                    hidden_bones: self.hidden_bones,
                    hovered_gizmo: self.hovered_gizmo,
                    active_gizmo: self.drag.active_gizmo,
                    show_shadows: self.shadows,
                    show_onion: compatible && self.onion,
                    show_trail: compatible && self.trail,
                    show_gizmo: self.show_gizmo && matches!(self.tool_mode, ToolMode::Rotate | ToolMode::Scale | ToolMode::Move),
                    bg_mode: self.bg_mode as f32,
                    theme_idx: self.theme_idx as f32,
                    impact_flash: if compatible { self.impact_flash } else { 0.0 },
                    anim_clock: self.clock_sec,
                    camera: &self.main_cam.current,
                    draw_world_backdrop: self.bg_mode == 1,
                },
            );
            self.dirty.main_viewport = false;
        }

        if self.dirty.card_viewport {
            let card_cam = CameraState {
                yaw: 1.48,
                pitch: 0.22,
                dist: 2.35,
                target: [0.0, 0.96, 0.55],
            };
            let zero_pose = RigPose::ZERO;
            gpu.render_to_surface(
                1,
                &RenderParams {
                    pose: &zero_pose,
                    ghost_pose: None,
                    ghost_next_pose: None,
                    clip,
                    current_time: 0.0,
                    weapon_id,
                    selected_bone: 99,
                    solo_weapon: true,
                    hidden_bones: [false; BONE_COUNT],
                    hovered_gizmo: GizmoAxis::None,
                    active_gizmo: GizmoAxis::None,
                    show_shadows: false,
                    show_onion: false,
                    show_trail: false,
                    show_gizmo: false,
                    bg_mode: 0.0,
                    theme_idx: self.theme_idx as f32,
                    impact_flash: 0.0,
                    anim_clock: 0.0,
                    camera: &card_cam,
                    draw_world_backdrop: false,
                },
            );
            self.dirty.card_viewport = false;
        }

        self.gpu = Some(gpu);
        (new_main, new_card)
    }
}

pub fn export_storyboard_strip(state: &mut AppState) -> Result<PathBuf, String> {
    state.ensure_active_weapon_gpu();
    let clip_idx = state.active_clip_idx.min(state.project.clips.len().saturating_sub(1));
    let weapon_idx = state.active_weapon_idx.min(state.weapons.len().saturating_sub(1));
    let Some(mut gpu) = state.gpu.take() else {
        return Err("GPU no inicializada".into());
    };
    let clip = &state.project.clips[clip_idx];
    let weapon_id = state.weapons[weapon_idx].id.as_str();

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
        let pose = sample_clip_pose(clip, t);
        let is_imp = clip.impact_time.map(|imp| (t - imp).abs() < 0.01).unwrap_or(false);
        gpu.render_to_surface(
            0,
            &RenderParams {
                pose: &pose,
                ghost_pose: None,
                ghost_next_pose: None,
                clip,
                current_time: t,
                weapon_id,
                selected_bone: 99,
                solo_weapon: false,
                hidden_bones: [false; BONE_COUNT],
                hovered_gizmo: GizmoAxis::None,
                active_gizmo: GizmoAxis::None,
                show_shadows: true,
                show_onion: false,
                show_trail: true,
                show_gizmo: false,
                bg_mode: 0.0,
                theme_idx: state.theme_idx as f32,
                impact_flash: if is_imp { 0.9 } else { 0.0 },
                anim_clock: 0.0,
                camera: &state.main_cam.current,
                draw_world_backdrop: false,
            },
        );
        let (rw, rh, raw) = gpu.readback_surface(0);
        if let Some(cell_img) = image::RgbaImage::from_raw(rw, rh, raw) {
            image::imageops::overlay(&mut strip, &cell_img, (i * cw) as i64, 0);
        }
    }
    state.gpu = Some(gpu);
    state.dirty.main_viewport = true;
    let out_dir = state.workspace_root.join("exports");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_file = out_dir.join(format!("{}_strip.png", state.project.clips[clip_idx].id));
    strip.save(&out_file).map_err(|e| e.to_string())?;
    Ok(out_file)
}
