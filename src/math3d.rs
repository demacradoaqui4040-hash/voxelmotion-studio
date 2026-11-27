// VoxelMotion Studio - Exact 3D Math matching Minecraft ModelPart kinematics 1:1

use crate::model::{BoneTransform, RigPose, ToolMode, BONE_COUNT};

pub const DEG2RAD: f32 = std::f32::consts::PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GizmoAxis {
    None,
    X,
    Y,
    Z,
    XY,
    XZ,
    YZ,
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
            Self::XY => "Plano XY",
            Self::XZ => "Plano XZ",
            Self::YZ => "Plano YZ",
            Self::Roll => "Giro Vista",
            Self::Free => "Libre 3D",
        }
    }
}

/// Constant screen-space world radius for gizmos regardless of zoom distance.
#[inline]
pub fn gizmo_world_scale(eye: Vec3, pivot: Vec3) -> f32 {
    (eye.sub(pivot).len() * 0.145).clamp(0.18, 1.85)
}

/// Unproject a logical screen coordinate into a normalized world-space `(origin, direction)` ray.
pub fn screen_ray(inv_vp: Mat4, mx: f32, my: f32, vw: f32, vh: f32) -> (Vec3, Vec3) {
    let nx = (mx / vw.max(1.0)) * 2.0 - 1.0;
    let ny = 1.0 - (my / vh.max(1.0)) * 2.0;
    let p0 = inv_vp.transform_point(Vec3::new(nx, ny, 0.0));
    let p1 = inv_vp.transform_point(Vec3::new(nx, ny, 0.98));
    (p0, p1.sub(p0).norm())
}

/// Intersect a ray `(ro, rd)` with plane `(plane_p, plane_n)` in front of the ray origin.
pub fn ray_plane_intersect(ro: Vec3, rd: Vec3, plane_p: Vec3, plane_n: Vec3) -> Option<Vec3> {
    let denom = rd.dot(plane_n);
    if denom.abs() < 1e-4 {
        return None;
    }
    let t = plane_p.sub(ro).dot(plane_n) / denom;
    if t <= 0.01 {
        return None;
    }
    Some(ro.add(rd.mul(t)))
}

/// Signed distance parameter `s` along `(line_p, line_d)` closest to ray `(ro, rd)`.
pub fn ray_closest_param_on_line(ro: Vec3, rd: Vec3, line_p: Vec3, line_d: Vec3) -> Option<f32> {
    let w0 = line_p.sub(ro);
    let b = line_d.dot(rd);
    let d = line_d.dot(w0);
    let e = rd.dot(w0);
    let denom = 1.0 - b * b;
    if denom < 1e-3 {
        return None;
    }
    let t = (e - b * d) / denom;
    if t <= 0.01 {
        return None;
    }
    Some((b * e - d) / denom)
}

