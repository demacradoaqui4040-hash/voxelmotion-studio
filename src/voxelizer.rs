// VoxelMotion Studio - Greedy 3D Pixel-Art Voxelizer & Humanoid/World Geometry Builder

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

/// Convert any 16x16 or 32x32 weapon RGBA sprite into a crisp 3D extruded voxel weapon mesh
/// with perimeter hidden-face culling. Oriented cleanly in hand/solo space.
pub fn voxelize_weapon_sprite(w: u32, h: u32, rgba: &[u8], archetype: &str) -> MeshData {
    let mut mesh = MeshData::new();
    let wi = w.max(1) as i32;
    let hi = h.max(1) as i32;
    let scale_px = 26.0 / (w.max(h) as f32);
    let thickness = match archetype {
        "heavy" => 1.15,
        "katana" | "sword" | "polearm" => 0.72,
        "dagger" => 0.56,
        _ => 0.70,
    };

    // Grip anchor near bottom-left of MC weapon sprites (~18% in from left, ~82% down from top)
    let gx = wi as f32 * 0.18;
    let gy = hi as f32 * 0.82;

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

    // Diagonal orientation in MC local hand pixels:
    // sprite bottom-left -> top-right rotates by +45 deg around X in hand space so blade points along -Z_mc!
    let c45 = std::f32::consts::FRAC_1_SQRT_2;
    let map_v = |u: f32, v: f32, depth: f32| -> [f32; 3] {
        // u along +X right on texture, v along +Y up on texture (flipped from image y)
        let dx = (u - gx) * scale_px;
        let dy = (gy - v) * scale_px;
        // Rotate by 45 deg so diagonal (dx=dy) lies along -Z_mc forward, perp lies along -Y_mc up!
        let forward = (dx + dy) * c45;
        let up = (dy - dx) * c45;
        [depth, -up, -forward]
    };

    let srgb_side = |c: [f32; 3], factor: f32| -> [f32; 4] {
        [
            (c[0] * factor).clamp(0.0, 1.0),
            (c[1] * factor).clamp(0.0, 1.0),
            (c[2] * factor).clamp(0.0, 1.0),
            1.0,
        ]
    };

    let half_t = thickness * 0.5;
    for y in 0..hi {
        for x in 0..wi {
            let Some(col) = sample(x, y) else { continue };
            let (u0, u1) = (x as f32, (x + 1) as f32);
            let (v0, v1) = (y as f32, (y + 1) as f32);

            let n_face = [0.0, c45, -c45];
            let _ = n_face;

            // Right side (+X_local = half_t) & Left side (-X_local = -half_t)
            let fl = map_v(u0, v0, half_t);
            let fr = map_v(u1, v0, half_t);
            let tr = map_v(u1, v1, half_t);
            let tl = map_v(u0, v1, half_t);
            mesh.push_quad(fl, fr, tr, tl, [1.0, 0.0, 0.0], srgb_side(col, 1.0));

            let bl = map_v(u0, v0, -half_t);
            let br = map_v(u1, v0, -half_t);
            let btr = map_v(u1, v1, -half_t);
            let btl = map_v(u0, v1, -half_t);
            mesh.push_quad(br, bl, btl, btr, [-1.0, 0.0, 0.0], srgb_side(col, 0.92));

            // Boundary perimeter edges only (greedy face culling of internal shared pixel walls!)
            if sample(x - 1, y).is_none() {
                mesh.push_quad(bl, fl, tl, btl, [0.0, c45, c45], srgb_side(col, 0.78));
            }
            if sample(x + 1, y).is_none() {
                mesh.push_quad(fr, br, btr, tr, [0.0, -c45, -c45], srgb_side(col, 0.85));
            }
            if sample(x, y - 1).is_none() {
                mesh.push_quad(fl, bl, br, fr, [0.0, -c45, -c45], srgb_side(col, 0.90));
            }
            if sample(x, y + 1).is_none() {
                mesh.push_quad(tl, tr, btr, btl, [0.0, c45, c45], srgb_side(col, 0.74));
            }
        }
    }
    mesh
}

