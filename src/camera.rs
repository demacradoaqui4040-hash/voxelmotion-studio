// VoxelMotion Studio - Unified 3D Viewport Camera Controller
// Handles 1:1 pointer orbit/pan/dolly, WASD free flight, and target-space cursor-pinned zoom.

use crate::math3d::{
    camera_basis, cursor_target_plane_point, evaluate_rig_matrices, pick_bone_ray_point, Vec3,
};
use crate::model::RigPose;
use crate::renderer3d::CameraState;
use std::time::Instant;

/// Encapsulates camera pose, smooth damping targets, and input-conflict-free motion primitives.
#[derive(Clone, Debug)]
pub struct ViewportCameraController {
    pub current: CameraState,
    pub target_dist: f32,
    pub target_pos: [f32; 3],
    pub target_yaw: f32,
    pub target_pitch: f32,
    pub min_dist: f32,
    pub max_dist: f32,
    pub default_target: [f32; 3],
    pub last_zoom_at: Option<Instant>,
    zoom_burst_anchor: Option<[f32; 3]>,
    zoom_burst_cursor: Option<(f32, f32)>,
}

impl ViewportCameraController {
    /// Create the primary animation studio camera controller.
    pub fn new_main() -> Self {
        let default_target = [0.0, 0.96, 0.0];
        Self {
            current: CameraState {
                yaw: 0.58,
                pitch: 0.24,
                dist: 3.35,
                target: default_target,
            },
            target_dist: 3.35,
            target_pos: default_target,
            target_yaw: 0.58,
            target_pitch: 0.24,
            min_dist: 0.18,
            max_dist: 16.0,
            default_target,
            last_zoom_at: None,
            zoom_burst_anchor: None,
            zoom_burst_cursor: None,
        }
    }

    #[cfg(test)]
    pub fn reset_main(&mut self) {
        self.target_pos = self.default_target;
        self.target_dist = 3.35;
        self.target_yaw = 0.58;
        self.target_pitch = 0.24;
        self.current = CameraState {
            yaw: 0.58,
            pitch: 0.24,
            dist: 3.35,
            target: self.default_target,
        };
        self.last_zoom_at = None;
        self.clear_zoom_burst();
    }

    /// Smooth Blender-style camera reset preserving 1:1 manual orbit responsiveness.
    pub fn reset_main_smooth(&mut self) {
        self.target_pos = self.default_target;
        self.target_dist = 3.35;
        self.target_yaw = 0.58;
        self.target_pitch = 0.24;
        self.clear_zoom_burst();
    }

    pub fn set_main_preset(&mut self, preset: i32) {
        self.target_pos = self.default_target;
        self.clear_zoom_burst();
        match preset {
            0 => {
                self.target_yaw = 0.58;
                self.target_pitch = 0.24;
                self.target_dist = 3.35;
            }
            1 => {
                self.target_yaw = 0.0;
                self.target_pitch = 0.14;
                self.target_dist = 3.2;
            }
            2 => {
                self.target_yaw = 1.57;
                self.target_pitch = 0.14;
                self.target_dist = 3.3;
            }
            _ => {
                self.target_yaw = std::f32::consts::PI;
                self.target_pitch = 0.18;
                self.target_dist = 3.3;
            }
        }
    }

    pub fn clear_zoom_burst(&mut self) {
        self.zoom_burst_anchor = None;
        self.zoom_burst_cursor = None;
    }

    pub fn focus_point(&mut self, point: Vec3, dist: f32) {
        self.clear_zoom_burst();
        self.target_pos = [
            point.x.clamp(-5.0, 5.0),
            point.y.clamp(-0.5, 4.0),
            point.z.clamp(-5.0, 5.0),
        ];
        self.target_dist = dist.clamp(self.min_dist, self.max_dist);
    }

    /// 1:1 immediate orbit (zero artificial lag on mouse drag).
    pub fn orbit(
        &mut self,
        dx: f32,
        dy: f32,
        yaw_sens: f32,
        pitch_sens: f32,
        min_pitch: f32,
        max_pitch: f32,
    ) {
        self.current.yaw -= dx * yaw_sens;
        self.current.pitch = (self.current.pitch + dy * pitch_sens).clamp(min_pitch, max_pitch);
        self.target_yaw = self.current.yaw;
        self.target_pitch = self.current.pitch;
        self.clear_zoom_burst();
    }

    /// Low-latency silky orbit with 38% immediate lead and exponential settle across 60-144Hz frames.
    pub fn orbit_smooth(
        &mut self,
        dx: f32,
        dy: f32,
        yaw_sens: f32,
        pitch_sens: f32,
        min_pitch: f32,
        max_pitch: f32,
    ) {
        let sy = -dx * yaw_sens;
        let sp = dy * pitch_sens;
        self.target_yaw += sy;
        self.target_pitch = (self.target_pitch + sp).clamp(min_pitch, max_pitch);
        self.current.yaw += sy * 0.38;
        self.current.pitch = (self.current.pitch + sp * 0.38).clamp(min_pitch, max_pitch);
        self.clear_zoom_burst();
    }

