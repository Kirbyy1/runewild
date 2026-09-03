# Runewild

A first-person fantasy voxel sandbox game built with **Rust** and [Bevy 0.14](https://bevy.org).

Deterministic procedural terrain, biomes, flowing water, block building and a
sparse-sectioned infinite world — all rendered with a custom greedy mesher.

The world and its textures are art-directed toward a stylized Hytale-like
look: land-forward continents with rolling meadows, chunky hills and
commanding mountain ranges; lush overgrown forests with big broadleaf
canopies; and a painterly procedural resource pack — saturated sunlit
palettes, hand-painted colour clumps, sun-rimmed foliage and detailed bark,
stone and soil — under warm golden daylight.

## Features

- Deterministic seeded voxel terrain (`u64` seed, fully reproducible)
- Plains, forest, desert and mountain biomes with stylized procedural trees
- Infinite horizontal streaming with sparse vertical chunk sections — no build ceiling, negative Y supported
- Greedy mesh batching split into opaque, transparent and water passes
- Cellular water simulation: rivers/lakes/oceans flood player-dug holes Minecraft-style
- First-person controller: walking, sprinting, jumping, double-tap-space flight
- Voxel raycasting for breaking/placing blocks, with placement guards against the player collider
- Title screen, pause menu, settings menu, hotbar, crosshair, FPS counter and chat-style log
- JSON persistence: only the seed, player position and *modified blocks* are saved
- Built-in visual regression harness (`cargo run -- --visual-test`)

## Controls

| Input | Action |
|---|---|
| `W A S D` | Move |
| `Mouse` | Look |
| `Left Mouse` | Capture mouse / break targeted block |
| `Right Mouse` | Place selected block |
| `Space` | Jump |
| `Double-tap Space` | Toggle flight |
| `Shift` | Sprint |
| `1-9` | Select hotbar slot |
| `F3` | Toggle debug overlay |
| `Esc` | Pause menu |

The world autosaves every 30 seconds, on quit, and on window close.

## Getting started

Requires a recent stable [Rust](https://rustup.rs) toolchain.

```bash
git clone https://github.com/Kirbyy1/runewild.git
cd runewild
cargo run --release
```

> **Tip:** debug builds compile fast but run slowly; use `--release` or rely on
> the tuned profiles in `Cargo.toml` (`opt-level = 1` locally, `3` for dependencies).

### Visual test harness

Renders fixed camera angles, saves screenshots to `visual_tests/` against the
`baseline/` set, and measures frame cost with vsync disabled:

```bash
cargo run --release -- --visual-test
```

The harness redirects its save/settings files into `visual_tests/` so your real
world is never touched.

---

## How the code works

Runewild is a plain Bevy app composed of five plugins, wired together in
`src/game/mod.rs`. The active screen is modelled as a Bevy state machine
(`GameState`: `MainMenu`, `Playing`, `Paused`, `Settings`); most gameplay
systems only run while `Playing`.

```text
src/
├── main.rs            Entry point; isolates the --visual-test save file
├── game/              App builder, plugin wiring, GameState, world seed
├── settings.rs        User-adjustable world/render settings (JSON)
├── world/             Terrain storage, generation, meshing, water, saving
├── player/            First-person controller and block interaction
├── rendering/         Texture atlas, sky/clouds atmosphere, materials
└── ui/                Menus, hotbar, crosshair, FPS, chat log
```

### World data model — `src/world/`

The world is a hash map of **sections**, not monolithic chunks. One section is
a 32×32×32 cube of voxels (`CHUNK_SIZE = 32`, one voxel = one metre); a
vertical stack of sections forms a column identified by a `ChunkCoord`.

- **Sparse storage** (`chunk_manager.rs`): a column only materialises the
  sections that actually contain content — stone up to the surface, water
  basins, vegetation and player edits. Empty air between them costs nothing,
  which makes an effectively unbounded Y range affordable. Sections outside
  the safety envelope (`SECTION_MIN_Y..SECTION_MAX_Y`) are refused.
- **Budgeted streaming**: each frame the manager generates at most one new
  column and rebuilds at most three meshes (`COLUMN_GENERATION_BUDGET`,
  `CHUNK_MESH_BUDGET`), loading/unloading columns in a ring around the player
  so frame times stay flat while exploring.

### Terrain generation — `src/world/generation/`

A layered pipeline driven entirely by the shared world seed, so any machine
generates identical terrain:

1. **`noise.rs`** — seeded multi-octave noise fields (via the `noise` crate)
   used by everything downstream.
2. **`biome.rs` / `regional.rs`** — low-frequency climate fields select plains,
   forest, desert or mountain regions and blend their parameters smoothly.
3. **`terrain.rs`** — shapes the height field relative to sea level
   (`SEA_LEVEL_METRES = 20`), carves river networks and fills ocean/river
   basins with water.
4. **`trees.rs`** — stamps stylized trees onto suitable surfaces using
   deterministic per-position hashing, so no tree depends on load order.
5. **`diagnostics.rs`** — optional export of biome/height maps for debugging.

Cave carving exists behind the `ENABLE_CAVES` flag until its pass is ready.

### Meshing — `src/world/meshing/greedy.rs`

Each dirty section is converted to triangle meshes on the CPU:

- Only faces adjacent to air/transparent blocks are emitted (hidden-face culling).
- Coplanar same-block faces are merged greedily into large quads, cutting
  vertex counts dramatically on flat terrain.
- Output is split into three buffers — **opaque**, **transparent** and
  **water** — each uploaded as its own Bevy entity so alpha blending is ordered
  correctly.

### Water — `src/world/water.rs`

A lightweight cellular automaton running after chunk loads and before meshing.
Procedural oceans/rivers stay static until a player edit touches them; any
procedural water cell neighbouring an edit then acts as an infinite source.
Flow cells fall first, then spread with a bounded horizontal level (max 6).
Flow itself is deliberately transient — only authored edits reach the save file.

### Player — `src/player/`

- **`controller.rs`** — kinematic character controller written by hand
  (gravity, jump, sprint, double-tap flight) with capsule-vs-voxel collision;
  mouse capture is managed across menu/game states.
- **`interaction.rs`** — DDA voxel raycast from the camera; left-click breaks,
  right-click places the selected hotbar block, and placements intersecting the
  player capsule are rejected.

### Rendering — `src/rendering/`

- **`texture_atlas.rs` / `texture_loader.rs`** — the block texture atlas is
  generated procedurally at startup (with a hand-authored override path under
  `assets/`), and the water slice is animated by scrolling UVs.
- **`atmosphere.rs`** — sky gradient, fog and drifting cloud entities.
- **`materials.rs`** — extended Bevy materials carrying per-face tinting so
  terrain gets cheap directional shading without extra lights.

### UI — `src/ui/`

Bevy-native UI nodes: main menu, pause menu, settings menu, hotbar, crosshair,
FPS counter and a Minecraft-style chat log that reports world events. All UI
systems respect `GameState` so overlays appear only when relevant.

### Persistence — `src/world/persistence/`

Saves are small JSON documents containing the seed, player position and a map
of edited voxels only. On load, the world regenerates from the seed and replays
edits on top — so saves stay tiny no matter how much terrain you explore.

---

## Development

```bash
cargo fmt --check     # formatting
cargo clippy          # lints
cargo test            # unit tests (meshing, coordinates, generation)
cargo run             # play
```

Profiling hooks can be enabled at runtime with `PROFILE_WORLD=1`.

## License

All rights reserved. See the repository for details.
