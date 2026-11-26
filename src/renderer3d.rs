// VoxelMotion Studio - Hardware DirectX12/Vulkan Zero-Copy wgpu 30 Multi-Viewport Renderer

use crate::math3d::{camera_basis, evaluate_rig_matrices, gizmo_axes_world, Mat4, Vec3, GizmoAxis};
use crate::model::{empty_pose, sample_clip_pose, BoneTransform, ClipDef, BONE_NAMES};
use crate::voxelizer::{build_character_bone_meshes, build_sakura_world_mesh, voxelize_weapon_sprite, MeshData, Vertex3D};
use slint::wgpu_30::wgpu;
use wgpu::util::DeviceExt;
use std::collections::HashMap;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniformData {
    vp: [f32; 16],
    inv_vp: [f32; 16],
    cam_pos: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshUniformData {
    model: [f32; 16],
    tint: [f32; 4],
}

use bytemuck::{Pod, Zeroable};

pub struct GpuMesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    index_count: u32,
}

impl GpuMesh {
    fn from_data(device: &wgpu::Device, data: &MeshData) -> Self {
        let verts = if data.vertices.is_empty() {
            vec![Vertex3D { pos: [0.0; 3], normal: [0.0, 1.0, 0.0], color: [0.0; 4] }]
        } else {
            data.vertices.clone()
        };
        let idxs = if data.indices.is_empty() { vec![0, 0, 0] } else { data.indices.clone() };
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&idxs),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self { vb, ib, index_count: idxs.len() as u32 }
    }
}

pub struct RenderTargetSurface {
    pub width: u32,
    pub height: u32,
    pub resolve_tex: wgpu::Texture,
    pub resolve_view: wgpu::TextureView,
    pub msaa_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
}

impl RenderTargetSurface {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let w = width.max(64);
        let h = height.max(64);
        let resolve_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_vp_resolve"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let resolve_view = resolve_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let msaa_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_vp_msaa"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let msaa_view = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_vp_depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            width: w,
            height: h,
            resolve_tex,
            resolve_view,
            msaa_view,
            depth_view,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: [f32; 3],
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            yaw: 0.62,
            pitch: 0.24,
            dist: 3.4,
            target: [0.0, 0.96, 0.0],
        }
    }
}

impl CameraState {
    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        Vec3::new(
            self.target[0] + self.dist * cp * self.yaw.sin(),
            self.target[1] + self.dist * self.pitch.sin(),
            self.target[2] + self.dist * cp * self.yaw.cos(),
        )
    }

    pub fn vp_and_inv(&self, aspect: f32) -> (Mat4, Mat4, Vec3) {
        let eye = self.eye();
        let proj = Mat4::perspective_wgpu(42.0 * crate::math3d::DEG2RAD, aspect, 0.05, 80.0);
        let view = Mat4::look_at(eye, Vec3::new(self.target[0], self.target[1], self.target[2]), Vec3::new(0.0, 1.0, 0.0));
        let vp = proj.mul(view);
        (vp, vp.invert(), eye)
    }
}

pub struct RenderParams<'a> {
    pub pose: &'a HashMap<String, BoneTransform>,
    pub ghost_pose: Option<&'a HashMap<String, BoneTransform>>,
    pub ghost_next_pose: Option<&'a HashMap<String, BoneTransform>>,
    pub clip: &'a ClipDef,
    pub current_time: f32,
    pub weapon_id: &'a str,
    pub selected_bone: usize,
    pub solo_weapon: bool,
    pub hidden_bones: [bool; 8],
    pub hovered_gizmo: GizmoAxis,
    pub active_gizmo: GizmoAxis,
    pub show_shadows: bool,
    pub show_onion: bool,
    pub show_trail: bool,
    pub show_gizmo: bool,
    pub bg_mode: f32, // 0=Studio, 1=Sakura, 2=Ocean
    pub impact_flash: f32,
    pub anim_clock: f32,
    pub camera: &'a CameraState,
    pub draw_world_backdrop: bool,
}

