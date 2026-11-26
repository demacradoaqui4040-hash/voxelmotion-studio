# Changelog

All notable changes to **VoxelMotion Studio** are documented in this file using [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) formatting and [Conventional Commits](https://www.conventionalcommits.org/).

## [Unreleased]

## [1.0.0] - 2026-11-25

### Added
- **Standalone VoxelMotion Studio (`src/`, `ui/`):** Clean pure-Rust native GPU 3D keyframe animation workbench powered by `Slint 1.18` and `wgpu 30` (`DirectX 12` / `Vulkan`, 4x MSAA viewport rendering).
- **Blender-Style Blank Startup Workflow (`src/model.rs`, `src/main.rs`):** Launches cleanly with a single neutral keyframed action (`Nueva Acción`), plus quick `En Blanco` reset and opt-in `+ Ejemplos` preset loading in the studio header and Project tab.
- **Procedural Starter Kit & Runtime Asset Importer (`src/catalog.rs`, `src/voxelizer.rs`):** Built-in procedural voxel item library with zero external asset dependencies, alongside a native `+ Importar` file dialog supporting custom `.png` item sprites and `.jar` / `.zip` resource archives.
- **Interactive 3D Viewport, Gizmos & Camera Rig (`src/math3d.rs`, `src/renderer3d.rs`, `src/shaders.wgsl`):**
  - 8-bone humanoid virtual-waist rig with per-bone eye visibility toggles, camera-relative 3D X/Y/Z axis + roll ring handles, and free drag transforms.
  - Smooth cursor-targeted zoom (`Mouse Wheel` / `Ctrl+Drag`), `[F]` focus on active bone, `[R]` camera reset, and continuous 144Hz `WASD`/`QE` navigation.
- **Multi-Track Timeline & Exporters (`src/exporter.rs`):** 60 FPS keyframe snapping, draggable keyframe diamonds, `AutoKF`, dual onion skinning, L/R mirror pose, undo/redo history, Bedrock `1.8.0` / GeckoLib `4.4.4` / timings JSON exporters, and 6-frame PNG storyboard rendering.
- **Open-Source Tooling & Automation (`.github/workflows/`, `scripts/`):** GitHub Actions CI (`ci.yml`), automated Windows release packaging (`release.yml`), MIT license, Conventional Commits hook, and contributor guidelines.
