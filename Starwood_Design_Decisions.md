# STARWOOD — Design Decisions & Agent Adjustments

Companion document to `Starwood_Build_Blueprint.md`. This locks the game-design direction and translates it into concrete changes for each agent. **Where this conflicts with the original blueprint, this wins.**

> **Timing note:** Codex, Cursor, and Claude Code are mid-build. The items marked **[CONTRACT]** change `starwood_core`'s public API that the other two are coding against — Codex must fold these in and log them in `CONTRACT_CHANGELOG.md` ASAP so render/UI stay aligned. The biggest ripple is the **item model** (see §3).

---

## 1. Locked design decisions

**Structure & progression**
- **Persistent, saveable campaign** (not roguelite). Long-form. Progress is **isolated per campaign** — no cross-campaign meta-progression.
- **Permadeath for companions; the main PC can be revived** (downed + revive) but at great cost. Companion death is permanent.
- **Difficulty select.** Easy = skewed RNG, ~25% in the player's favor. Hard = true, honest 1/20 odds. Normal = true odds, standard tuning. (Enemy-tuning-per-difficulty: see open Q1.)
- **No New Game+.** After the main boss, a short **optional bonus quest** as post-game.
- **Level cap 100**, but the main story is balanced for roughly **level ~35**. Players can keep grinding and **delay finishing the main quest** — so the world needs renewable/optional content and enemy scaling past the critical path.

**Combat**
- **Lane / formation model** (Darkest-Dungeon-style ranks), not a tactical grid.
- **Strict initiative** — all combatants interleaved in initiative order (not side-based).
- **D&D action economy**: action + bonus action + move. "Move" = **swapping ranks/positions** in the lane.
- **Reach & range matter via rank**: melee hits front ranks, ranged reaches back ranks; enemies likewise.
- **AoE with friendly fire / positioning: yes** (hits ranks; can catch your own party).
- **Death:** companions die instantly (permanent). The **main PC** gets a **death-save / revive window at great cost** (cost TBD — open Q2).
- **No fleeing.** Instead, an optional **surrender** that triggers a procedurally narrated follow-up (escape attempt, or a re-fight under easier circumstances).
- **Status effects: light/minimal** (a small set — e.g., poison, stun, a couple of buffs/debuffs).
- **No damage preview** — commit to actions without seeing the outcome first.

**Characters**
- **No multiclassing.** **Talent/skill tree** choices on level-up.
- **Magic = mana pool + cooldowns** (not Vancian spell slots).
- **Subclasses/archetypes unlock at level 10.**
- **Tight, polished roster at launch** (~6 races / 6 classes).
- **Ability scores: 4d6-drop-lowest** by default (drives the dice theater at creation).
- **XP from kills + milestones; kills give the most.**

**Party**
- **Start solo:** create **one full custom PC**, and at creation also **pick the classes of the 3 companions** who will join later. Companions are **recruited during the journey** (party grows 1 → 4) and arrive with their pre-chosen class. **Fixed party of 4, no bench.**
- Companion identities/backstories are **procedurally generated (Wildermyth-style)** — see open Q3 for how much you customize at creation vs. at recruitment.
- **No party relationship/morale system.**

**World & exploration**
- **Slay-the-Spire branching node map**, spanning **multiple regions/acts** to form a broader world.
- Node types include **towns, shops, rest sites**, plus combat/elite/event/boss.
- **Choose-your-path text events** with skill checks, prominent throughout.
- **No camp/rest resource mechanic.** **Full heal (HP + mana) between encounters** — so resource management lives *within* a single fight, not across. (Rest-site nodes are therefore repurposed — see open Q4.)

**Loot & economy**
- **Gold currency + shops** (buy & sell).
- **Diablo-style procedurally-rolled items** with **rarity tiers** shown via **colored frames/backgrounds** (Fortnite/Diablo style).
- **No crafting or upgrading.**
- **Consumables only — no consumable weapons:** **food** & **potions** (buffs) and **scrolls** (one-time spell casts).
- **Inventory cap: 20 items**, equipped weapons/armor **not** counted.

**Narrative & tone**
- **Story-driven but procedurally generated** — emergent yet **impactful, à la Hades**. The story should land emotionally, not feel like filler.
- **Classic high fantasy**, **heroic/earnest** tone.
- **Branching choices.** The **final boss is procedurally different each campaign** — identity, role, purpose, and goals are generated and threaded through the story.
- **No bestiary.** **Wildermyth-style procedural character stories/events: yes.**

**Saves, feel, v1 scope**
- **Autosave**, **3 campaign slots**, **delete-campaign** option.
- **Full heal between encounters** (restated — no attrition system).
- **No tutorial. No controller support. No colorblind treatment.**
- Audio scope (music/SFX) was unanswered — see open Q5.

