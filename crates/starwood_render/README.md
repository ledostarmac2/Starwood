# starwood_render

The **visual world layer** for Starwood. Owns `crates/starwood_render` and the
`assets/sprites/` folder. Depends only on `starwood_core` and obeys its Shared
Contract; it never touches the UI or core crates.

Add it with `StarwoodRenderPlugin` (the binary already does this).

## What it renders

- **Lane / rank layout** — a 2D camera plus two backdrop halves: the **party**
  musters on the left (≤4 members), the **enemy line** forms on the right (≤5),
  meeting at center. `slot` is the **rank** (0 = front).
- **Paper-doll units** — characters/enemies composed from `SpriteParts` +
  `Equipment`.
- **Animations** — idle bob, attack lunge, hurt flash + shake, rank-swap slide,
  death dissolve, and a distinct downed state for the player character.
- **Item icons** — items rendered as a 32×32 sprite over a **rarity-colored
  frame** for the UI crate.

The Dice Theater, egui panels, menus, and HUD are **not** here — they belong to
`starwood_ui`.

## Lane / rank layout

Combat uses a Darkest-Dungeon-style formation, so positioning is mechanically
meaningful. A unit's `slot` is its **rank**:

- **Rank 0 = front** (closest to the enemy line at screen center); higher ranks
  step back, up, and shrink slightly (`rank_scale`) so the formation reads with
  depth and back ranks render behind front ranks.
- The party fills ranks leftward from center; enemies mirror it rightward.
- `party_slot_position(slot)` / `enemy_slot_position(slot)` are the public
  helpers; both saturate (party at 3, enemies at 4).

**Rank-swap ("move").** When a unit's `slot` is reassigned, `apply_rank_slide`
animates its root to the new rank position over ~280 ms instead of teleporting.
The slide is ready now; it fires automatically once core drives rank changes at
runtime (an explicit `RankChanged` message is requested in `NEEDS_FROM_CORE.md`,
but diffing `slot` works without it).

## Paper-doll layering

For any entity with `SpriteParts` + `Equipment`, the unit is built as a nested
hierarchy so multiple animations never fight over the same `Transform`:

```text
core entity (root)   -- world position by slot, scaled by UNIT_SCALE
  └─ bob_pivot         -- idle breathing bob (bevy_tweening, looped + mirrored)
       └─ lunge_pivot   -- attack lunge (bevy_tweening sequence, on demand)
            └─ fx_pivot   -- hurt shake (timer system)
                 ├─ base body layer        (z 0.0)
                 ├─ feet  layer            (z 0.1)
                 ├─ body  layer            (z 0.2)
                 ├─ off_hand layer         (z 0.3)
                 ├─ head  layer            (z 0.4)
                 └─ main_hand layer        (z 0.5)
```

Each equipped `ItemInstanceId` is resolved to its base item template via
`base_item_for_instance()` and drawn at a fixed 64×64 anchor slot. Layers are
tagged with `UnitLayer { owner }` so a unit's whole stack can be found at once.

- **Party** members have fully swappable equipment. On `EquipmentChanged` the
  unit's layers are despawned and recomposed immediately.
- **Enemies** carry an empty `Equipment`, so only their baked archetype body is
  drawn (no swapping needed).

Units appear automatically: any logical entity with `SpriteParts` + `Equipment`
that lacks visuals is picked up and composed, so the render crate stays purely
reactive to `core`.

## Animation & combat reactions

| Trigger (core event)        | Effect                                              |
|-----------------------------|-----------------------------------------------------|
| spawn                       | infinite idle bob (desynced per unit)               |
| `slot` reassigned           | rank-swap slide to the new rank                     |
| `CombatActionRequest` (Attack) | lunge actor toward target and back               |
| `DamageDealt`               | red flash on the target's layers + shake (bigger on crit) |
| `UnitDied` (companion/enemy)| dissolve (alpha fade), then visuals torn down        |
| `UnitDied` (player character)| **downed** state (pulsing pale tint), not dissolve |
| health restored while downed| downed state cleared, full color returns (revive)   |
| `EncounterEnded`            | enemy entities despawned (stage cleared)             |

Cosmetic randomness (shake, bob desync) uses a render-only `RenderRng` so it can
never desynchronize `core`'s authoritative `GameRng`.

### Friendly fire / AoE

Hurt-flash is keyed to **each** `DamageDealt` target, so an AoE that damages
your own ranks lights up those allies too — no lane-specific special-casing. A
coarser shared blast effect can be added once core emits an AoE-resolution
message (see `NEEDS_FROM_CORE.md`).

### Downed vs. death (two-tier)

Companion and enemy deaths are permanent: their layers dissolve and the visual
hierarchy is torn down. The **player character** instead enters a *downed*
state — a slow pulsing pale-blue tint, clearly distinct from the red hurt-flash
and the death dissolve — and is restored to full color when core removes
`Downed` or its health returns above 0 (a revive). The PC is identified via
core's `PlayerCharacter` marker; render mirrors core's `Downed` component when
clearing the visual.

## Placeholder pixel-art

Real art does not exist yet, so every sprite key referenced by `GameData` is
turned into a small, distinct, palette-consistent texture **at runtime**:

- Race base bodies and enemy archetypes: **64×64**.
- Items: **32×32**, shaped by their `ItemSlot` (weapon / shield / armor / helm /
  boots / potion / gem).
- Rarity frames: **48×48**, one per rarity tier — a bright border + soft
  background in the tier's color, with brighter corner accents on higher tiers.

Generation is deterministic (a sprite key always produces the same art) and uses
one cohesive limited palette. Enemy silhouettes vary by archetype so the stage
reads as a varied group.

Generated handles are stored in two places:

- `SpriteManifest` (this crate), and
- the shared `AssetHandles.sprites` resource, keyed by sprite key,

so the UI crate can resolve icons through either.

### Dropping in real art later

Replace the procedural texture for a key without touching code: load a real
image into `Assets<Image>` and overwrite the handle stored under that sprite key
in `AssetHandles.sprites` (and `SpriteManifest`). Because everything resolves by
sprite key, the rest of the pipeline is unchanged.

## Item rendering (per-instance)

Items render **per instance**: the base item sprite is drawn on top of a
rarity-colored frame chosen via `rarity_rank(instance.rarity)`. Use
`ItemIcon::from_instance` / `from_instance_id` (or `instance_icon_handle`) when
you have an `ItemInstance`; `ItemIcon::new` / `with_rarity` remain for bare
base-item previews. The rarity palette (`rarity_style`) is classic high-fantasy
and bright: gray → green → blue → purple → gold → red.

## Public API for the UI crate

- `ItemIcon::from_instance(instance)` / `from_instance_id(id, &ItemInstances)` —
  preferred: build an icon from a rolled instance (base sprite + rarity frame).
- `ItemIcon::new(item)` / `ItemIcon::with_rarity(item, tier)` — plain or framed
  icon for a base item template (previews, shops showing templates, etc.).
- `item_icon_handle(&SpriteManifest, &GameData, &ItemId) -> Option<Handle<Image>>`
  — resolve a base item's icon texture directly (e.g. to register with egui).
- `instance_icon_handle(&SpriteManifest, &GameData, &ItemInstances, &ItemInstanceId)`
  — resolve an instance's base-item icon texture.
- `rarity_frame_handle(&SpriteManifest, tier) -> Option<Handle<Image>>` —
  resolve a rarity-frame texture for the same purpose.
- `rarity_style(tier) -> RarityStyle` — the frame/fill colors for a tier.
- `SpriteManifest` / shared `AssetHandles` — key → `Handle<Image>` maps.
