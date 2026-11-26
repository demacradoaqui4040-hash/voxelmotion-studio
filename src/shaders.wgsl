// VoxelMotion Studio - WGSL DirectX12/Vulkan Shaders (Sky/Studio Grid, MSAA 3D Voxel Lighting, Additive Ribbon & Gizmos)

struct CameraUniform {
    vp: mat4x4<f32>,
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,
    params: vec4<f32>, // x=mode (0 studio, 1 sakura world, 2 ocean), y=time, z=shadows (0/1), w=aspect
};

struct MeshUniform {
    model: mat4x4<f32>,
    tint: vec4<f32>, // rgb additive tint or ghost alpha, w=highlight_mode (0 none, 1 active cyan/violet, 2 ghost, 3 unlit emissive)
};

@group(0) @binding(0)
var<uniform> cam: CameraUniform;

@group(1) @binding(0)
var<uniform> mesh: MeshUniform;

// ---- 1. Fullscreen Procedural Sky / Neon Studio Grid ----
struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_bg(@builtin(vertex_index) vid: u32) -> BgOut {
    var pts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    var out: BgOut;
    out.pos = vec4<f32>(pts[vid], 0.9999, 1.0);
    out.ndc = pts[vid];
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
}

@fragment
fn fs_bg(in: BgOut) -> @location(0) vec4<f32> {
    let p_near_h = cam.inv_vp * vec4<f32>(in.ndc, 0.0, 1.0);
    let p_far_h = cam.inv_vp * vec4<f32>(in.ndc, 1.0, 1.0);
    let p_near = p_near_h.xyz / p_near_h.w;
    let p_far = p_far_h.xyz / p_far_h.w;
    let rd = normalize(p_far - p_near);
    let ro = p_near;

    let mode = cam.params.x;
    var sky = vec3<f32>(0.0);

    if (mode > 0.5 && mode < 1.5) {
        // Mode 1: Sakura Cherry-Blossom Sunset Sky (Vista Previa en Juego)
        let h = clamp(rd.y * 0.85 + 0.18, 0.0, 1.0);
        let sky_top = vec3<f32>(0.13, 0.16, 0.34);
        let sky_mid = vec3<f32>(0.42, 0.32, 0.56);
        let sky_hor = vec3<f32>(0.92, 0.62, 0.64);
        sky = mix(sky_hor, sky_mid, smoothstep(0.0, 0.38, h));
        sky = mix(sky, sky_top, smoothstep(0.32, 0.95, h));
        let sun_dir = normalize(vec3<f32>(0.38, 0.24, -0.88));
        let sun_glow = pow(max(dot(rd, sun_dir), 0.0), 18.0);
        sky += vec3<f32>(1.0, 0.82, 0.62) * sun_glow * 0.55;
        // Soft dusk cloud bands
        let cloud_n = sin(rd.x * 9.0 + cam.params.y * 0.12) * cos(rd.y * 16.0 + rd.z * 5.0);
        sky += vec3<f32>(0.95, 0.72, 0.82) * max(cloud_n, 0.0) * smoothstep(0.05, 0.55, h) * 0.07;
    } else if (mode >= 1.5) {
        // Mode 2: Ocean Horizon
        let h = clamp(rd.y * 0.9 + 0.15, 0.0, 1.0);
        let top = vec3<f32>(0.05, 0.11, 0.24);
        let hor = vec3<f32>(0.22, 0.62, 0.82);
        sky = mix(hor, top, smoothstep(0.0, 0.75, h));
    } else {
        // Mode 0: Obsidian-Violet Studio Void matching mockup
        let r2 = dot(in.ndc * vec2<f32>(1.0, 0.9), in.ndc * vec2<f32>(1.0, 0.9));
        let center_glow = exp(-r2 * 1.45);
        let base_dark = vec3<f32>(0.043, 0.041, 0.072);
        let core_violet = vec3<f32>(0.115, 0.095, 0.205);
        sky = mix(base_dark, core_violet, center_glow * 0.82);
    }

    // Intersect Y=0 studio ground plane for crisp perspective grid in studio/ocean modes
    if (rd.y < -0.002 && mode < 0.5) {
        let t = -ro.y / rd.y;
        if (t > 0.0 && t < 48.0) {
            let hit = ro + rd * t;
            let dist = length(hit.xz);
            let fade = exp(-dist * dist * 0.022);

            let cell_minor = abs(fract(hit.xz * 2.0 - 0.5) - 0.5) / (fwidth(hit.xz * 2.0) + vec2<f32>(0.0001));
            let line_minor = 1.0 - min(min(cell_minor.x, cell_minor.y), 1.0);

            let cell_major = abs(fract(hit.xz * 0.5 - 0.5) - 0.5) / (fwidth(hit.xz * 0.5) + vec2<f32>(0.0001));
            let line_major = 1.0 - min(min(cell_major.x, cell_major.y), 1.0);

            let grid_col_minor = vec3<f32>(0.22, 0.20, 0.36);
            let grid_col_major = vec3<f32>(0.42, 0.34, 0.72);
            sky = mix(sky, grid_col_minor, line_minor * fade * 0.42);
            sky = mix(sky, grid_col_major, line_major * fade * 0.65);

            // Axis lines (X subtle crimson, Z subtle sapphire)
            let ax_x = 1.0 - clamp(abs(hit.z) / max(fwidth(hit.z) * 1.5, 0.004), 0.0, 1.0);
            let ax_z = 1.0 - clamp(abs(hit.x) / max(fwidth(hit.x) * 1.5, 0.004), 0.0, 1.0);
            sky = mix(sky, vec3<f32>(0.75, 0.22, 0.38), ax_x * fade * 0.45);
            sky = mix(sky, vec3<f32>(0.25, 0.48, 0.88), ax_z * fade * 0.45);
        }
    }

    return vec4<f32>(sky, 1.0);
}