/// Decompose a pure rotation matrix into ZYX Euler degrees `(rx, ry, rz)` continuously relative to `prev_deg`.
pub fn decompose_zyx_continuous(rot_m: Mat4, prev_deg: [f32; 3]) -> [f32; 3] {
    let m = &rot_m.m;
    let sy = (-m[2]).clamp(-1.0, 1.0);
    let cy = (1.0 - sy * sy).max(0.0).sqrt();
    let rad2deg = 180.0 / std::f32::consts::PI;
    let unwrap = |deg: f32, prev: f32| -> f32 {
        prev + (deg - prev + 180.0).rem_euclid(360.0) - 180.0
    };

    let mut out = if cy < 1e-4 {
        let y = sy.asin() * rad2deg;
        let z = prev_deg[2];
        let x = if sy > 0.0 {
            z + m[4].atan2(m[5]) * rad2deg
        } else {
            (-m[4]).atan2(m[5]) * rad2deg - z
        };
        [unwrap(x, prev_deg[0]), unwrap(y, prev_deg[1]), unwrap(z, prev_deg[2])]
    } else {
        let y1 = sy.asin();
        let x1 = m[6].atan2(m[10]);
        let z1 = m[1].atan2(m[0]);
        let cand1 = [
            unwrap(x1 * rad2deg, prev_deg[0]),
            unwrap(y1 * rad2deg, prev_deg[1]),
            unwrap(z1 * rad2deg, prev_deg[2]),
        ];

        let y2 = std::f32::consts::PI - y1;
        let x2 = (-m[6]).atan2(-m[10]);
        let z2 = (-m[1]).atan2(-m[0]);
        let cand2 = [
            unwrap(x2 * rad2deg, prev_deg[0]),
            unwrap(y2 * rad2deg, prev_deg[1]),
            unwrap(z2 * rad2deg, prev_deg[2]),
        ];

        let d1: f32 = (0..3).map(|i| (cand1[i] - prev_deg[i]).powi(2)).sum();
        let d2: f32 = (0..3).map(|i| (cand2[i] - prev_deg[i]).powi(2)).sum();
        if d1 <= d2 { cand1 } else { cand2 }
    };

    for i in 0..3 {
        if out[i].abs() < 1e-4 && prev_deg[i].abs() < 1e-4 {
            out[i] = 0.0;
        }
        out[i] = out[i].clamp(-720.0, 720.0);
    }
    out
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

/// Return world pivot and normalized axes (bone-local when `world_space == false`, world XYZ when `true`).
pub fn gizmo_axes_for_space(
    bone_world: Mat4,
    solo_weapon: bool,
    world_space: bool,
) -> (Vec3, [Vec3; 3]) {
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
    if world_space {
        return (
            pivot,
            [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
            ],
        );
    }
    // Notice bone_world includes mc_to_world (180deg X flip), so column dirs match MC bone axes directly
    let x = Vec3::new(m[0], m[1], m[2]).norm();
    let y = Vec3::new(m[4], m[5], m[6]).norm();
    let z = Vec3::new(m[8], m[9], m[10]).norm();
    (pivot, [x, y, z])
}

/// Return the normalized orthonormal basis of a parent world transform matrix.
pub fn matrix_axes_world(parent_world: Mat4) -> [Vec3; 3] {
    let m = &parent_world.m;
    [
        Vec3::new(m[0], m[1], m[2]).norm(),
        Vec3::new(m[4], m[5], m[6]).norm(),
        Vec3::new(m[8], m[9], m[10]).norm(),
    ]
}

/// Pick hovered/clicked gizmo handle in screen pixels using exact geometry for the active tool.
#[allow(clippy::too_many_arguments)]
pub fn pick_gizmo_handle(
    tool: ToolMode,
    mx: f32,
    my: f32,
    vp: Mat4,
    inv_vp: Mat4,
    vw: f32,
    vh: f32,
    pivot: Vec3,
    axes: [Vec3; 3],
    cam_right: Vec3,
    eye: Vec3,
) -> GizmoAxis {
    if !matches!(tool, ToolMode::Rotate | ToolMode::Move | ToolMode::Scale) {
        return GizmoAxis::None;
    }
    let Some((px, py)) = project_to_screen(vp, pivot, vw, vh) else {
        return GizmoAxis::None;
    };
    let scale = gizmo_world_scale(eye, pivot);
    let to_cam = eye.sub(pivot).norm();
    let center_d = (mx - px).hypot(my - py);

    match tool {
        ToolMode::Rotate => {
            // Check 3 orthogonal 3D rings first, strongly preferring front-hemisphere arcs.
            let rings = [
                (axes[1], axes[2], GizmoAxis::X),
                (axes[2], axes[0], GizmoAxis::Y),
                (axes[0], axes[1], GizmoAxis::Z),
            ];
            let segs = 48usize;
            let mut best_ring = GizmoAxis::None;
            let mut best_score = f32::MAX;
            for (u_ax, v_ax, axis_id) in rings {
                for i in 0..segs {
                    let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
                    let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
                    let p0 = pivot.add(u_ax.mul(a0.cos() * scale)).add(v_ax.mul(a0.sin() * scale));
                    let p1 = pivot.add(u_ax.mul(a1.cos() * scale)).add(v_ax.mul(a1.sin() * scale));
                    let (Some((sx0, sy0)), Some((sx1, sy1))) = (
                        project_to_screen(vp, p0, vw, vh),
                        project_to_screen(vp, p1, vw, vh),
                    ) else {
                        continue;
                    };
                    let d = point_to_segment_dist(mx, my, sx0, sy0, sx1, sy1);
                    let mid = p0.add(p1).mul(0.5).sub(pivot).norm();
                    let front = mid.dot(to_cam);
                    let max_d = if front >= -0.10 { 12.0 } else { 8.0 };
                    if d <= max_d {
                        let score = d + if front < -0.10 { 6.5 } else { 0.0 };
                        if score < best_score {
                            best_score = score;
                            best_ring = axis_id;
                        }
                    }
                }
            }
            if best_ring != GizmoAxis::None {
                return best_ring;
            }

            if let Some((rx, ry)) = project_to_screen(vp, pivot.add(cam_right.mul(scale * 1.18)), vw, vh) {
                let ring_px = (rx - px).hypot(ry - py).max(14.0);
                if (center_d - ring_px).abs() <= 10.0 {
                    return GizmoAxis::Roll;
                }
                if center_d <= (ring_px * 0.90).max(18.0) {
                    return GizmoAxis::Free;
                }
            } else if center_d <= 18.0 {
                return GizmoAxis::Free;
            }
            GizmoAxis::None
        }
        ToolMode::Move => {
            if center_d <= 15.0 {
                return GizmoAxis::Free;
            }
            // Check 2D planar square handles (`[0.20..0.48] * scale`) first when facing camera
            let (ro, rd) = screen_ray(inv_vp, mx, my, vw, vh);
            let planes = [
                (axes[0], axes[1], axes[2], GizmoAxis::XY),
                (axes[0], axes[2], axes[1], GizmoAxis::XZ),
                (axes[1], axes[2], axes[0], GizmoAxis::YZ),
            ];
            let mut best_plane = GizmoAxis::None;
            let mut best_t = f32::MAX;
            for (u_ax, v_ax, n_ax, pid) in planes {
                if n_ax.dot(to_cam).abs() < 0.18 {
                    continue;
                }
                if let Some(hit) = ray_plane_intersect(ro, rd, pivot, n_ax) {
                    let rel = hit.sub(pivot);
                    let u = rel.dot(u_ax) / scale;
                    let v = rel.dot(v_ax) / scale;
                    if (0.18..=0.50).contains(&u) && (0.18..=0.50).contains(&v) {
                        let t = hit.sub(ro).len();
                        if t < best_t {
                            best_t = t;
                            best_plane = pid;
                        }
                    }
                }
            }
            if best_plane != GizmoAxis::None {
                return best_plane;
            }

            let mut best_axis = GizmoAxis::None;
            let mut best_d = 13.0f32;
            let axis_ids = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z];
            for i in 0..3 {
                if let (Some((sx, sy)), Some((tx, ty))) = (
                    project_to_screen(vp, pivot.add(axes[i].mul(0.08 * scale)), vw, vh),
                    project_to_screen(vp, pivot.add(axes[i].mul(1.08 * scale)), vw, vh),
                ) {
                    let d = point_to_segment_dist(mx, my, sx, sy, tx, ty);
                    if d < best_d {
                        best_d = d;
                        best_axis = axis_ids[i];
                    }
                }
            }
            best_axis
        }
        ToolMode::Scale => {
            if center_d <= 16.0 {
                return GizmoAxis::Free;
            }
            let mut best_axis = GizmoAxis::None;
            let mut best_d = 13.0f32;
            let axis_ids = [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z];
            for i in 0..3 {
                if let (Some((sx, sy)), Some((tx, ty))) = (
                    project_to_screen(vp, pivot.add(axes[i].mul(0.08 * scale)), vw, vh),
                    project_to_screen(vp, pivot.add(axes[i].mul(1.04 * scale)), vw, vh),
                ) {
                    let d = point_to_segment_dist(mx, my, sx, sy, tx, ty);
                    if d < best_d {
                        best_d = d;
                        best_axis = axis_ids[i];
                    }
                }
            }
            best_axis
        }
        _ => GizmoAxis::None,
    }
}