pub struct GpuRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    bg_pipeline: wgpu::RenderPipeline,
    lit_pipeline: wgpu::RenderPipeline,
    alpha_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bg: wgpu::BindGroup,
    mesh_ubo_stride: u64,
    mesh_buf: wgpu::Buffer,
    mesh_bg: wgpu::BindGroup,

    char_bones: [GpuMesh; 7],
    sakura_world: GpuMesh,
    shadow_disk: GpuMesh,
    gizmo_ring: GpuMesh,
    gizmo_axes: [GpuMesh; 4], // X red, Y green, Z blue, center handle
    weapon_cache: HashMap<String, Arc<GpuMesh>>,
    dynamic_vb: wgpu::Buffer,
    dynamic_ib: wgpu::Buffer,

    pub main_surface: RenderTargetSurface,
    pub world_surface: RenderTargetSurface,
    pub card_surface: RenderTargetSurface,
    pub thumb_surface: RenderTargetSurface,
}

fn make_axis_handle_mesh(axis: usize) -> MeshData {
    let mut m = MeshData::new();
    match axis {
        0 => {
            let c = [0.96, 0.24, 0.34];
            m.push_box([0.04, -0.014, -0.014, 0.35, 0.014, 0.014], c);
            m.push_box([0.35, -0.032, -0.032, 0.42, 0.032, 0.032], c);
        }
        1 => {
            let c = [0.22, 0.88, 0.54];
            m.push_box([-0.014, 0.04, -0.014, 0.014, 0.35, 0.014], c);
            m.push_box([-0.032, 0.35, -0.032, 0.032, 0.42, 0.032], c);
        }
        2 => {
            let c = [0.30, 0.62, 0.98];
            m.push_box([-0.014, -0.014, 0.04, 0.014, 0.014, 0.35], c);
            m.push_box([-0.032, -0.032, 0.35, 0.032, 0.032, 0.42], c);
        }
        _ => {
            m.push_box([-0.035, -0.035, -0.035, 0.035, 0.035, 0.035], [0.95, 0.94, 1.0]);
        }
    }
    m
}

fn make_ring_mesh(radius: f32, thickness: f32, segs: usize, color: [f32; 4]) -> MeshData {
    let mut m = MeshData::new();
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
        let r0 = radius - thickness * 0.5;
        let r1 = radius + thickness * 0.5;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        m.push_quad(
            [c0 * r0, s0 * r0, 0.0],
            [c0 * r1, s0 * r1, 0.0],
            [c1 * r1, s1 * r1, 0.0],
            [c1 * r0, s1 * r0, 0.0],
            [0.0, 0.0, 1.0],
            color,
        );
    }
    m
}

fn make_shadow_disk(segs: usize) -> MeshData {
    let mut m = MeshData::new();
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * std::f32::consts::TAU;
        let base = m.vertices.len() as u32;
        m.vertices.push(Vertex3D { pos: [0.0, 0.004, 0.0], normal: [0.0, 1.0, 0.0], color: [0.02, 0.01, 0.04, 0.52] });
        m.vertices.push(Vertex3D { pos: [a0.cos() * 0.45, 0.004, a0.sin() * 0.45], normal: [0.0, 1.0, 0.0], color: [0.02, 0.01, 0.04, 0.0] });
        m.vertices.push(Vertex3D { pos: [a1.cos() * 0.45, 0.004, a1.sin() * 0.45], normal: [0.0, 1.0, 0.0], color: [0.02, 0.01, 0.04, 0.0] });
        m.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    m
}

