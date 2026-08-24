# Runewild

A first-person fantasy voxel sandbox foundation built with Rust and Bevy.

## Current Milestone

- Deterministic seeded voxel terrain
- Horizontal chunk loading and unloading around the player
- Chunk mesh batching with visible-face generation
- Opaque and transparent mesh separation
- Plains, forest, desert and mountain biome logic
- Stylized procedural trees
- First-person movement, gravity, jumping and sprinting
- Voxel raycast block breaking and placement
- Placement guard against the player collider
- Center crosshair, 1-9 hotbar and F3 debug overlay
- Title screen and pause menu (`Esc`) with save / quit options
- Sparse vertical chunk sections: no build ceiling, negative Y supported
- JSON persistence for seed, player position and modified blocks only

## Controls

- `Left Mouse`: capture mouse / break targeted block
- `Right Mouse`: place selected block
- `WASD`: move
- `Space`: jump
- `Double-tap Space`: toggle flight
- `Shift`: sprint
- `1-9`: select hotbar slot
- `F3`: toggle debug overlay
- `Esc`: open the pause menu (or resume from it)

## Menus

- **Title screen** (shown at startup): `Play` enters the world, `Quit` exits
- Left-click anywhere in-game to capture the mouse
- **Pause menu** (`Esc`): `Resume`, `Save & Main Menu`, `Quit Game`
- The world autosaves every 30 s, on quit and on window close

## Verification

Run these once Rust is installed and `cargo` is on PATH:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run
```
