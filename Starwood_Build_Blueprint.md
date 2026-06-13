# STARWOOD — Build Blueprint & Agent Prompts

A procedurally-generated, text-driven tactical RPG (D&D-style) built **entirely in Rust**, with polished 2D pixel-art visuals.

This document has two parts:

- **Part A — The Blueprint** (Sections 1–5): tech decisions, architecture, and the *Shared Contract* every agent must obey.
- **Part B — The Three Prompts** (Sections 6–8): one heavy prompt for **Codex**, two lighter ones for **Cursor** and **Claude Code**.

> **How to use this:** Give each agent **Sections 1–5 (the blueprint + contract)** *plus* that agent's specific prompt. The contract is the glue — all three must implement the exact same type and event signatures so their work snaps together with no merge conflicts.

---

# PART A — THE BLUEPRINT

## 1. Tech decisions (and why)

| Concern | Choice | Why |
|---|---|---|
| **Engine** | **Bevy 0.18** (latest stable; 0.19 is still RC) | The de-facto Rust game engine. Its ECS is *ideal* for managing a party (≤4), enemies (≤5), inventory items, and dice as entities. Best-documented, largest ecosystem. Simulation-heavy/roguelike games are its sweet spot. |
| **Art style** | **Modern pixel art** — crisp, limited-palette sprites with modern lighting/glow + smooth tweened motion (think *Sea of Stars / Moonlighter / Eastward*) | Asset-efficient, reads as "polished," and — critically — **modular**: a paper-doll layering system (swappable armor/weapons on a base body) is a solved problem in pixel art. Procedural composition is natural. |
| **UI / panels** | **`bevy_egui`**, heavily themed (custom font + fantasy palette + framed panels) | Fastest path to data-dense panels (inventory grid, skills tab, character sheet, tooltips). Themed so it does **not** look like a debug tool. |
| **World sprites** | Bevy native 2D sprites + texture atlases | Characters, enemies, items, dice. |
| **Tweening** | `bevy_tweening` | Idle "breathing" bob, attack lunge, dice toss, screen transitions. Lets us ship polish *without* frame-by-frame animation art at first. |
| **Particles** | `bevy_hanabi` (GPU particles) with a **sprite-particle fallback** | For the nat-20 golden burst / nat-1 ominous smoke. Hanabi can be version-sensitive — fallback ensures the marquee dice effects ship regardless. |
| **RNG** | `rand` + `rand_chacha` (seeded) | Reproducible procedural generation and dice. |
| **Data + saves** | `serde` + `ron` | Game data (races/classes/items/enemies) authored as `.ron` files; save/load via serde. |

**Pixel-art spec:** characters on a **64×64** base canvas with fixed anchor *slots* (head, body, main-hand, off-hand, etc.) so equipment layers align; items at **32×32**; one cohesive limited palette (dark-/high-fantasy). **Placeholder sprites are generated programmatically** for every id so the game is fully playable before any real art exists.

