// VoxelMotion Studio - WGSL DirectX12/Vulkan Shaders (Sky/Studio Grid, MSAA 3D Voxel Lighting, Additive Ribbon & Gizmos)

struct CameraUniform {
    vp: mat4x4<f32>,
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,
    params: vec4<f32>, // x=mode (0 studio, 1 sakura world, 2 ocean), y=time, z=shadows (0/1), w=theme_idx (0 Precision, 1 Obsidian, 2 Classic)
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

fn aces_filmic(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
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

    let theme = cam.params.w;
    let is_neutral = theme < 0.5;
    let is_hardware = theme >= 0.5 && theme < 1.5;
    let is_daylight = theme >= 1.5 && theme < 2.5;
    let is_blueprint = theme >= 2.5 && theme < 3.5;

    if (mode > 0.5 && mode < 1.5) {
        // Mode 1: Crisp Voxel Dojo & Sakura Sea Sunset Sky (Vista Previa en Juego)
        let h = clamp(rd.y * 0.92 + 0.14, 0.0, 1.0);
        let sky_top = vec3<f32>(0.10, 0.22, 0.48);
        let sky_mid = vec3<f32>(0.36, 0.44, 0.72);
        let sky_hor = vec3<f32>(0.98, 0.70, 0.60);
        sky = mix(sky_hor, sky_mid, smoothstep(0.0, 0.34, h));
        sky = mix(sky, sky_top, smoothstep(0.28, 0.92, h));
        let sun_dir = normalize(vec3<f32>(0.42, 0.26, -0.87));
        let cos_sun = max(dot(rd, sun_dir), 0.0);
        let sun_disc = smoothstep(0.996, 0.9992, cos_sun);
        let sun_glow = pow(cos_sun, 14.0);
        sky += vec3<f32>(1.0, 0.85, 0.62) * sun_glow * 0.52;
        sky += vec3<f32>(1.0, 0.95, 0.84) * sun_disc * 0.95;

        // Crisp stylized Minecraft voxel cloud strata
        if (rd.y > 0.03) {
            let ct = rd.xz / (rd.y + 0.18);
            let qct = floor(ct * 1.4 + vec2<f32>(cam.params.y * 0.08, 0.0));
            let hash = fract(sin(dot(qct, vec2<f32>(12.9898, 78.233))) * 43758.5453);
            if (hash > 0.62) {
                let cloud_col = mix(vec3<f32>(1.0, 0.84, 0.82), vec3<f32>(0.96, 0.92, 0.98), clamp(rd.y * 2.0, 0.0, 1.0));
                sky = mix(sky, cloud_col, 0.26 * smoothstep(0.03, 0.22, rd.y));
            }
        }

        // Distant stylized Minecraft voxel ocean sea surrounding the dojo island
        if (rd.y < -0.002) {
            let tw = (-0.22 - ro.y) / rd.y;
            if (tw > 0.0 && tw < 160.0) {
                let hit = ro + rd * tw;
                let tile = floor(hit.xz * 1.15 + vec2<f32>(cam.params.y * 0.14, 0.0));
                let thash = fract(sin(dot(tile, vec2<f32>(12.9898, 78.233))) * 43758.5453);
                let dist_island = length(hit.xz);
                let coastal = 1.0 - smoothstep(5.5, 11.5, dist_island);
                let sea_deep = vec3<f32>(0.09, 0.27, 0.49);
                let sea_mid = vec3<f32>(0.14, 0.42, 0.64);
                let sea_shallow = vec3<f32>(0.22, 0.60, 0.76);
                var sea = mix(sea_deep, sea_mid, step(0.45, thash) * 0.65 + thash * 0.25);
                sea = mix(sea, sea_shallow, coastal * 0.65);
                let refl = reflect(rd, normalize(vec3<f32>((thash - 0.5) * 0.06, 1.0, 0.04)));
                let spec = pow(max(dot(refl, sun_dir), 0.0), 48.0);
                sea += vec3<f32>(1.0, 0.84, 0.60) * spec * 0.72;
                let hfog = clamp((tw - 20.0) / 88.0, 0.0, 0.75);
                sky = mix(sea, sky_hor * 0.88, hfog);
            }
        }
    } else if (mode >= 1.5) {
        // Mode 2: Ocean Horizon
        let h = clamp(rd.y * 0.9 + 0.15, 0.0, 1.0);
        let top = vec3<f32>(0.05, 0.11, 0.24);
        let hor = vec3<f32>(0.22, 0.62, 0.82);
        sky = mix(hor, top, smoothstep(0.0, 0.75, h));
    } else {
        // Mode 0: 5 Hero Studio Stages (Neutral, Hardware, Daylight Mid-Grey, CAD Blueprint, Sculpt Clay)
        let r2 = dot(in.ndc * vec2<f32>(1.0, 0.88), in.ndc * vec2<f32>(1.0, 0.88));
        let center_glow = exp(-r2 * 1.45);
        var base_dark = vec3<f32>(0.068, 0.056, 0.048);
        var core_glow = vec3<f32>(0.145, 0.118, 0.098);
        if (is_neutral) {
            base_dark = vec3<f32>(0.028, 0.031, 0.038);
            core_glow = vec3<f32>(0.072, 0.080, 0.102);
        } else if (is_hardware) {
            base_dark = vec3<f32>(0.024, 0.026, 0.031);
            core_glow = vec3<f32>(0.056, 0.060, 0.072);
        } else if (is_daylight) {
            base_dark = vec3<f32>(0.73, 0.76, 0.81);
            core_glow = vec3<f32>(0.88, 0.90, 0.94);
        } else if (is_blueprint) {
            base_dark = vec3<f32>(0.028, 0.068, 0.140);
            core_glow = vec3<f32>(0.060, 0.138, 0.258);
        }
        sky = mix(base_dark, core_glow, center_glow * 0.78);
    }

    // Intersect Y=0 studio ground plane for calibrated perspective / technical grid
    if (rd.y < -0.002 && mode < 0.5) {
        let t = -ro.y / rd.y;
        if (t > 0.0 && t < 48.0) {
            let hit = ro + rd * t;
            let dist = length(hit.xz);
            let fade = exp(-dist * dist * 0.024);

            let cell_minor = abs(fract(hit.xz * 2.0 - 0.5) - 0.5) / (fwidth(hit.xz * 2.0) + vec2<f32>(0.0001));
            let line_minor = 1.0 - min(min(cell_minor.x, cell_minor.y), 1.0);

            let cell_major = abs(fract(hit.xz * 0.5 - 0.5) - 0.5) / (fwidth(hit.xz * 0.5) + vec2<f32>(0.0001));
            let line_major = 1.0 - min(min(cell_major.x, cell_major.y), 1.0);

            var grid_col_minor = vec3<f32>(0.15, 0.13, 0.11);
            var grid_col_major = vec3<f32>(0.52, 0.30, 0.16);
            var minor_alpha = 0.22;
            var major_alpha = 0.34;
            if (is_neutral) {
                grid_col_minor = vec3<f32>(0.14, 0.15, 0.18);
                grid_col_major = vec3<f32>(0.22, 0.36, 0.62);
                minor_alpha = 0.20;
                major_alpha = 0.34;
            } else if (is_hardware) {
                grid_col_minor = vec3<f32>(0.16, 0.17, 0.20);
                grid_col_major = vec3<f32>(0.78, 0.32, 0.08);
                minor_alpha = 0.30;
                major_alpha = 0.52;
            } else if (is_daylight) {
                grid_col_minor = vec3<f32>(0.55, 0.59, 0.66);
                grid_col_major = vec3<f32>(0.26, 0.46, 0.78);
                minor_alpha = 0.24;
                major_alpha = 0.38;
            } else if (is_blueprint) {
                grid_col_minor = vec3<f32>(0.09, 0.24, 0.46);
                grid_col_major = vec3<f32>(0.22, 0.68, 0.96);
                minor_alpha = 0.48;
                major_alpha = 0.72;
            }
            sky = mix(sky, grid_col_minor, line_minor * fade * minor_alpha);
            sky = mix(sky, grid_col_major, line_major * fade * major_alpha);

            // Subtle X / Z studio floor axes
            let ax_x = 1.0 - clamp(abs(hit.z) / max(fwidth(hit.z) * 1.4, 0.004), 0.0, 1.0);
            let ax_z = 1.0 - clamp(abs(hit.x) / max(fwidth(hit.x) * 1.4, 0.004), 0.0, 1.0);
            sky = mix(sky, vec3<f32>(0.78, 0.24, 0.28), ax_x * fade * 0.42);
            sky = mix(sky, vec3<f32>(0.22, 0.52, 0.90), ax_z * fade * 0.42);
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
        normalize(vec3<f32>(0.55, 0.82, 0.65)),
        is_sakura
    );
    let fill_dir = normalize(vec3<f32>(-0.72, 0.42, -0.52));

    let ndl = max(dot(n, key_dir), 0.0);
    let fill = max(dot(n, fill_dir), 0.0) * 0.48;
    let hemi = (n.y * 0.5 + 0.5) * 0.26;

    let view_dir = normalize(cam.cam_pos.xyz - in.world_pos);
    let rim = pow(1.0 - max(dot(n, view_dir), 0.0), 2.8) * 0.40;

    let lin_albedo = srgb_to_linear(in.color.rgb);
    let theme = cam.params.w;
    var key_col = select(vec3<f32>(1.04, 0.96, 0.88), vec3<f32>(1.14, 0.98, 0.88), is_sakura);
    var theme_rim = vec3<f32>(0.90, 0.52, 0.26);
    var theme_hi = vec3<f32>(0.88, 0.48, 0.22);
    if (theme < 0.5) {
        key_col = select(vec3<f32>(1.0, 0.99, 1.0), key_col, is_sakura);
        theme_rim = vec3<f32>(0.22, 0.56, 1.0);
        theme_hi = vec3<f32>(0.04, 0.52, 1.0);
    } else if (theme < 1.5) {
        key_col = select(vec3<f32>(1.02, 0.99, 0.96), key_col, is_sakura);
        theme_rim = vec3<f32>(1.0, 0.42, 0.10);
        theme_hi = vec3<f32>(1.0, 0.45, 0.12);
    } else if (theme < 2.5) {
        key_col = select(vec3<f32>(1.06, 1.06, 1.06), key_col, is_sakura);
        theme_rim = vec3<f32>(0.28, 0.60, 0.96);
        theme_hi = vec3<f32>(0.08, 0.48, 0.92);
    } else if (theme < 3.5) {
        key_col = select(vec3<f32>(0.94, 0.99, 1.06), key_col, is_sakura);
        theme_rim = vec3<f32>(0.22, 0.78, 1.0);
        theme_hi = vec3<f32>(0.22, 0.76, 0.98);
    }
    let rim_col = select(theme_rim, vec3<f32>(1.0, 0.68, 0.84), is_sakura);

    var lit = lin_albedo * (0.32 + hemi + ndl * 0.84 * key_col + fill * vec3<f32>(0.80, 0.86, 1.0))
        + rim_col * rim * 0.30;

    // Gentle distant aerial perspective only on far scenery so hero & arena stay sharp
    if (is_sakura) {
        let d = length(in.world_pos - cam.cam_pos.xyz);
        let fog = clamp((d - 16.0) / 26.0, 0.0, 0.38);
        lit = mix(lit, srgb_to_linear(vec3<f32>(0.82, 0.64, 0.72)), fog);
    }

    var srgb = linear_to_srgb(aces_filmic(lit * 1.12));

    if (mesh.tint.w > 4.5) {
        // Training dummy / target hit-impact flash
        srgb = mix(srgb, vec3<f32>(1.0, 0.72, 0.32), clamp(mesh.tint.r, 0.0, 0.82));
    } else if (mesh.tint.w > 0.5 && mesh.tint.w < 1.5) {
        // Active bone highlight
        srgb = mix(srgb, theme_hi, 0.26);
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
