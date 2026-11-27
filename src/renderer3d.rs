// VoxelMotion Studio - Hardware DirectX12/Vulkan Zero-Copy wgpu 30 Multi-Viewport Renderer

use crate::math3d::{camera_basis, evaluate_rig_matrices, gizmo_axes_world, GizmoAxis, Mat4, Vec3};
use crate::model::{sample_clip_pose, ClipDef, RigPose, VfxKind, BONE_COUNT};
use crate::voxelizer::{
    build_character_bone_meshes, build_sakura_world_mesh, build_training_dummy_mesh,
    voxelize_weapon_sprite, MeshData, Vertex3D,
};
use slint::wgpu_30::wgpu;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const MAX_CACHED_WEAPON_MESHES: usize = 32;

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
    pub pose: &'a RigPose,
    pub ghost_pose: Option<&'a RigPose>,
    pub ghost_next_pose: Option<&'a RigPose>,
    pub clip: &'a ClipDef,
    pub current_time: f32,
    pub weapon_id: &'a str,
    pub selected_bone: usize,
    pub solo_weapon: bool,
    pub hidden_bones: [bool; BONE_COUNT],
    pub hovered_gizmo: GizmoAxis,
    pub active_gizmo: GizmoAxis,
    pub show_shadows: bool,
    pub show_onion: bool,
    pub show_trail: bool,
    pub show_gizmo: bool,
    pub bg_mode: f32, // 0=Studio, 1=Sakura, 2=Ocean
    pub theme_idx: f32,
    pub impact_flash: f32,
    pub anim_clock: f32,
    pub camera: &'a CameraState,
    pub draw_world_backdrop: bool,
}

/// Bounded LRU cache for voxelized GPU weapon meshes to cap VRAM growth across large modpacks.
pub struct WeaponMeshCache {
    map: HashMap<String, Arc<GpuMesh>>,
    order: VecDeque<String>,
    cap: usize,
}

