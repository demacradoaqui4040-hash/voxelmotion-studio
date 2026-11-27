// VoxelMotion Studio - Exact 3D Math matching Minecraft ModelPart kinematics 1:1

use crate::model::{BoneTransform, RigPose, ToolMode, BONE_COUNT};

pub const DEG2RAD: f32 = std::f32::consts::PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoAxis {
    None,
    X,
    Y,
    Z,
    Roll,
    Free,
}

impl GizmoAxis {
    pub fn label_es(self) -> &'static str {
        match self {
            Self::None => "",
            Self::X => "Eje X",
            Self::Y => "Eje Y",
            Self::Z => "Eje Z",
            Self::Roll => "Giro Z/Vista",
            Self::Free => "Libre 3D",
        }
    }
}

/// Camera orthonormal basis in world space: (right, up, look_forward).
pub fn camera_basis(yaw: f32, pitch: f32) -> (Vec3, Vec3, Vec3) {
    let cp = pitch.cos();
    let look_fwd = Vec3::new(-cp * yaw.sin(), -pitch.sin(), -cp * yaw.cos()).norm();
    let right = look_fwd.cross(Vec3::new(0.0, 1.0, 0.0)).norm();
    let up = right.cross(look_fwd).norm();
    (right, up, look_fwd)
}

pub fn project_to_screen(vp: Mat4, p: Vec3, vw: f32, vh: f32) -> Option<(f32, f32)> {
    let m = &vp.m;
    let cx = m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12];
    let cy = m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13];
    let cw = m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15];
    if cw <= 0.02 {
        return None;
    }
    let nx = cx / cw;
    let ny = cy / cw;
    Some(((nx * 0.5 + 0.5) * vw, (0.5 - ny * 0.5) * vh))
}

pub fn point_to_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-4 {
        return (px - ax).hypot(py - ay);
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0);
    let qx = ax + dx * t;
    let qy = ay + dy * t;
    (px - qx).hypot(py - qy)
}

/// Return world pivot and normalized bone-local axis directions (for X/Y/Z handles).
pub fn gizmo_axes_world(bone_world: Mat4, solo_weapon: bool) -> (Vec3, [Vec3; 3]) {
    let m = &bone_world.m;
    if solo_weapon {
        let mut x = Vec3::new(m[0], m[1], m[2]).norm();
        let mut y = Vec3::new(m[4], m[5], m[6]).norm();
        let mut z = Vec3::new(m[8], m[9], m[10]).norm();
        if x.len() < 0.1 { x = Vec3::new(1.0, 0.0, 0.0); }
        if y.len() < 0.1 { y = Vec3::new(0.0, 1.0, 0.0); }
        if z.len() < 0.1 { z = Vec3::new(0.0, 0.0, 1.0); }
        return (Vec3::new(0.0, 0.96, 0.0), [x, y, z]);
    }
    let pivot = bone_world.transform_point(Vec3::ZERO);
    // Notice bone_world includes mc_to_world (180deg X flip), so column dirs match MC bone axes directly
    let x = Vec3::new(m[0], m[1], m[2]).norm();
    let y = Vec3::new(m[4], m[5], m[6]).norm();
    let z = Vec3::new(m[8], m[9], m[10]).norm();
    (pivot, [x, y, z])
}

/// Pick hovered/clicked gizmo handle in screen pixels.
pub fn pick_gizmo_handle(
    mx: f32,
    my: f32,
    vp: Mat4,
    vw: f32,
    vh: f32,
    pivot: Vec3,
    axes: [Vec3; 3],
    cam_right: Vec3,
    axis_len_world: f32,
    ring_radius_world: f32,
) -> GizmoAxis {
    let Some((px, py)) = project_to_screen(vp, pivot, vw, vh) else {
        return GizmoAxis::None;
    };
    let center_d = (mx - px).hypot(my - py);
    if center_d <= 10.0 {
        return GizmoAxis::Free;
    }

    let mut best_axis = GizmoAxis::None;
    let mut best_d = 11.5f32;
    let axis_ids = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z];
    for i in 0..3 {
        if let Some((tx, ty)) = project_to_screen(vp, pivot.add(axes[i].mul(axis_len_world)), vw, vh) {
            let d = point_to_segment_dist(mx, my, px, py, tx, ty);
            if d < best_d {
                best_d = d;
                best_axis = axis_ids[i];
            }
        }
    }
    if best_axis != GizmoAxis::None {
        return best_axis;
    }

    if let Some((rx, ry)) = project_to_screen(vp, pivot.add(cam_right.mul(ring_radius_world)), vw, vh) {
        let ring_px = (rx - px).hypot(ry - py).max(12.0);
        if (center_d - ring_px).abs() <= 9.0 {
            return GizmoAxis::Roll;
        }
    }
    GizmoAxis::None
}