    /// Additive pan applied to BOTH current and target positions so panning never cancels active zoom interpolation.
    #[cfg(test)]
    pub fn pan(&mut self, dx: f32, dy: f32, sens_factor: f32) {
        let (cam_r, cam_u, _) = camera_basis(self.current.yaw, self.current.pitch);
        let sens = self.current.dist * sens_factor;
        let delta = cam_r.mul(-dx * sens).add(cam_u.mul(dy * sens));
        self.apply_target_delta(delta);
    }

    /// Silky 45%-lead damped pan eliminating stair-stepping on 125Hz mice and touchpads.
    pub fn pan_smooth(&mut self, dx: f32, dy: f32, sens_factor: f32) {
        let (cam_r, cam_u, _) = camera_basis(self.current.yaw, self.current.pitch);
        let sens = self.current.dist * sens_factor;
        let delta = cam_r.mul(-dx * sens).add(cam_u.mul(dy * sens));
        let clamp_target = |p: &mut [f32; 3], d: Vec3| {
            p[0] = (p[0] + d.x).clamp(-6.0, 6.0);
            p[1] = (p[1] + d.y).clamp(-0.5, 4.0);
            p[2] = (p[2] + d.z).clamp(-6.0, 6.0);
        };
        clamp_target(&mut self.target_pos, delta);
        clamp_target(&mut self.current.target, delta.mul(0.45));
        if let Some(ref mut anchor) = self.zoom_burst_anchor {
            clamp_target(anchor, delta);
        }
    }

    /// Smooth Ctrl+drag dolly zoom with 45% lead + exponential glide.
    pub fn dolly_smooth(&mut self, dy: f32) {
        let next_t = (self.target_dist * (dy * 0.0075).exp()).clamp(0.22, self.max_dist);
        self.target_dist = next_t;
        self.current.dist += (next_t - self.current.dist) * 0.45;
        self.last_zoom_at = Some(Instant::now());
        self.clear_zoom_burst();
    }

    /// Additive free-flight movement along camera basis, keeping active zoom smoothing intact.
    pub fn move_free(&mut self, fwd_step: f32, right_step: f32, up_step: f32) {
        let (r, u, f) = camera_basis(self.current.yaw, self.current.pitch);
        let delta = f.mul(fwd_step).add(r.mul(right_step)).add(u.mul(up_step));
        self.apply_target_delta(delta);
    }

    #[inline]
    fn apply_target_delta(&mut self, delta: Vec3) {
        let clamp_target = |p: &mut [f32; 3]| {
            p[0] = (p[0] + delta.x).clamp(-6.0, 6.0);
            p[1] = (p[1] + delta.y).clamp(-0.5, 4.0);
            p[2] = (p[2] + delta.z).clamp(-6.0, 6.0);
        };
        clamp_target(&mut self.current.target);
        clamp_target(&mut self.target_pos);
        if let Some(ref mut anchor) = self.zoom_burst_anchor {
            clamp_target(anchor);
        }
    }

