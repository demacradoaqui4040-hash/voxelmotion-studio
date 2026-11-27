# Changelog

All notable changes to **VoxelMotion Studio** are documented in this file using [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) formatting and [Conventional Commits](https://www.conventionalcommits.org/).

## [Unreleased]

## [2.0.0] - 2026-11-26

### Added
- **Continuous Surface v2 UI Redesign (`DEC-01`–`DEC-18` in `DESIGN_MASTER.md`, `ui/`):**
  - `36px` **Silent Chrome** top bar with `Archivo ▾` menu, centered inline-renamable clip switcher (`Attack 1 • ▾`), `[⚙]` Settings modal trigger, and custom isometric logo (`assets/voxelmotion-icon.svg`).
  - Dedicated left **Clips** panel featuring a monochrome `[Arma ▾] < >` selector and pinnable collapsible weapon picker (`Fijar`).
  - Minimal 3-tool floating left rail (`Mover`, `Rotar`, `Escalar`) plus gizmo visibility toggle, and a clean top-right `[Cámara ▾] [Vista ▾] [⛶]` HUD without an XYZ triad overlay.
  - Streamlined 2-tab right Inspector (`Pose` | `Reglas y VFX`) with a direct 8-bone skeleton list, transform axis scrubbers, and animation easing/duration controls.
  - Unified `156px` bottom Timeline with centered vector media transport, **Spotlight** command palette (`Ctrl+K`), **Exportar** (`Ctrl+E`), and **Ajustes** (`Ctrl+,`) modals.
  - Unified Lucide `1.65px` vector iconography (`ui/widgets.slint`) and bundled OFL typography (`Plus Jakarta Sans` & `JetBrains Mono` in `ui/fonts/`).
- **Data-Driven `DesignSpec` Engine (`src/design_spec.rs`, `src/shaders.wgsl`):**
  - Five genuinely distinct artistic directions driven from Rust: **Pro Studio Neutral**, **Field Hardware**, **Daylight Studio**, **CAD Blueprint**, and **Studio Clay**.
  - Dynamically switches corner radii (`0px`–`8px`), wireframe vs solid border polarity, mechanical toggle vs pill switch geometry, monospace vs sans headers, and matching WGSL 3D stages/lighting.
- **True Per-Bone Keyframe Editing (`src/model.rs`, `src/app_state.rs`, `src/ui_bridge.rs`, `ui/app.slint`):**
  - Backwards-compatible per-keyframe `bone_mask: u8` bitmask with independent per-bone channel interpolation inside `sample_clip_pose`.
  - Discrete `Hueso | Todo` Timeline scope pill (`DEC-18`), `Shift+Click` multi-frame range selection with lane highlight, cross-clip bone clipboard, L/R mirrored bone paste, full bone track copy/clear, and right-click context menus on bone tracks and diamonds.
  - Contextual shortcuts: `Ctrl+C` / `Ctrl+V` and `Supr` / `Backspace` obey the active scope (`Hueso` or `Todo`), while `Ctrl+Shift+C`, `Ctrl+Shift+V`, and `Shift+Supr` always target the full 8-bone pose.
- **Modular Rust Architecture & Combat Ecosystem (`src/app_state.rs`, `src/camera.rs`, `src/ui_bridge.rs`, `src/modpack.rs`, `src/player_animator.rs`):**
  - Decoupled the monolithic entrypoint into dedicated state, camera, UI presenter, `DesignSpec`, modpack, and `playerAnimator` modules.
  - Added `playerAnimator` JSON importer (waist-leg composition and held-item hand-space conjugation), local modpack auto-discovery, 10-family weapon classifier, per-clip compatibility rules, curation status tracking, VFX timing events (`SlashTrail`, `ParrySpark`, `SonarRing`, `HitStop`, `ScreenShake`), and a training dummy mesh.
- **Automated Verification & Benchmarks (`src/main.rs`, `src/ui_bridge.rs`):** Added `--bench-window`, `--bench-orbit-zoom`, `--capture-window` / `--capture-theme` / `--capture-tab` / `--capture-time`, and expanded unit test coverage to 18 release tests.

### Changed
- **3D Stage & Rendering (`src/shaders.wgsl`, `src/renderer3d.rs`, `src/math3d.rs`):** Upgraded to 3-point studio lighting with ACES filmic tone mapping, soft grounded contact shadow disk, theme-calibrated floor grids (including a neutral mid-grey 3D floor for `Daylight Studio` so VFX trails stay readable), and a thinner low-opacity rotation gizmo ring.
- **Zero-Allocation Pose Pipeline (`src/model.rs`, `src/voxelizer.rs`):** Replaced `HashMap`-based pose sampling with stack-allocated `RigPose([BoneTransform; 8])` (keeping JSON backwards compatibility), `Arc<SpritePixels>` sharing, and an LRU GPU weapon mesh cache.
- **Camera Controller (`src/camera.rs`):** Switched to additive zoom + pan composition, immediate fine touchpad zoom, fast-settling wheel damping, smooth animated camera presets, and clean left-drag orbit directly over the character mesh (short clicks `< 4.5px` pick the bone).
- **Exporters & Local Persistence (`src/exporter.rs`, `src/modpack.rs`, `.gitignore`):** Added RELL Seas (`F3+T`) & curated weapon export with approval filtering and VFX event baking, atomic `profile.local.json` writes, and ignored `profile.local.json` in `.gitignore`.

### Performance
- Eliminated `orbit → release → zoom` input stutter via per-frame redraw coalescing (`redraw_queued`), idle-gated thumbnail batches (`batch(2)` after a `250ms` quiet cooldown), off-UI-thread atomic profile persistence, and removing dynamic `drop-shadow-blur` overlays above the 3D viewport (`> 130 FPS` sustained in `--bench-window` and `--bench-orbit-zoom`).
- Replaced polling loops with event-driven single-shot timers (`TOAST_WAKE`, `SAVE_WAKE`, `THUMB_WAKE`), achieving `0.000%` idle CPU usage once settled.

### Fixed
- Selecting a timeline diamond now activates its bone lane (`track_idx`) cleanly alongside jumping to its timestamp.
- Replaced OS-dependent Unicode chevron characters in `[Cámara ▾]` and `[Vista ▾]` HUD buttons with crisp SVG vector chevrons.
- Deduplicated header clip counts in the Export modal and tightened vertical alignment in Settings and Inspector layouts.

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
