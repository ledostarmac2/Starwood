# assets/sprites

Owned by the render crate (`starwood_render`).

This folder is intentionally (mostly) empty: all character, enemy, and item
sprites are **generated programmatically at runtime** as placeholders, keyed by
the `sprite_key` fields in `assets/data/*.ron`.

- Race base bodies & enemy archetypes: 64×64
- Items: 32×32

To ship real art, drop image files here and load them over the handle stored
under the matching sprite key in the shared `AssetHandles.sprites` map (see
`crates/starwood_render/README.md`). No gameplay or layout code needs to change.