/// Transform a bone using camera-relative screen motion or an explicit X/Y/Z/Roll handle.
#[allow(clippy::too_many_arguments)]
pub fn apply_gizmo_drag(
    tr: &mut BoneTransform,
    tool: ToolMode,
    axis: GizmoAxis,
    axes: [Vec3; 3],
    cam_right: Vec3,
    cam_up: Vec3,
    cam_fwd: Vec3,
    vp: Mat4,
    vw: f32,
    vh: f32,
    pivot: Vec3,
    prev_mx: f32,
    prev_my: f32,
    cur_mx: f32,
    cur_my: f32,
) {
    let dx = cur_mx - prev_mx;
    let dy = cur_my - prev_my;
    if dx.abs() < 1e-4 && dy.abs() < 1e-4 {
        return;
    }

    match tool {
        ToolMode::Move => {
            // World-space movement converted to bone-parent MC pixels (remember pos[1] is negated UP!)
            let world_delta = match axis {
                GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => {
                    let ai = match axis {
                        GizmoAxis::X => 0,
                        GizmoAxis::Y => 1,
                        _ => 2,
                    };
                    let sign_proj = if let (Some((px, py)), Some((tx, ty))) = (
                        project_to_screen(vp, pivot, vw, vh),
                        project_to_screen(vp, pivot.add(axes[ai].mul(0.4)), vw, vh),
                    ) {
                        let vx = tx - px;
                        let vy = ty - py;
                        let l = vx.hypot(vy).max(1.0);
                        (dx * vx + dy * vy) / l
                    } else {
                        dx
                    };
                    axes[ai].mul(sign_proj * 0.0055)
                }
                _ => cam_right.mul(dx * 0.0042).add(cam_up.mul(-dy * 0.0042)),
            };
            let px = world_delta.dot(axes[0]) * 16.0;
            let py = -world_delta.dot(axes[1]) * 16.0; // bone_local uses base_y - pos[1], axes[1] points down in MC
            let pz = world_delta.dot(axes[2]) * 16.0;
            tr.pos[0] = (tr.pos[0] + px).clamp(-24.0, 24.0);
            tr.pos[1] = (tr.pos[1] + py).clamp(-24.0, 24.0);
            tr.pos[2] = (tr.pos[2] + pz).clamp(-24.0, 24.0);
        }
        ToolMode::Scale => {
            match axis {
                GizmoAxis::X => tr.scale[0] = (tr.scale[0] * (1.0 + dx * 0.006 - dy * 0.004)).clamp(0.15, 3.0),
                GizmoAxis::Y => tr.scale[1] = (tr.scale[1] * (1.0 - dy * 0.006)).clamp(0.15, 3.0),
                GizmoAxis::Z => tr.scale[2] = (tr.scale[2] * (1.0 + dx * 0.006 + dy * 0.004)).clamp(0.15, 3.0),
                _ => {
                    let factor = (1.0 + (dx - dy) * 0.005).clamp(0.85, 1.15);
                    for c in 0..3 {
                        tr.scale[c] = (tr.scale[c] * factor).clamp(0.15, 3.0);
                    }
                }
            }
        }
        _ => {
            // Rotate / Select-drag: camera-aligned arcball or exact axis spin
            match axis {
                GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => {
                    let ai = match axis {
                        GizmoAxis::X => 0,
                        GizmoAxis::Y => 1,
                        _ => 2,
                    };
                    let a_vec = axes[ai];
                    let eye_dir = cam_fwd.mul(-1.0);
                    let deg = if a_vec.dot(eye_dir).abs() > 0.85 {
                        if let Some((px, py)) = project_to_screen(vp, pivot, vw, vh) {
                            let a0 = (prev_my - py).atan2(prev_mx - px);
                            let a1 = (cur_my - py).atan2(cur_mx - px);
                            let mut da = a1 - a0;
                            if da > std::f32::consts::PI { da -= std::f32::consts::TAU; }
                            if da < -std::f32::consts::PI { da += std::f32::consts::TAU; }
                            let sign = if a_vec.dot(eye_dir) >= 0.0 { -1.0 } else { 1.0 };
                            da * (180.0 / std::f32::consts::PI) * sign
                        } else {
                            dx * 0.65
                        }
                    } else {
                        let tangent = a_vec.cross(eye_dir).norm();
                        if let (Some((px, py)), Some((tx, ty))) = (
                            project_to_screen(vp, pivot, vw, vh),
                            project_to_screen(vp, pivot.add(tangent.mul(0.35)), vw, vh),
                        ) {
                            let vx = tx - px;
                            let vy = ty - py;
                            let l = vx.hypot(vy).max(1.0);
                            ((dx * vx + dy * vy) / l) * 0.68
                        } else {
                            (dx - dy) * 0.55
                        }
                    };
                    tr.rot[ai] = (tr.rot[ai] + deg).clamp(-180.0, 180.0);
                }
                GizmoAxis::Roll => {
                    let deg = if let Some((px, py)) = project_to_screen(vp, pivot, vw, vh) {
                        let a0 = (prev_my - py).atan2(prev_mx - px);
                        let a1 = (cur_my - py).atan2(cur_mx - px);
                        let mut da = a1 - a0;
                        if da > std::f32::consts::PI { da -= std::f32::consts::TAU; }
                        if da < -std::f32::consts::PI { da += std::f32::consts::TAU; }
                        -da * (180.0 / std::f32::consts::PI)
                    } else {
                        dx * 0.65
                    };
                    let w_axis = cam_fwd.mul(-1.0);
                    for ai in 0..3 {
                        let proj = w_axis.dot(axes[ai]);
                        tr.rot[ai] = (tr.rot[ai] + deg * proj).clamp(-180.0, 180.0);
                    }
                }
                _ => {
                    // Camera-relative free rotation: screen horizontal rotates around cam_up, screen vertical around cam_right
                    let world_spin = cam_up.mul(dx * 0.62).add(cam_right.mul(dy * 0.62));
                    for ai in 0..3 {
                        let d = world_spin.dot(axes[ai]);
                        tr.rot[ai] = (tr.rot[ai] + d).clamp(-180.0, 180.0);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } }
    pub fn add(self, o: Self) -> Self { Self::new(self.x + o.x, self.y + o.y, self.z + o.z) }
    pub fn sub(self, o: Self) -> Self { Self::new(self.x - o.x, self.y - o.y, self.z - o.z) }
    pub fn mul(self, s: f32) -> Self { Self::new(self.x * s, self.y * s, self.z * s) }
    pub fn dot(self, o: Self) -> f32 { self.x * o.x + self.y * o.y + self.z * o.z }
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn len(self) -> f32 { self.dot(self).sqrt() }
    pub fn norm(self) -> Self {
        let l = self.len();
        if l > 1e-6 { self.mul(1.0 / l) } else { Self::new(0.0, 1.0, 0.0) }
    }
    pub fn arr(self) -> [f32; 3] { [self.x, self.y, self.z] }
}

/// Column-major 4x4 transformation matrix.
#[derive(Clone, Copy, Debug)]
pub struct Mat4 {
    pub m: [f32; 16],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn mul(self, b: Self) -> Self {
        let a = &self.m;
        let b = &b.m;
        let mut o = [0.0f32; 16];
        for c in 0..4 {
            let (b0, b1, b2, b3) = (b[c * 4], b[c * 4 + 1], b[c * 4 + 2], b[c * 4 + 3]);
            o[c * 4] = a[0] * b0 + a[4] * b1 + a[8] * b2 + a[12] * b3;
            o[c * 4 + 1] = a[1] * b0 + a[5] * b1 + a[9] * b2 + a[13] * b3;
            o[c * 4 + 2] = a[2] * b0 + a[6] * b1 + a[10] * b2 + a[14] * b3;
            o[c * 4 + 3] = a[3] * b0 + a[7] * b1 + a[11] * b2 + a[15] * b3;
        }
        Self { m: o }
    }

    pub fn translate(x: f32, y: f32, z: f32) -> Self {
        let mut o = Self::IDENTITY.m;
        o[12] = x;
        o[13] = y;
        o[14] = z;
        Self { m: o }
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        let mut o = Self::IDENTITY.m;
        o[0] = x;
        o[5] = y;
        o[10] = z;
        Self { m: o }
    }

    pub fn rot_x(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self {
            m: [
                1.0, 0.0, 0.0, 0.0,
                0.0, c, s, 0.0,
                0.0, -s, c, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    pub fn rot_y(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self {
            m: [
                c, 0.0, -s, 0.0,
                0.0, 1.0, 0.0, 0.0,
                s, 0.0, c, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    pub fn rot_z(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self {
            m: [
                c, s, 0.0, 0.0,
                -s, c, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Exact JOML / Minecraft ModelPart order: Rz * Ry * Rx
    pub fn rot_zyx_deg(rx: f32, ry: f32, rz: f32) -> Self {
        Self::rot_z(rz * DEG2RAD)
            .mul(Self::rot_y(ry * DEG2RAD))
            .mul(Self::rot_x(rx * DEG2RAD))
    }

    /// WGPU depth 0..1 perspective matrix.
    pub fn perspective_wgpu(fov_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_rad * 0.5).tan();
        let k = far / (near - far);
        Self {
            m: [
                f / aspect.max(0.01), 0.0, 0.0, 0.0,
                0.0, f, 0.0, 0.0,
                0.0, 0.0, k, -1.0,
                0.0, 0.0, near * k, 0.0,
            ],
        }
    }

    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = target.sub(eye).norm();
        let s = f.cross(up).norm();
        let u = s.cross(f);
        Self {
            m: [
                s.x, u.x, -f.x, 0.0,
                s.y, u.y, -f.y, 0.0,
                s.z, u.z, -f.z, 0.0,
                -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
            ],
        }
    }

    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let m = &self.m;
        let w = m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15];
        let iw = if w.abs() > 1e-6 { 1.0 / w } else { 1.0 };
        Vec3::new(
            (m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12]) * iw,
            (m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13]) * iw,
            (m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14]) * iw,
        )
    }

    pub fn invert(self) -> Self {
        let m = &self.m;
        let mut inv = [0.0f32; 16];
        inv[0] = m[5]*m[10]*m[15] - m[5]*m[11]*m[14] - m[9]*m[6]*m[15] + m[9]*m[7]*m[14] + m[13]*m[6]*m[11] - m[13]*m[7]*m[10];
        inv[4] = -m[4]*m[10]*m[15] + m[4]*m[11]*m[14] + m[8]*m[6]*m[15] - m[8]*m[7]*m[14] - m[12]*m[6]*m[11] + m[12]*m[7]*m[10];
        inv[8] = m[4]*m[9]*m[15] - m[4]*m[11]*m[13] - m[8]*m[5]*m[15] + m[8]*m[7]*m[13] + m[12]*m[5]*m[11] - m[12]*m[7]*m[9];
        inv[12] = -m[4]*m[9]*m[14] + m[4]*m[10]*m[13] + m[8]*m[5]*m[14] - m[8]*m[6]*m[13] - m[12]*m[5]*m[10] + m[12]*m[6]*m[9];
        inv[1] = -m[1]*m[10]*m[15] + m[1]*m[11]*m[14] + m[9]*m[2]*m[15] - m[9]*m[3]*m[14] - m[13]*m[2]*m[11] + m[13]*m[3]*m[10];
        inv[5] = m[0]*m[10]*m[15] - m[0]*m[11]*m[14] - m[8]*m[2]*m[15] + m[8]*m[3]*m[14] + m[12]*m[2]*m[11] - m[12]*m[3]*m[10];
        inv[9] = -m[0]*m[9]*m[15] + m[0]*m[11]*m[13] + m[8]*m[1]*m[15] - m[8]*m[3]*m[13] - m[12]*m[1]*m[11] + m[12]*m[3]*m[9];
        inv[13] = m[0]*m[9]*m[14] - m[0]*m[10]*m[13] - m[8]*m[1]*m[14] + m[8]*m[2]*m[13] + m[12]*m[1]*m[10] - m[12]*m[2]*m[9];
        inv[2] = m[1]*m[6]*m[15] - m[1]*m[7]*m[14] - m[5]*m[2]*m[15] + m[5]*m[3]*m[14] + m[13]*m[2]*m[7] - m[13]*m[3]*m[6];
        inv[6] = -m[0]*m[6]*m[15] + m[0]*m[7]*m[14] + m[4]*m[2]*m[15] - m[4]*m[3]*m[14] - m[12]*m[2]*m[7] + m[12]*m[3]*m[6];
        inv[10] = m[0]*m[5]*m[15] - m[0]*m[7]*m[13] - m[4]*m[1]*m[15] + m[4]*m[3]*m[13] + m[12]*m[1]*m[7] - m[12]*m[3]*m[5];
        inv[14] = -m[0]*m[5]*m[14] + m[0]*m[6]*m[13] + m[4]*m[1]*m[14] - m[4]*m[2]*m[13] - m[12]*m[1]*m[6] + m[12]*m[2]*m[5];
        inv[3] = -m[1]*m[6]*m[11] + m[1]*m[7]*m[10] + m[5]*m[2]*m[11] - m[5]*m[3]*m[10] - m[9]*m[2]*m[7] + m[9]*m[3]*m[6];
        inv[7] = m[0]*m[6]*m[11] - m[0]*m[7]*m[10] - m[4]*m[2]*m[11] + m[4]*m[3]*m[10] + m[8]*m[2]*m[7] - m[8]*m[3]*m[6];
        inv[11] = -m[0]*m[5]*m[11] + m[0]*m[7]*m[9] + m[4]*m[1]*m[11] - m[4]*m[3]*m[9] - m[8]*m[1]*m[7] + m[8]*m[3]*m[5];
        inv[15] = m[0]*m[5]*m[10] - m[0]*m[6]*m[9] - m[4]*m[1]*m[10] + m[4]*m[2]*m[9] + m[8]*m[1]*m[6] - m[8]*m[2]*m[5];
        let det = m[0]*inv[0] + m[1]*inv[4] + m[2]*inv[8] + m[3]*inv[12];
        if det.abs() < 1e-8 { return Self::IDENTITY; }
        let idet = 1.0 / det;
        for v in &mut inv { *v *= idet; }
        Self { m: inv }
    }
}

/// Exact det=+1 transform from Minecraft ModelPart pixel coordinates (+X left, +Y down, -Z forward)
/// into native WGPU world meters (0..2m vertical, facing +Z).
pub fn mc_to_world() -> Mat4 {
    Mat4::scale(1.0 / 16.0, 1.0 / 16.0, 1.0 / 16.0)
        .mul(Mat4::translate(0.0, 24.0, 0.0))
        .mul(Mat4::rot_x(std::f32::consts::PI))
}

fn bone_local(base_x: f32, base_y: f32, base_z: f32, p: &BoneTransform) -> Mat4 {
    let tx = base_x + p.pos[0];
    let ty = base_y - p.pos[1];
    let tz = base_z + p.pos[2];
    Mat4::translate(tx, ty, tz)
        .mul(Mat4::rot_zyx_deg(p.rot[0], p.rot[1], p.rot[2]))
        .mul(Mat4::scale(p.scale[0].max(0.05), p.scale[1].max(0.05), p.scale[2].max(0.05)))
}

/// Evaluate exact world matrices for all 8 bones matching standard virtual waist hierarchy.
pub fn evaluate_rig_matrices(pose: &RigPose) -> [Mat4; BONE_COUNT] {
    let root = mc_to_world();

    let waist_mc = bone_local(0.0, 12.0, 0.0, pose.bone(0));
    let body_mc = waist_mc.mul(bone_local(0.0, -12.0, 0.0, pose.bone(1)));
    let head_mc = waist_mc.mul(bone_local(0.0, -12.0, 0.0, pose.bone(2)));
    let right_arm_mc = waist_mc.mul(bone_local(-5.0, -10.0, 0.0, pose.bone(3)));
    let left_arm_mc = waist_mc.mul(bone_local(5.0, -10.0, 0.0, pose.bone(4)));
    let right_leg_mc = bone_local(-1.9, 12.0, 0.0, pose.bone(6));
    let left_leg_mc = bone_local(1.9, 12.0, 0.0, pose.bone(7));

    let right_item_mc = right_arm_mc.mul(bone_local(-1.0, 9.0, -2.0, pose.bone(5)));

    [
        root.mul(waist_mc),
        root.mul(body_mc),
        root.mul(head_mc),
        root.mul(right_arm_mc),
        root.mul(left_arm_mc),
        root.mul(right_item_mc),
        root.mul(right_leg_mc),
        root.mul(left_leg_mc),
    ]
}

pub fn pick_bone_ray(
    nx: f32,
    ny: f32,
    inv_vp: Mat4,
    mats: &[Mat4; 8],
    solo_weapon: bool,
) -> Option<usize> {
    pick_bone_ray_point(nx, ny, inv_vp, mats, solo_weapon).map(|(idx, _)| idx)
}

/// Exact ray-OBB pick returning both the bone index and the 3D world hit point.
pub fn pick_bone_ray_point(
    nx: f32,
    ny: f32,
    inv_vp: Mat4,
    mats: &[Mat4; 8],
    solo_weapon: bool,
) -> Option<(usize, Vec3)> {
    let p_near = inv_vp.transform_point(Vec3::new(nx, ny, 0.0));
    let p_far = inv_vp.transform_point(Vec3::new(nx, ny, 1.0));
    let dir = p_far.sub(p_near).norm();
    if solo_weapon {
        let pivot = Vec3::new(0.0, 0.96, 0.0);
        let denom = dir.dot(p_far.sub(p_near).norm());
        let dist = pivot.sub(p_near).len();
        let _ = denom;
        return Some((5, p_near.add(dir.mul(dist))));
    }

    // Pick boxes in bone-local ModelPart pixel coordinates
    let boxes: [(usize, [f32; 6]); 8] = [
        (0, [-4.3, -1.5, -2.4, 4.3, 1.5, 2.4]),     // waist
        (1, [-4.1, 0.0, -2.1, 4.1, 11.2, 2.1]),     // body
        (2, [-4.1, -8.1, -4.1, 4.1, 0.0, 4.1]),     // head
        (3, [-3.1, -2.0, -2.1, 1.1, 10.1, 2.1]),    // right_arm
        (4, [-1.1, -2.0, -2.1, 3.1, 10.1, 2.1]),    // left_arm
        (5, [-2.2, -5.0, -26.0, 2.2, 5.0, 6.0]),    // right_item weapon envelope
        (6, [-2.1, 0.0, -2.1, 2.1, 12.0, 2.1]),     // right_leg
        (7, [-2.1, 0.0, -2.1, 2.1, 12.0, 2.1]),     // left_leg
    ];

    let mut best_idx = None;
    let mut best_t = f32::INFINITY;
    for &(idx, b) in &boxes {
        let inv = mats[idx].invert();
        let ro = inv.transform_point(p_near);
        let r1 = inv.transform_point(p_near.add(dir));
        let rd = r1.sub(ro);

        let rds = [rd.x, rd.y, rd.z];
        let ros = [ro.x, ro.y, ro.z];
        let mut tmin = f32::NEG_INFINITY;
        let mut tmax = f32::INFINITY;
        let mut hit = true;
        for i in 0..3 {
            if rds[i].abs() < 1e-6 {
                if ros[i] < b[i] || ros[i] > b[i + 3] {
                    hit = false;
                    break;
                }
            } else {
                let mut t1 = (b[i] - ros[i]) / rds[i];
                let mut t2 = (b[i + 3] - ros[i]) / rds[i];
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                tmin = tmin.max(t1);
                tmax = tmax.min(t2);
                if tmin > tmax {
                    hit = false;
                    break;
                }
            }
        }
        if hit && tmax > 0.0 && tmin < best_t {
            best_t = tmin.max(0.0);
            best_idx = Some(idx);
        }
    }
    best_idx.map(|idx| (idx, p_near.add(dir.mul(best_t))))
}

/// Ray-plane fallback point under cursor at the camera target plane.
pub fn cursor_target_plane_point(nx: f32, ny: f32, inv_vp: Mat4, target: Vec3, look_fwd: Vec3) -> Vec3 {
    let p_near = inv_vp.transform_point(Vec3::new(nx, ny, 0.0));
    let p_far = inv_vp.transform_point(Vec3::new(nx, ny, 1.0));
    let dir = p_far.sub(p_near).norm();
    let denom = dir.dot(look_fwd);
    if denom.abs() < 1e-4 {
        return target;
    }
    let t = target.sub(p_near).dot(look_fwd) / denom;
    if t > 0.05 && t < 30.0 {
        p_near.add(dir.mul(t))
    } else {
        target
    }
}