/// Transform a bone using ray-plane / axis-angle matrix composition (Rotate) or 1:1 ray lock (Move/Scale).
/// Returns signed delta degrees for Rotate drags (used for live angle readout), or `0.0` for Move/Scale.
#[allow(clippy::too_many_arguments)]
pub fn apply_gizmo_drag(
    tr: &mut BoneTransform,
    tool: ToolMode,
    axis: GizmoAxis,
    axes: [Vec3; 3],
    parent_axes: [Vec3; 3],
    cam_right: Vec3,
    cam_up: Vec3,
    cam_fwd: Vec3,
    vp: Mat4,
    inv_vp: Mat4,
    eye: Vec3,
    vw: f32,
    vh: f32,
    pivot: Vec3,
    prev_mx: f32,
    prev_my: f32,
    cur_mx: f32,
    cur_my: f32,
) -> f32 {
    let dx = cur_mx - prev_mx;
    let dy = cur_my - prev_my;
    if dx.abs() < 1e-4 && dy.abs() < 1e-4 {
        return 0.0;
    }
    let scale = gizmo_world_scale(eye, pivot);

    match tool {
        ToolMode::Move => {
            let (ro0, rd0) = screen_ray(inv_vp, prev_mx, prev_my, vw, vh);
            let (ro1, rd1) = screen_ray(inv_vp, cur_mx, cur_my, vw, vh);
            let world_delta = match axis {
                GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => {
                    let ai = match axis {
                        GizmoAxis::X => 0,
                        GizmoAxis::Y => 1,
                        _ => 2,
                    };
                    if let (Some(s0), Some(s1)) = (
                        ray_closest_param_on_line(ro0, rd0, pivot, axes[ai]),
                        ray_closest_param_on_line(ro1, rd1, pivot, axes[ai]),
                    ) {
                        axes[ai].mul((s1 - s0).clamp(-1.5, 1.5))
                    } else if let (Some((px, py)), Some((tx, ty))) = (
                        project_to_screen(vp, pivot, vw, vh),
                        project_to_screen(vp, pivot.add(axes[ai].mul(scale)), vw, vh),
                    ) {
                        let vx = tx - px;
                        let vy = ty - py;
                        let l = vx.hypot(vy).max(1.0);
                        axes[ai].mul(((dx * vx + dy * vy) / l) * scale * 0.015)
                    } else {
                        Vec3::ZERO
                    }
                }
                GizmoAxis::XY | GizmoAxis::XZ | GizmoAxis::YZ => {
                    let n_ax = match axis {
                        GizmoAxis::XY => axes[2],
                        GizmoAxis::XZ => axes[1],
                        _ => axes[0],
                    };
                    if let (Some(p0), Some(p1)) = (
                        ray_plane_intersect(ro0, rd0, pivot, n_ax),
                        ray_plane_intersect(ro1, rd1, pivot, n_ax),
                    ) {
                        let d = p1.sub(p0);
                        if d.len() > 1.8 {
                            d.norm().mul(1.8)
                        } else {
                            d
                        }
                    } else {
                        cam_right.mul(dx * scale * 0.012).add(cam_up.mul(-dy * scale * 0.012))
                    }
                }
                _ => {
                    if let (Some(p0), Some(p1)) = (
                        ray_plane_intersect(ro0, rd0, pivot, cam_fwd),
                        ray_plane_intersect(ro1, rd1, pivot, cam_fwd),
                    ) {
                        let d = p1.sub(p0);
                        if d.len() > 1.8 {
                            d.norm().mul(1.8)
                        } else {
                            d
                        }
                    } else {
                        cam_right.mul(dx * scale * 0.012).add(cam_up.mul(-dy * scale * 0.012))
                    }
                }
            };
            let px = world_delta.dot(parent_axes[0]) * 16.0;
            let py = -world_delta.dot(parent_axes[1]) * 16.0;
            let pz = world_delta.dot(parent_axes[2]) * 16.0;
            tr.pos[0] = (tr.pos[0] + px).clamp(-64.0, 64.0);
            tr.pos[1] = (tr.pos[1] + py).clamp(-64.0, 64.0);
            tr.pos[2] = (tr.pos[2] + pz).clamp(-64.0, 64.0);
            0.0
        }
        ToolMode::Scale => {
            match axis {
                GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => {
                    let ai = match axis {
                        GizmoAxis::X => 0,
                        GizmoAxis::Y => 1,
                        _ => 2,
                    };
                    let proj = if let (Some((px, py)), Some((tx, ty))) = (
                        project_to_screen(vp, pivot, vw, vh),
                        project_to_screen(vp, pivot.add(axes[ai].mul(scale)), vw, vh),
                    ) {
                        let vx = tx - px;
                        let vy = ty - py;
                        let l = vx.hypot(vy).max(1.0);
                        (dx * vx + dy * vy) / l
                    } else {
                        dx - dy
                    };
                    let factor = (1.0 + proj * 0.008).clamp(0.82, 1.22);
                    tr.scale[ai] = (tr.scale[ai] * factor).clamp(0.10, 4.0);
                }
                _ => {
                    let factor = (1.0 + (dx - dy) * 0.006).clamp(0.85, 1.18);
                    for c in 0..3 {
                        tr.scale[c] = (tr.scale[c] * factor).clamp(0.10, 4.0);
                    }
                }
            }
            0.0
        }
        _ => {
            // Exact matrix-composed 3D rotation around world or local ring axis / screen Roll / Trackball
            let r_cur = Mat4::rot_zyx_deg(tr.rot[0], tr.rot[1], tr.rot[2]);
            let apply_world_axis_angle = |u_world: Vec3, delta_rad: f32, prev: [f32; 3]| -> [f32; 3] {
                let u_parent = Vec3::new(
                    u_world.dot(parent_axes[0]),
                    u_world.dot(parent_axes[1]),
                    u_world.dot(parent_axes[2]),
                )
                .norm();
                let r_new = Mat4::rot_axis_angle(u_parent, delta_rad).mul(r_cur);
                decompose_zyx_continuous(r_new, prev)
            };

            match axis {
                GizmoAxis::X | GizmoAxis::Y | GizmoAxis::Z => {
                    let ai = match axis {
                        GizmoAxis::X => 0,
                        GizmoAxis::Y => 1,
                        _ => 2,
                    };
                    let a_vec = axes[ai];
                    let to_cam = eye.sub(pivot).norm();
                    let mut delta_rad = 0.0f32;
                    let mut used_plane = false;
                    if a_vec.dot(to_cam).abs() >= 0.22 {
                        let (ro0, rd0) = screen_ray(inv_vp, prev_mx, prev_my, vw, vh);
                        let (ro1, rd1) = screen_ray(inv_vp, cur_mx, cur_my, vw, vh);
                        if let (Some(p0), Some(p1)) = (
                            ray_plane_intersect(ro0, rd0, pivot, a_vec),
                            ray_plane_intersect(ro1, rd1, pivot, a_vec),
                        ) {
                            let d0 = p0.sub(pivot);
                            let d1 = p1.sub(pivot);
                            if d0.len() > 1e-4 && d1.len() > 1e-4 {
                                let v0 = d0.norm();
                                let v1 = d1.norm();
                                delta_rad = a_vec.dot(v0.cross(v1)).atan2(v0.dot(v1));
                                used_plane = true;
                            }
                        }
                    }
                    if !used_plane {
                        let tangent = a_vec.cross(to_cam).norm();
                        delta_rad = if let (Some((px, py)), Some((tx, ty))) = (
                            project_to_screen(vp, pivot, vw, vh),
                            project_to_screen(vp, pivot.add(tangent.mul(scale * 0.8)), vw, vh),
                        ) {
                            let vx = tx - px;
                            let vy = ty - py;
                            let l = vx.hypot(vy).max(1.0);
                            (((dx * vx + dy * vy) / l) * 0.78) * DEG2RAD
                        } else {
                            ((dx - dy) * 0.55) * DEG2RAD
                        };
                    }
                    delta_rad = delta_rad.clamp(-0.65, 0.65);
                    tr.rot = apply_world_axis_angle(a_vec, delta_rad, tr.rot);
                    delta_rad.to_degrees()
                }
                GizmoAxis::Roll => {
                    let da = if let Some((px, py)) = project_to_screen(vp, pivot, vw, vh) {
                        let a0 = (prev_my - py).atan2(prev_mx - px);
                        let a1 = (cur_my - py).atan2(cur_mx - px);
                        let mut d = a1 - a0;
                        if d > std::f32::consts::PI {
                            d -= std::f32::consts::TAU;
                        }
                        if d < -std::f32::consts::PI {
                            d += std::f32::consts::TAU;
                        }
                        -d
                    } else {
                        dx * 0.011
                    }
                    .clamp(-0.65, 0.65);
                    tr.rot = apply_world_axis_angle(cam_fwd.mul(-1.0), da, tr.rot);
                    da.to_degrees()
                }
                _ => {
                    // Camera-relative free 3D trackball via exact axis-angle composition
                    let up_p = Vec3::new(
                        cam_up.dot(parent_axes[0]),
                        cam_up.dot(parent_axes[1]),
                        cam_up.dot(parent_axes[2]),
                    )
                    .norm();
                    let rt_p = Vec3::new(
                        cam_right.dot(parent_axes[0]),
                        cam_right.dot(parent_axes[1]),
                        cam_right.dot(parent_axes[2]),
                    )
                    .norm();
                    let rx_rad = (dy * 0.011).clamp(-0.65, 0.65);
                    let ry_rad = (dx * 0.011).clamp(-0.65, 0.65);
                    let r_delta = Mat4::rot_axis_angle(up_p, ry_rad)
                        .mul(Mat4::rot_axis_angle(rt_p, rx_rad));
                    tr.rot = decompose_zyx_continuous(r_delta.mul(r_cur), tr.rot);
                    0.0
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

    /// Arbitrary-axis right-handed Rodrigues rotation matrix (radians).
    pub fn rot_axis_angle(axis: Vec3, rad: f32) -> Self {
        let u = axis.norm();
        let (s, c) = rad.sin_cos();
        let t = 1.0 - c;
        let (x, y, z) = (u.x, u.y, u.z);
        Self {
            m: [
                t * x * x + c,     t * x * y + s * z, t * x * z - s * y, 0.0,
                t * x * y - s * z, t * y * y + c,     t * y * z + s * x, 0.0,
                t * x * z + s * y, t * y * z - s * x, t * z * z + c,     0.0,
                0.0,               0.0,               0.0,               1.0,
            ],
        }
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

/// Lowest world Y coordinate across all 8 corners of a leg mesh bounding box (`[-2.1..2.1, 0..12, -2.1..2.1]`).
pub fn leg_lowest_world_y(leg_mat: Mat4) -> f32 {
    let mut min_y = f32::INFINITY;
    for &x in &[-2.1_f32, 2.1] {
        for &y in &[0.0_f32, 12.0] {
            for &z in &[-2.1_f32, 2.1] {
                let wy = leg_mat.transform_point(Vec3::new(x, y, z)).y;
                if wy < min_y {
                    min_y = wy;
                }
            }
        }
    }
    min_y
}

/// Lowest world Y across both boots (`right_leg` = 6, `left_leg` = 7).
pub fn feet_lowest_world_y(mats: &[Mat4; BONE_COUNT]) -> f32 {
    leg_lowest_world_y(mats[6]).min(leg_lowest_world_y(mats[7]))
}

/// Evaluate raw unclamped world matrices for all 10 bones.
pub fn evaluate_rig_matrices_raw(pose: &RigPose) -> [Mat4; BONE_COUNT] {
    let root_world = mc_to_world();
    let root_mc = bone_local(0.0, 24.0, 0.0, pose.bone(8));

    let waist_mc = root_mc.mul(bone_local(0.0, -12.0, 0.0, pose.bone(0)));
    let body_mc = waist_mc.mul(bone_local(0.0, -12.0, 0.0, pose.bone(1)));
    let head_mc = body_mc.mul(bone_local(0.0, 0.0, 0.0, pose.bone(2)));
    let right_arm_mc = body_mc.mul(bone_local(-5.0, 2.0, 0.0, pose.bone(3)));
    let left_arm_mc = body_mc.mul(bone_local(5.0, 2.0, 0.0, pose.bone(4)));
    let right_leg_mc = root_mc.mul(bone_local(-1.9, -12.0, 0.0, pose.bone(6)));
    let left_leg_mc = root_mc.mul(bone_local(1.9, -12.0, 0.0, pose.bone(7)));

    let right_item_mc = right_arm_mc.mul(bone_local(-1.0, 9.0, -2.0, pose.bone(5)));
    let left_item_mc = left_arm_mc.mul(bone_local(1.0, 9.0, -2.0, pose.bone(9)));

    [
        root_world.mul(waist_mc),
        root_world.mul(body_mc),
        root_world.mul(head_mc),
        root_world.mul(right_arm_mc),
        root_world.mul(left_arm_mc),
        root_world.mul(right_item_mc),
        root_world.mul(right_leg_mc),
        root_world.mul(left_leg_mc),
        root_world.mul(root_mc),
        root_world.mul(left_item_mc),
    ]
}

/// Evaluate world matrices for all 10 bones, clamping feet cleanly at or above the ground (`y >= 0.0`).
pub fn evaluate_rig_matrices(pose: &RigPose) -> [Mat4; BONE_COUNT] {
    let mut mats = evaluate_rig_matrices_raw(pose);
    let min_y = feet_lowest_world_y(&mats);
    if min_y < 0.0 {
        let lift = -min_y;
        for m in &mut mats {
            m.m[13] += lift;
        }
    }
    mats
}

/// Parent world matrix of any bone index in the 10-bone hierarchy (used for accurate World/Local translations).
pub fn bone_parent_world_matrix(bone_idx: usize, mats: &[Mat4; BONE_COUNT]) -> Mat4 {
    match bone_idx {
        8 => mc_to_world(),
        0 | 6 | 7 => mats[8],
        1 => mats[0],
        2..=4 => mats[1],
        5 => mats[3],
        9 => mats[4],
        _ => mats[8],
    }
}

pub fn pick_bone_ray(
    nx: f32,
    ny: f32,
    inv_vp: Mat4,
    mats: &[Mat4; BONE_COUNT],
    solo_weapon: bool,
) -> Option<usize> {
    pick_bone_ray_point(nx, ny, inv_vp, mats, solo_weapon).map(|(idx, _)| idx)
}

/// Exact ray-OBB pick returning both the bone index and the 3D world hit point.
pub fn pick_bone_ray_point(
    nx: f32,
    ny: f32,
    inv_vp: Mat4,
    mats: &[Mat4; BONE_COUNT],
    solo_weapon: bool,
) -> Option<(usize, Vec3)> {
    let p_near = inv_vp.transform_point(Vec3::new(nx, ny, 0.0));
    let p_far = inv_vp.transform_point(Vec3::new(nx, ny, 1.0));
    let dir = p_far.sub(p_near).norm();
    if solo_weapon {
        let pivot = Vec3::new(0.0, 0.96, 0.0);
        let dist = pivot.sub(p_near).len();
        return Some((5, p_near.add(dir.mul(dist))));
    }

    // Pick boxes in bone-local ModelPart pixel coordinates (0..=7 primary limbs, 9 offhand, 8 root ground handle fallback)
    let boxes: [(usize, [f32; 6]); 9] = [
        (0, [-4.3, -1.5, -2.4, 4.3, 1.5, 2.4]),     // waist
        (1, [-4.1, 0.0, -2.1, 4.1, 11.2, 2.1]),     // body
        (2, [-4.1, -8.1, -4.1, 4.1, 0.0, 4.1]),     // head
        (3, [-3.1, -2.0, -2.1, 1.1, 10.1, 2.1]),    // right_arm
        (4, [-1.1, -2.0, -2.1, 3.1, 10.1, 2.1]),    // left_arm
        (5, [-2.2, -5.0, -26.0, 2.2, 5.0, 6.0]),    // right_item weapon envelope
        (9, [-2.2, -2.5, -8.0, 2.2, 2.5, 3.0]),     // left_item hand anchor
        (6, [-2.1, 0.0, -2.1, 2.1, 12.0, 2.1]),     // right_leg
        (7, [-2.1, 0.0, -2.1, 2.1, 12.0, 2.1]),     // left_leg
    ];

    let test_box = |idx: usize, b: [f32; 6]| -> Option<f32> {
        let inv = mats[idx].invert();
        let ro = inv.transform_point(p_near);
        let r1 = inv.transform_point(p_near.add(dir));
        let rd = r1.sub(ro);

        let rds = [rd.x, rd.y, rd.z];
        let ros = [ro.x, ro.y, ro.z];
        let mut tmin = f32::NEG_INFINITY;
        let mut tmax = f32::INFINITY;
        for i in 0..3 {
            if rds[i].abs() < 1e-6 {
                if ros[i] < b[i] || ros[i] > b[i + 3] {
                    return None;
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
                    return None;
                }
            }
        }
        (tmax > 0.0).then(|| tmin.max(0.0))
    };

    let mut best_idx = None;
    let mut best_t = f32::INFINITY;
    for &(idx, b) in &boxes {
        if let Some(t) = test_box(idx, b) {
            if t < best_t {
                best_t = t;
                best_idx = Some(idx);
            }
        }
    }
    if best_idx.is_none() {
        if let Some(t) = test_box(8, [-7.0, -1.6, -7.0, 7.0, 0.4, 7.0]) {
            best_t = t;
            best_idx = Some(8);
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
