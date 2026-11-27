// VoxelMotion Studio - Native Rust Studio Entrypoint, Headless Verification & Unit Tests
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod camera;
mod catalog;
mod design_spec;
mod exporter;
mod math3d;
mod model;
mod modpack;
mod player_animator;
mod renderer3d;
mod ui_bridge;
mod voxelizer;

use app_state::{detect_workspace_root, export_storyboard_strip, AppState};
use exporter::{export_project, ExportFormat};
use math3d::GizmoAxis;
use model::{mirror_pose, sample_clip_pose, BONE_COUNT};
use renderer3d::{GpuRenderer, RenderParams};
use slint::ComponentHandle;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;
use ui_bridge::{sync_dirty_ui, wire_ui_and_gpu};

slint::include_modules!();

fn run_headless_smoke(out_png: Option<&Path>) -> Result<(), String> {
    let root = detect_workspace_root();
    let mut state = AppState::new(root.clone());

    if state.weapons.is_empty() {
        return Err("No starter weapons available!".into());
    }
    if state.project.clips.is_empty() {
        return Err("Startup project missing blank clip!".into());
    }

    let added_demos = state.load_demo_presets();
    if added_demos == 0 && state.project.clips.len() < 10 {
        return Err("Failed to load demo preset library!".into());
    }
    let orig_rot = state.active_clip().keyframes[0].bones["right_arm"].rot;
    state.push_undo();
    state.ensure_active_weapon_gpu();
    let ki = state.ensure_kf_at_current();
    state.active_clip_mut().keyframes[ki].bones["right_arm"].rot[0] += 15.0;
    assert_ne!(state.active_clip().keyframes[ki].bones["right_arm"].rot, orig_rot);
    if let Some(prev) = state.undo_stack.pop() {
        state.active_clip_mut().clone_from(&prev);
    }

    state.selected_category = 2; // Katanas
    state.rebuild_weapon_filter();
    assert!(!state.filtered_indices.is_empty());
    state.weapon_search = "katana".into();
    state.rebuild_weapon_filter();
    assert!(!state.filtered_indices.is_empty());
    state.selected_category = 0;
    state.weapon_search.clear();
    state.rebuild_weapon_filter();

    let clip_snapshot = state.active_clip().clone();
    let p_mir = mirror_pose(&clip_snapshot.keyframes[0].bones);
    assert_eq!(p_mir["left_arm"].rot[1], -clip_snapshot.keyframes[0].bones["right_arm"].rot[1]);

    // Verify real GPU wgpu adapter, 120-frame 4x MSAA performance benchmark & offscreen render
    let instance = slint::wgpu_30::wgpu::Instance::default();
    let adapter = pollster_block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("No GPU adapter: {e:?}"))?;
    let (device, queue) = pollster_block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("Device failed: {e:?}"))?;
    let mut gpu = GpuRenderer::new(device, queue);
    if let Some(sp) = &state.weapons[0].icon_rgba {
        gpu.ensure_weapon_mesh(
            &state.weapons[0].id,
            sp.width,
            sp.height,
            &sp.rgba,
            &state.weapons[0].mesh_archetype,
        );
    }
    let clip = state.active_clip().clone();
    let t_start = Instant::now();
    for f in 0..120 {
        let t = (f as f32 / 120.0) * clip.duration;
        let pose = sample_clip_pose(&clip, t);
        gpu.render_to_surface(
            0,
            &RenderParams {
                pose: &pose,
                ghost_pose: None,
                ghost_next_pose: None,
                clip: &clip,
                current_time: t,
                weapon_id: &state.weapons[0].id,
                selected_bone: 3,
                solo_weapon: false,
                hidden_bones: [false; BONE_COUNT],
                hovered_gizmo: GizmoAxis::None,
                active_gizmo: GizmoAxis::None,
                show_shadows: true,
                show_onion: false,
                show_trail: true,
                show_gizmo: true,
                bg_mode: 1.0,
                theme_idx: state.theme_idx as f32,
                impact_flash: if (t - clip.impact_time.unwrap_or(0.2)).abs() < 0.04 {
                    0.8
                } else {
                    0.0
                },
                anim_clock: t,
                camera: &state.main_cam.current,
                draw_world_backdrop: true,
            },
        );
    }
    let _ = gpu.device.poll(slint::wgpu_30::wgpu::PollType::wait_indefinitely());
    let gpu_120_ms = t_start.elapsed().as_secs_f64() * 1000.0;
    let avg_fps = 120.0 / (gpu_120_ms / 1000.0);

    if let Some(path) = out_png {
        let (rw, rh, raw) = gpu.readback_surface(0);
        if let Some(img) = image::RgbaImage::from_raw(rw, rh, raw) {
            img.save(path).map_err(|e| e.to_string())?;
        }
    }

    state.gpu = Some(gpu);
    let strip_path = export_storyboard_strip(&mut state)?;
    assert!(strip_path.is_file());

    // Explicitly tear down GPU resources BEFORE adapter/instance drop to avoid Windows DX12 COM teardown crash
    state.gpu = None;
    drop(adapter);
    drop(instance);

    let smoke_out = root.join("exports");
    let summary = export_project(&state.project, &root, Some(&smoke_out), ExportFormat::Both, true, true)?;
    if summary.clips_exported < 10 || summary.targets.is_empty() {
        return Err(format!("Export verification failed: {} clips", summary.clips_exported));
    }

    let report = format!(
        "VoxelMotion Studio SELF-TEST OK: {} weapons, {} clips exported, {} BC overrides, {} timing events | GPU 120x 4x-MSAA frames in {:.2} ms ({:.0} FPS) | strip={}\n",
        state.weapons.len(),
        summary.clips_exported,
        summary.bc_overrides,
        summary.event_timings,
        gpu_120_ms,
        avg_fps,
        strip_path.display()
    );
    let _ = std::fs::write(root.join("exports/self_test_report.txt"), &report);
    println!("{report}");
    Ok(())
}