impl WeaponMeshCache {
    pub fn new(cap: usize) -> Self {
        Self {
            map: HashMap::with_capacity(cap.min(64)),
            order: VecDeque::with_capacity(cap.min(64)),
            cap: cap.max(1),
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn get(&self, id: &str) -> Option<Arc<GpuMesh>> {
        self.map.get(id).cloned()
    }

    pub fn first_any(&self) -> Option<Arc<GpuMesh>> {
        self.map.values().next().cloned()
    }

    pub fn insert(&mut self, id: String, mesh: Arc<GpuMesh>) {
        if self.map.contains_key(&id) {
            self.order.retain(|k| k != &id);
            self.order.push_back(id.clone());
            self.map.insert(id, mesh);
            return;
        }
        while self.map.len() >= self.cap {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
        self.order.push_back(id.clone());
        self.map.insert(id, mesh);
    }
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
    mesh_staging_cpu: Vec<u8>,

    char_bones: [GpuMesh; 7],
    sakura_world: Option<GpuMesh>,
    training_dummy: Option<GpuMesh>,
    shadow_disk: GpuMesh,
    gizmo_ring: GpuMesh,
    gizmo_axes: [GpuMesh; 4], // X red, Y green, Z blue, center handle
    pub weapon_cache: WeaponMeshCache,
    dynamic_vb: wgpu::Buffer,
    dynamic_ib: wgpu::Buffer,

    pub main_surface: RenderTargetSurface,
    pub card_surface: RenderTargetSurface,
    thumb_msaa_views: Option<(wgpu::TextureView, wgpu::TextureView)>,
}

fn make_axis_handle_mesh(axis: usize) -> MeshData {
    let mut m = MeshData::new();
    match axis {
        0 => {
            let c = [0.96, 0.24, 0.22];
            m.push_box([0.04, -0.011, -0.011, 0.36, 0.011, 0.011], c);
            m.push_box([0.36, -0.026, -0.026, 0.42, 0.026, 0.026], c);
        }
        1 => {
            let c = [0.19, 0.82, 0.35];
            m.push_box([-0.011, 0.04, -0.011, 0.011, 0.36, 0.011], c);
            m.push_box([-0.026, 0.36, -0.026, 0.026, 0.42, 0.026], c);
        }
        2 => {
            let c = [0.04, 0.52, 1.0];
            m.push_box([-0.011, -0.011, 0.04, 0.011, 0.011, 0.36], c);
            m.push_box([-0.026, -0.026, 0.36, 0.026, 0.026, 0.42], c);
        }
        _ => {
            m.push_box([-0.032, -0.032, -0.032, 0.032, 0.032, 0.032], [0.96, 0.96, 0.98]);
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
        m.vertices.push(Vertex3D { pos: [0.0, 0.004, 0.0], normal: [0.0, 1.0, 0.0], color: [0.01, 0.01, 0.02, 0.48] });
        m.vertices.push(Vertex3D { pos: [a0.cos() * 0.58, 0.004, a0.sin() * 0.58], normal: [0.0, 1.0, 0.0], color: [0.01, 0.01, 0.02, 0.0] });
        m.vertices.push(Vertex3D { pos: [a1.cos() * 0.58, 0.004, a1.sin() * 0.58], normal: [0.0, 1.0, 0.0], color: [0.01, 0.01, 0.02, 0.0] });
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

        let make_bg_pipe = |sample_count: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
                multiview_mask: None,
                cache: None,
            })
        };
        let bg_pipeline = make_bg_pipe(4);

        let v_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        let v_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &v_attrs,
        };

        let make_mesh_pipe = |name, sample_count: u32, cull: Option<wgpu::Face>, blend, depth_write, cmp| {
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
                multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
                multiview_mask: None,
                cache: None,
            })
        };

        let lit_pipeline = make_mesh_pipe("lit_opaque", 4, Some(wgpu::Face::Back), None, true, wgpu::CompareFunction::Less);
        let alpha_pipeline = make_mesh_pipe("lit_alpha", 4, None, Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Less);
        let overlay_pipeline = make_mesh_pipe("overlay_gizmo", 4, None, Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always);

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
        let mesh_staging_cpu = vec![0u8; (mesh_ubo_stride * max_slots) as usize];
        let shadow_disk = GpuMesh::from_data(&device, &make_shadow_disk(36));
        let gizmo_ring = GpuMesh::from_data(&device, &make_ring_mesh(5.8, 0.08, 64, [0.90, 0.92, 0.96, 0.24]));
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
        let card_surface = RenderTargetSurface::new(&device, 280, 150);

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
            mesh_staging_cpu,
            char_bones,
            sakura_world: None,
            training_dummy: None,
            shadow_disk,
            gizmo_ring,
            gizmo_axes,
            weapon_cache: WeaponMeshCache::new(MAX_CACHED_WEAPON_MESHES),
            dynamic_vb,
            dynamic_ib,
            main_surface,
            card_surface,
            thumb_msaa_views: None,
        }
    }

    fn ensure_thumb_msaa(&mut self) -> (wgpu::TextureView, wgpu::TextureView) {
        if let Some(ref views) = self.thumb_msaa_views {
            return views.clone();
        }
        let msaa_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_thumb_msaa_4x"),
            size: wgpu::Extent3d { width: 96, height: 96, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let mv = msaa_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_thumb_depth_4x"),
            size: wgpu::Extent3d { width: 96, height: 96, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 4,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let dv = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.thumb_msaa_views = Some((mv.clone(), dv.clone()));
        (mv, dv)
    }

    pub fn release_thumb_surface(&mut self) {
        self.thumb_msaa_views = None;
    }

    /// Render a 96x96 4x-MSAA resolved zero-copy GPU thumbnail ready for `slint::Image::try_from(tex)`.
    pub fn render_thumb_texture(&mut self, params: &RenderParams<'_>) -> wgpu::Texture {
        let (mv, dv) = self.ensure_thumb_msaa();
        let resolve_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("vm_thumb_resolve"),
            size: wgpu::Extent3d { width: 96, height: 96, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let rv = resolve_tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.render_to_views(2, Some((mv, rv, dv, 96, 96)), params);
        resolve_tex
    }

    pub fn ensure_weapon_mesh(&mut self, id: &str, w: u32, h: u32, rgba: &[u8], archetype: &str) {
        if self.weapon_cache.contains(id) {
            return;
        }
        // Cap GPU voxelization grid to <= 32x32 so 64x64 / 128x128 mod sprites stay fast & low-VRAM.
        let max_dim = w.max(h);
        let mesh_data = if max_dim > 32 && rgba.len() >= (w * h * 4) as usize {
            let scale = (max_dim as f32 / 32.0).ceil() as u32;
            let nw = (w / scale).max(1).min(32);
            let nh = (h / scale).max(1).min(32);
            let mut small = vec![0u8; (nw * nh * 4) as usize];
            for ny in 0..nh {
                let sy = ((ny * h) / nh).min(h - 1);
                for nx in 0..nw {
                    let sx = ((nx * w) / nw).min(w - 1);
                    let src_i = ((sy * w + sx) * 4) as usize;
                    let dst_i = ((ny * nw + nx) * 4) as usize;
                    small[dst_i..dst_i + 4].copy_from_slice(&rgba[src_i..src_i + 4]);
                }
            }
            voxelize_weapon_sprite(nw, nh, &small, archetype)
        } else {
            voxelize_weapon_sprite(w, h, rgba, archetype)
        };
        self.weapon_cache
            .insert(id.to_string(), Arc::new(GpuMesh::from_data(&self.device, &mesh_data)));
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

    fn build_trail_mesh(
        &self,
        clip: &ClipDef,
        current_time: f32,
        solo_weapon: bool,
        impact_flash: f32,
        eye: Vec3,
    ) -> MeshData {
        let mut m = MeshData::new();
        let t_end = current_time.max(0.01);
        let t_start = (t_end - (clip.duration * 0.20).min(0.085)).max(0.0);
        let steps = 14usize;
        let dt_step = ((t_end - t_start) / steps as f32).max(0.001);

        let slash_ev = clip.vfx_events.iter().find(|ev| {
            ev.kind == VfxKind::SlashTrail && {
                let half = (ev.duration.max(0.08)) * 0.65;
                current_time >= (ev.time - half) && current_time <= (ev.time + half)
            }
        });
        let trail_rgb = slash_ev.map(|ev| ev.color).unwrap_or([0.56, 0.42, 0.98]);
        let trail_scale = slash_ev.map(|ev| ev.scale.clamp(0.5, 2.2)).unwrap_or(1.0);

        let tip_local = Vec3::new(0.0, -0.7, -15.5 - 9.0 * trail_scale);
        let base_local = Vec3::new(0.0, -0.4, -15.5);

        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = t_start + (t_end - t_start) * (i as f32 / steps as f32);
            let mats = if solo_weapon {
                let p = sample_clip_pose(clip, t);
                let mut only = RigPose::ZERO;
                only.0[5] = p.0[5];
                evaluate_rig_matrices(&only)
            } else {
                evaluate_rig_matrices(&sample_clip_pose(clip, t))
            };
            let wm = mats[5]; // right_item
            let hot = clip.impact_time.map(|imp| (t - imp).abs() < 0.035).unwrap_or(false);
            pts.push((wm.transform_point(tip_local), wm.transform_point(base_local), hot, wm));
        }

        for i in 0..steps {
            let (tip0, base0, h0, _) = pts[i];
            let (tip1, base1, h1, _) = pts[i + 1];
            let tip_speed = tip1.sub(tip0).len() / dt_step;
            if tip_speed < 2.2 && slash_ev.is_none() {
                continue;
            }
            let alpha_mul = if slash_ev.is_some() { 0.54 } else { 0.42 };
            let f0 = (i as f32 / steps as f32).powi(3) * alpha_mul;
            let f1 = ((i + 1) as f32 / steps as f32).powi(3) * alpha_mul;
            let c0 = if h0 && slash_ev.is_none() { [0.98, 0.72, 0.25, f0] } else { [trail_rgb[0], trail_rgb[1], trail_rgb[2], f0] };
            let c1 = if h1 && slash_ev.is_none() { [0.98, 0.72, 0.25, f1] } else { [trail_rgb[0], trail_rgb[1], trail_rgb[2], f1] };
            let base = m.vertices.len() as u32;
            m.vertices.push(Vertex3D { pos: tip0.arr(), normal: [0.0, 1.0, 0.0], color: c0 });
            m.vertices.push(Vertex3D { pos: base0.arr(), normal: [0.0, 1.0, 0.0], color: [c0[0], c0[1], c0[2], 0.0] });
            m.vertices.push(Vertex3D { pos: tip1.arr(), normal: [0.0, 1.0, 0.0], color: c1 });
            m.vertices.push(Vertex3D { pos: base1.arr(), normal: [0.0, 1.0, 0.0], color: [c1[0], c1[1], c1[2], 0.0] });
            m.indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }

        let (tip_now, base_now, _, hand_wm) = pts[pts.len() - 1];
        let spark_center = tip_now.mul(0.65).add(base_now.mul(0.35));

        // Timeline VFX Events (`ParrySpark`, `SonarRing`, `HitStop`) inspired by HakiVoxelVfxManager
        for ev in &clip.vfx_events {
            let dur = ev.duration.max(0.06);
            if current_time < ev.time || current_time > ev.time + dur {
                continue;
            }
            let prog = ((current_time - ev.time) / dur).clamp(0.0, 1.0);
            let alpha = (1.0 - prog).clamp(0.0, 1.0);
            let col = [ev.color[0], ev.color[1], ev.color[2], alpha * ev.intensity.clamp(0.3, 1.0)];

            match ev.kind {
                VfxKind::ParrySpark => {
                    // Radial voxel star burst matching vfx_parry_star / parry_spark_flipbook
                    let radius = (0.14 + prog * 0.42) * ev.scale.clamp(0.5, 2.5);
                    let fwd = eye.sub(spark_center).norm();
                    let right = Vec3::new(0.0, 1.0, 0.0).cross(fwd).norm();
                    let up = fwd.cross(right);
                    for ray in 0..8 {
                        let ang = ray as f32 * (std::f32::consts::PI * 0.25);
                        let dir = right.mul(ang.cos()).add(up.mul(ang.sin()));
                        let perp = right.mul(-ang.sin()).add(up.mul(ang.cos())).mul(0.024 * (1.0 - prog * 0.5));
                        let p0 = spark_center.add(dir.mul(radius * 0.18));
                        let p1 = spark_center.add(dir.mul(radius));
                        let b = m.vertices.len() as u32;
                        m.vertices.push(Vertex3D { pos: p0.sub(perp).arr(), normal: [0.0, 1.0, 0.0], color: col });
                        m.vertices.push(Vertex3D { pos: p0.add(perp).arr(), normal: [0.0, 1.0, 0.0], color: col });
                        m.vertices.push(Vertex3D { pos: p1.arr(), normal: [0.0, 1.0, 0.0], color: [col[0], col[1], col[2], 0.0] });
                        m.indices.extend_from_slice(&[b, b + 1, b + 2]);
                    }
                    let core = make_ring_mesh(0.08 + prog * 0.22 * ev.scale, 0.022, 18, col);
                    let off = m.vertices.len() as u32;
                    for v in core.vertices {
                        let p = spark_center.add(right.mul(v.pos[0])).add(up.mul(v.pos[1]));
                        m.vertices.push(Vertex3D { pos: p.arr(), normal: v.normal, color: v.color });
                    }
                    for idx in core.indices {
                        m.indices.push(off + idx);
                    }
                }
                VfxKind::SonarRing => {
                    // Expanding ground/waist shockwave ring matching vfx_pixel_ring / anime_geppo_cyan
                    let radius = (0.28 + prog * 1.15) * ev.scale.clamp(0.5, 2.5);
                    let ring = make_ring_mesh(radius, 0.045 * (1.0 - prog * 0.4), 24, col);
                    let off = m.vertices.len() as u32;
                    for v in ring.vertices {
                        m.vertices.push(Vertex3D {
                            pos: [v.pos[0], 0.06 + prog * 0.18, v.pos[1]],
                            normal: [0.0, 1.0, 0.0],
                            color: v.color,
                        });
                    }
                    for idx in ring.indices {
                        m.indices.push(off + idx);
                    }
                }
                VfxKind::HitStop => {
                    let ring = make_ring_mesh((0.14 + prog * 0.28) * ev.scale, 0.028, 20, col);
                    let fwd = eye.sub(tip_now).norm();
                    let right = Vec3::new(0.0, 1.0, 0.0).cross(fwd).norm();
                    let up = fwd.cross(right);
                    let off = m.vertices.len() as u32;
                    for v in ring.vertices {
                        let p = tip_now.add(right.mul(v.pos[0])).add(up.mul(v.pos[1]));
                        m.vertices.push(Vertex3D { pos: p.arr(), normal: v.normal, color: v.color });
                    }
                    for idx in ring.indices {
                        m.indices.push(off + idx);
                    }
                }
                VfxKind::SlashTrail | VfxKind::ScreenShake => {}
            }
        }
        let _ = hand_wm;

        if impact_flash > 0.05 {
            let fwd = eye.sub(tip_now).norm();
            let right = Vec3::new(0.0, 1.0, 0.0).cross(fwd).norm();
            let up = fwd.cross(right);
            let ring = make_ring_mesh(0.18 + (1.0 - impact_flash) * 0.34, 0.025, 24, [1.0, 0.80, 0.28, impact_flash]);
            let offset = m.vertices.len() as u32;
            for v in ring.vertices {
                let p = tip_now.add(right.mul(v.pos[0])).add(up.mul(v.pos[1]));
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
        self.render_to_views(surface_idx, None, params);
    }

    fn render_to_views(
        &mut self,
        surface_idx: u8,
        thumb_override: Option<(wgpu::TextureView, wgpu::TextureView, wgpu::TextureView, u32, u32)>,
        params: &RenderParams<'_>,
    ) {
        if params.draw_world_backdrop {
            if self.sakura_world.is_none() {
                self.sakura_world = Some(GpuMesh::from_data(&self.device, &build_sakura_world_mesh()));
            }
            if self.training_dummy.is_none() {
                self.training_dummy = Some(GpuMesh::from_data(&self.device, &build_training_dummy_mesh()));
            }
        }
        let (tw, th) = if let Some((_, _, _, w, h)) = thumb_override {
            (w, h)
        } else if surface_idx == 0 {
            (self.main_surface.width, self.main_surface.height)
        } else {
            (self.card_surface.width, self.card_surface.height)
        };
        let aspect = tw as f32 / th.max(1) as f32;
        let mut cam_eval = params.camera.clone();
        if surface_idx == 0 && !params.solo_weapon && params.show_trail {
            for ev in &params.clip.vfx_events {
                if ev.kind == VfxKind::ScreenShake {
                    let dur = ev.duration.max(0.05);
                    if params.current_time >= ev.time && params.current_time <= ev.time + dur {
                        let prog = ((params.current_time - ev.time) / dur).clamp(0.0, 1.0);
                        let amp = (1.0 - prog) * ev.intensity.clamp(0.1, 2.0) * 0.038;
                        let phase = params.current_time * 92.0 + params.anim_clock * 35.0;
                        cam_eval.target[0] += phase.sin() * amp;
                        cam_eval.target[1] += (phase * 1.37).cos() * amp * 0.8;
                    }
                }
            }
        }
        let (vp, inv_vp, eye) = cam_eval.vp_and_inv(aspect);

        let cam_ubo = CameraUniformData {
            vp: vp.m,
            inv_vp: inv_vp.m,
            cam_pos: [eye.x, eye.y, eye.z, 1.0],
            params: [
                params.bg_mode,
                params.anim_clock,
                if params.show_shadows { 1.0 } else { 0.0 },
                params.theme_idx,
            ],
        };
        self.queue.write_buffer(&self.cam_buf, 0, bytemuck::bytes_of(&cam_ubo));

        // Build trail dynamic mesh before taking disjoint mutable slice borrow for slot packing
        let dyn_mesh = if params.show_trail && !params.solo_weapon {
            self.build_trail_mesh(params.clip, params.current_time, false, params.impact_flash, eye)
        } else {
            MeshData::new()
        };

        // Pack all per-draw uniforms into CPU staging slice and upload in ONE queue.write_buffer call.
        let stride = self.mesh_ubo_stride as usize;
        let max_slots = self.mesh_staging_cpu.len() / stride.max(1);
        let mut slot = 0usize;
        let mesh_staging_cpu = &mut self.mesh_staging_cpu;
        let mut write_slot = |slot_idx: usize, model: [f32; 16], tint: [f32; 4]| -> u32 {
            let safe_slot = slot_idx.min(max_slots.saturating_sub(1));
            let u = MeshUniformData { model, tint };
            let bytes = bytemuck::bytes_of(&u);
            let off = safe_slot * stride;
            mesh_staging_cpu[off..off + bytes.len()].copy_from_slice(bytes);
            off as u32
        };

        let mats = evaluate_rig_matrices(params.pose);
        let bone_order = [0usize, 1, 2, 3, 4, 6, 7];
        let mut opaque_draws: Vec<(u32, usize)> = Vec::new(); // (ubo_offset, mesh_kind)
        // mesh_kind: 0..6 = char_bones, 10 = weapon, 11 = sakura_world

        if params.draw_world_backdrop {
            let off = write_slot(slot, Mat4::IDENTITY.m, [1.0, 1.0, 1.0, 0.0]);
            opaque_draws.push((off, 11));
            slot += 1;

            // Training dummy placed in front of the player (+Z forward) with impact recoil & flash
            let mut hit_age: Option<f32> = None;
            if let Some(imp) = params.clip.impact_time {
                if params.current_time >= imp && (params.current_time - imp) <= 0.30 {
                    hit_age = Some(params.current_time - imp);
                }
            }
            for ev in &params.clip.vfx_events {
                if matches!(ev.kind, VfxKind::ParrySpark | VfxKind::HitStop | VfxKind::SonarRing)
                    && params.current_time >= ev.time
                    && (params.current_time - ev.time) <= 0.30
                {
                    let age = params.current_time - ev.time;
                    hit_age = Some(hit_age.map_or(age, |prev| prev.min(age)));
                }
            }
            let (recoil_deg, flash_amt) = if let Some(age) = hit_age {
                let damp = (-age * 9.5).exp();
                let osc = (age * 36.0).sin().abs();
                (damp * (16.0 + osc * 8.0), damp * 0.72)
            } else {
                (0.0, 0.0)
            };
            let dummy_m = Mat4::translate(0.0, 0.0, 1.42)
                .mul(Mat4::rot_x(recoil_deg * crate::math3d::DEG2RAD));
            let dummy_tint = if flash_amt > 0.02 {
                [flash_amt, 0.7, 0.3, 5.0]
            } else {
                [1.0, 1.0, 1.0, 0.0]
            };
            let doff = write_slot(slot, dummy_m.m, dummy_tint);
            opaque_draws.push((doff, 12));
            slot += 1;
        }

        let weapon_model = if params.solo_weapon {
            // Showcase weapon angled horizontally from side/3-4 so full silhouette reads across frame!
            let ri = params.pose.bone(5);
            let rot_anim = Mat4::rot_zyx_deg(ri.rot[0], ri.rot[1], ri.rot[2]);
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
            if params.draw_world_backdrop {
                let d_shadow_m = Mat4::translate(0.0, 0.016, 1.42).mul(Mat4::scale(0.95, 1.0, 0.95));
                let doff = write_slot(slot, d_shadow_m.m, [1.0, 1.0, 1.0, 3.0]);
                alpha_draws.push((doff, 20));
                slot += 1;
            }
        }
        if params.show_onion && !params.solo_weapon && surface_idx == 0 {
            let mut push_ghost = |gp: &RigPose, mode_w: f32| {
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
        let used_bytes = (slot.min(max_slots) * stride).min(self.mesh_staging_cpu.len());
        if used_bytes > 0 {
            self.queue
                .write_buffer(&self.mesh_buf, 0, &self.mesh_staging_cpu[..used_bytes]);
        }

        let (color_view, resolve_target, depth_view) = if let Some((ref mv, ref rv, ref dv, _, _)) = thumb_override {
            (mv, Some(rv), dv)
        } else if surface_idx == 0 {
            (&self.main_surface.msaa_view, Some(&self.main_surface.resolve_view), &self.main_surface.depth_view)
        } else {
            (&self.card_surface.msaa_view, Some(&self.card_surface.resolve_view), &self.card_surface.depth_view)
        };

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.04, g: 0.04, b: 0.07, a: 1.0 }),
                        store: if resolve_target.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
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
            let fallback_weapon = self.weapon_cache.first_any();
            let active_weapon = self.weapon_cache.get(params.weapon_id).or(fallback_weapon);

            for (off, kind) in opaque_draws {
                rpass.set_bind_group(1, &self.mesh_bg, &[off]);
                let mesh_ref: Option<&GpuMesh> = match kind {
                    0..=6 => Some(&self.char_bones[kind]),
                    10 => active_weapon.as_deref(),
                    11 => self.sakura_world.as_ref(),
                    12 => self.training_dummy.as_ref(),
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

    /// Read back an offscreen surface texture to RGBA bytes (used by --self-test and PNG strip export).
    pub fn readback_surface(&self, surface_idx: u8) -> (u32, u32, Vec<u8>) {
        let target = if surface_idx == 0 { &self.main_surface } else { &self.card_surface };
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
