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
