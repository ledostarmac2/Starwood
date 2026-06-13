# Starwood

Starwood is a procedurally generated, text-driven tactical RPG foundation built
in Rust with Bevy 0.18.

## Workspace

- `crates/starwood`: binary app wiring Bevy, egui, core, render, and UI.
- `crates/starwood_core`: shared contract, domain logic, data loading, dice,
  rules, procedural generation, combat resources, and save/load DTOs.
- `crates/starwood_render`: empty `StarwoodRenderPlugin` stub for the render
  agent.
- `crates/starwood_ui`: empty `StarwoodUiPlugin` stub for the UI agent.
- `assets/data`: RON catalogs for races, classes, skills, traits, items, and
  enemies.

## Run

Install Rust, then run:

```powershell
cargo run -p starwood
```

The current milestone launches a black Bevy window with core systems and plugin
stubs registered. Rendering and UI crates are intentionally empty so other
agents can fill them without touching core files.

## Seeds

`StarwoodCorePlugin` defaults to seed `0x57A2_C0DE`. The binary reads
`STARWOOD_SEED` at launch:

```powershell
$env:STARWOOD_SEED=12345
cargo run -p starwood
```

Map generation, dice, encounter picks, and loot helpers use `ChaCha8Rng` so the
same seed produces repeatable runs.

## Dice Event Flow

Core is authoritative for dice:

1. UI, render, or gameplay emits `RollRequest`.
2. `starwood_core` consumes it, rolls with `GameRng`, and emits `RollResolved`.
3. The UI Dice Theater animates toward that already decided result.
4. UI emits `RollAnimationComplete` with the same id.
5. Core systems may then apply consequences that depend on the roll.

The helper `PendingRolls` resource is the first correlation map for consequences
that must wait for dice animation completion.

Bevy 0.18 calls these app events "messages"; the contract structs retain their
blueprint names and are registered with `add_message`.

## Game Flow

Core registers additive request messages for app flow:

- `NewGameRequested { seed }` resets run resources, seeds RNG, generates a map,
  and enters `CharacterCreation`.
- `CreationStepAdvanceRequested` advances the `CreationStep` sub-state.
- `CharacterBuildRequested` validates race/class ids, spawns a party entity with
  the shared contract components, updates `PartyRoster` / `Inventory`, and fires
  `CharacterFinalized`, `EquipmentChanged`, and `InventoryChanged`.
- `FinishPartyCreationRequested` enters `Exploration` once the party is non-empty.
- `EncounterRequested { difficulty }` picks 1-5 enemies, spawns enemy entities,
  builds a turn order, sets `ActiveTurn`, fires `EncounterStarted`, and enters
  `Encounter`.

## Data

Data is loaded from `assets/data/*.ron` at startup into `GameData`. The render
and UI crates should resolve ids through `GameData`; sprite keys are already
present on races, items, and enemy archetypes for placeholder-art generation.
