# Workspace Dependencies Todo

Agents should append requested shared dependencies here instead of editing the
root workspace manifest directly.

## starwood_render (added for integration)

These are declared in `crates/starwood_render/Cargo.toml` and should be hoisted
to the workspace `[workspace.dependencies]` when the root manifest is next edited:

- `bevy_tweening = "0.15"` — attack lunge, rank slides, death dissolve tweens
- `rand = "0.9"` — cosmetic render RNG (shake direction, bob desync)
- `rand_chacha = "0.9"` — seeded `RenderRng` separate from core `GameRng`

## starwood_ui (Claude Code)

No **new** shared workspace dependencies were required for the UI integration.
`crates/starwood_ui/Cargo.toml` uses only existing workspace deps (`bevy`,
`bevy_egui`, `starwood_core`, and `rand_chacha` — the latter solely to type the
seeded RNG threaded into core's `roll_item_instance` for shop stock). The Dice
Theater is hand-rolled `bevy` sprites, so it needs neither `bevy_tweening` nor
`bevy_hanabi`.
