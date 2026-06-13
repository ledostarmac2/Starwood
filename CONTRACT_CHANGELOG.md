# Contract Changelog

## 2026-06-13 - Design Decisions Contract Amendments

Breaking contract amendments from `Starwood_Design_Decisions.md` are now
published in `starwood_core`:

- Item model: added `Rarity`, `FrameColor`, `RarityData`, `AffixKind`,
  `AffixTemplate`, `Affix`, `ItemInstance`, `ItemInstances`, and
  `ItemInstanceId`. `Equipment` slots and `Inventory.items` now hold instance
  ids. Existing string-backed ids remain source-compatible while render/UI move
  to instance resolution with `base_item_for_instance`.
- `items.ron` is now a catalog with `bases`, `affixes`, and `rarities`.
  Rarity frame colors are data-driven. Added seeded item rolling helpers:
  `roll_item_instance`, `roll_loot_instances`, and `rarity_frame_color`.
- Added `Difficulty` plus `GameDifficulty` resource and `DifficultyTuning`.
  Core applies Easy d20 skew before nat-1/nat-20 detection; Normal/Hard remain
  uniform. Enemy tuning uses the same difficulty resource.
- Added `Mana`, `Cooldowns`, and `AbilityCooldown`, plus class data fields
  `base_mana`, `ability_mana_costs`, and `ability_cooldowns`. Encounter end
  fully resets HP, mana, and cooldowns.
- Added rank/lane combat helpers: `Rank`, `CombatSide`, `Reach`,
  `can_reach_rank`, `reachable_ranks`, `rank_swap`, `aoe_targets_by_rank`, and
  `aoe_friendly_fire_risk`. Party/enemy `slot` values now mean rank.
- Replaced flee semantics with `SurrenderRequested`; surrender returns to the
  narrative/exploration branch instead of game over.
- Added `Gold`, `ShopTransaction`, `ShopTransactionRequested`,
  `ConsumableCategory`, and `ConsumableUseRequested`. Inventory cap is exposed
  as `INVENTORY_CAPACITY` and equipped items are excluded.
- Added talent/subclass contract: `TalentId`, `TalentTreeData`,
  `TalentNodeData`, `Talents`, `TalentPoints`, `Character.subclass`, and
  `SUBCLASS_UNLOCK_LEVEL`.
- Added party planning/recruitment contract: `PlannedCompanions` resource and
  `CompanionRecruited` message. `CreationStep::Companions` is the post-review
  companion-class planning step.
- Added campaign/narrative save surface: `Antagonist`, `CampaignMetadata`,
  `CampaignSaves`, `CampaignSlot`, expanded `SaveGame`, autosave flag, gold,
  planned companions, difficulty, antagonist, and item-instance persistence.
- Added PC revive contract: `PlayerCharacter`, `Downed`, `RevivePenalty`,
  `ReviveAttempt`, and tunable revive constants.
- Added map node types `Town`, `Shop`, and `BonusQuest` for the persistent
  campaign loop and post-boss optional quest.

## 2026-06-13 - Initial Bevy 0.18 Contract

- Bevy 0.18 renamed app events to messages. Blueprint event structs keep their
  public names, derive `Message`, and are registered with `add_message`.
  `starwood_core` exports `EventReader` / `EventWriter` compatibility aliases.
- Added initial game-flow messages:
  `NewGameRequested`, `CreationStepAdvanceRequested`,
  `CharacterBuildRequested`, `FinishPartyCreationRequested`, and
  `EncounterRequested`.
- Added `ClassData::ability_mods` with `#[serde(default)]`.
