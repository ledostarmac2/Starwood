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
   render add a single shared shockwave/ring at the blast and is optional.

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

3. **Resolved: PC build is message-driven.** Review-confirm fires
   `CharacterBuildRequested`; core spawns the PC. The UI keeps
   `spawn_member_from_saved` only for the Continue/save-load path.

4. **Resolved: core owns encounter exits.** `handle_encounter_ended_state`
   transitions to Exploration/GameOver and `SurrenderRequested` replaces flee.

5. **Camera ownership.** Render owns the `Camera2d` in `StarwoodRenderPlugin`;
   UI should remove its fallback camera spawn once integrated.

6. **Turn advancement has no owner.** Core builds the initial turn order and sets
   the first `ActiveTurn`, but nothing moves it afterward, so the UI advances
   `ActiveTurn` (in `hud::advance_turn_after_action`, on `RollAnimationComplete`
   or a no-roll action). *Ask:* if core wants combat fully authoritative, a core
   turn-advance system (driven off `RollAnimationComplete` + an
   `ActionResolved`/`TurnEnded` message) would let the UI stop owning this.

7. **Resolved: dead/leftover foes are despawned on encounter cleanup.**
   `handle_encounter_ended_state` now despawns every entity still tracked in
   `EncounterState.enemies` before clearing encounter state.

8. **Resolved: enemy attacks use archetype stats.** `request_combat_actions`
   now uses `EnemyArchetypeData.attack_bonus` and `damage` when the actor is an
   `EnemyUnit`.

9. **No enemy rank-collapse.** Enemies keep their spawn `slot`, so once a
   melee-reachable front rank dies the back ranks stay unreachable. The UI keeps
   the frontmost living foe always-targetable to avoid a soft-lock. A core
   rank-collapse (or `RankChanged`) on death would make reach honest.