---

## 2. Major shifts from the original blueprint

1. **Roguelite → persistent campaign.** Save/load moves from a nice-to-have to the backbone: 3 slots, autosave, delete, campaign metadata (seed, difficulty, progress, party). The node map persists across sessions per campaign seed.
2. **Difficulty drives the dice engine.** The most fundamental piece — the d20 roller — becomes difficulty-aware (Easy skews the player's results). This must live in core so it stays authoritative; the dice theater just animates whatever core decides.
3. **Items become rolled instances, not static IDs. [highest ripple]** Diablo-style affixes + rarity mean each item is a unique *instance* with rolled stats, not a bare `ItemId`. This touches Equipment, Inventory, item rendering, and every tooltip — i.e., all three agents. Fix the model now (see §3) rather than after render/UI harden against `ItemId`.
4. **Magic: spell slots → mana + cooldowns.** Diverges from the "5e-like" framing for spellcasting specifically. Resources reset on the full heal between encounters.
5. **Lanes with rank-based reach/range.** Confirms the Darkest-Dungeon model; the `slot` fields in the contract now carry real combat meaning (rank), and "movement" is rank-swapping.
6. **Talent trees + subclass-at-10.** New progression system: per-class trees, point allocation on level-up, subclass gated at 10.
7. **Solo start + pre-chosen companion classes.** Character creation is "build 1 PC + pick 3 future classes," not "build a party of 4."
8. **Procedural antagonist + impactful procedural narrative.** The most ambitious and least-defined pillar (see §6 — recommend a dedicated design pass).

---

## 3. Contract amendments Codex must make (and log)

Concrete additions/changes to `starwood_core`'s public API:

- **`Difficulty`** enum (`Easy`/`Normal`/`Hard`) + a `Difficulty` resource. The dice resolver reads it and applies the Easy skew **before** setting `is_nat20`/`is_nat1`.
- **Item model → instances. [biggest change]** Introduce something like `ItemInstance { instance_id: u64, base: ItemId, rarity: Rarity, affixes: Vec<Affix>, .. }` and a `Rarity` enum (with associated frame colors as data). `Equipment` slots and `Inventory` now hold **instances / instance ids**, not bare `ItemId`. `items.ron` becomes **base-item templates + affix pools**, and core gains an affix-rolling generator (seeded).
- **`Mana { current, max }`** component + per-ability **cooldown** tracking; a system that **full-resets HP, mana, and cooldowns between encounters**.
- **Combat targeting by rank:** helpers for "which ranks can this melee/ranged ability reach," rank-swap movement, and AoE-by-rank (with friendly-fire). Strict-initiative ordering across all combatants.
- **Two-tier death:** `UnitDied` for companions (permanent); a downed state + `ReviveAttempt`/revive-cost path for the PC only.
- **Surrender:** a `SurrenderRequested` event whose resolution branches into a narrative outcome (replaces flee).
- **Progression:** talent-tree data schema per class + `Talents` / `TalentPoints` components; `Character.subclass: Option<ClassId>` unlocked at level 10; XP from kills (weighted) + milestones; cap 100; enemy **scaling** + renewable encounters for grinding past ~35.
- **Party/recruitment:** creation outputs 1 PC + `PlannedCompanions([ClassId; 3])`; companions join via a `CompanionRecruited` event at story beats with Wildermyth-generated identities.
- **Economy:** `Gold` resource; shop buy/sell transactions; consumable categories (food/potion/scroll) + use logic (scroll = one-shot spell; food/potion = buff); inventory cap 20 (equipped excluded).
- **Antagonist:** generate the final boss's identity/role/purpose/goals at campaign start into an `Antagonist` resource, seeded, and thread it into the narrative.
- **Campaign saves:** expand save/load DTOs to campaign metadata + 3 slots + delete; autosave hooks.

Everything stays **seeded & deterministic per campaign**, and every amendment goes in `CONTRACT_CHANGELOG.md` with the public API kept stable for render/UI.

---

## 4. Per-agent adjustment checklists

### CODEX — `starwood_core`, binary, `assets/data/`
- Reframe to a persistent **campaign** (slots/autosave/delete + metadata); make save/load central.
- Make the **dice engine difficulty-aware** (Easy skew per open Q1); keep core authoritative; add tests proving Easy ≈ +25% success, Hard/Normal = true odds.
- Replace spell slots with **mana + cooldowns**; full reset between encounters.
- **Overhaul items to rolled instances** (rarity + affixes); convert `items.ron` to base templates + affix pools; build the seeded roller.
- Implement **lane/rank combat**: reach/range by rank, rank-swap movement, AoE+friendly-fire, strict initiative, no damage preview.
- **Two-tier death** + PC revive cost; **surrender → narrative branch**.
- **Talent trees** + **subclass-at-10**; XP (kills + milestones); cap 100 + scaling for optional grind.
- **Party model:** 1 PC + 3 planned companion classes; `CompanionRecruited` flow.
- **Economy:** gold, shops, consumables (food/potion/scroll), 20-slot inventory.
- **Procedural antagonist** generator; thread into narrative.
- Keep it **green** (check/clippy/test) and **log every contract change**.

### CURSOR — `starwood_render`, `assets/sprites/`
- Lay out party and enemies in **ranks** (positioning is now mechanically meaningful); support rank-swap visuals.
- **Item rendering is now per-instance:** draw **rarity-colored frames/backgrounds** behind item icons; resolve paper-doll sprites from each instance's **base** item sprite-key. Adopt the `ItemInstance` model.
- Add a distinct **"downed" visual** for the PC (vs. the death/dissolve used for companions).
- AoE/positioning, hurt/death effects keyed to the lane model.
- Extend **placeholder generation** to rarity frames + rank layouts.
- Keep the palette **classic high fantasy / heroic** (bright and earnest, not grim).

### CLAUDE CODE — `starwood_ui`, `assets/fonts/`
- **Creation flow:** build **one full PC**, then a step to **pick the 3 companions' classes** (+ open Q3), with a planned-party preview. Not "create 4."
- **Difficulty-select** screen at new-campaign start; **save-slot UI** (3 slots, autosave indicator, continue, delete).
- **Talent-tree UI** for level-ups; **subclass selection** unlocking at level 10.
- **Mana + cooldown** display in the action bar (replaces spell-slot UI).
- **Inventory:** 20-slot grid, **rarity-colored frames**, tooltips showing **rolled affixes + rarity**; consumable use (food/potion/scroll); equipped gear shown separately (not counted in 20).
- **Shop UI** (buy/sell) + **gold** in the HUD.
- **Combat UI:** strict-initiative turn order (interleaved), action+bonus+move buttons, **rank-based target selection** (reach/range constraints), **no damage preview**, **surrender** option, AoE targeting that signals friendly-fire risk.
- **Narrative/event UI:** choose-your-path text events with skill-check options, surrender-outcome RP scenes, Wildermyth-style companion story moments. (Sizable new surface.)
- **Post-game:** surface the optional bonus quest after the final boss.
- **Dice theater unchanged:** it animates the result core hands it (already skewed on Easy); effects stay purely cosmetic; **no UI-side RNG**.
- Scope: **no tutorial, no controller, no colorblind** treatment.

---

## 5. The riskiest pillar: procedural narrative

"Story-driven but procedural, impactful like Hades" + a **procedurally-generated final boss** + Wildermyth-style companion arcs is the **highest-risk, least-specified** part of the design. Pure-random text reads as hollow; Hades' impact comes from a **large bank of hand-authored fragments** assembled by rules. Recommendation: treat this as its own design workstream with a **templated-authoring** approach — authored beat templates, character/villain archetype tables, and a "story director" that selects and sequences them against the campaign seed. Worth a dedicated prompt once the systems above are landing.

---

## 6. Open questions to resolve (with proposed defaults)

1. **Easy-mode "25% better" — exact mechanic?** Proposed default: on Easy, the player gets a **hidden additive bonus to d20 checks/attacks/saves** (and/or enemy rolls are reduced) calibrated so success rates rise ~25%; Normal/Hard use true odds. Also: does difficulty scale **enemy stats** too, or **only RNG**? Default: Easy/Hard adjust **both** RNG and enemy tuning.
2. **PC revive "great cost" — what is it?** Options: permanent **max-HP loss**, a large **gold + XP** cost, consuming a **rare resource/item**, or a lasting **story "scar."** Default suggestion: a stacking permanent max-HP reduction + a gold cost, so reviving repeatedly is genuinely punishing.
3. **Companion customization at creation:** do you pick only the 3 companions' **classes** (race/name/appearance/personality generated at recruitment), or more? Default: **class only at creation**; everything else Wildermyth-generated when they join.
4. **Rest-site nodes** (since healing is automatic): repurpose them as what — **save points, story moments, respec/talent-reset, a merchant**, or remove them? Default: keep as **safe story/merchant nodes** (no healing role).
5. **Audio in v1?** Default: **wire sound hooks now** (the dice theater already has them), add real music/SFX assets later.
6. **Item rendering source:** confirm the move to **ItemInstance** everywhere (this is assumed from the Diablo-rolls answer and is the biggest contract change).