/// Phase-1 Mandatory Calibration Spike: convert 2 real Better Combat PlayerAnimator clips
/// (`one_handed_slash_horizontal_right` and `two_handed_slam`), bake 6-frame 3D storyboard PNGs
/// + Bedrock 1.8.0 JSON preview so bone pivots, signs, and weapon orientation can be visually audited.
fn run_calibration_spike() -> Result<(), String> {
    let root = detect_workspace_root();
    let bc_jar = root
        .parent()
        .map(|p| p.join("servernuevo/sklauncher_modpack/mods/bettercombat-forge-1.9.0+1.20.1.jar"))
        .ok_or_else(|| "Missing workspace parent".to_string())?;
    if !bc_jar.is_file() {
        return Err(format!("BetterCombat JAR not found at {}", bc_jar.display()));
    }

    let file = std::fs::File::open(&bc_jar).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let targets = [
        (
            "assets/bettercombat/attack_animations/one_handed_slash_horizontal_right.json",
            "spike_slash_right",
            0usize, // Espada de Acero
        ),
        (
            "assets/bettercombat/attack_animations/two_handed_slam.json",
            "spike_two_handed_slam",
            2usize, // Mandoble Pesado
        ),
    ];

    let mut state = AppState::new(root.clone());
    state.project.clips.clear();
    for (zip_path, clip_id, _) in &targets {
        let mut entry = zip.by_name(zip_path).map_err(|e| format!("{zip_path}: {e}"))?;
        let mut raw = String::new();
        std::io::Read::read_to_string(&mut entry, &mut raw).map_err(|e| e.to_string())?;
        let clip = player_animator::parse_player_animator_json(
            &raw,
            clip_id,
            "Better Combat",
            vec![format!("bettercombat:{}", clip_id.trim_start_matches("spike_"))],
        )?;
        println!(
            "SPIKE_PARSED {}: dur={:.2}s impact={:?} kfs={}",
            clip.id,
            clip.duration,
            clip.impact_time,
            clip.keyframes.len()
        );
        state.project.clips.push(clip);
    }

    let instance = slint::wgpu_30::wgpu::Instance::default();
    let adapter = pollster_block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("No GPU adapter: {e:?}"))?;
    let (device, queue) = pollster_block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("Device failed: {e:?}"))?;
    state.gpu = Some(GpuRenderer::new(device, queue));

    for (idx, (_, _, weapon_idx)) in targets.iter().enumerate() {
        state.active_clip_idx = idx;
        state.active_weapon_idx = *weapon_idx;
        state.ensure_active_weapon_gpu();
        let out_strip = export_storyboard_strip(&mut state)?;
        println!("SPIKE_STRIP_OK: {}", out_strip.display());
    }
    state.gpu = None;
    drop(adapter);
    drop(instance);
    Ok(())
}

