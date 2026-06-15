# starwood_render

The **visual world layer** for Starwood. Owns `crates/starwood_render` and the
`assets/sprites/` folder. Depends only on `starwood_core` and obeys its Shared
Contract; it never touches the UI or core crates.

Add it with `StarwoodRenderPlugin` (the binary already does this).

## Integration with the live game loop

Render is **reactive** to core — it does not drive gameplay:

| Core signal | Render response |
|---|---|
| Entity with `SpriteParts` + `Equipment` appears | Paper-doll spawned into rank layout |
| `EquipmentChanged` | Layers recomposed from `ItemInstance` base sprites |
| `PartyMember.slot` / `EnemyUnit.slot` changes | Rank-swap slide (~280 ms) |
| `CombatActionRequest` (Attack) | Attack lunge toward target |
| `DamageDealt` (each target, incl. friendly fire) | Red flash + shake on that unit |
| `UnitDied` + `PlayerCharacter` | **Downed** visual (pale pulse); core keeps entity |
| `UnitDied` + `EnemyUnit` | Death dissolve, then hide |
| `UnitDied` + companion | *(none)* — core despawns companions immediately |
| `Downed` added on PC | Downed visual (backup path) |
| `Downed` removed / HP restored | Downed visual cleared |
| `EncounterEnded` | Enemy entities despawned (stage cleared) |

Party ranks 0..=3 on the **left**; enemy ranks 0..=4 on the **right**. Slot
`0` is the front rank nearest center.

## Debug overlay (F3)

Press **F3** during play to toggle `RenderDebugOverlay`:

- Semi-transparent **rank slot boxes** for every party/enemy rank position
- **Anchor crosshairs** and rank labels (`P0`..`P3`, `E0`..`E4`) tracking live units

Use this when tuning formation layout or paper-doll anchor offsets.

## Placeholder coverage

At startup, once `GameData` loads, the crate programmatically generates a
distinct pixel-art texture for **every sprite key** referenced in
`assets/data/*.ron`:

- **6** race base bodies (`race_*`, 64×64)
- **8** enemy archetypes (`enemy_*`, 64×64)
- **16** item bases (`item_*`, 32×32) — including consumables like
  `item_ember_soup` and `item_scroll_of_sparks`
- **5** rarity frames (`rarity_frame_0`..`4`) using core's `RarityData.frame_color`

Keys are registered in both `SpriteManifest` and shared `AssetHandles.sprites`.
If a key is missing at compose time (e.g. a rolled instance references a new
base), `ensure_body` / `ensure_item` / `ensure_fallback` generates one on
demand so the game never shows a blank sprite.

Run `cargo test -p starwood_render all_game_data_sprite_keys_have_placeholders`
to verify coverage against the live data catalog.

Real art can replace placeholders by overwriting the handle for a sprite key —
no code changes required.

## Item rendering (per-instance)

- **`ItemIcon(instance_id)`** — spawn an entity; render resolves the base sprite
  from `ItemInstances` + `base_item_for_instance`, with a rarity frame from
  `rarity_rank(instance.rarity)` / core `RarityData`.
- **`instance_icon_handle`** / **`rarity_frame_handle`** — resolve handles for
  egui/inventory without spawning entities.

Paper-doll equipment layers use the same instance → base sprite-key resolution.

## Public API summary

- `ItemIcon`, `instance_icon_handle`, `item_icon_handle`, `rarity_frame_handle`
- `party_slot_position`, `enemy_slot_position`, `rank_scale`
- `RenderDebugOverlay`, `SpriteManifest`, `DownedVisual`
- `rarity_style`, `rarity_frame_key_for(Rarity)`
