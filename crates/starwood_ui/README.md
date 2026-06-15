# starwood_ui

The UI layer for **Starwood**: every egui screen, the combat UI, the inventory /
shop, a debug overlay, and the **Dice Theater**. It depends only on
`starwood_core`, imports all shared types from there, and never edits another
crate.

Add it with `StarwoodUiPlugin` (the binary already does).

## Modules

| Module | Responsibility |
|---|---|
| `theme` | Fantasy egui theme + rarity-frame colours + optional display font from `assets/fonts/`. |
| `menu` | Main menu: difficulty select, 3 campaign slots (continue/delete), New Game. |
| `creation` | One full PC (Race → Class → Abilities → Talents → Review) then the 3 companion **classes** (Companions step). |
| `hud` | Party panel (HP/mana/gold, revive), Exploration map, and the encounter combat UI + turn advancement. |
| `inventory` | 20-slot inventory on rolled item instances (rarity frames, affix tooltips), shop buy/sell, character sheet, skills tab, talent tree + subclass. |
| `debug` | Toggleable overlay (F1). |
| `save` | Autosave slot, index on startup, deferred Continue load. |
| `dice` | The Dice Theater. |

egui draws in **`EguiPrimaryContextPass`**; world/sprite/flow logic in **`Update`**.

## The UI drives the live game through messages

`starwood_core` is authoritative and message-driven. The UI **fires request
messages and reflects state** — it never resolves rules itself:

| Player action | Message fired | Core does |
|---|---|---|
| New Game | `NewGameRequested { seed }` | reset run, generate map, → creation |
| Continue a step | `CreationStepAdvanceRequested` | advance the `CreationStep` sub-state |
| Confirm the PC | `CharacterBuildRequested { … }` | spawn the PC entity (first = `PlayerCharacter`) |
| Begin expedition | `FinishPartyCreationRequested` | → Exploration |
| Enter a fight | `EncounterRequested { difficulty }` | spawn foes, roll initiative, build turn order, → Encounter |
| Attack / Cast | `CombatActionRequest { actor, target, Attack }` | build the attack roll, resolve, apply damage on completion |
| Surrender | `SurrenderRequested { actor }` | end encounter to the narrative/Exploration branch |
| Use a potion | `ConsumableUseRequested { actor, item }` | apply the consumable |
| Buy / sell | `ShopTransactionRequested { item, … }` | move gold + inventory |
| Revive a downed PC | `ReviveAttempt { entity }` | pay the cost, clear `Downed` |

The **one** combat job the UI owns is **turn advancement**: core never moves
`ActiveTurn`, so `hud::advance_turn_after_action` moves it to the next living
combatant once an action resolves. `hud::drive_enemy_turns` fires an enemy's
`CombatActionRequest` on its turn. Equip/unequip mutate `Equipment` + fire
`EquipmentChanged`/`InventoryChanged`; shop stock is rolled into core's
`ItemInstances`.

## Dice Theater handshake (must never hang combat)

```
(anyone) RollRequest ─▶ core resolves (difficulty-aware) ─▶ RollResolved { id, total, is_nat20, is_nat1 }
                                                                   │
                              dice::enqueue_resolved pushes one animation job per event
                                                                   │
                              dice::run_theater spawns a d20 at world-centre, tumbles it,
                              and LANDS on `total` (never recomputed); plays nat-20 / nat-1
                              flourishes from the event's flags
                                                                   │
                              at the end of every job it ALWAYS fires:
                              RollAnimationComplete { id }   ── exactly once, same id
                                                                   │
                              core applies the consequences (damage / death / encounter end)
```

Guarantees:

* **Never authoritative** — the die animates toward whatever core decided,
  including Easy-mode skew and Codex's "force nat-20/nat-1" debug option (we only
  read `total` / `is_nat20` / `is_nat1`).
* **No desync under rapid rolls** — jobs are a FIFO queue, animated one at a
  time; each `RollResolved.id` produces exactly one `RollAnimationComplete.id`.
  Character-creation fires six ability rolls at once: they queue and complete in
  order.
* **Cannot hang** — `run_theater` advances on `Time` every `Update` frame
  regardless of rendering or game state, so completion always fires. The die sits
  at world-centre, which the encounter layout leaves uncovered.

Rendering is hand-rolled `bevy` sprites + `Text2d` + a sprite-particle fallback
(no version-sensitive particle/tweening dependency).

## Debug overlay (F1)

`debug::debug_overlay_ui` shows the current state, the last roll core resolved
(raw dice + final skewed total + nat flags), the active combatant, and
party/enemy resources + gold. Read-only; it never drives the game.

## Notes for the integrator

* **Camera** is spawned by the UI only if none exists (render is still a stub).
* **Item icons**: the inventory shows rarity-framed name tiles; real 32×32 item
  sprites belong to `starwood_render` (swap to `egui::Image` later, no structural
  change).
* Core clears the encounter enemy list but does not despawn the enemy entities
  (its death handler skips non-party units), so `hud::despawn_stale_enemies`
  tidies the stage on entering Exploration. Noted in `NEEDS_FROM_CORE.md`.
