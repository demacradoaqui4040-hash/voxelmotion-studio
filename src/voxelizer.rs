// VoxelMotion Studio - Greedy 3D Pixel-Art Voxelizer & Humanoid/World Geometry Builder

use crate::model::WeaponCategory;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex3D {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

pub struct MeshData {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn push_quad(&mut self, p0: [f32; 3], p1: [f32; 3], p2: [f32; 3], p3: [f32; 3], normal: [f32; 3], color: [f32; 4]) {
        let base = self.vertices.len() as u32;
        for p in [p0, p1, p2, p3] {
            self.vertices.push(Vertex3D { pos: p, normal, color });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn push_box(&mut self, b: [f32; 6], color: [f32; 3]) {
        let [x0, y0, z0, x1, y1, z1] = b;
        let c = [color[0], color[1], color[2], 1.0];
        // +X
        self.push_quad([x1,y0,z0],[x1,y1,z0],[x1,y1,z1],[x1,y0,z1], [1.0,0.0,0.0], c);
        // -X
        self.push_quad([x0,y0,z1],[x0,y1,z1],[x0,y1,z0],[x0,y0,z0], [-1.0,0.0,0.0], c);
        // +Y
        self.push_quad([x0,y1,z0],[x0,y1,z1],[x1,y1,z1],[x1,y1,z0], [0.0,1.0,0.0], c);
        // -Y
        self.push_quad([x0,y0,z1],[x0,y0,z0],[x1,y0,z0],[x1,y0,z1], [0.0,-1.0,0.0], c);
        // +Z
        self.push_quad([x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1], [0.0,0.0,1.0], c);
        // -Z
        self.push_quad([x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0], [0.0,0.0,-1.0], c);
    }
}

pub struct VoxelWeaponResult {
    pub mesh: MeshData,
    pub blade_base_mc: [f32; 3],
    pub blade_tip_mc: [f32; 3],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WeaponOrient {
    Diagonal45,
    ShieldFlat,
    BowVertical,
    FirearmHorizontal,
    FistGauntlet,
}

fn weapon_calibration(category: WeaponCategory, archetype: &str) -> (f32, f32, f32, f32, WeaponOrient) {
    match category {
        WeaponCategory::Dagas => (18.5, 0.56, 0.22, 0.78, WeaponOrient::Diagonal45),
        WeaponCategory::Espadas => (26.5, 0.72, 0.18, 0.82, WeaponOrient::Diagonal45),
        WeaponCategory::Katanas => (31.5, 0.64, 0.16, 0.84, WeaponOrient::Diagonal45),
        WeaponCategory::Mandobles => (37.5, 1.12, 0.16, 0.84, WeaponOrient::Diagonal45),
        WeaponCategory::Hachas => (29.5, 1.02, 0.20, 0.80, WeaponOrient::Diagonal45),
        WeaponCategory::Lanzas => (42.0, 0.78, 0.34, 0.66, WeaponOrient::Diagonal45),
        WeaponCategory::Escudos => (22.0, 1.45, 0.50, 0.50, WeaponOrient::ShieldFlat),
        WeaponCategory::Arcos => (26.0, 0.76, 0.48, 0.52, WeaponOrient::BowVertical),
        WeaponCategory::Fuego => (27.5, 0.94, 0.26, 0.72, WeaponOrient::FirearmHorizontal),
        WeaponCategory::Punos => (15.5, 1.10, 0.45, 0.55, WeaponOrient::FistGauntlet),
        WeaponCategory::Todas | WeaponCategory::Otros => match archetype {
            "dagger" => (18.5, 0.56, 0.22, 0.78, WeaponOrient::Diagonal45),
            "katana" => (31.5, 0.64, 0.16, 0.84, WeaponOrient::Diagonal45),
            "heavy" | "claymore" => (37.5, 1.12, 0.16, 0.84, WeaponOrient::Diagonal45),
            "polearm" | "spear" | "halberd" => (42.0, 0.78, 0.34, 0.66, WeaponOrient::Diagonal45),
            "shield" => (22.0, 1.45, 0.50, 0.50, WeaponOrient::ShieldFlat),
            "arc" | "bow" => (26.0, 0.76, 0.48, 0.52, WeaponOrient::BowVertical),
            "ranged" | "musket" | "gun" => (27.5, 0.94, 0.26, 0.72, WeaponOrient::FirearmHorizontal),
            _ => (26.0, 0.72, 0.18, 0.82, WeaponOrient::Diagonal45),
        },
    }
}

#[inline]
fn quad_normal(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> [f32; 3] {
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let nx = e1[1] * e2[2] - e1[2] * e2[1];
    let ny = e1[2] * e2[0] - e1[0] * e2[2];
    let nz = e1[0] * e2[1] - e1[1] * e2[0];
    let l = (nx * nx + ny * ny + nz * nz).sqrt();
    if l > 1e-6 {
        [nx / l, ny / l, nz / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Convert any 16x16 or 32x32 weapon RGBA sprite into a category-calibrated 3D extruded voxel mesh
/// with perimeter hidden-face culling and auto-computed blade bounds for trail alignment.
pub fn voxelize_weapon_sprite(
    w: u32,
    h: u32,
    rgba: &[u8],
    archetype: &str,
    category: WeaponCategory,
) -> VoxelWeaponResult {
    let mut mesh = MeshData::new();
    let wi = w.max(1) as i32;
    let hi = h.max(1) as i32;
    let (scale_px_base, thickness, gx_frac, gy_frac, orient) = weapon_calibration(category, archetype);
    let scale_px = scale_px_base / (w.max(h).max(1) as f32);

    let gx = wi as f32 * gx_frac;
    let gy = hi as f32 * gy_frac;

    let sample = |x: i32, y: i32| -> Option<[f32; 3]> {
        if x < 0 || y < 0 || x >= wi || y >= hi {
            return None;
        }
        let idx = ((y as usize) * (wi as usize) + (x as usize)) * 4;
        if idx + 3 >= rgba.len() || rgba[idx + 3] < 96 {
            return None;
        }
        Some([
            rgba[idx] as f32 / 255.0,
            rgba[idx + 1] as f32 / 255.0,
            rgba[idx + 2] as f32 / 255.0,
        ])
    };

    let c45 = std::f32::consts::FRAC_1_SQRT_2;
    let map_v = |u: f32, v: f32, depth: f32| -> [f32; 3] {
        let dx = (u - gx) * scale_px;
        let dy = (gy - v) * scale_px;
        match orient {
            WeaponOrient::Diagonal45 | WeaponOrient::FistGauntlet => {
                let forward = (dx + dy) * c45;
                let up = (dy - dx) * c45;
                [depth, -up, -forward]
            }
            WeaponOrient::BowVertical | WeaponOrient::FirearmHorizontal => [depth, -dy, -dx],
            WeaponOrient::ShieldFlat => [dx, -dy, -2.0 + depth],
        }
    };

    let srgb_side = |c: [f32; 3], factor: f32| -> [f32; 4] {
        [
            (c[0] * factor).clamp(0.0, 1.0),
            (c[1] * factor).clamp(0.0, 1.0),
            (c[2] * factor).clamp(0.0, 1.0),
            1.0,
        ]
    };

    let mut min_z = f32::MAX;
    let mut pixel_centers: Vec<[f32; 3]> = Vec::new();

    let half_t = thickness * 0.5;
    let (n_pos, n_neg) = if orient == WeaponOrient::ShieldFlat {
        ([0.0, 0.0, 1.0], [0.0, 0.0, -1.0])
    } else {
        ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0])
    };

    for y in 0..hi {
        for x in 0..wi {
            let Some(col) = sample(x, y) else { continue };
            let (u0, u1) = (x as f32, (x + 1) as f32);
            let (v0, v1) = (y as f32, (y + 1) as f32);

            let c = map_v(u0 + 0.5, v0 + 0.5, 0.0);
            if c[2] < min_z {
                min_z = c[2];
            }
            pixel_centers.push(c);

            let fl = map_v(u0, v0, half_t);
            let fr = map_v(u1, v0, half_t);
            let tr = map_v(u1, v1, half_t);
            let tl = map_v(u0, v1, half_t);
            mesh.push_quad(fl, fr, tr, tl, n_pos, srgb_side(col, 1.0));

            let bl = map_v(u0, v0, -half_t);
            let br = map_v(u1, v0, -half_t);
            let btr = map_v(u1, v1, -half_t);
            let btl = map_v(u0, v1, -half_t);
            mesh.push_quad(br, bl, btl, btr, n_neg, srgb_side(col, 0.92));

            if sample(x - 1, y).is_none() {
                mesh.push_quad(bl, fl, tl, btl, quad_normal(bl, fl, tl), srgb_side(col, 0.78));
            }
            if sample(x + 1, y).is_none() {
                mesh.push_quad(fr, br, btr, tr, quad_normal(fr, br, btr), srgb_side(col, 0.85));
            }
            if sample(x, y - 1).is_none() {
                mesh.push_quad(fl, bl, br, fr, quad_normal(fl, bl, br), srgb_side(col, 0.90));
            }
            if sample(x, y + 1).is_none() {
                mesh.push_quad(tl, tr, btr, btl, quad_normal(tl, tr, btr), srgb_side(col, 0.74));
            }
        }
    }

    let (blade_base_mc, blade_tip_mc) = match orient {
        WeaponOrient::ShieldFlat | WeaponOrient::FistGauntlet => ([0.0, 0.0, -3.5], [0.0, 0.0, -6.5]),
        _ => {
            if pixel_centers.is_empty() || !min_z.is_finite() {
                ([0.0, -0.4, -12.0], [0.0, -0.7, -22.0])
            } else {
                let mut tip_y_sum = 0.0f32;
                let mut tip_count = 0u32;
                for c in &pixel_centers {
                    if (c[2] - min_z).abs() <= 2.5 {
                        tip_y_sum += c[1];
                        tip_count += 1;
                    }
                }
                let tip_y = if tip_count > 0 { tip_y_sum / tip_count as f32 } else { -0.5 };
                let tip_z = min_z.min(-6.0);
                let base_z = (tip_z * 0.45).min(-4.0).max(tip_z + 2.0);
                ([0.0, tip_y * 0.55, base_z], [0.0, tip_y, tip_z])
            }
        }
    };

    VoxelWeaponResult {
        mesh,
        blade_base_mc,
        blade_tip_mc,
    }
}

/// Build the 7 stylized obsidian/violet character bone meshes (supports classic 4px or slim 3px arms).
pub fn build_character_bone_meshes() -> [MeshData; 7] {
    build_character_bone_meshes_with_slim(false)
}

/// Build the stylized humanoid character meshes with either classic (4px) or slim Alex (3px) arms.
pub fn build_character_bone_meshes_with_slim(slim_arms: bool) -> [MeshData; 7] {
    let mut waist = MeshData::new();
    waist.push_box([-4.2, -1.2, -2.2, 4.2, 1.2, 2.2], [0.14, 0.13, 0.20]);
    waist.push_box([-1.2, -1.35, -2.35, 1.2, 1.05, -2.0], [0.58, 0.34, 0.92]); // violet belt clasp

    let mut body = MeshData::new();
    body.push_box([-4.0, 0.0, -2.0, 4.0, 12.0, 2.0], [0.16, 0.17, 0.25]); // main coat
    body.push_box([-4.2, 0.0, -2.2, 4.2, 2.6, 2.2], [0.11, 0.11, 0.17]);   // collar/yoke
    body.push_box([-0.75, 0.4, -2.22, 0.75, 11.2, -1.95], [0.68, 0.44, 0.95]); // violet front trim

    let mut head = MeshData::new();
    head.push_box([-4.0, -8.0, -4.0, 4.0, 0.0, 4.0], [0.80, 0.64, 0.54]); // skin
    head.push_box([-4.2, -8.2, -4.2, 4.2, -5.0, 4.2], [0.10, 0.09, 0.14]); // dark hair hood
    head.push_box([-4.2, -5.2, 0.8, 4.2, -0.4, 4.2], [0.10, 0.09, 0.14]);  // rear hair
    head.push_box([-4.2, -2.8, -4.18, 4.2, 0.05, -1.2], [0.14, 0.13, 0.20]); // mask/scarf
    head.push_box([-2.5, -4.4, -4.22, -0.9, -3.2, -3.95], [0.12, 0.14, 0.22]); // right eye
    head.push_box([0.9, -4.4, -4.22, 2.5, -3.2, -3.95], [0.12, 0.14, 0.22]); // left eye

    let (rx0, rx1, lx0, lx1) = if slim_arms {
        (-2.0, 1.0, -1.0, 2.0)
    } else {
        (-3.0, 1.0, -1.0, 3.0)
    };

    let mut right_arm = MeshData::new();
    right_arm.push_box([rx0, -2.0, -2.0, rx1, 7.2, 2.0], [0.17, 0.18, 0.26]);
    right_arm.push_box([rx0 - 0.12, 5.4, -2.12, rx1 + 0.12, 7.1, 2.12], [0.58, 0.36, 0.88]); // cuff
    right_arm.push_box([rx0, 7.1, -2.0, rx1, 10.0, 2.0], [0.80, 0.64, 0.54]);

    let mut left_arm = MeshData::new();
    left_arm.push_box([lx0, -2.0, -2.0, lx1, 7.2, 2.0], [0.17, 0.18, 0.26]);
    left_arm.push_box([lx0 - 0.12, 5.4, -2.12, lx1 + 0.12, 7.1, 2.12], [0.58, 0.36, 0.88]);
    left_arm.push_box([lx0, 7.1, -2.0, lx1, 10.0, 2.0], [0.80, 0.64, 0.54]);

    let mut right_leg = MeshData::new();
    right_leg.push_box([-2.0, 0.0, -2.0, 2.0, 8.2, 2.0], [0.13, 0.14, 0.20]);
    right_leg.push_box([-2.1, 8.2, -2.1, 2.1, 12.0, 2.1], [0.09, 0.09, 0.12]);

    let mut left_leg = MeshData::new();
    left_leg.push_box([-2.0, 0.0, -2.0, 2.0, 8.2, 2.0], [0.13, 0.14, 0.20]);
    left_leg.push_box([-2.1, 8.2, -2.1, 2.1, 12.0, 2.1], [0.09, 0.09, 0.12]);

    [waist, body, head, right_arm, left_arm, right_leg, left_leg]
}

fn normalize_mc_skin(rgba: &[u8], w: u32, h: u32) -> (u32, u32, Vec<u8>) {
    let sw = 64u32;
    let sh = if w >= h.saturating_mul(2) { 32u32 } else { 64u32 };
    if w == sw && h == sh && rgba.len() >= (sw * sh * 4) as usize {
        return (sw, sh, rgba[..(sw * sh * 4) as usize].to_vec());
    }
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for ny in 0..sh {
        let sy = ((ny * h.max(1)) / sh).min(h.saturating_sub(1));
        for nx in 0..sw {
            let sx = ((nx * w.max(1)) / sw).min(w.saturating_sub(1));
            let src = ((sy * w.max(1) + sx) * 4) as usize;
            let dst = ((ny * sw + nx) * 4) as usize;
            if src + 3 < rgba.len() {
                out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
            }
        }
    }
    (sw, sh, out)
}

/// Detect whether a Minecraft skin uses the Alex slim (3px arm) UV format by inspecting right-arm unused columns.
pub fn detect_skin_slim_heuristic(rgba: &[u8], w: u32, h: u32) -> bool {
    let (sw, sh, skin) = normalize_mc_skin(rgba, w, h);
    let mut wide_opaque = false;
    let mut inner_opaque = false;
    for v in 20u32..32u32.min(sh) {
        for u in 54u32..56u32.min(sw) {
            let idx = ((v * sw + u) * 4 + 3) as usize;
            if idx < skin.len() && skin[idx] >= 16 {
                wide_opaque = true;
            }
        }
        for u in 44u32..47u32.min(sw) {
            let idx = ((v * sw + u) * 4 + 3) as usize;
            if idx < skin.len() && skin[idx] >= 128 {
                inner_opaque = true;
            }
        }
    }
    !wide_opaque && inner_opaque
}

fn push_mc_textured_box(
    mesh: &mut MeshData,
    skin: &[u8],
    sw: u32,
    sh: u32,
    bounds: [f32; 6],
    dims: (u32, u32, u32),
    uv: (u32, u32),
    mirror_x: bool,
    is_overlay: bool,
) {
    let [x0, y0, z0, x1, y1, z1] = bounds;
    let (w, h, d) = dims;
    let (tx, ty) = uv;
    let px_x = |i: u32| -> f32 { x0 + (i as f32 / w as f32) * (x1 - x0) };
    let px_y = |j: u32| -> f32 { y0 + (j as f32 / h as f32) * (y1 - y0) };
    let px_z = |k: u32| -> f32 { z0 + (k as f32 / d as f32) * (z1 - z0) };

    let sample = |u: u32, v: u32| -> Option<[f32; 4]> {
        if u >= sw || v >= sh {
            return if is_overlay { None } else { Some([0.78, 0.62, 0.52, 1.0]) };
        }
        let idx = ((v * sw + u) * 4) as usize;
        if idx + 3 >= skin.len() {
            return if is_overlay { None } else { Some([0.78, 0.62, 0.52, 1.0]) };
        }
        let a = skin[idx + 3];
        if is_overlay {
            if a < 128 {
                return None;
            }
        } else if a < 16 {
            return Some([0.78, 0.62, 0.52, 1.0]);
        }
        Some([
            skin[idx] as f32 / 255.0,
            skin[idx + 1] as f32 / 255.0,
            skin[idx + 2] as f32 / 255.0,
            1.0,
        ])
    };

    // 1. Front (z = z0, normal [0, 0, -1])
    for j in 0..h {
        let ya = px_y(j);
        let yb = px_y(j + 1);
        for i in 0..w {
            let u = tx + d + if mirror_x { w - 1 - i } else { i };
            let v = ty + d + j;
            if let Some(c) = sample(u, v) {
                let xa = px_x(i);
                let xb = px_x(i + 1);
                mesh.push_quad([xa, ya, z0], [xa, yb, z0], [xb, yb, z0], [xb, ya, z0], [0.0, 0.0, -1.0], c);
            }
        }
    }

    // 2. Back (z = z1, normal [0, 0, +1])
    for j in 0..h {
        let ya = px_y(j);
        let yb = px_y(j + 1);
        for i in 0..w {
            let u = tx + d + w + d + if mirror_x { w - 1 - i } else { i };
            let v = ty + d + j;
            if let Some(c) = sample(u, v) {
                let xa = px_x(w - 1 - i);
                let xb = px_x(w - i);
                mesh.push_quad([xa, ya, z1], [xb, ya, z1], [xb, yb, z1], [xa, yb, z1], [0.0, 0.0, 1.0], c);
            }
        }
    }

    // 3. Right side of bone (x = x0, normal [-1, 0, 0])
    for j in 0..h {
        let ya = px_y(j);
        let yb = px_y(j + 1);
        for k in 0..d {
            let u = if mirror_x { tx + d + w + (d - 1 - k) } else { tx + k };
            let v = ty + d + j;
            if let Some(c) = sample(u, v) {
                let za = px_z(d - 1 - k);
                let zb = px_z(d - k);
                mesh.push_quad([x0, ya, zb], [x0, yb, zb], [x0, yb, za], [x0, ya, za], [-1.0, 0.0, 0.0], c);
            }
        }
    }

    // 4. Left side of bone (x = x1, normal [+1, 0, 0])
    for j in 0..h {
        let ya = px_y(j);
        let yb = px_y(j + 1);
        for k in 0..d {
            let u = if mirror_x { tx + (d - 1 - k) } else { tx + d + w + k };
            let v = ty + d + j;
            if let Some(c) = sample(u, v) {
                let za = px_z(k);
                let zb = px_z(k + 1);
                mesh.push_quad([x1, ya, za], [x1, yb, za], [x1, yb, zb], [x1, ya, zb], [1.0, 0.0, 0.0], c);
            }
        }
    }

    // 5. Top (y = y0, normal [0, -1, 0] -> world UP)
    for k in 0..d {
        let za = px_z(d - 1 - k);
        let zb = px_z(d - k);
        for i in 0..w {
            let u = tx + d + if mirror_x { w - 1 - i } else { i };
            let v = ty + k;
            if let Some(c) = sample(u, v) {
                let xa = px_x(i);
                let xb = px_x(i + 1);
                mesh.push_quad([xa, y0, zb], [xa, y0, za], [xb, y0, za], [xb, y0, zb], [0.0, -1.0, 0.0], c);
            }
        }
    }

    // 6. Bottom (y = y1, normal [0, +1, 0] -> world DOWN)
    for k in 0..d {
        let za = px_z(k);
        let zb = px_z(k + 1);
        for i in 0..w {
            let u = tx + d + w + if mirror_x { w - 1 - i } else { i };
            let v = ty + k;
            if let Some(c) = sample(u, v) {
                let xa = px_x(i);
                let xb = px_x(i + 1);
                mesh.push_quad([xa, y1, za], [xa, y1, zb], [xb, y1, zb], [xb, y1, za], [0.0, 1.0, 0.0], c);
            }
        }
    }
}

#[inline]
fn inflate_bounds(b: [f32; 6], pad: f32) -> [f32; 6] {
    [b[0] - pad, b[1] - pad, b[2] - pad, b[3] + pad, b[4] + pad, b[5] + pad]
}

/// Build crisp pixel-exact 3D character meshes from any 64x64 (or legacy 64x32 / HD) Minecraft skin PNG.
pub fn build_skin_character_meshes(
    rgba: &[u8],
    w: u32,
    h: u32,
    slim_arms: bool,
    outer_layer: bool,
) -> [MeshData; 7] {
    let (sw, sh, skin) = normalize_mc_skin(rgba, w, h);
    let waist = MeshData::new();

    let mut body = MeshData::new();
    let body_b = [-4.0, 0.0, -2.0, 4.0, 12.0, 2.0];
    push_mc_textured_box(&mut body, &skin, sw, sh, body_b, (8, 12, 4), (16, 16), false, false);
    if outer_layer && sh >= 64 {
        push_mc_textured_box(&mut body, &skin, sw, sh, inflate_bounds(body_b, 0.25), (8, 12, 4), (16, 32), false, true);
    }

    let mut head = MeshData::new();
    let head_b = [-4.0, -8.0, -4.0, 4.0, 0.0, 4.0];
    push_mc_textured_box(&mut head, &skin, sw, sh, head_b, (8, 8, 8), (0, 0), false, false);
    if outer_layer {
        push_mc_textured_box(&mut head, &skin, sw, sh, inflate_bounds(head_b, 0.50), (8, 8, 8), (32, 0), false, true);
    }

    let arm_w = if slim_arms { 3 } else { 4 };
    let r_arm_b = if slim_arms { [-2.0, -2.0, -2.0, 1.0, 10.0, 2.0] } else { [-3.0, -2.0, -2.0, 1.0, 10.0, 2.0] };
    let mut right_arm = MeshData::new();
    push_mc_textured_box(&mut right_arm, &skin, sw, sh, r_arm_b, (arm_w, 12, 4), (40, 16), false, false);
    if outer_layer && sh >= 64 {
        push_mc_textured_box(&mut right_arm, &skin, sw, sh, inflate_bounds(r_arm_b, 0.25), (arm_w, 12, 4), (40, 32), false, true);
    }

    let l_arm_b = if slim_arms { [-1.0, -2.0, -2.0, 2.0, 10.0, 2.0] } else { [-1.0, -2.0, -2.0, 3.0, 10.0, 2.0] };
    let mut left_arm = MeshData::new();
    if sh >= 64 {
        push_mc_textured_box(&mut left_arm, &skin, sw, sh, l_arm_b, (arm_w, 12, 4), (32, 48), false, false);
        if outer_layer {
            push_mc_textured_box(&mut left_arm, &skin, sw, sh, inflate_bounds(l_arm_b, 0.25), (arm_w, 12, 4), (48, 48), false, true);
        }
    } else {
        push_mc_textured_box(&mut left_arm, &skin, sw, sh, l_arm_b, (arm_w, 12, 4), (40, 16), true, false);
    }

    let leg_b = [-2.0, 0.0, -2.0, 2.0, 12.0, 2.0];
    let mut right_leg = MeshData::new();
    push_mc_textured_box(&mut right_leg, &skin, sw, sh, leg_b, (4, 12, 4), (0, 16), false, false);
    if outer_layer && sh >= 64 {
        push_mc_textured_box(&mut right_leg, &skin, sw, sh, inflate_bounds(leg_b, 0.25), (4, 12, 4), (0, 32), false, true);
    }

    let mut left_leg = MeshData::new();
    if sh >= 64 {
        push_mc_textured_box(&mut left_leg, &skin, sw, sh, leg_b, (4, 12, 4), (16, 48), false, false);
        if outer_layer {
            push_mc_textured_box(&mut left_leg, &skin, sw, sh, inflate_bounds(leg_b, 0.25), (4, 12, 4), (0, 48), false, true);
        }
    } else {
        push_mc_textured_box(&mut left_leg, &skin, sw, sh, leg_b, (4, 12, 4), (0, 16), true, false);
    }

    [waist, body, head, right_arm, left_arm, right_leg, left_leg]
}

/// Build a vibrant Minecraft-voxel Sakura Dojo Courtyard & Pier Arena for "Vista Previa en Juego".
pub fn build_sakura_world_mesh() -> MeshData {
    let mut m = MeshData::new();

    // Raised stone foundation under the courtyard island
    m.push_box([-7.5, -0.35, -7.5, 7.5, -0.02, 7.5], [0.16, 0.17, 0.22]);

    // Lush emerald moss-grass perimeter surround
    for gx in -7i32..7 {
        for gz in -7i32..7 {
            if (-4..4).contains(&gx) && (-4..4).contains(&gz) {
                continue;
            }
            let x0 = gx as f32;
            let z0 = gz as f32;
            let checker = (gx + gz).rem_euclid(2) == 0;
            let grass_top = if checker { [0.22, 0.52, 0.30] } else { [0.19, 0.46, 0.27] };
            m.push_box([x0, -0.02, z0, x0 + 0.98, 0.01, z0 + 0.98], grass_top);
        }
    }

    // Crisp tiled Dojo Stone Courtyard (`[-4..4]` meters with combat timber center walkway)
    for tx in -4i32..4 {
        for tz in -4i32..4 {
            let x0 = tx as f32;
            let z0 = tz as f32;
            let is_bridge = (-1..1).contains(&tx) && (-2..3).contains(&tz);
            let col = if is_bridge {
                if (tx + tz).rem_euclid(2) == 0 {
                    [0.44, 0.30, 0.20] // cherry-oak combat plank A
                } else {
                    [0.38, 0.25, 0.17] // cherry-oak combat plank B
                }
            } else if (tx + tz).rem_euclid(2) == 0 {
                [0.28, 0.30, 0.38] // polished stone slate
            } else {
                [0.23, 0.25, 0.32]
            };
            m.push_box([x0 + 0.02, -0.005, z0 + 0.02, x0 + 0.98, 0.015, z0 + 0.98], col);
        }
    }

    // Carved courtyard edge curbs + corner mossy accent blocks
    m.push_box([-4.2, -0.01, -4.2, 4.2, 0.12, -3.95], [0.34, 0.36, 0.44]);
    m.push_box([-4.2, -0.01, 3.95, 4.2, 0.12, 4.2], [0.34, 0.36, 0.44]);
    m.push_box([-4.2, -0.01, -4.0, -3.95, 0.12, 4.0], [0.34, 0.36, 0.44]);
    m.push_box([3.95, -0.01, -4.0, 4.2, 0.12, 4.0], [0.34, 0.36, 0.44]);

    // Japanese Pagoda / Dojo Shrine behind the player (-Z background)
    m.push_box([-3.8, 0.0, -10.8, 3.8, 0.35, -6.5], [0.24, 0.25, 0.31]); // stone plinth
    m.push_box([-3.3, 0.35, -10.4, 3.3, 2.7, -6.9], [0.20, 0.13, 0.16]); // lower timber wall
    m.push_box([-4.3, 2.65, -11.2, 4.3, 3.05, -6.2], [0.11, 0.10, 0.16]); // eave 1
    m.push_box([-4.6, 2.95, -11.5, 4.6, 3.16, -5.9], [0.15, 0.14, 0.22]); // upturned rim
    m.push_box([-2.4, 3.15, -9.8, 2.4, 5.0, -7.2], [0.22, 0.15, 0.19]); // upper tier
    m.push_box([-3.3, 4.9, -10.5, 3.3, 5.32, -6.6], [0.11, 0.10, 0.16]); // eave 2
    m.push_box([-0.22, 5.32, -8.75, 0.22, 6.55, -8.3], [0.95, 0.76, 0.34]); // golden spire
    // Warm glowing shoji doors & windows
    m.push_box([-1.15, 0.4, -6.94, 1.15, 2.25, -6.82], [1.0, 0.78, 0.40]);
    m.push_box([-2.8, 0.7, -6.94, -2.0, 1.9, -6.82], [0.98, 0.70, 0.34]);
    m.push_box([2.0, 0.7, -6.94, 2.8, 1.9, -6.82], [0.98, 0.70, 0.34]);

    // Ornate Vermilion Torii Gate + Flanking Stone Toro Lanterns
    m.push_box([-2.15, 0.0, -4.6, -1.80, 2.7, -4.25], [0.76, 0.20, 0.22]);
    m.push_box([1.80, 0.0, -4.6, 2.15, 2.7, -4.25], [0.76, 0.20, 0.22]);
    m.push_box([-2.75, 2.52, -4.72, 2.75, 2.82, -4.15], [0.84, 0.22, 0.24]);
    m.push_box([-2.95, 2.80, -4.78, 2.95, 2.96, -4.08], [0.14, 0.13, 0.20]);
    m.push_box([-2.4, 2.08, -4.55, 2.4, 2.28, -4.30], [0.72, 0.19, 0.21]);

    for lx in [-2.65f32, 2.65f32] {
        let lz = -3.1f32;
        m.push_box([lx - 0.24, 0.0, lz - 0.24, lx + 0.24, 0.78, lz + 0.24], [0.34, 0.36, 0.42]);
        m.push_box([lx - 0.20, 0.78, lz - 0.20, lx + 0.20, 1.18, lz + 0.20], [1.0, 0.82, 0.38]); // warm lantern core
        m.push_box([lx - 0.34, 1.18, lz - 0.34, lx + 0.34, 1.38, lz + 0.34], [0.22, 0.24, 0.30]);
    }

    // Multi-tone Voxel Sakura Trees + Floating Blossom Petals
    let trees = [
        (-4.7f32, -5.0f32, 1.05f32),
        (4.8, -4.8, 1.1),
        (-5.6, 1.8, 0.95),
        (5.6, 2.2, 1.0),
    ];
    for (tx, tz, sc) in trees {
        m.push_box([tx - 0.32 * sc, 0.0, tz - 0.32 * sc, tx + 0.32 * sc, 2.5 * sc, tz + 0.32 * sc], [0.24, 0.14, 0.16]);
        m.push_box([tx - 0.24 * sc, 2.2 * sc, tz - 0.24 * sc, tx + 0.8 * sc, 3.0 * sc, tz + 0.24 * sc], [0.24, 0.14, 0.16]);
        m.push_box([tx - 1.9 * sc, 2.2 * sc, tz - 1.7 * sc, tx + 1.9 * sc, 3.3 * sc, tz + 1.7 * sc], [0.80, 0.34, 0.58]);
        m.push_box([tx - 1.5 * sc, 3.2 * sc, tz - 1.4 * sc, tx + 1.5 * sc, 4.1 * sc, tz + 1.4 * sc], [0.95, 0.54, 0.76]);
        m.push_box([tx - 0.9 * sc, 3.9 * sc, tz - 0.9 * sc, tx + 1.1 * sc, 4.55 * sc, tz + 0.9 * sc], [1.0, 0.72, 0.88]);
    }

    // Subtle suspended sakura blossom petals framing the courtyard
    for (px, py, pz) in [
        (-1.8f32, 1.85f32, -1.6f32),
        (1.9, 2.15, -1.2),
        (-2.4, 1.35, 0.9),
        (2.3, 1.60, 1.4),
        (0.9, 2.35, -2.5),
        (-1.1, 2.45, 2.1),
    ] {
        m.push_box([px, py, pz, px + 0.09, py + 0.05, pz + 0.09], [1.0, 0.70, 0.86]);
    }

    m
}

/// Stylized Minecraft-voxel Dojo Training Dummy standing at melee strike distance for live hit feedback.
pub fn build_training_dummy_mesh() -> MeshData {
    let mut m = MeshData::new();
    // Crossed oak beam base + collar brace
    m.push_box([-0.38, 0.0, -0.10, 0.38, 0.08, 0.10], [0.30, 0.20, 0.14]);
    m.push_box([-0.10, 0.0, -0.38, 0.10, 0.08, 0.38], [0.30, 0.20, 0.14]);
    m.push_box([-0.15, 0.08, -0.15, 0.15, 0.16, 0.15], [0.24, 0.25, 0.30]);
    // Main oak stand post + iron reinforcement ring
    m.push_box([-0.075, 0.16, -0.075, 0.075, 1.44, 0.075], [0.38, 0.26, 0.17]);
    m.push_box([-0.095, 0.56, -0.095, 0.095, 0.64, 0.095], [0.20, 0.21, 0.26]);
    // Crossbeam arms + rope cuffs
    m.push_box([-0.46, 1.15, -0.06, 0.46, 1.29, 0.06], [0.42, 0.28, 0.18]);
    m.push_box([-0.44, 1.14, -0.075, -0.34, 1.30, 0.075], [0.70, 0.56, 0.30]);
    m.push_box([0.34, 1.14, -0.075, 0.44, 1.30, 0.075], [0.70, 0.56, 0.30]);
    // Woven straw barrel torso + oak stave ribs
    m.push_box([-0.23, 0.68, -0.165, 0.23, 1.36, 0.165], [0.74, 0.58, 0.31]);
    m.push_box([-0.24, 0.68, -0.175, -0.17, 1.36, 0.175], [0.48, 0.34, 0.20]);
    m.push_box([0.17, 0.68, -0.175, 0.24, 1.36, 0.175], [0.48, 0.34, 0.20]);
    // Crimson dojo waist sash + cream & scarlet chest target emblem facing player (-Z)
    m.push_box([-0.245, 0.80, -0.18, 0.245, 0.94, 0.18], [0.78, 0.18, 0.24]);
    m.push_box([-0.14, 1.00, -0.18, 0.14, 1.28, -0.15], [0.92, 0.88, 0.78]);
    m.push_box([-0.08, 1.06, -0.19, 0.08, 1.22, -0.16], [0.86, 0.18, 0.22]);
    // Carved dummy head + crimson hachimaki headband + straw kasa hat trim
    m.push_box([-0.17, 1.40, -0.17, 0.17, 1.74, 0.17], [0.80, 0.66, 0.36]);
    m.push_box([-0.18, 1.60, -0.18, 0.18, 1.67, 0.18], [0.82, 0.18, 0.24]);
    m.push_box([-0.10, 1.50, -0.18, -0.04, 1.56, -0.16], [0.16, 0.13, 0.16]);
    m.push_box([0.04, 1.50, -0.18, 0.10, 1.56, -0.16], [0.16, 0.13, 0.16]);
    m.push_box([-0.27, 1.72, -0.27, 0.27, 1.79, 0.27], [0.64, 0.50, 0.26]);
    m.push_box([-0.16, 1.79, -0.16, 0.16, 1.87, 0.16], [0.56, 0.43, 0.22]);
    m
}
