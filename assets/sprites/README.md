# assets/sprites

Owned by the render crate (`starwood_render`).

This folder is intentionally empty at rest: **all sprites are generated
programmatically at runtime** from keys in `assets/data/*.ron` (races, enemies,
items, and rarity frames). See `crates/starwood_render/README.md`.

- Race/enemy bodies: 64×64
- Items: 32×32
- Rarity frames: 48×48 (colors from core `RarityData`)

Press **F3** in-game to toggle rank/anchor debug overlays.

Real art can replace placeholders by overwriting handles in `AssetHandles.sprites`.