impl GpuRenderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vm_shaders"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });

        let cam_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let mesh_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<MeshUniformData>() as u64),
                },
                count: None,
            }],
        });

        let bg_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&cam_bgl)],
            immediate_size: 0,
        });

        let mesh_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&cam_bgl), Some(&mesh_bgl)],
            immediate_size: 0,
        });

        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bg_pipe"),
            layout: Some(&bg_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_bg"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_bg"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: 4, ..Default::default() },
            multiview_mask: None,
            cache: None,
        });

        let v_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        let v_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &v_attrs,
        };

        let make_mesh_pipe = |name, cull: Option<wgpu::Face>, blend, depth_write, cmp| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&mesh_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_lit"),
                    buffers: &[Some(v_layout.clone())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_lit"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(cmp),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: 4, ..Default::default() },
                multiview_mask: None,
                cache: None,
            })
        };

        let lit_pipeline = make_mesh_pipe("lit_opaque", Some(wgpu::Face::Back), None, true, wgpu::CompareFunction::Less);
        let alpha_pipeline = make_mesh_pipe("lit_alpha", None, Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Less);
        let overlay_pipeline = make_mesh_pipe("overlay_gizmo", None, Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always);

        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<CameraUniformData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cam_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cam_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: cam_buf.as_entire_binding() }],
        });

        let align = device.limits().min_uniform_buffer_offset_alignment.max(256) as u64;
        let mesh_ubo_stride = align;
        let max_slots = 64u64;
        let mesh_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: mesh_ubo_stride * max_slots,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mesh_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &mesh_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &mesh_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<MeshUniformData>() as u64),
                }),
            }],
        });

        let bones_data = build_character_bone_meshes();
        let char_bones = [
            GpuMesh::from_data(&device, &bones_data[0]),
            GpuMesh::from_data(&device, &bones_data[1]),
            GpuMesh::from_data(&device, &bones_data[2]),
            GpuMesh::from_data(&device, &bones_data[3]),
            GpuMesh::from_data(&device, &bones_data[4]),
            GpuMesh::from_data(&device, &bones_data[5]),
            GpuMesh::from_data(&device, &bones_data[6]),
        ];
        let sakura_world = GpuMesh::from_data(&device, &build_sakura_world_mesh());
        let shadow_disk = GpuMesh::from_data(&device, &make_shadow_disk(28));
        let gizmo_ring = GpuMesh::from_data(&device, &make_ring_mesh(6.8, 0.24, 64, [0.56, 0.42, 1.0, 0.78]));
        let gizmo_axes = [
            GpuMesh::from_data(&device, &make_axis_handle_mesh(0)),
            GpuMesh::from_data(&device, &make_axis_handle_mesh(1)),
            GpuMesh::from_data(&device, &make_axis_handle_mesh(2)),
            GpuMesh::from_data(&device, &make_axis_handle_mesh(3)),
        ];

        let dynamic_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dynamic_ib = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32768,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let main_surface = RenderTargetSurface::new(&device, 960, 560);
        let world_surface = RenderTargetSurface::new(&device, 640, 320);
        let card_surface = RenderTargetSurface::new(&device, 280, 150);
        let thumb_surface = RenderTargetSurface::new(&device, 144, 144);

        Self {
            device,
            queue,
            bg_pipeline,
            lit_pipeline,
            alpha_pipeline,
            overlay_pipeline,
            cam_buf,
            cam_bg,
            mesh_ubo_stride,
            mesh_buf,
            mesh_bg,
            char_bones,
            sakura_world,
            shadow_disk,
            gizmo_ring,
            gizmo_axes,
            weapon_cache: HashMap::new(),
            dynamic_vb,
            dynamic_ib,
            main_surface,
            world_surface,
            card_surface,
            thumb_surface,
        }
    }

    pub fn ensure_weapon_mesh(&mut self, id: &str, w: u32, h: u32, rgba: &[u8], archetype: &str) {
        if self.weapon_cache.contains_key(id) {
            return;
        }
        let mesh_data = voxelize_weapon_sprite(w, h, rgba, archetype);
        self.weapon_cache.insert(id.to_string(), Arc::new(GpuMesh::from_data(&self.device, &mesh_data)));
    }

    pub fn resize_main_if_needed(&mut self, w: u32, h: u32) -> bool {
        let tw = w.clamp(240, 2048);
        let th = h.clamp(160, 1440);
        if (self.main_surface.width as i32 - tw as i32).abs() > 4
            || (self.main_surface.height as i32 - th as i32).abs() > 4
        {
            self.main_surface = RenderTargetSurface::new(&self.device, tw, th);
            return true;
        }
        false
    }

    pub fn resize_world_if_needed(&mut self, w: u32, h: u32) -> bool {
        let tw = w.clamp(200, 1280);
        let th = h.clamp(120, 720);
        if (self.world_surface.width as i32 - tw as i32).abs() > 4
            || (self.world_surface.height as i32 - th as i32).abs() > 4
        {
            self.world_surface = RenderTargetSurface::new(&self.device, tw, th);
            return true;
        }
        false
    }

    fn build_trail_mesh(
        &self,
        clip: &ClipDef,
        current_time: f32,
        solo_weapon: bool,
        impact_flash: f32,
    ) -> MeshData {
        let mut m = MeshData::new();
        let t_end = current_time.max(0.01);
        let t_start = (t_end - (clip.duration * 0.20).min(0.085)).max(0.0);
        let steps = 16usize;
        let dt_step = ((t_end - t_start) / steps as f32).max(0.001);

        let tip_local = Vec3::new(0.0, -0.7, -24.5);
        let base_local = Vec3::new(0.0, -0.4, -15.5);

        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = t_start + (t_end - t_start) * (i as f32 / steps as f32);
            let mats = if solo_weapon {
                let p = sample_clip_pose(clip, t);
                let mut only = empty_pose();
                if let Some(item) = p.get("right_item") {
                    only.insert("right_item".into(), item.clone());
                }
                evaluate_rig_matrices(&only)
            } else {
                evaluate_rig_matrices(&sample_clip_pose(clip, t))
            };
            let wm = mats[5]; // right_item
            let hot = clip.impact_time.map(|imp| (t - imp).abs() < 0.035).unwrap_or(false);
            pts.push((wm.transform_point(tip_local), wm.transform_point(base_local), hot));
        }

        for i in 0..steps {
            let (tip0, base0, h0) = pts[i];
            let (tip1, base1, h1) = pts[i + 1];
            let tip_speed = tip1.sub(tip0).len() / dt_step;
            if tip_speed < 2.4 {
                continue;
            }
            let f0 = (i as f32 / steps as f32).powi(3) * 0.42;
            let f1 = ((i + 1) as f32 / steps as f32).powi(3) * 0.42;
            let c0 = if h0 { [0.98, 0.72, 0.25, f0] } else { [0.56, 0.42, 0.98, f0] };
            let c1 = if h1 { [0.98, 0.72, 0.25, f1] } else { [0.56, 0.42, 0.98, f1] };
            let base = m.vertices.len() as u32;
            m.vertices.push(Vertex3D { pos: tip0.arr(), normal: [0.0, 1.0, 0.0], color: c0 });
            m.vertices.push(Vertex3D { pos: base0.arr(), normal: [0.0, 1.0, 0.0], color: [c0[0], c0[1], c0[2], 0.0] });
            m.vertices.push(Vertex3D { pos: tip1.arr(), normal: [0.0, 1.0, 0.0], color: c1 });
            m.vertices.push(Vertex3D { pos: base1.arr(), normal: [0.0, 1.0, 0.0], color: [c1[0], c1[1], c1[2], 0.0] });
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }

        if impact_flash > 0.05 {
            let (tip, _, _) = pts[pts.len() - 1];
            let eye = Vec3::new(0.4, 1.2, 3.0); // soft forward orient
            let fwd = eye.sub(tip).norm();
            let right = Vec3::new(0.0, 1.0, 0.0).cross(fwd).norm();
            let up = fwd.cross(right);
            let ring = make_ring_mesh(0.18 + (1.0 - impact_flash) * 0.34, 0.025, 36, [1.0, 0.80, 0.28, impact_flash]);
            let offset = m.vertices.len() as u32;
            for v in ring.vertices {
                let p = tip.add(right.mul(v.pos[0])).add(up.mul(v.pos[1]));
                m.vertices.push(Vertex3D {
                    pos: p.arr(),
                    normal: v.normal,
                    color: v.color,
                });
            }
            for idx in ring.indices {
                m.indices.push(offset + idx);
            }
        }
        m
    }

    pub fn render_to_surface(&mut self, surface_idx: u8, params: &RenderParams<'_>) {
        let target = match surface_idx {
            0 => &self.main_surface,
            1 => &self.world_surface,
            2 => &self.card_surface,
            _ => &self.thumb_surface,
        };
        let aspect = target.width as f32 / target.height as f32;
        let (vp, inv_vp, eye) = params.camera.vp_and_inv(aspect);

        let cam_ubo = CameraUniformData {
            vp: vp.m,
            inv_vp: inv_vp.m,
            cam_pos: [eye.x, eye.y, eye.z, 1.0],
            params: [
                params.bg_mode,
                params.anim_clock,
                if params.show_shadows { 1.0 } else { 0.0 },
                aspect,
            ],
        };
        self.queue.write_buffer(&self.cam_buf, 0, bytemuck::bytes_of(&cam_ubo));

        // Write uniform slots dynamically
        let mut slot = 0u64;
        let write_slot = |slot_idx: u64, model: [f32; 16], tint: [f32; 4]| {
            let u = MeshUniformData { model, tint };
            self.queue.write_buffer(&self.mesh_buf, slot_idx * self.mesh_ubo_stride, bytemuck::bytes_of(&u));
            (slot_idx * self.mesh_ubo_stride) as u32
        };

        let mats = evaluate_rig_matrices(params.pose);
        let bone_order = [0usize, 1, 2, 3, 4, 6, 7];
        let mut opaque_draws: Vec<(u32, usize)> = Vec::new(); // (ubo_offset, mesh_kind)
        // mesh_kind: 0..6 = char_bones, 10 = weapon, 11 = sakura_world

        if params.draw_world_backdrop {
            let off = write_slot(slot, Mat4::IDENTITY.m, [1.0, 1.0, 1.0, 0.0]);
            opaque_draws.push((off, 11));
            slot += 1;
        }

        let weapon_model = if params.solo_weapon {
            // Showcase weapon angled horizontally from side/3-4 so full silhouette reads across frame!
            let rot_anim = Mat4::rot_zyx_deg(
                params.pose.get("right_item").map(|b| b.rot[0]).unwrap_or(0.0),
                params.pose.get("right_item").map(|b| b.rot[1]).unwrap_or(0.0),
                params.pose.get("right_item").map(|b| b.rot[2]).unwrap_or(0.0),
            );
            Mat4::translate(0.0, 0.96, 0.55)
                .mul(rot_anim)
                .mul(Mat4::rot_y(0.18))
                .mul(Mat4::rot_x(-0.14))
                .mul(Mat4::scale(1.0 / 16.0, 1.0 / 16.0, 1.0 / 16.0))
                .mul(Mat4::rot_x(std::f32::consts::PI))
        } else {
            mats[5]
        };

        if !params.solo_weapon {
            for (mesh_i, &bone_idx) in bone_order.iter().enumerate() {
                if params.hidden_bones[bone_idx] {
                    continue;
                }
                let hi = if params.selected_bone == bone_idx && surface_idx == 0 { 1.0 } else { 0.0 };
                let off = write_slot(slot, mats[bone_idx].m, [1.0, 1.0, 1.0, hi]);
                opaque_draws.push((off, mesh_i));
                slot += 1;
            }
        }
        if params.solo_weapon || !params.hidden_bones[5] {
            let w_hi = if params.selected_bone == 5 && surface_idx == 0 { 1.0 } else { 0.0 };
            let w_off = write_slot(slot, weapon_model.m, [1.0, 1.0, 1.0, w_hi]);
            opaque_draws.push((w_off, 10));
            slot += 1;
        }

        // Alpha draws (shadow + dual onion ghosts + slash ribbon)
        let mut alpha_draws: Vec<(u32, u8)> = Vec::new();
        if params.show_shadows {
            let shadow_m = Mat4::translate(0.0, 0.0, 0.0).mul(Mat4::scale(1.15, 1.0, 1.15));
            let off = write_slot(slot, shadow_m.m, [1.0, 1.0, 1.0, 3.0]);
            alpha_draws.push((off, 20)); // shadow disk
            slot += 1;
        }
        if params.show_onion && !params.solo_weapon && surface_idx == 0 {
            let mut push_ghost = |gp: &HashMap<String, BoneTransform>, mode_w: f32| {
                let gmats = evaluate_rig_matrices(gp);
                for (mesh_i, &bone_idx) in bone_order.iter().enumerate() {
                    if params.hidden_bones[bone_idx] {
                        continue;
                    }
                    let off = write_slot(slot, gmats[bone_idx].m, [1.0, 1.0, 1.0, mode_w]);
                    alpha_draws.push((off, mesh_i as u8));
                    slot += 1;
                }
                if !params.hidden_bones[5] {
                    let gwoff = write_slot(slot, gmats[5].m, [1.0, 1.0, 1.0, mode_w]);
                    alpha_draws.push((gwoff, 10));
                    slot += 1;
                }
            };
            if let Some(gp) = params.ghost_pose {
                push_ghost(gp, 2.0);
            }
            if let Some(gp_next) = params.ghost_next_pose {
                push_ghost(gp_next, 4.0);
            }
        }

        let dyn_mesh = if params.show_trail && !params.solo_weapon {
            self.build_trail_mesh(params.clip, params.current_time, false, params.impact_flash)
        } else {
            MeshData::new()
        };
        let dyn_has_tris = !dyn_mesh.indices.is_empty();
        let dyn_off = if dyn_has_tris {
            let vbytes = bytemuck::cast_slice(&dyn_mesh.vertices);
            let ibytes = bytemuck::cast_slice(&dyn_mesh.indices);
            if vbytes.len() <= 65536 && ibytes.len() <= 32768 {
                self.queue.write_buffer(&self.dynamic_vb, 0, vbytes);
                self.queue.write_buffer(&self.dynamic_ib, 0, ibytes);
                Some((write_slot(slot, Mat4::IDENTITY.m, [1.0, 1.0, 1.0, 3.0]), dyn_mesh.indices.len() as u32))
            } else {
                None
            }
        } else {
            None
        };
        if dyn_off.is_some() {
            slot += 1;
        }

        let mut gizmo_draws: Vec<(u32, usize)> = Vec::new(); // usize: 99 = ring, 0..=3 = X/Y/Z/center
        if params.show_gizmo && surface_idx == 0 {
            let bone_m = if params.solo_weapon {
                weapon_model
            } else {
                mats[params.selected_bone.min(7)]
            };
            let (pivot_world, axes) = gizmo_axes_world(bone_m, params.solo_weapon);
            let hl = if params.active_gizmo != GizmoAxis::None { params.active_gizmo } else { params.hovered_gizmo };
            let axis_tint = |axis: GizmoAxis| -> [f32; 4] {
                if hl == axis {
                    [1.75, 1.60, 0.55, 3.0]
                } else {
                    [1.0, 1.0, 1.0, 3.0]
                }
            };

            let (cam_r, cam_u, cam_f) = camera_basis(params.camera.yaw, params.camera.pitch);
            let g_ring = Mat4 {
                m: [
                    cam_r.x / 16.0, cam_r.y / 16.0, cam_r.z / 16.0, 0.0,
                    cam_u.x / 16.0, cam_u.y / 16.0, cam_u.z / 16.0, 0.0,
                    -cam_f.x / 16.0, -cam_f.y / 16.0, -cam_f.z / 16.0, 0.0,
                    pivot_world.x, pivot_world.y, pivot_world.z, 1.0,
                ],
            };
            let ring_t = if hl == GizmoAxis::Roll { [1.5, 1.4, 0.7, 3.0] } else { [1.0, 1.0, 1.0, 3.0] };
            gizmo_draws.push((write_slot(slot, g_ring.m, ring_t), 99));
            slot += 1;

            let g_axes_m = Mat4 {
                m: [
                    axes[0].x, axes[0].y, axes[0].z, 0.0,
                    axes[1].x, axes[1].y, axes[1].z, 0.0,
                    axes[2].x, axes[2].y, axes[2].z, 0.0,
                    pivot_world.x, pivot_world.y, pivot_world.z, 1.0,
                ],
            };
            let axis_kinds = [
                (0usize, GizmoAxis::X),
                (1usize, GizmoAxis::Y),
                (2usize, GizmoAxis::Z),
                (3usize, GizmoAxis::Free),
            ];
            for (kind, ax_enum) in axis_kinds {
                gizmo_draws.push((write_slot(slot, g_axes_m.m, axis_tint(ax_enum)), kind));
                slot += 1;
            }
        }
        let _ = BONE_NAMES;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&target.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.04, g: 0.04, b: 0.07, a: 1.0 }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // 1. Background / Sky / Grid
            rpass.set_pipeline(&self.bg_pipeline);
            rpass.set_bind_group(0, &self.cam_bg, &[]);
            rpass.draw(0..3, 0..1);

            // 2. Opaque Lit Geometry
            rpass.set_pipeline(&self.lit_pipeline);
            rpass.set_bind_group(0, &self.cam_bg, &[]);
            let fallback_weapon = self.weapon_cache.values().next().cloned();
            let active_weapon = self.weapon_cache.get(params.weapon_id).cloned().or(fallback_weapon);

            for (off, kind) in opaque_draws {
                rpass.set_bind_group(1, &self.mesh_bg, &[off]);
                let mesh_ref: Option<&GpuMesh> = match kind {
                    0..=6 => Some(&self.char_bones[kind]),
                    10 => active_weapon.as_deref(),
                    11 => Some(&self.sakura_world),
                    _ => None,
                };
                if let Some(gm) = mesh_ref {
                    rpass.set_vertex_buffer(0, gm.vb.slice(..));
                    rpass.set_index_buffer(gm.ib.slice(..), wgpu::IndexFormat::Uint32);
                    rpass.draw_indexed(0..gm.index_count, 0, 0..1);
                }
            }

            // 3. Translucent Shadows, Onion Skin & Dynamic Slash Trail
            rpass.set_pipeline(&self.alpha_pipeline);
            rpass.set_bind_group(0, &self.cam_bg, &[]);
            for (off, kind) in alpha_draws {
                rpass.set_bind_group(1, &self.mesh_bg, &[off]);
                let gm: Option<&GpuMesh> = match kind {
                    0..=6 => Some(&self.char_bones[kind as usize]),
                    10 => active_weapon.as_deref(),
                    20 => Some(&self.shadow_disk),
                    _ => None,
                };
                if let Some(m) = gm {
                    rpass.set_vertex_buffer(0, m.vb.slice(..));
                    rpass.set_index_buffer(m.ib.slice(..), wgpu::IndexFormat::Uint32);
                    rpass.draw_indexed(0..m.index_count, 0, 0..1);
                }
            }

            if let Some((off, cnt)) = dyn_off {
                rpass.set_bind_group(1, &self.mesh_bg, &[off]);
                rpass.set_vertex_buffer(0, self.dynamic_vb.slice(..));
                rpass.set_index_buffer(self.dynamic_ib.slice(..), wgpu::IndexFormat::Uint32);
                rpass.draw_indexed(0..cnt, 0, 0..1);
            }

            // 4. Gizmo Orbit Ring + 3D X/Y/Z Axes Overlay
            if !gizmo_draws.is_empty() {
                rpass.set_pipeline(&self.overlay_pipeline);
                rpass.set_bind_group(0, &self.cam_bg, &[]);
                for (off, kind) in gizmo_draws {
                    rpass.set_bind_group(1, &self.mesh_bg, &[off]);
                    let gm = if kind == 99 { &self.gizmo_ring } else { &self.gizmo_axes[kind] };
                    rpass.set_vertex_buffer(0, gm.vb.slice(..));
                    rpass.set_index_buffer(gm.ib.slice(..), wgpu::IndexFormat::Uint32);
                    rpass.draw_indexed(0..gm.index_count, 0, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Read back an offscreen surface texture to RGBA bytes (used ONLY for PNG export or gallery thumb generation!).
    pub fn readback_surface(&self, surface_idx: u8) -> (u32, u32, Vec<u8>) {
        let target = match surface_idx {
            0 => &self.main_surface,
            1 => &self.world_surface,
            2 => &self.card_surface,
            _ => &self.thumb_surface,
        };
        let (w, h) = (target.width, target.height);
        let unpadded = w * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = ((unpadded + align - 1) / align) * align;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.resolve_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let _ = rx.recv();
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        if let Ok(data) = slice.get_mapped_range() {
            for row in 0..h as usize {
                let start = row * padded as usize;
                out.extend_from_slice(&data[start..start + unpadded as usize]);
            }
            drop(data);
        }
        staging.unmap();
        (w, h, out)
    }
}