> **Version discipline (read this — it's the #1 cause of Bevy build failures):** Pin **Bevy 0.18.x**. For *every* `bevy_*` ecosystem crate (`bevy_egui`, `bevy_tweening`, `bevy_hanabi`), select the release tagged compatible with **Bevy 0.18** and pin exact versions. Versions are centralized in `[workspace.dependencies]` so all crates stay aligned. If a crate has no 0.18-compatible release, substitute an alternative or vendor the behavior — do **not** mix Bevy minor versions.

## 2. Workspace architecture (4 crates, zero overlap)

```
starwood/
├── Cargo.toml                  # [workspace] + [workspace.dependencies] (pins Bevy 0.18.x)
├── assets/
│   ├── data/                   # *.ron game data — owned by Codex
│   ├── sprites/                # pixel art + generated placeholders — owned by Cursor
│   └── fonts/                  # fantasy UI font — owned by Claude Code
└── crates/
    ├── starwood/               # BINARY (main.rs). App setup, adds all plugins.   [CODEX]
    ├── starwood_core/          # LIB. Shared contract + ALL domain logic.          [CODEX]
    ├── starwood_render/        # LIB. World sprites + paper-doll. StarwoodRenderPlugin. [CURSOR]
    └── starwood_ui/            # LIB. egui screens/panels + Dice Theater. StarwoodUiPlugin. [CLAUDE CODE]
```

**Dependency direction:** `render` and `ui` each depend on `core`. They **never** depend on each other. The binary depends on all three and wires them together.

**Why this guarantees no merge conflicts:** each agent owns a *separate crate*. Codex creates `render` and `ui` as **empty plugin stubs** first (so the binary compiles on day one), then B and C fill in *only their own crate*. Nobody edits anyone else's files. All communication happens through the Shared Contract below.

## 3. Game flow (state machine)

```
MainMenu
  └─> CharacterCreation  (sub-steps: Race → Class → AbilityRoll → SkillsTraits → Review → repeat per party member, ≤4)
        └─> Exploration  (navigate a procedurally generated node map)
              ├─> Encounter (turn-based combat vs ≤5 enemies; initiative, actions, dice)
              ├─> Inventory (overlay)
              └─> back to Exploration … until boss / defeat
```

Persistent across Exploration/Encounter: the **party HUD** (≤4 member models + HP + equipped gear icons) and the **enemy stage** (blank during exploration; up to 5 enemy models during an encounter).

## 4. THE SHARED CONTRACT  ⚠️ all three agents implement these *exact* signatures

`starwood_core` defines everything below. **Codex implements it verbatim; Cursor and Claude Code import and obey it.** If an agent needs a new field/event, it must **not** invent it silently — it adds a note to `NEEDS_FROM_CORE.md` and the human reconciles it (same idea as the `new_dependencies.txt` trick from ReplyRight).

```rust
// ===== STATES =====
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default] MainMenu,
    CharacterCreation,
    Exploration,
    Encounter,
    GameOver,
}

#[derive(SubStates, Default, Debug, Clone, PartialEq, Eq, Hash)]
#[source(GameState = GameState::CharacterCreation)]
pub enum CreationStep {
    #[default] Race, Class, AbilityRoll, SkillsTraits, Review,
}

// Inventory is an overlay toggle, tracked as a resource flag (not a hard state) so the
// world keeps rendering underneath. UI reads/writes `InventoryOpen(bool)`.
#[derive(Resource, Default)] pub struct InventoryOpen(pub bool);

// ===== IDENTITY / DATA KEYS =====  (string ids resolved against GameData tables)
pub type RaceId = String; pub type ClassId = String; pub type SkillId = String;
pub type TraitId = String; pub type ItemId = String; pub type EnemyArchetypeId = String;

// ===== CORE COMPONENTS =====
#[derive(Component, Clone)] pub struct Character { pub name: String, pub race: RaceId, pub class: ClassId, pub level: u32, pub xp: u32 }
#[derive(Component, Clone, Copy)] pub struct Abilities { pub str_: u8, pub dex: u8, pub con: u8, pub int: u8, pub wis: u8, pub cha: u8 }
#[derive(Component, Clone, Copy)] pub struct Derived { pub armor_class: i32, pub max_hp: i32, pub initiative_mod: i32, pub proficiency: i32, pub speed: i32 }
#[derive(Component, Clone, Copy)] pub struct Health { pub current: i32, pub max: i32 }
#[derive(Component, Clone)] pub struct SkillSet { pub proficient: Vec<SkillId> }
#[derive(Component, Clone)] pub struct Traits(pub Vec<TraitId>);

#[derive(Component, Clone, Copy)] pub struct PartyMember { pub slot: u8 }   // slot 0..=3
#[derive(Component, Clone)]       pub struct EnemyUnit { pub archetype: EnemyArchetypeId, pub slot: u8 } // slot 0..=4

// Equipment slots -> equipped item (None = empty). Drives the paper-doll.
#[derive(Component, Clone, Default)]
pub struct Equipment { pub head: Option<ItemId>, pub body: Option<ItemId>, pub main_hand: Option<ItemId>, pub off_hand: Option<ItemId>, pub feet: Option<ItemId> }

// Tells RENDER which sprite layers to draw. Render owns interpretation; core sets ids.
#[derive(Component, Clone)] pub struct SpriteParts { pub base_body: String }

// Combat markers
#[derive(Component)] pub struct ActiveTurn;          // entity whose turn it is
#[derive(Component, Clone, Copy)] pub struct Initiative(pub i32);

// ===== RESOURCES =====
#[derive(Resource)] pub struct GameRng(pub rand_chacha::ChaCha8Rng);   // seeded
#[derive(Resource, Default)] pub struct GameData { /* loaded RON: races, classes, skills, traits, items, enemy archetypes */ }
#[derive(Resource, Default)] pub struct PartyRoster { pub members: Vec<Entity> }      // ≤4
#[derive(Resource, Default)] pub struct MapState { /* node graph of the run */ }
#[derive(Resource, Default)] pub struct EncounterState { pub enemies: Vec<Entity>, pub turn_order: Vec<Entity>, pub turn_index: usize }
#[derive(Resource, Default)] pub struct AssetHandles { /* render & ui populate their own handle maps */ }

// ===== DICE (domain, in core) =====
#[derive(Clone, Copy, Debug)] pub enum AdvState { Normal, Advantage, Disadvantage }
#[derive(Clone, Debug)] pub struct DiceExpr { pub count: u32, pub sides: u32, pub modifier: i32 } // e.g. 2d6+3
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RollKind { Initiative, Attack, Damage, AbilityCheck, SavingThrow, AbilityScoreGen, Generic }

// ===== EVENTS (the cross-crate API) =====
// Anyone can REQUEST a roll. Core is the SOLE authority that resolves it.
#[derive(Event)] pub struct RollRequest { pub id: u64, pub expr: DiceExpr, pub kind: RollKind, pub source: Option<Entity>, pub advantage: AdvState }

// Core fires this with the AUTHORITATIVE result. The UI Dice Theater listens & animates toward it.
#[derive(Event)] pub struct RollResolved { pub id: u64, pub rolls: Vec<u32>, pub total: i32, pub is_nat20: bool, pub is_nat1: bool, pub kind: RollKind }

// UI fires this when the dice ANIMATION finishes. Gameplay logic that depends on a roll
// must WAIT for this before applying consequences (so visuals and logic stay in sync).
#[derive(Event)] pub struct RollAnimationComplete { pub id: u64 }

// Paper-doll re-composition trigger. Render listens.
#[derive(Event)] pub struct EquipmentChanged { pub entity: Entity }

// Combat / encounter lifecycle. Render spawns/despawns enemy models; UI updates HUD.
#[derive(Event)] pub struct EncounterStarted { pub enemies: Vec<Entity> }
#[derive(Event)] pub struct EncounterEnded   { pub victory: bool }
#[derive(Event)] pub struct DamageDealt { pub target: Entity, pub amount: i32, pub is_crit: bool }
#[derive(Event)] pub struct UnitDied    { pub entity: Entity }

// Character creation lifecycle. UI drives steps; core finalizes the entity.
#[derive(Event)] pub struct CharacterFinalized { pub entity: Entity }
#[derive(Event)] pub struct InventoryChanged;
```

**Golden rule for dice (prevents two agents duplicating logic):**
`core` computes the *result* and is authoritative — it never fudges based on animation. It fires `RollResolved`. The `ui` Dice Theater animates the dice *toward the already-decided value*; the nat-20 / nat-1 effects are purely cosmetic reactions to the `is_nat20`/`is_nat1` flags. When the animation ends, `ui` fires `RollAnimationComplete`, and `core`'s gameplay systems then apply consequences. Logic lives in core; theater lives in ui; they meet only at these three events.

## 5. Integration & parallelism workflow

This mirrors the ReplyRight split (zero file overlap, parallel builds, deferred wiring):

1. **Codex goes first on scaffolding** (~the first thing it does): creates the workspace, `[workspace.dependencies]` pinning Bevy 0.18.x, all four crates, and **empty plugin stubs** `StarwoodRenderPlugin` / `StarwoodUiPlugin` that do nothing yet. It implements the **entire Shared Contract** in `starwood_core`. → **The binary now compiles and runs (black screen) on day one.** Cursor and Claude Code can start immediately against the contract.
2. **All three build in parallel.** Codex fills in domain logic + data + binary. Cursor fills in `starwood_render` only. Claude Code fills in `starwood_ui` only. **No shared files.**
3. **New dependencies:** each agent adds crates to *its own* `Cargo.toml`, but Bevy + `bevy_*` versions come from `[workspace.dependencies]` (Codex owns that). If an agent needs a new shared workspace dep, it writes it to `WORKSPACE_DEPS_TODO.md` instead of editing the root — human merges (30-second manual step, prevents a guaranteed conflict).
4. **Contract changes:** if Cursor or Claude Code needs a new event/field/component, it appends to `NEEDS_FROM_CORE.md` and keeps working with a local placeholder. Human reconciles into `starwood_core` after.
5. **Merge** all three branches (separate crates ⇒ trivial). Then a small **follow-up wiring pass** connects cross-cutting flows (e.g., the UI action bar actually driving `core`'s combat resolution) — but because everything talks through the event contract, this is mostly already done.

**Definition of done for the first milestone:** create a party of up to 4 with the animated creation flow, walk the procedural map, enter an encounter with up to 5 enemies, take turns with the dice theater firing on initiative + attacks (nat-1/nat-20 effects visible), open inventory and swap a weapon/armor (paper-doll updates), and win/lose the encounter — all on placeholder art.

---

# PART B — THE THREE PROMPTS

> Reminder: paste **Sections 1–5 above** together with the relevant prompt below into each agent.

---

## 6. PROMPT FOR CODEX  *(the heavy one — engine, contract, all game logic, binary)*

```
You are building STARWOOD, a procedurally-generated tactical RPG (D&D-style) in Rust with the Bevy 0.18 engine. You own the foundation that two other agents (building rendering and UI in separate crates) will plug into. Adhere EXACTLY to the Shared Contract in Section 4 of the blueprint — those type and event signatures are law; other agents are coding against them in parallel right now.

=== SCOPE: you own `crates/starwood` (binary), `crates/starwood_core` (lib), and `assets/data/`. ===

STEP 1 — Scaffold the workspace so the project compiles and runs on day one:
- Create a Cargo workspace with members: starwood (bin), starwood_core, starwood_render, starwood_ui.
- In the root Cargo.toml `[workspace.dependencies]`, pin EXACT compatible versions: bevy = "0.18.x", and add (also pinned to their Bevy-0.18-compatible releases) bevy_egui, bevy_tweening, bevy_hanabi, plus rand, rand_chacha, serde, ron. This is the single source of version truth — verify each bevy_* crate actually has a 0.18-compatible release before pinning; do not mix Bevy minor versions.
- Create starwood_render and starwood_ui as EMPTY STUB CRATES that each expose a public plugin (`StarwoodRenderPlugin`, `StarwoodUiPlugin`) which currently does nothing. (The other agents will fill these in; you must not touch their internals beyond this stub.)
- In the binary, build the Bevy App: DefaultPlugins, EguiPlugin, your CorePlugin, plus StarwoodRenderPlugin and StarwoodUiPlugin. The app must compile and launch to a black window.

STEP 2 — Implement the ENTIRE Shared Contract in starwood_core (states, sub-states, components, resources, dice types, and ALL events) exactly as specified. Register all states and events on the app. Re-export everything from the crate root.

STEP 3 — Domain logic (pure, heavily unit-tested — aim for the rigor of a 150+ test suite):
- DICE ENGINE: parse "NdM+K" into DiceExpr; roll with the seeded GameRng; implement Advantage/Disadvantage (roll two d20, keep higher/lower); detect nat-20/nat-1 on any single d20. This is the ONLY place dice are resolved. Add a system that consumes RollRequest, computes the result, and fires the authoritative RollResolved (with is_nat20/is_nat1).
- RULES ENGINE (5e-like): ability modifier = floor((score-10)/2); proficiency bonus by level; armor class; initiative; attack roll vs AC; damage; saving throws; skill checks (d20 + ability mod + proficiency if proficient). Unit-test the math thoroughly, including edge cases.
- CHARGEN: apply race + class to produce a Character with Abilities/Derived/Health/SkillSet/Traits. Support three ability-score methods: 4d6-drop-lowest, standard array, and point-buy. Compute all Derived stats. XP/leveling.
- PROCEDURAL GENERATION (all seeded & reproducible from a seed): a node-graph run map (node types: Combat, Elite, Treasure, Event, Rest, Boss) with branching paths; encounter composition that picks 1–5 enemy archetypes scaled to party level/difficulty; enemy stat-block generation; loot/item generation.
- COMBAT FLOW: roll initiative for all combatants, build turn order in EncounterState, mark ActiveTurn, resolve actions (attack/cast/use-item/flee), apply damage (firing DamageDealt with is_crit), detect deaths (UnitDied), and detect victory/defeat (EncounterEnded). CRUCIAL: any consequence that depends on a roll must be gated on RollAnimationComplete for that roll id — do not apply damage until the UI signals the dice animation finished. Use an event/id correlation map.
- SAVE/LOAD: serialize/deserialize the full run (party, map, inventory, seed) via serde + ron.

STEP 4 — GAME DATA as RON files in assets/data/ (and a loader into GameData): at least 6 races and 6 classes (each with descriptions, ability modifiers, starting kit, and class abilities), a skill list, a trait list, an item catalog (weapons/armor with slot, stats, and a sprite-key field for the render crate), and 8+ enemy archetypes (with stat blocks and a sprite-key). Design the schema so the other crates can resolve ids to data.

STEP 5 — Wire the state machine and the systems that own game flow (menu → creation → exploration → encounter → game over), the InventoryOpen toggle resource, the PartyRoster (max 4), and EncounterState (max 5 enemies). Spawn party/enemy ENTITIES with the contract components (Character, Abilities, Derived, Health, Equipment, SpriteParts, PartyMember/EnemyUnit) — but do NOT render them (that's the render crate). Fire EncounterStarted/Ended, EquipmentChanged, CharacterFinalized, InventoryChanged at the right moments so the other crates can react.

Rules: keep `starwood_core` free of egui and rendering concerns — it is logic + data only. Freeze the contract's public API; if you must extend it, document the change clearly in a CONTRACT_CHANGELOG.md so the other agents can sync. Write a README documenting how to run, the seed system, and the event flow. Run the full test suite and report the final test count.
```

---

## 7. PROMPT FOR CURSOR  *(lighter — world sprites + paper-doll + placeholder art)*

```
You are building the VISUAL WORLD layer for STARWOOD, a Rust/Bevy 0.18 tactical RPG. You own ONE crate: `crates/starwood_render`, plus the `assets/sprites/` folder. Another agent owns the game logic in `starwood_core`; a third owns UI/menus in `starwood_ui`. You depend on `starwood_core` and import its Shared Contract (Section 4 of the blueprint) — implement the `StarwoodRenderPlugin` and obey those exact types/events. Do NOT edit any other crate. The art style is MODERN PIXEL ART (crisp, limited cohesive palette, modern lighting/glow); characters on a 64×64 base canvas with fixed anchor slots, items at 32×32.

Implement `StarwoodRenderPlugin` to do the following:

1. SCENE SETUP: a 2D camera and a layered scene with clear regions: a persistent PARTY area (left/bottom) for up to 4 member models, and an ENEMY STAGE (right/top) that is blank during Exploration and holds up to 5 enemy models during an Encounter.

2. PAPER-DOLL COMPOSITION (the core feature): for any entity with `SpriteParts` + `Equipment`, draw the base body, then layer equipped items (head, body, main_hand, off_hand, feet) in correct z-order at the matching 64×64 anchor slots. Resolve each ItemId to a sprite via the item's sprite-key (from GameData). React to the `EquipmentChanged` event by re-composing that entity's layers immediately. PARTY members have fully swappable equipment; ENEMIES use a single generic weapon/armor set baked into their archetype sprite (no swapping needed).

3. SPAWN/DESPAWN: on `EncounterStarted`, spawn enemy models into the 5 stage slots (positioned by `EnemyUnit.slot`); on `EncounterEnded`, clear the stage. Position party models by `PartyMember.slot`.

4. ANIMATION via bevy_tweening (no frame-by-frame art needed yet): a subtle idle "breathing" bob on all models; an attack lunge (translate toward target and back) when a unit acts; a red hurt-flash + small shake on `DamageDealt`; a fade/dissolve on `UnitDied`. Keep it tasteful and readable.

5. ITEM ICON RENDERING: expose a way to render any ItemId as its 2D 32×32 sprite, so the UI crate can show item icons in the inventory grid and on HUD equipment slots. Coordinate via the AssetHandles resource / a public component the UI can attach. (Do NOT build the inventory UI itself — only the item sprites.)

6. PLACEHOLDER ASSET GENERATION (do this EARLY so the game is playable before real art exists): programmatically generate simple, distinct, palette-consistent placeholder pixel sprites for every race base body, every equipment item, and every enemy archetype, keyed by their sprite-keys from GameData. A colored silhouette + simple shape per id is fine. Build a sprite manifest mapping keys → handles. Real art can drop in later by replacing files without code changes.

IMPORTANT BOUNDARIES: You render the WORLD — characters, enemies, equipment layers, item sprites, and the dice/effect sprites are NOT yours (the UI crate owns the Dice Theater). You do not own any egui/menu/panel code. If you need a new field or event from core, append it to NEEDS_FROM_CORE.md and keep working with a local placeholder — do not modify starwood_core. If you need a new shared workspace dependency, add it to WORKSPACE_DEPS_TODO.md rather than editing the root Cargo.toml. Document how the paper-doll layering and placeholder system work.
```

---

## 8. PROMPT FOR CLAUDE CODE  *(lighter — egui screens/panels + the Dice Theater)*

```
You are building the UI and the DICE THEATER for STARWOOD, a Rust/Bevy 0.18 tactical RPG. You own ONE crate: `crates/starwood_ui`, plus the `assets/fonts/` folder. Another agent owns game logic in `starwood_core`; a third owns world sprites in `starwood_render`. You depend on `starwood_core` and import its Shared Contract (Section 4 of the blueprint) — implement `StarwoodUiPlugin` and obey those exact types/events. Use bevy_egui for panels and Bevy sprites + particles for the dice. Do NOT edit any other crate.

FIRST: theme egui so it looks like a polished fantasy game, NOT a debug tool — load a fantasy display font from assets/fonts/, set a dark cohesive palette, framed/parchment-style panels, and consistent spacing. Everything below should sit on this theme.

Implement `StarwoodUiPlugin`:

1. MAIN MENU: New Game / Continue / Quit, styled.

2. ANIMATED CHARACTER CREATION (driven by the CreationStep sub-state; repeat for each party member up to 4):
   - Race step: scrollable list of races, each showing description + ability modifiers (from GameData), with the race's model previewed (coordinate with the render crate to show the base body). Selecting advances the step.
   - Class step: same pattern — description + class abilities + model preview.
   - Ability roll step: a button that fires RollRequest(kind = AbilityScoreGen) for the 4d6-drop-lowest rolls; play the Dice Theater; let the player assign the resulting scores to STR/DEX/CON/INT/WIS/CHA. Also offer standard-array and point-buy modes.
   - Skills/Traits step: checkboxes for skills and traits within the allowed budget, each with a description tooltip.
   - Review step: full summary; confirming spawns/finalizes the member and fires the appropriate completion so core adds them to the party. Use smooth transitions between steps (bevy_tweening) so the flow feels animated, not snappy.

3. PERSISTENT HUD (during Exploration + Encounter): a party panel showing each of the ≤4 members — name, HP bar (from Health), level, and small equipped weapon/armor icons (use the render crate's item-sprite rendering). During an Encounter, also show the turn order and whose turn it is (ActiveTurn), and an ACTION BAR (Attack / Cast / Use Item / Flee) whose buttons fire the appropriate RollRequest / action events to core.

4. INVENTORY OVERLAY (toggled by the InventoryOpen resource): a grid of item icons (icons rendered by the render crate), with tooltips showing item descriptions/stats, and equip/unequip actions that fire EquipmentChanged. React to InventoryChanged.

5. SKILLS TAB: list the character's skills with proficiency, computed modifier, and descriptions. Plus a full CHARACTER SHEET (abilities, derived stats, traits, equipped gear).

6. THE DICE THEATER (the marquee feature — make it feel great):
   - Listen for RollResolved. Play a 2D dice-toss animation (a d20 sprite tumbling via tween / texture-atlas) that LANDS on the already-decided value from the event. Never compute the result yourself — core is authoritative; you only animate toward result.total / result.rolls.
   - If is_nat20: a triumphant moment — golden particle burst, glow/bloom on the die, a brief celebratory flourish (and a sound hook). Make it unmistakably significant.
   - If is_nat1: an ominous moment — red crack/shatter, screen dim, a small shake (and a sound hook). Equally unmistakable, but negative.
   - When the animation (and any crit flourish) finishes, fire RollAnimationComplete with the same id — core waits on this before applying consequences, so getting this right is essential.
   - Use bevy_hanabi for particles if a 0.18-compatible version is available; otherwise implement a simple sprite-based particle fallback so the effect still ships.

IMPORTANT BOUNDARIES: You own UI/menus/HUD and the dice + dice-effect sprites ONLY. Character/enemy/item world sprites and the paper-doll belong to the render crate — request item icons from it, don't draw bodies/equipment yourself. Logic and dice results belong to core — only ever REQUEST rolls and animate the RESULT. If you need a new field/event from core, append it to NEEDS_FROM_CORE.md and keep working with a local placeholder; do not modify starwood_core. New shared workspace deps go in WORKSPACE_DEPS_TODO.md, not the root Cargo.toml. Document the Dice Theater's event flow (RollResolved → animate → RollAnimationComplete).
```

---

### Suggested kickoff order
1. Run **Codex** first far enough to land Step 1–2 (workspace + contract compiling). 
2. Then fire **Cursor** and **Claude Code** in parallel against the frozen contract while Codex continues Steps 3–5.
3. Merge all three crates; do the short wiring follow-up; then iterate on art, balance, and polish.
