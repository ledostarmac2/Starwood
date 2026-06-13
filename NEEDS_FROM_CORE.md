# Needs From Core

Agents should append desired shared contract additions here for human
reconciliation.

## From `starwood_render` (Cursor)

Resolved: `ItemInstance` / `Rarity` / `ItemInstances`, `base_item_for_instance()`,
`rarity_rank()`, equipment slots holding `ItemInstanceId`, and the
`PlayerCharacter` + `Downed` two-tier death markers. Render now resolves
paper-doll layers and item icons from instances and keys the downed visual off
`PlayerCharacter` / `Downed`.

Still optional (nice-to-have, not blocking):

1. **Rank semantics + rank-swap signal.** Confirm `PartyMember.slot` /
   `EnemyUnit.slot` are the **rank** (0 = front). Render already animates a
   slide whenever a unit's `slot` changes; an explicit `RankChanged { entity,
   from, to }` message would be cleaner than diffing `slot` each frame, but is
   optional.

2. **AoE / friendly-fire resolution.** Render flashes each target it sees in a
   `DamageDealt`, so friendly-fire already "just works" if core emits
   `DamageDealt` for every affected entity (including allies). A coarse
   `AoeResolved { origin_rank, side, affected: Vec<Entity> }` message would let
   render add a single shared shockwave/ring at the blast and is preferred for
   the marquee AoE effect, but is optional.

## From `starwood_ui` (Claude Code)

The UI currently works against the existing contract with local placeholders for
each item below. None of these block the milestone; they would just let the UI
stop interpreting results and let core stay the single source of truth.

1. **Resolved: `AbilityScoreGen` drops the lowest die in core.**
   `RollResolved.total` is now authoritative for 4d6-drop-lowest ability
   generation; the UI no longer computes dice outcomes locally.

2. **Resolved: shared inventory contract exists in core.**
   `Inventory` now holds `ItemInstanceId`s, `ItemInstances` stores the rolled
   item data, and `INVENTORY_CAPACITY` exposes the 20-slot cap. UI still keeps a
   local transition stash for its current overlay, but the published core
   contract is available.

3. **A character-finalize / party-spawn helper in core.**
   On Review-confirm the UI builds the member entity (via the public
   `apply_race_mods` / `derived_stats` / … functions) and pushes it onto
   `PartyRoster`. *Ask:* a `core` helper such as
   `spawn_party_member(commands, draft-like input) -> Entity` so entity assembly
   lives next to the rules it depends on. The same helper would serve the
   Continue/save-load path (`spawn_member_from_saved`).

4. **Encounter exits could be owned by core.**
   The UI listens for `EncounterEnded` and performs the state transition +
   stage cleanup (despawn foes, clear `EncounterState`, award loot), and adds a
   "flee" path (`EncounterEnded { victory: false }` plus a UI-side `fled` flag so
   it resolves to Exploration rather than GameOver). Note `detect_encounter_end`
   re-fires `EncounterEnded` every frame until `EncounterState.enemies` is
   cleared; the UI guards against the duplicates, but a one-shot guard in core
   would be cleaner.

5. **Camera ownership.** Render is the nominal camera owner but ships as a stub,
   so `starwood_ui` spawns a `Camera2d` only if none exists. Please pick one
   crate (likely render) to own it so the guard can be removed.

## Current Out-of-Lane Workspace Blocker

`cargo check --workspace` is blocked in `starwood_render`: `resolve_item_icons`
is public while its `Without<IconResolved>` query marker is private. This is a
render-crate visibility fix; Codex did not edit render per the lane boundary.
