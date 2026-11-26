# VoxelMotion Studio

A fast, standalone **GPU-accelerated 3D voxel & humanoid keyframe animation studio** written in pure Rust (`Slint` + `wgpu` DirectX 12 / Vulkan, 4x MSAA).

> [!NOTE]
> **Project Transparency:** This project was built using **AI-assisted development** (architecture, Rust/WGSL/Slint engineering, and tooling) and verified with automated tests and live GPU benchmarks. It is published openly so anyone can inspect, download, or build on it.

Designed to start cleanly like **Blender** — opening to a single neutral keyframed scene so animators craft custom motions frame-by-frame without clutter — while offering opt-in preset templates and one-click runtime `.png` / `.jar` / `.zip` voxel asset importing.

## Highlights

- **Blender-Style Clean Startup:** Launches into a single clean neutral clip (`Nueva Acción`), ready for custom keyframe posing. Load built-in sample clips on demand with `+ Ejemplos` or reset anytime with `En Blanco`.
- **Native GPU Viewport (DirectX 12 / Vulkan):** Hardware `4x MSAA` viewport with real-time shadows, dual onion skinning (previous violet / next cyan), motion trail ribbon, solo-item inspection, and a live world backdrop preview.
- **Responsive Free Camera & Zoom-to-Cursor:**
  - Cursor-targeted exponential zoom (`Mouse Wheel` or `Ctrl + Drag`) — zooms directly toward the limb or weapon under the cursor.
  - `[F]` focuses smoothly on the active bone (`BrazoDerecho`, `Cabeza`, etc.); `[R]` resets camera view; `1..4` camera presets.
  - Continuous 144Hz free navigation (`W/A/S/D`, `Q/E`, `+/-`), middle/right-drag orbit & pan (`Shift + Drag`).
- **Interactive 3D Gizmo & Timeline:**
  - Full 8-bone humanoid virtual-waist rig (`Cintura`, `Cuerpo`, `Cabeza`, `BrazoDerecho`, `BrazoIzquierdo`, `Espada`, `PiernaDerecha`, `PiernaIzquierda`) with per-bone visibility toggles.
  - Color-coded X/Y/Z axis arrows + screen-space roll ring + center free-transform drag (`Rotate`, `Move`, `Scale`, `Select`).
  - Multi-track timeline at 60 FPS snap, draggable keyframe diamonds, `AutoKF`, Copy/Paste/Mirror pose symmetry, Undo/Redo, and impact marker placement.
- **Built-in Procedural Voxelizer & Runtime Importer:** Includes 8 clean procedural starter items plus a file dialog (`+ Importar`) to voxelize custom `.png` item sprites or extract textures from `.jar` / `.zip` packs on the fly.
- **Multi-Format Export:**
  - **Bedrock `1.8.0`** (`voxelmotion.animation.json`)
  - **GeckoLib `4.4.4`** (`voxelmotion_geckolib.animation.json`)
  - **Event Timings & Compat JSON** (`voxelmotion_timings.json`)
  - **6-Frame PNG Storyboard Strip**

## Keyboard Shortcuts

| Key | Action |
|---|---|
| `Space` | Play / Pause |
| `Left` / `Right` (or `,` / `.`) | Step frame backward / forward |
| `[` / `]` | Jump to previous / next keyframe |
| `K` / `Del` | Insert keyframe / Delete active keyframe |
| `F` / `R` | Focus active bone / Reset camera |
| `M` | Mirror L/R pose |
| `1` – `5` | Select / Rotate / Scale / Camera / Move tools |
| `W A S D Q E` / `+` `-` | Continuous free 3D viewport navigation |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `Ctrl+C` / `Ctrl+V` | Copy / Paste pose |

## Build & Run

Requirements: [Rust toolchain](https://rustup.rs/) (stable) and a DirectX 12 or Vulkan capable GPU.

```sh
cargo test --release
cargo run --release
```

On Windows, you can also run `launch_voxelmotion.bat` directly.

## Contributing & Repository Policy

Anyone can freely **clone, download Releases, and fork** this repository under the [MIT License](LICENSE). Direct edits/pushes to `main` are restricted — external contributors can fork the repo and open a Pull Request, and **only the repository owner decides which changes get accepted and merged** after review (see [CONTRIBUTING.md](CONTRIBUTING.md)).