/// Build the 8 Humanoid Character Bone Meshes (dark obsidian/violet stylized rig).
pub fn build_character_bone_meshes() -> [MeshData; 7] {
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
    head.push_box([-4.2, -2.8, -4.18, 4.2, 0.05, -1.2], [0.14, 0.13, 0.20]); // mask/scarf matching mockup!
    head.push_box([-2.5, -4.4, -4.22, -0.9, -3.2, -3.95], [0.12, 0.14, 0.22]); // right eye
    head.push_box([0.9, -4.4, -4.22, 2.5, -3.2, -3.95], [0.12, 0.14, 0.22]); // left eye

    let mut right_arm = MeshData::new();
    right_arm.push_box([-3.0, -2.0, -2.0, 1.0, 7.2, 2.0], [0.17, 0.18, 0.26]);
    right_arm.push_box([-3.12, 5.4, -2.12, 1.12, 7.1, 2.12], [0.58, 0.36, 0.88]); // violet-gold cuff
    right_arm.push_box([-3.0, 7.1, -2.0, 1.0, 10.0, 2.0], [0.80, 0.64, 0.54]);

    let mut left_arm = MeshData::new();
    left_arm.push_box([-1.0, -2.0, -2.0, 3.0, 7.2, 2.0], [0.17, 0.18, 0.26]);
    left_arm.push_box([-1.12, 5.4, -2.12, 3.12, 7.1, 2.12], [0.58, 0.36, 0.88]);
    left_arm.push_box([-1.0, 7.1, -2.0, 3.0, 10.0, 2.0], [0.80, 0.64, 0.54]);

    let mut right_leg = MeshData::new();
    right_leg.push_box([-2.0, 0.0, -2.0, 2.0, 8.2, 2.0], [0.13, 0.14, 0.20]);
    right_leg.push_box([-2.1, 8.2, -2.1, 2.1, 12.0, 2.1], [0.09, 0.09, 0.12]);

    let mut left_leg = MeshData::new();
    left_leg.push_box([-2.0, 0.0, -2.0, 2.0, 8.2, 2.0], [0.13, 0.14, 0.20]);
    left_leg.push_box([-2.1, 8.2, -2.1, 2.1, 12.0, 2.1], [0.09, 0.09, 0.12]);

    [waist, body, head, right_arm, left_arm, right_leg, left_leg]
}

/// Build the atmospheric stylized Sakura Temple World Backdrop for "Vista Previa en Juego".
pub fn build_sakura_world_mesh() -> MeshData {
    let mut m = MeshData::new();

    // Stone wet temple courtyard floor slabs
    m.push_box([-14.0, -0.20, -14.0, 14.0, -0.002, 14.0], [0.15, 0.16, 0.22]);
    // Center cobblestone runner blocks
    for i in -5..=5 {
        let z = i as f32 * 1.4;
        m.push_box([-1.6, -0.001, z - 0.6, 1.6, 0.015, z + 0.6], [0.21, 0.22, 0.29]);
    }
    // Stone border curbs
    m.push_box([-14.0, -0.01, -9.0, 14.0, 0.45, -8.3], [0.22, 0.21, 0.27]);

    // Japanese Pagoda / Temple silhouette behind the player (-Z background)
    m.push_box([-3.6, 0.0, -11.5, 3.6, 2.6, -7.2], [0.14, 0.11, 0.15]);
    m.push_box([-4.4, 2.5, -12.1, 4.4, 3.0, -6.7], [0.09, 0.08, 0.13]); // eave 1
    m.push_box([-2.7, 3.0, -10.8, 2.7, 4.9, -7.7], [0.16, 0.13, 0.18]);
    m.push_box([-3.4, 4.8, -11.3, 3.4, 5.25, -7.3], [0.09, 0.08, 0.13]); // eave 2
    m.push_box([-0.25, 5.25, -9.6, 0.25, 6.4, -9.1], [0.85, 0.68, 0.30]); // golden finial
    // Warm glowing temple shoji doors & lanterns
    m.push_box([-1.2, 0.3, -7.25, 1.2, 2.1, -7.15], [0.95, 0.68, 0.34]);
    m.push_box([-3.1, 0.6, -7.25, -2.3, 1.8, -7.15], [0.92, 0.62, 0.30]);
    m.push_box([2.3, 0.6, -7.25, 3.1, 1.8, -7.15], [0.92, 0.62, 0.30]);

    // Iconic Sakura Cherry-Blossom trees left & right
    let trees = [
        (-4.6f32, -5.8f32, 1.0f32),
        (5.0, -6.2, 1.05),
        (-7.4, -9.2, 1.2),
        (7.8, -9.0, 1.15),
    ];
    for (tx, tz, sc) in trees {
        // Dark twisted trunk
        m.push_box([tx - 0.32 * sc, 0.0, tz - 0.32 * sc, tx + 0.32 * sc, 2.5 * sc, tz + 0.32 * sc], [0.18, 0.12, 0.13]);
        // Lush layered pink sakura canopies
        m.push_box([tx - 1.9 * sc, 2.2 * sc, tz - 1.7 * sc, tx + 1.9 * sc, 3.4 * sc, tz + 1.7 * sc], [0.86, 0.44, 0.66]);
        m.push_box([tx - 1.4 * sc, 3.3 * sc, tz - 1.3 * sc, tx + 1.4 * sc, 4.2 * sc, tz + 1.3 * sc], [0.94, 0.56, 0.76]);
        m.push_box([tx - 2.3 * sc, 1.9 * sc, tz - 1.1 * sc, tx - 0.8 * sc, 2.9 * sc, tz + 1.1 * sc], [0.78, 0.38, 0.60]);
    }

    // Wooden torii pillars in mid-distance
    m.push_box([-2.2, 0.0, -5.6, -1.9, 2.6, -5.3], [0.58, 0.18, 0.24]);
    m.push_box([1.9, 0.0, -5.6, 2.2, 2.6, -5.3], [0.58, 0.18, 0.24]);
    m.push_box([-2.7, 2.45, -5.7, 2.7, 2.72, -5.2], [0.64, 0.20, 0.26]);

    m
}
