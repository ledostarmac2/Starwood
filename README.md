# Starwood

Starwood is a procedurally generated, text-driven tactical RPG foundation built
in Rust with Bevy 0.18.

## Workspace

- `crates/starwood`: binary app wiring Bevy, egui, core, render, and UI.
- `crates/starwood_core`: shared contract, domain logic, data loading, dice,
  rules, procedural generation, combat resources, and save/load DTOs.
- `crates/starwood_render`: Bevy 2D scene, generated placeholder sprites,
  rank layout, paper-doll composition, and item-icon/rarity frames.
- `crates/starwood_ui`: egui menus, character creation, HUD, inventory/shop,
  Dice Theater, save-slot UI, and debug overlay.
- `assets/data`: RON catalogs for races, classes, skills, traits, items, and
  enemies.

## Run

Install Rust, then run:

```powershell
cargo run -p starwood
```

On Windows without Visual Studio admin rights, use the local MinGW setup:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup-mingw-build.ps1
$env:PATH = "$env:USERPROFILE\starwood-toolchain\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu run -p starwood
```

The current vertical slice launches to a real menu, creates one PC plus planned
companions, explores a seeded node map, enters rank-based encounters, resolves
dice through the Dice Theater, awards loot/shop items as rolled instances, and
autosaves on return to exploration.

## Seeds

`StarwoodCorePlugin` defaults to seed `0x57A2_C0DE`. The binary reads
`STARWOOD_SEED` at launch:

```powershell
$env:STARWOOD_SEED=12345
cargo run -p starwood
```

The CLI also accepts `--seed 12345`, which takes precedence for that launch.
Map generation, dice, encounter picks, and loot helpers use `ChaCha8Rng` so the
same seed produces repeatable runs.

## Debug Harness

The binary owns an integration harness for direct system testing:

```powershell
cargo run -p starwood -- --debug-encounter --seed 123 --enemy goblin_cutpurse
cargo run -p starwood -- --force-roll nat20
cargo run -p starwood -- --spawn-item iron_sword:legendary
cargo run -p starwood -- --difficulty easy
cargo run -p starwood -- --headless-smoke --seed 123
```

- `--debug-encounter` skips creation and spawns a premade party into a scripted
  encounter. In the windowed app, the UI Dice Theater still completes rolls.
- `--headless-smoke` runs the same encounter without a window and exits after
  victory; CI/test automation uses this path.
- `--force-roll nat20|nat1|N` forces the next authoritative core roll.
- `--spawn-item base:rarity` rolls that item instance into inventory.
- `--difficulty easy|normal|hard` overrides the starting difficulty.

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

## Verification

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
```