// ---- 2. Lit 3D Voxel / Humanoid / World Shader ----
struct VertIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_n: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_lit(in: VertIn) -> VertOut {
    var out: VertOut;
    let wp = mesh.model * vec4<f32>(in.pos, 1.0);
    out.world_pos = wp.xyz;
    out.clip_pos = cam.vp * wp;
    let nmat = mat3x3<f32>(mesh.model[0].xyz, mesh.model[1].xyz, mesh.model[2].xyz);
    out.world_n = normalize(nmat * in.normal);
    out.color = in.color;
    return out;
}

@fragment
fn fs_lit(in: VertOut) -> @location(0) vec4<f32> {
    if (mesh.tint.w > 2.5 && mesh.tint.w < 3.5) {
        // Emissive / unlit gizmo / contact shadow
        return vec4<f32>(in.color.rgb * mesh.tint.rgb, in.color.a * mesh.tint.a);
    }

    let n = normalize(in.world_n);
    let is_sakura = cam.params.x > 0.5 && cam.params.x < 1.5;

    let key_dir = select(
        normalize(vec3<f32>(0.52, 0.84, 0.62)),
        normalize(vec3<f32>(0.42, 0.58, -0.70)),
        is_sakura
    );
    let fill_dir = normalize(vec3<f32>(-0.72, 0.28, -0.45));

    let ndl = max(dot(n, key_dir), 0.0);
    let fill = max(dot(n, fill_dir), 0.0) * 0.34;
    let hemi = (n.y * 0.5 + 0.5) * 0.16;

    let view_dir = normalize(cam.cam_pos.xyz - in.world_pos);
    let rim = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0) * 0.38;

    let lin_albedo = srgb_to_linear(in.color.rgb);
    let key_col = select(vec3<f32>(1.0, 0.98, 1.0), vec3<f32>(1.05, 0.88, 0.80), is_sakura);
    let rim_col = select(vec3<f32>(0.55, 0.36, 0.96), vec3<f32>(0.96, 0.56, 0.78), is_sakura);

    var lit = lin_albedo * (0.28 + hemi + ndl * 0.74 * key_col + fill * vec3<f32>(0.68, 0.74, 0.96))
        + rim_col * rim * 0.30;

    // Subtle distance atmospheric haze only inside world preview for depth
    if (is_sakura) {
        let d = length(in.world_pos - cam.cam_pos.xyz);
        let fog = clamp((d - 6.0) / 18.0, 0.0, 0.65);
        lit = mix(lit, srgb_to_linear(vec3<f32>(0.74, 0.52, 0.66)), fog);
    }

    var srgb = linear_to_srgb(lit);

    if (mesh.tint.w > 0.5 && mesh.tint.w < 1.5) {
        // Active bone highlight (electric violet-cyan edge pulse)
        srgb = mix(srgb, vec3<f32>(0.56, 0.44, 1.0), 0.26);
    } else if (mesh.tint.w >= 1.5 && mesh.tint.w < 2.5) {
        // Onion-skin previous keyframe ghost (violet)
        srgb = mix(srgb, vec3<f32>(0.56, 0.40, 0.98), 0.68);
        return vec4<f32>(srgb, 0.24);
    } else if (mesh.tint.w >= 3.5) {
        // Onion-skin next keyframe ghost (cool cyan)
        srgb = mix(srgb, vec3<f32>(0.20, 0.78, 0.98), 0.68);
        return vec4<f32>(srgb, 0.22);
    }

    return vec4<f32>(srgb, in.color.a);
}