    /// Queue cursor-pinned wheel zoom with stable burst anchor and 1:1 immediate tracking for fine touchpad deltas.
    pub fn queue_zoom_at_cursor(
        &mut self,
        mx: f32,
        my: f32,
        vw: f32,
        vh: f32,
        dy: f32,
        pose: Option<&RigPose>,
        solo_weapon: bool,
    ) {
        if dy.abs() < 0.01 {
            return;
        }
        let fine_continuous = dy.abs() < 24.0;
        let notch = (dy / 60.0).clamp(-6.0, 6.0);
        let old_d = self.target_dist.max(self.min_dist);
        let new_d = (old_d * (-notch * 0.22).exp()).clamp(self.min_dist, self.max_dist);

        if vw > 1.0 && vh > 1.0 {
            let burst_expired = self
                .last_zoom_at
                .is_none_or(|t| t.elapsed().as_millis() > 180)
                || self
                    .zoom_burst_cursor
                    .is_none_or(|(bx, by)| (mx - bx).hypot(my - by) > 36.0);

            let cur_tgt = Vec3::new(self.target_pos[0], self.target_pos[1], self.target_pos[2]);
            let anchor = if !burst_expired {
                if let Some(a) = self.zoom_burst_anchor {
                    Vec3::new(a[0], a[1], a[2])
                } else {
                    cur_tgt
                }
            } else {
                let nx = (mx / vw) * 2.0 - 1.0;
                let ny = 1.0 - (my / vh) * 2.0;
                // Unproject from target camera state so rapid scrolls stay pinned cleanly to cursor.
                let target_cam = CameraState {
                    yaw: self.current.yaw,
                    pitch: self.current.pitch,
                    dist: old_d,
                    target: self.target_pos,
                };
                let (_vp, inv_vp, _) = target_cam.vp_and_inv(vw / vh);
                let (_, _, cam_f) = camera_basis(self.current.yaw, self.current.pitch);
                let picked = pose
                    .and_then(|p| {
                        let mats = evaluate_rig_matrices(p);
                        pick_bone_ray_point(nx, ny, inv_vp, &mats, solo_weapon).map(|(_, pt)| pt)
                    })
                    .unwrap_or_else(|| cursor_target_plane_point(nx, ny, inv_vp, cur_tgt, cam_f));
                self.zoom_burst_anchor = Some([picked.x, picked.y, picked.z]);
                self.zoom_burst_cursor = Some((mx, my));
                picked
            };

            let ratio = (new_d / old_d).clamp(0.25, 1.35);
            let mut next_tgt = anchor.add(cur_tgt.sub(anchor).mul(ratio));
            if dy < 0.0 && new_d > 3.6 {
                let relax = ((new_d - 3.6) / 4.0).clamp(0.0, 0.20);
                let def = Vec3::new(
                    self.default_target[0],
                    self.default_target[1],
                    self.default_target[2],
                );
                next_tgt = next_tgt.add(def.sub(next_tgt).mul(relax));
            }
            self.target_pos = [
                next_tgt.x.clamp(-5.0, 5.0),
                next_tgt.y.clamp(-0.5, 4.0),
                next_tgt.z.clamp(-5.0, 5.0),
            ];
        }

        self.target_dist = new_d;
        self.last_zoom_at = Some(Instant::now());

        // Hardware touchpads and smooth-scroll wheels already interpolate at native polling rate;
        // apply them 1:1 immediately to avoid compounding software lag on top of device smoothing.
        if fine_continuous {
            self.current.dist = self.target_dist;
            self.current.target = self.target_pos;
        }
    }

    /// Matched-rate exponential damping toward `target_dist` and `target_pos` (settles within ~25-35ms).
    /// Intended to run once per rendered frame in `BeforeRendering`.
    pub fn step_damping(&mut self, dt: f32) -> bool {
        let dt_clamped = dt.clamp(0.001, 0.05);
        let alpha = 1.0 - (-82.0 * dt_clamped).exp();
        let mut moved = false;

        let diff_d = self.target_dist - self.current.dist;
        if diff_d.abs() <= 0.0005 {
            if diff_d != 0.0 {
                self.current.dist = self.target_dist;
                moved = true;
            }
        } else {
            self.current.dist += diff_d * alpha;
            moved = true;
        }

        let dx = self.target_pos[0] - self.current.target[0];
        let dy = self.target_pos[1] - self.current.target[1];
        let dz = self.target_pos[2] - self.current.target[2];
        if dx * dx + dy * dy + dz * dz > 1e-7 {
            self.current.target[0] += dx * alpha;
            self.current.target[1] += dy * alpha;
            self.current.target[2] += dz * alpha;
            moved = true;
        } else if dx != 0.0 || dy != 0.0 || dz != 0.0 {
            self.current.target = self.target_pos;
            moved = true;
        }

        let alpha_rot = 1.0 - (-32.0 * dt_clamped).exp();
        let dyaw = (self.target_yaw - self.current.yaw)
            .sin()
            .atan2((self.target_yaw - self.current.yaw).cos());
        if dyaw.abs() <= 0.001 {
            if (self.current.yaw - self.target_yaw).abs() > 1e-5 {
                self.current.yaw = self.target_yaw;
                moved = true;
            }
        } else {
            self.current.yaw += dyaw * alpha_rot;
            moved = true;
        }

        let dpitch = self.target_pitch - self.current.pitch;
        if dpitch.abs() <= 0.001 {
            if dpitch != 0.0 {
                self.current.pitch = self.target_pitch;
                moved = true;
            }
        } else {
            self.current.pitch += dpitch * alpha_rot;
            moved = true;
        }

        moved
    }

    #[inline]
    pub fn is_animating(&self) -> bool {
        let dd = (self.target_dist - self.current.dist).abs();
        let dx = self.target_pos[0] - self.current.target[0];
        let dy = self.target_pos[1] - self.current.target[1];
        let dz = self.target_pos[2] - self.current.target[2];
        let dyaw = (self.target_yaw - self.current.yaw)
            .sin()
            .atan2((self.target_yaw - self.current.yaw).cos())
            .abs();
        let dpitch = (self.target_pitch - self.current.pitch).abs();
        dd > 0.0005 || (dx * dx + dy * dy + dz * dz) > 1e-7 || dyaw > 0.001 || dpitch > 0.001
    }

    #[inline]
    pub fn is_actively_zooming(&self) -> bool {
        self.is_animating()
            || self
                .last_zoom_at
                .is_some_and(|t| t.elapsed().as_millis() < 350)
    }
}