/// Minimal zero-dep pollster-free Future executor using thread parking (safe for adapter/device init).
fn pollster_block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn clone_fn(ptr: *const ()) -> RawWaker {
        RawWaker::new(ptr, &VTABLE)
    }
    fn wake_fn(ptr: *const ()) {
        let t = unsafe { &*(ptr as *const std::thread::Thread) };
        t.unpark();
    }
    fn drop_fn(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_fn, wake_fn, wake_fn, drop_fn);
    let current = std::thread::current();
    let waker = unsafe { Waker::from_raw(RawWaker::new(&current as *const _ as *const (), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut pinned = std::pin::pin!(fut);
    loop {
        match pinned.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => std::thread::park(),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--spike-clips") {
        if let Err(e) = run_calibration_spike() {
            eprintln!("SPIKE ERROR: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--self-test") {
        let out_png = args
            .windows(2)
            .find(|w| w[0] == "--render-3d")
            .map(|w| PathBuf::from(&w[1]));
        if let Err(e) = run_headless_smoke(out_png.as_deref()) {
            eprintln!("SELF-TEST ERROR: {e}");
            std::process::exit(1);
        }
        return;
    }

    let bench_window = args.iter().any(|a| a == "--bench-window");
    let bench_orbit_zoom = args.iter().any(|a| a == "--bench-orbit-zoom");
    let root = detect_workspace_root();
    let startup_profile = modpack::load_local_profile(&root);

    let mut wgpu_settings = slint::wgpu_30::WGPUSettings::default();
    wgpu_settings.power_preference = if startup_profile.gpu_low_power {
        slint::wgpu_30::wgpu::PowerPreference::LowPower
    } else {
        slint::wgpu_30::wgpu::PowerPreference::HighPerformance
    };
    #[cfg(target_os = "windows")]
    {
        // Opaque frameless MainWindow renders directly to HWND flip-chain without DWM visual composition overhead.
        wgpu_settings.backends = slint::wgpu_30::wgpu::Backends::from_env()
            .unwrap_or(slint::wgpu_30::wgpu::Backends::DX12);
        if slint::wgpu_30::wgpu::Dx12SwapchainKind::from_env().is_none() {
            wgpu_settings.backend_options.dx12.presentation_system =
                slint::wgpu_30::wgpu::Dx12SwapchainKind::DxgiFromHwnd;
        }
    }
    slint::BackendSelector::new()
        .require_wgpu_30(slint::wgpu_30::WGPUConfiguration::Automatic(wgpu_settings))
        .select()
        .expect("Failed to initialize WGPU 30 + FemtoVG GPU backend");

    let win = MainWindow::new().expect("Failed to build MainWindow");
    let state = Rc::new(RefCell::new(AppState::new(root)));

    let auto_capture_path: Option<PathBuf> = args
        .windows(2)
        .find(|w| w[0] == "--capture-window")
        .map(|w| PathBuf::from(&w[1]));
    let capture_tab: Option<i32> = args
        .windows(2)
        .find(|w| w[0] == "--capture-tab")
        .and_then(|w| w[1].parse().ok());
    let capture_time: Option<f32> = args
        .windows(2)
        .find(|w| w[0] == "--capture-time")
        .and_then(|w| w[1].parse().ok());
    let capture_theme: Option<i32> = args
        .windows(2)
        .find(|w| w[0] == "--capture-theme")
        .and_then(|w| w[1].parse().ok());

    if let Some(tab) = capture_tab {
        match tab {
            2 => win.set_export_modal_open(true),
            3 => win.set_settings_modal_open(true),
            4 => win.set_weapons_pinned(true),
            _ => {}
        }
    }
    if let Some(ti) = capture_theme {
        let mut s = state.borrow_mut();
        s.theme_idx = ti.clamp(0, 4);
        s.dirty.main_viewport = true;
        s.dirty.inspector = true;
    }
    if let Some(t) = capture_time {
        let mut s = state.borrow_mut();
        let dur = s.active_clip().duration.max(0.05);
        s.current_time = crate::model::snap_time_to_frame(t, dur);
        s.solo_weapon = false;
        s.playing = false;
        s.impact_flash = s
            .active_clip()
            .impact_time
            .map(|imp| if (s.current_time - imp).abs() < 0.04 { 0.85 } else { 0.0 })
            .unwrap_or(0.0);
        s.dirty.main_viewport = true;
    }

    sync_dirty_ui(&mut state.borrow_mut(), &win);
    if capture_time.is_some() {
        win.set_playing(true);
    }

    let vp_size_cell = Rc::new(Cell::new((900u32, 520u32)));

    let _timers = wire_ui_and_gpu(
        &win,
        &state,
        &vp_size_cell,
        auto_capture_path,
        bench_window,
        bench_orbit_zoom,
    );

    win.run().expect("Event loop error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::ViewportCameraController;
    use crate::math3d::{apply_gizmo_drag, camera_basis, pick_gizmo_handle};
    use crate::model::{
        build_demo_presets, create_blank_clip, next_keyframe_time, optimize_track_points,
        prev_keyframe_time, BoneTransform, RigPose, ToolMode,
    };

    #[test]
    fn starts_clean_in_blender_blank_mode_and_loads_opt_in_presets() {
        let tmp = std::env::temp_dir().join("vm_blank_startup_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut s = AppState::new(tmp.clone());
        assert_eq!(s.project.clips.len(), 1, "Clean startup must have 1 blank clip like Blender");
        assert_eq!(s.project.clips[0].id, "action_1");
        assert!(s.project.clips[0].impact_time.is_none());

        let added = s.load_demo_presets();
        assert!(added >= 10, "Opt-in demo preset pack must append all sample clips");
        assert!(s.project.clips.len() >= 11);

        s.reset_to_blank_scene();
        assert_eq!(s.project.clips.len(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn samples_virtual_waist_and_optimizes_tracks() {
        let clips = build_demo_presets();
        assert!(clips.len() >= 15);
        for clip in &clips {
            let pose = sample_clip_pose(clip, clip.impact_time.unwrap_or(0.15));
            let mats = math3d::evaluate_rig_matrices(&pose);
            for m in mats {
                assert!(m.m.iter().all(|v| v.is_finite()));
            }
        }
        let samples = vec![(0.0, [0.0, 0.0, 0.0]), (0.25, [25.0, 0.0, 0.0]), (0.5, [50.0, 0.0, 0.0])];
        let opt = optimize_track_points(&samples, 0.5);
        assert_eq!(opt.len(), 2, "Collinear midpoint must be pruned cleanly");
    }

    #[test]
    fn generates_starter_weapons_imports_custom_png_and_exports_both_formats() {
        let weapons = catalog::build_starter_weapons();
        assert!(weapons.len() >= 8, "Should provide starter procedural voxel weapons");
        assert!(weapons[0].icon_rgba.is_some());

        let sp = weapons[0].icon_rgba.as_ref().unwrap();
        let mesh = voxelizer::voxelize_weapon_sprite(sp.width, sp.height, &sp.rgba, &weapons[0].mesh_archetype);
        assert!(!mesh.indices.is_empty());

        let tmp = std::env::temp_dir().join("vm_import_export_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let custom_png = tmp.join("cobalt_katana.png");
        let (_, _, katana_px) = catalog::make_archetype_sprite("katana", [60, 140, 240]);
        image::RgbaImage::from_raw(16, 16, katana_px).unwrap().save(&custom_png).unwrap();

        let mut st = AppState::new(tmp.clone());
        let before = st.weapons.len();
        let imported = st.import_weapon_paths(&[custom_png]);
        assert_eq!(imported, 1);
        assert_eq!(st.weapons.len(), before + 1);
        assert_eq!(st.active_weapon().name, "Cobalt Katana");

        let export_dir = tmp.join("out");
        let proj = model::AnimatorProject { version: 1, clips: build_demo_presets() };
        let res = export_project(&proj, &tmp, Some(&export_dir), ExportFormat::Both, true, true).unwrap();
        assert_eq!(res.clips_exported, proj.clips.len());
        assert!(export_dir.join("voxelmotion.animation.json").is_file());
        assert!(export_dir.join("voxelmotion_geckolib.animation.json").is_file());
        assert!(export_dir.join("voxelmotion_timings.json").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn filters_library_gallery_and_edits_keyframes_cleanly() {
        let root = detect_workspace_root();
        let mut s = AppState::new(root);
        let total = s.weapons.len();
        assert!(total >= 8);

        s.selected_category = 2; // Katanas
        s.rebuild_weapon_filter();
        assert!(s.filtered_indices.len() <= total);
        assert!(!s.filtered_indices.is_empty());

        s.weapon_search = "katana".into();
        s.rebuild_weapon_filter();
        assert!(!s.filtered_indices.is_empty());

        s.selected_category = 0;
        s.weapon_search.clear();
        s.rebuild_weapon_filter();
        assert_eq!(s.filtered_indices.len(), total);

        s.current_time = 0.12;
        let ki = s.ensure_kf_at_current();
        assert!(ki < s.active_clip().keyframes.len());
        let p = sample_clip_pose(s.active_clip(), 0.12);
        let mir = mirror_pose(&p);
        assert!((mir["left_arm"].rot[1] + p["right_arm"].rot[1]).abs() < 1e-4);
        assert!((mir["right_arm"].rot[2] + p["left_arm"].rot[2]).abs() < 1e-4);

        let snap_clip = s.project.clips[0].clone();
        let p0 = sample_clip_pose(&snap_clip, 0.0);
        let pend = sample_clip_pose(&snap_clip, snap_clip.duration);
        assert!(p0["right_arm"].rot[0].is_finite());
        assert!(pend["right_arm"].rot[0].is_finite());
    }

    #[test]
    fn gizmo_handles_free_camera_and_auto_kf_work_accurately() {
        let root = detect_workspace_root();
        let mut s = AppState::new(root);

        let blank = create_blank_clip(99);
        assert_eq!(blank.keyframes.len(), 2);
        s.project.clips.push(blank);
        s.active_clip_idx = s.project.clips.len() - 1;
        assert!(next_keyframe_time(s.active_clip(), 0.0).is_some());
        assert!(prev_keyframe_time(s.active_clip(), 0.6).is_some());

        let orig_len = s.active_clip().keyframes.len();
        s.current_time = 0.25;
        s.auto_keyframe = false;
        let _idx_manual = s.editable_kf_index();
        assert_eq!(s.active_clip().keyframes.len(), orig_len);
        s.auto_keyframe = true;
        let _idx_auto = s.editable_kf_index();
        assert_eq!(s.active_clip().keyframes.len(), orig_len + 1);

        let (pivot, axes, vp) = s.active_gizmo_geom();
        let (cam_r, cam_u, cam_f) = camera_basis(s.main_cam.current.yaw, s.main_cam.current.pitch);
        let center_px = math3d::project_to_screen(vp, pivot, 900.0, 520.0).unwrap();
        let pick_c = pick_gizmo_handle(center_px.0, center_px.1, vp, 900.0, 520.0, pivot, axes, cam_r, 0.42, 0.425);
        assert_eq!(pick_c, GizmoAxis::Free);

        let y_tip = math3d::project_to_screen(vp, pivot.add(axes[1].mul(0.35)), 900.0, 520.0).unwrap();
        let pick_y = pick_gizmo_handle(y_tip.0, y_tip.1, vp, 900.0, 520.0, pivot, axes, cam_r, 0.42, 0.425);
        assert_eq!(pick_y, GizmoAxis::Y);

        let mut bt = BoneTransform::default();
        apply_gizmo_drag(&mut bt, ToolMode::Rotate, GizmoAxis::Y, axes, cam_r, cam_u, cam_f, vp, 900.0, 520.0, pivot, 400.0, 260.0, 440.0, 260.0);
        assert!(bt.rot[1].abs() > 1.0 && bt.rot[0] == 0.0 && bt.rot[2] == 0.0);

        apply_gizmo_drag(&mut bt, ToolMode::Move, GizmoAxis::Free, axes, cam_r, cam_u, cam_f, vp, 900.0, 520.0, pivot, 400.0, 260.0, 420.0, 240.0);
        assert!(bt.pos.iter().any(|v| v.abs() > 0.01));

        let before = s.main_cam.current.target;
        s.main_cam.move_free(0.5, 0.2, 0.1);
        assert_ne!(s.main_cam.current.target, before);
        s.main_cam.reset_main();
        assert_eq!(s.main_cam.current.target, [0.0, 0.96, 0.0]);

        s.active_bone_idx = 3; // BrazoDerecho
        let arm_px = {
            let (pivot, _, vp) = s.active_gizmo_geom();
            math3d::project_to_screen(vp, pivot, 900.0, 520.0).unwrap()
        };
        let orig_tgt = s.main_cam.target_pos;
        s.queue_zoom_at_cursor(arm_px.0, arm_px.1, 900.0, 520.0, 60.0);
        assert!(s.main_cam.target_dist < 3.35 && s.main_cam.target_pos != orig_tgt);

        s.focus_active_bone();
        assert_eq!(s.main_cam.target_dist, 0.95);
        for _ in 0..50 {
            s.main_cam.step_damping(0.016);
        }
        assert!((s.main_cam.current.dist - 0.95).abs() < 0.002);

        let mut cam = ViewportCameraController::new_main();
        cam.queue_zoom_at_cursor(450.0, 260.0, 900.0, 520.0, 12.0, None, false);
        assert!(cam.target_dist < 3.35 && cam.target_dist > 3.10, "Touchpad delta should produce fine micro zoom");
        cam.queue_zoom_at_cursor(450.0, 260.0, 900.0, 520.0, 60.0, None, false);
        let target_final = cam.target_dist;
        for _ in 0..40 {
            cam.step_damping(0.016);
        }
        assert!((cam.current.dist - target_final).abs() < 0.002);
    }

    #[test]
    fn rig_pose_deserializes_legacy_json_map_and_compacts_identity_bones() {
        let legacy_json = r#"{"right_arm":{"pos":[0.0,0.0,0.0],"rot":[-45.0,12.0,0.0],"scale":[1.0,1.0,1.0]}}"#;
        let pose: RigPose = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(pose["right_arm"].rot, [-45.0, 12.0, 0.0]);
        assert!(pose["left_arm"].is_identity());

        let serialized = serde_json::to_string(&pose).unwrap();
        assert!(serialized.contains("right_arm"));
        assert!(!serialized.contains("left_arm"), "Identity bones should be omitted to minimize saved RAM/disk size");
    }

    #[test]
    fn camera_zoom_and_pan_compose_additively_without_fighting() {
        let mut cam = ViewportCameraController::new_main();
        let pose = RigPose::ZERO;
        cam.queue_zoom_at_cursor(600.0, 220.0, 900.0, 520.0, 60.0, Some(&pose), false);
        let zoomed_target = cam.target_pos;
        assert_ne!(zoomed_target, [0.0, 0.96, 0.0]);

        // Panning mid-zoom must shift BOTH current and target uniformly without discarding zoom focus
        let before_cur = cam.current.target;
        cam.pan(40.0, -25.0, 0.0018);
        let delta_cur = [
            cam.current.target[0] - before_cur[0],
            cam.current.target[1] - before_cur[1],
            cam.current.target[2] - before_cur[2],
        ];
        let delta_tgt = [
            cam.target_pos[0] - zoomed_target[0],
            cam.target_pos[1] - zoomed_target[1],
            cam.target_pos[2] - zoomed_target[2],
        ];
        for i in 0..3 {
            assert!((delta_cur[i] - delta_tgt[i]).abs() < 1e-4);
        }

        for _ in 0..30 {
            cam.step_damping(0.016);
        }
        assert!(!cam.is_animating());
    }

    #[test]
    fn gpu_weapon_cache_evicts_lru_above_capacity() {
        let instance = slint::wgpu_30::wgpu::Instance::default();
        if let Ok(adapter) = pollster_block_on(instance.request_adapter(&Default::default())) {
            if let Ok((device, queue)) = pollster_block_on(adapter.request_device(&Default::default())) {
                let mut gpu = GpuRenderer::new(device, queue);
                let (_, _, px) = catalog::make_archetype_sprite("sword", [200, 200, 220]);
                for i in 0..40 {
                    gpu.ensure_weapon_mesh(&format!("w_{i}"), 16, 16, &px, "sword");
                }
                assert_eq!(gpu.weapon_cache.len(), 32, "LRU cache must bound GPU mesh count");
                assert!(!gpu.weapon_cache.contains("w_0"));
                assert!(gpu.weapon_cache.contains("w_39"));
            }
        }
    }

    #[test]
    fn camera_fine_touchpad_zoom_is_immediate_and_discrete_settles_fast() {
        let mut cam = ViewportCameraController::new_main();
        let pose = RigPose::ZERO;

        // Touchpad / smooth-wheel fine deltas (<24px) must apply 1:1 immediately without damping lag.
        cam.queue_zoom_at_cursor(450.0, 260.0, 900.0, 520.0, 10.0, Some(&pose), false);
        assert!(!cam.is_animating());
        assert_eq!(cam.current.dist, cam.target_dist);

        // Discrete mouse wheel notches must advance >70% on the very first 16ms wakeup frame and settle <=35ms.
        let before = cam.current.dist;
        cam.queue_zoom_at_cursor(450.0, 260.0, 900.0, 520.0, 60.0, Some(&pose), false);
        assert!(cam.is_animating());
        assert!(cam.step_damping(0.016));
        let first_frame_ratio = (before - cam.current.dist) / (before - cam.target_dist);
        assert!(first_frame_ratio > 0.70, "Expected >70% progress on frame 1, got {first_frame_ratio:.2}");
        cam.step_damping(0.016);
        cam.step_damping(0.016);
        let after_3_frames = (cam.current.dist - cam.target_dist).abs() / (before - cam.target_dist);
        assert!(after_3_frames < 0.03, "Expected >97% settled within 3 frames, residual={after_3_frames:.3}");
    }

    #[test]
    fn blocks_incompatible_playback_and_allows_quick_family_toggle() {
        let tmp = std::env::temp_dir().join(format!("vm_compat_guard_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut s = AppState::new(tmp.clone());
        s.load_demo_presets();
        let flint_idx = s
            .weapons
            .iter()
            .position(|w| w.category == crate::model::WeaponCategory::Fuego)
            .expect("starter flintlock must exist");
        s.active_weapon_idx = flint_idx;

        let atk_idx = s
            .project
            .clips
            .iter()
            .position(|c| c.id == "attack_1")
            .expect("demo attack_1 must exist");
        s.active_clip_idx = atk_idx;

        assert!(!s.is_active_clip_compatible());
        s.playing = true;
        s.current_time = 0.0;
        s.step_frame_simulation(0.016);
        assert!(!s.playing, "Incompatible clip playback must be stopped immediately");
        assert_eq!(s.current_time, 0.0, "Time must not advance while blocked");

        s.toggle_active_clip_family(crate::model::WeaponCategory::Fuego);
        assert!(s.is_active_clip_compatible());

        s.toggle_active_clip_only_current_weapon();
        assert!(s.is_active_clip_compatible());
        assert_eq!(s.active_clip().allowed_weapons.len(), 1);

        s.set_active_clip_universal();
        assert!(s.active_clip().is_universal());
        assert!(s.is_active_clip_compatible());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn smooth_camera_presets_and_profile_preferences_roundtrip() {
        let mut cam = ViewportCameraController::new_main();
        cam.set_main_preset(2); // Side
        assert!(cam.is_animating(), "Preset must animate smoothly");
        for _ in 0..40 {
            cam.step_damping(0.016);
        }
        assert!((cam.current.yaw - 1.57).abs() < 0.01);
        cam.reset_main_smooth();
        assert!(cam.is_animating());
        for _ in 0..40 {
            cam.step_damping(0.016);
        }
        assert!((cam.current.yaw - 0.58).abs() < 0.01);

        let prof = crate::modpack::LocalProfile {
            theme_idx: 1,
            ui_motion_enabled: false,
            gpu_low_power: true,
            ..Default::default()
        };
        let raw = serde_json::to_string(&prof).unwrap();
        let parsed: crate::modpack::LocalProfile = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.theme_idx, 1);
        assert!(!parsed.ui_motion_enabled);
        assert!(parsed.gpu_low_power);
    }

    #[test]
    fn per_bone_keyframe_copy_paste_range_and_delete_preserve_other_bones() {
        let root = detect_workspace_root();
        let mut s = AppState::new(root);
        let mut clip = create_blank_clip(50);
        clip.duration = 0.60;
        clip.keyframes[0].bones.0[3].rot = [0.0, 0.0, 0.0];
        clip.keyframes[0].bones.0[4].rot = [12.0, -8.0, 4.0];
        clip.keyframes[1].bones.0[3].rot = [90.0, 0.0, 0.0];
        clip.keyframes[1].bones.0[4].rot = [12.0, -8.0, 4.0];
        s.project.clips.push(clip);
        s.active_clip_idx = s.project.clips.len() - 1;

        // Add per-bone keyframe on right_arm (3) at 0.30s -> should NOT disturb left_arm (4)
        s.current_time = 0.30;
        let k_mid = s.ensure_bone_kf_at(0.30, 3, crate::model::EasingMode::Linear);
        s.active_clip_mut().keyframes[k_mid].bones.0[3].rot = [-60.0, 24.0, -10.0];
        assert_eq!(s.active_clip().keyframes[k_mid].bone_mask, 1u8 << 3);

        // Single-frame bone copy from right_arm @ 0.30s -> paste @ 0.45s
        assert_eq!(s.copy_bone_frames(3, false), 1);
        s.current_time = 0.45;
        assert_eq!(s.paste_bone_frames(3, false), 1);
        let pose_45 = sample_clip_pose(s.active_clip(), 0.45);
        assert!((pose_45.bone(3).rot[0] - (-60.0)).abs() < 1e-3);
        assert!((pose_45.bone(4).rot[0] - 12.0).abs() < 1e-3, "Other bone must stay intact");

        // Shift-range copy [0.30s .. 0.45s] on right_arm -> mirrored paste onto left_arm (4) @ 0.0s
        s.selected_range_bone = Some(3);
        s.selected_range_times = Some((0.30, 0.45));
        assert_eq!(s.copy_bone_frames(3, false), 2);
        s.current_time = 0.0;
        assert_eq!(s.paste_bone_frames(4, true), 2);
        let mir_0 = sample_clip_pose(s.active_clip(), 0.0);
        assert!((mir_0.bone(4).rot[0] - (-60.0)).abs() < 1e-3);
        assert!((mir_0.bone(4).rot[1] - (-24.0)).abs() < 1e-3);
        assert!((mir_0.bone(4).rot[2] - 10.0).abs() < 1e-3);

        // Delete middle single bone frame of right_arm @ 0.30s: interpolates smoothly between 0.0s (0 deg) and 0.45s (-60 deg)
        s.clear_bone_selection();
        s.current_time = 0.30;
        assert_eq!(s.delete_bone_frames(3, false), 1);
        let interp_30 = sample_clip_pose(s.active_clip(), 0.30);
        assert!(interp_30.bone(3).rot[0] < -20.0 && interp_30.bone(3).rot[0] > -50.0);

        // Undo restores deleted keyframe cleanly
        let prev = s.undo_stack.pop().expect("undo snapshot");
        *s.active_clip_mut() = prev;
        let restored = sample_clip_pose(s.active_clip(), 0.30);
        assert!((restored.bone(3).rot[0] - (-60.0)).abs() < 1e-3);

        // Full-track copy on right_arm -> full-track clear on right_arm
        let track_frames = s.copy_bone_frames(3, true);
        assert!(track_frames >= 3);
        let cleared = s.delete_bone_frames(3, true);
        assert!(cleared >= 3);
        assert!(s.active_clip().keyframes.iter().all(|k| !k.has_bone(3)));
        assert!((sample_clip_pose(s.active_clip(), 0.0).bone(4).rot[0] - (-60.0)).abs() < 1e-3);
    }

    #[test]
    fn ruler_ticks_adapt_to_chosen_frame_counts() {
        let t29 = ui_bridge::build_ruler_ticks(29);
        assert_eq!(t29.len(), 30);
        let labels29: Vec<String> = t29.iter().filter(|t| !t.label.is_empty()).map(|t| t.label.to_string()).collect();
        // 29f fits every intermediate number (0..=29) cleanly at 1160px
        assert_eq!(labels29.len(), 30);
        assert!(labels29.contains(&"10".into()) && labels29.contains(&"11".into()) && labels29.contains(&"12".into()));
        assert!(labels29.contains(&"13".into()) && labels29.contains(&"14".into()) && labels29.contains(&"15".into()));
        assert_eq!(labels29.last().map(String::as_str), Some("29"));
        assert!(t29[5].major && !t29[1].major);

        // 60f at 1.0x uses even labels (0, 2, ..., 60) with every frame ticked; 4x zoom around 9..24f pops in 10..15!
        let t60 = ui_bridge::build_ruler_ticks(60);
        assert_eq!(t60.len(), 61);
        assert_eq!(t60[10].label.as_str(), "10");
        assert!(t60[5].mid && !t60[5].major, "Mid notch expected on 5f");

        let t60_zoom = ui_bridge::build_ruler_ticks_window(60, 0.15, 0.25, 1160.0);
        let labels_z: Vec<String> = t60_zoom.iter().filter(|t| !t.label.is_empty()).map(|t| t.label.to_string()).collect();
        for f in ["10", "11", "12", "13", "14", "15"] {
            assert!(labels_z.contains(&f.to_string()), "Zoomed 60f ruler missing {f}: {labels_z:?}");
        }
    }

    #[test]
    fn keyframes_snap_strictly_onto_frame_grid_and_resolve_collisions() {
        let mut clip = create_blank_clip(77);
        clip.duration = 0.48; // 29 frames
        clip.impact_time = Some(0.19);
        clip.keyframes = vec![
            crate::model::Keyframe { time: 0.0, easing: crate::model::EasingMode::EaseOut, bones: Default::default(), bone_mask: crate::model::ALL_BONES_MASK },
            crate::model::Keyframe { time: 0.09, easing: crate::model::EasingMode::Snap, bones: Default::default(), bone_mask: 1u8 << 3 },
            crate::model::Keyframe { time: 0.19, easing: crate::model::EasingMode::EaseOut, bones: Default::default(), bone_mask: crate::model::ALL_BONES_MASK },
            crate::model::Keyframe { time: 0.342, easing: crate::model::EasingMode::EaseInOut, bones: Default::default(), bone_mask: 1u8 << 3 },
            crate::model::Keyframe { time: 0.352, easing: crate::model::EasingMode::EaseOut, bones: Default::default(), bone_mask: 1u8 << 4 },
            crate::model::Keyframe { time: 0.48, easing: crate::model::EasingMode::Linear, bones: Default::default(), bone_mask: crate::model::ALL_BONES_MASK },
        ];
        crate::model::snap_clip_to_frame_grid(&mut clip);
        let dur = clip.duration;
        let frames = (dur * 60.0).round();
        for k in &clip.keyframes {
            let f_pos = (k.time / dur) * frames;
            assert!((f_pos - f_pos.round()).abs() < 1e-4, "Keyframe off-grid: time={} f_pos={f_pos}", k.time);
        }
        let f_imp = (clip.impact_time.unwrap() / dur) * frames;
        assert!((f_imp - f_imp.round()).abs() < 1e-4);
        assert!(clip.keyframes.windows(2).all(|w| w[1].time > w[0].time + 0.005));
    }
}
