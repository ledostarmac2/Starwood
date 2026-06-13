# starwood_ui

The UI layer for **Starwood**: every egui screen plus the **Dice Theater**. This
crate depends only on `starwood_core` and obeys the Shared Contract (Section 4 of
the build blueprint). It never edits `starwood_core` or `starwood_render`.

Add it with `StarwoodUiPlugin` (the binary already does this).

## What's in here

| Module | Responsibility |
|---|---|
| `theme` | Fantasy egui theme: dark gold-and-parchment palette, framed panels, optional display font from `assets/fonts/`. |
| `menu` | Main menu (New Game / Continue / Quit) and the Game Over screen. |
| `creation` | Animated character creation across the `CreationStep` sub-states. |
| `hud` | Persistent party panel, the Exploration map, the encounter turn order, the action bar, and the small flow systems that keep combat moving. |
| `inventory` | Inventory overlay (equip/unequip), character sheet, and skills tab. |
| `dice` | The Dice Theater. |

### Schedules

egui draws in **`EguiPrimaryContextPass`** (required by bevy_egui 0.39's
multipass primary context). All world/sprite/animation/flow logic runs in
**`Update`**. The egui systems are chained so the theme is applied first and the
inventory overlay draws last; because they all borrow the egui context mutably
they are serialised anyway.

## The Dice Theater (event flow)

The theater is the marquee feature and the one place getting the contract exactly
right matters most:

```
(anyone) RollRequest ─▶ core resolves ─▶ RollResolved { id, total, is_nat20, is_nat1 }
                                                  │
                       dice::enqueue_resolved pushes an animation job
                                                  │
                       dice::run_theater spawns a d20 sprite, tumbles it,
                       and LANDS it on `total` (never recomputed here)
                                                  │
                       on is_nat20 → golden particle burst + warm flash
                       on is_nat1  → red shards + screen dim + camera shake
                                                  │
                       when the animation + flourish finish:
                       RollAnimationComplete { id }  ── fired once, same id
                                                  │
                       core applies the roll's consequences (damage, death, …)
```

We are **never authoritative**: `core` decides the result; we animate toward it.
The nat-20 / nat-1 effects are cosmetic reactions to the event's flags.

Rendering uses plain `bevy` sprites + `Text2d` and a sprite-based particle
fallback (the blueprint's permitted alternative to `bevy_hanabi`), so the theater
carries no version-sensitive particle/tweening dependency.

The theater die is drawn at world-centre. It is fully visible during encounters
(the centre of the screen is intentionally left uncovered there). During
character-creation ability rolls the same roll → resolve → complete flow runs and
the generated scores appear as chips; the centred die may sit behind the creation
panel.

## How combat is wired (without touching core)

`core` resolves rolls and, given a `PendingRolls` entry, applies damage after
`RollAnimationComplete`. The UI supplies the glue using only public APIs:

1. The action bar fires `RollRequest`(Attack) and records the in-flight attack.
2. `hud::register_pending_attacks` reads the authoritative `RollResolved` and
   writes a `PendingAttack` into `core`'s `PendingRolls`.
3. The Dice Theater animates and fires `RollAnimationComplete`.
4. `core` applies the damage; `hud::advance_turn_on_complete` moves the turn on.

Initiative works the same way: `EncounterStarted` ⇒ one `RollRequest`(Initiative)
per combatant ⇒ `hud::collect_initiative_rolls` builds the turn order from the
results via `core::build_turn_order`.

## Notes for the integrator

* **Camera**: the blueprint assigns the 2D camera to `starwood_render`, which is
  still a stub. `setup_ui_camera` spawns one only if none exists, so egui and the
  dice sprites render today; once `render` spawns its own camera this becomes a
  no-op.
* **Item icons**: real 32×32 item sprites belong to `starwood_render`. Until it
  exposes textures, the inventory shows lettered tiles. Swapping to `egui::Image`
  later needs no structural change.
* Requests for core/contract changes are in `NEEDS_FROM_CORE.md`. No new shared
  workspace dependencies were needed (`WORKSPACE_DEPS_TODO.md` is unchanged).
