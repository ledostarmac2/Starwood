use bevy::prelude::*;
use rand::{Rng, SeedableRng, seq::IndexedRandom};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

pub struct StarwoodCorePlugin {
    pub seed: u64,
}

impl Default for StarwoodCorePlugin {
    fn default() -> Self {
        Self { seed: 0x57A2_C0DE }
    }
}

impl Plugin for StarwoodCorePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_sub_state::<CreationStep>()
            .insert_resource(InventoryOpen::default())
            .insert_resource(CampaignSeed(self.seed))
            .insert_resource(GameRng(ChaCha8Rng::seed_from_u64(self.seed)))
            .insert_resource(GameData::default())
            .insert_resource(GameDifficulty::default())
            .insert_resource(DifficultyTuning::default())
            .insert_resource(PartyRoster::default())
            .insert_resource(PlannedCompanions::default())
            .insert_resource(Inventory::default())
            .insert_resource(ItemInstances::default())
            .insert_resource(Gold::default())
            .insert_resource(Antagonist::generate(self.seed))
            .insert_resource(CampaignSaves::default())
            .insert_resource(MapState::default())
            .insert_resource(EncounterState::default())
            .insert_resource(AssetHandles::default())
            .insert_resource(PendingRolls::default())
            .insert_resource(DebugDiceOverride::default())
            .add_message::<RollRequest>()
            .add_message::<RollResolved>()
            .add_message::<RollAnimationComplete>()
            .add_message::<EquipmentChanged>()
            .add_message::<EncounterStarted>()
            .add_message::<EncounterEnded>()
            .add_message::<DamageDealt>()
            .add_message::<UnitDied>()
            .add_message::<CharacterFinalized>()
            .add_message::<InventoryChanged>()
            .add_message::<CombatActionRequest>()
            .add_message::<NewGameRequested>()
            .add_message::<CreationStepAdvanceRequested>()
            .add_message::<CharacterBuildRequested>()
            .add_message::<FinishPartyCreationRequested>()
            .add_message::<EncounterRequested>()
            .add_message::<CompanionRecruited>()
            .add_message::<SurrenderRequested>()
            .add_message::<ReviveAttempt>()
            .add_message::<ShopTransactionRequested>()
            .add_message::<ConsumableUseRequested>()
            .add_systems(Startup, load_game_data_system)
            .add_systems(
                Update,
                (
                    handle_new_game_requests,
                    handle_creation_step_advance_requests,
                    handle_character_build_requests,
                    handle_finish_party_creation_requests,
                    handle_encounter_requests,
                    request_combat_actions,
                    handle_surrender_requests,
                    handle_shop_transactions,
                    handle_consumable_use_requests,
                    handle_revive_attempts,
                    resolve_roll_requests,
                    capture_attack_roll_results,
                    complete_pending_roll_actions,
                    detect_dead_units,
                    handle_unit_death_outcomes,
                    detect_encounter_end,
                    reset_between_encounters,
                    handle_encounter_ended_state,
                    enforce_roster_caps,
                ),
            );
    }
}

// ===== STATES =====
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    MainMenu,
    CharacterCreation,
    Exploration,
    Encounter,
    GameOver,
}

#[derive(SubStates, Default, Debug, Clone, PartialEq, Eq, Hash)]
#[source(GameState = GameState::CharacterCreation)]
pub enum CreationStep {
    #[default]
    Race,
    Class,
    AbilityRoll,
    SkillsTraits,
    Review,
    Companions,
}

#[derive(Resource, Default)]
pub struct InventoryOpen(pub bool);

// ===== IDENTITY / DATA KEYS =====
pub type RaceId = String;
pub type ClassId = String;
pub type SkillId = String;
pub type TraitId = String;
pub type ItemId = String;
pub type ItemInstanceId = String;
pub type AffixId = String;
pub type TalentId = String;
pub type AbilityId = String;
pub type EnemyArchetypeId = String;
pub type CampaignSlotId = u8;

// ===== CORE COMPONENTS =====
#[derive(Component, Clone)]
pub struct Character {
    pub name: String,
    pub race: RaceId,
    pub class: ClassId,
    pub subclass: Option<ClassId>,
    pub level: u32,
    pub xp: u32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Abilities {
    pub str_: u8,
    pub dex: u8,
    pub con: u8,
    pub int: u8,
    pub wis: u8,
    pub cha: u8,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Derived {
    pub armor_class: i32,
    pub max_hp: i32,
    pub initiative_mod: i32,
    pub proficiency: i32,
    pub speed: i32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mana {
    pub current: i32,
    pub max: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityCooldown {
    pub ability_id: AbilityId,
    pub remaining: u32,
    pub max: u32,
}

#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cooldowns {
    pub abilities: Vec<AbilityCooldown>,
}

#[derive(Component, Clone, Default)]
pub struct SkillSet {
    pub proficient: Vec<SkillId>,
}

#[derive(Component, Clone, Default)]
pub struct Traits(pub Vec<TraitId>);

#[derive(Component, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Talents(pub Vec<TalentId>);

#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalentPoints(pub u32);

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCharacter;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Downed;

#[derive(Component, Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevivePenalty {
    pub stacks: u32,
}

#[derive(Component, Clone, Copy)]
pub struct PartyMember {
    pub slot: u8,
}

#[derive(Component, Clone)]
pub struct EnemyUnit {
    pub archetype: EnemyArchetypeId,
    pub slot: u8,
}

#[derive(Component, Clone, Default)]
pub struct Equipment {
    pub head: Option<ItemInstanceId>,
    pub body: Option<ItemInstanceId>,
    pub main_hand: Option<ItemInstanceId>,
    pub off_hand: Option<ItemInstanceId>,
    pub feet: Option<ItemInstanceId>,
}

#[derive(Component, Clone)]
pub struct SpriteParts {
    pub base_body: String,
}

#[derive(Component)]
pub struct ActiveTurn;

#[derive(Component, Clone, Copy)]
pub struct Initiative(pub i32);

// ===== RESOURCES =====
#[derive(Resource)]
pub struct GameRng(pub ChaCha8Rng);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignSeed(pub u64);

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct GameData {
    pub races: HashMap<RaceId, RaceData>,
    pub classes: HashMap<ClassId, ClassData>,
    pub skills: HashMap<SkillId, SkillData>,
    pub traits: HashMap<TraitId, TraitData>,
    pub items: HashMap<ItemId, ItemData>,
    pub affixes: HashMap<AffixId, AffixTemplate>,
    pub rarities: HashMap<Rarity, RarityData>,
    pub talent_trees: HashMap<ClassId, TalentTreeData>,
    pub enemies: HashMap<EnemyArchetypeId, EnemyArchetypeData>,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameDifficulty(pub Difficulty);

impl Default for GameDifficulty {
    fn default() -> Self {
        Self(Difficulty::Normal)
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DifficultyTuning {
    pub easy_player_d20_bonus: i32,
    pub easy_enemy_multiplier: f32,
    pub normal_enemy_multiplier: f32,
    pub hard_enemy_multiplier: f32,
}

impl Default for DifficultyTuning {
    fn default() -> Self {
        Self {
            easy_player_d20_bonus: 5,
            easy_enemy_multiplier: 0.85,
            normal_enemy_multiplier: 1.0,
            hard_enemy_multiplier: 1.15,
        }
    }
}

#[derive(Resource, Default)]
pub struct PartyRoster {
    pub members: Vec<Entity>,
}

#[derive(Resource, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannedCompanions {
    pub classes: [ClassId; 3],
}

impl Default for PlannedCompanions {
    fn default() -> Self {
        Self {
            classes: ["fighter".into(), "cleric".into(), "rogue".into()],
        }
    }
}

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inventory {
    pub items: Vec<ItemInstanceId>,
}

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemInstances {
    pub instances: HashMap<ItemInstanceId, ItemInstance>,
    pub next_serial: u64,
}

#[derive(Resource, Default, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Gold(pub u32);

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignSaves {
    pub slots: [CampaignSlot; 3],
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignSlot {
    pub metadata: Option<CampaignMetadata>,
    pub autosave: bool,
}

#[derive(Resource, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Antagonist {
    pub identity: String,
    pub role: String,
    pub purpose: String,
    pub goals: Vec<String>,
}

impl Default for Antagonist {
    fn default() -> Self {
        Self {
            identity: "The Hollow Regent".into(),
            role: "fallen monarch".into(),
            purpose: "bind the Starwood to an old oath".into(),
            goals: vec![
                "claim the three green crowns".into(),
                "turn the final grove into a throne".into(),
            ],
        }
    }
}

impl Antagonist {
    pub fn generate(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x0A17_A601_57A9);
        let identities = [
            "The Thorn-Crowned Seer",
            "Maerwyn of the Glass Root",
            "The Ashen Hart",
            "Sir Cael of the Last Lantern",
            "The Hollow Regent",
            "Veyra Star-Eater",
        ];
        let roles = [
            "fallen monarch",
            "oathbound archmage",
            "exiled guardian",
            "saint of a broken order",
            "dragon-blooded usurper",
            "oracle lost to prophecy",
        ];
        let purposes = [
            "bind the Starwood to an old oath",
            "wake a buried kingdom beneath the roots",
            "trade mortal names for immortal peace",
            "unmake the road between death and dawn",
            "forge a crown from stolen seasons",
            "silence every future but one",
        ];
        let goal_bank = [
            "claim the three green crowns",
            "turn the final grove into a throne",
            "break the moonwell seals",
            "recruit a companion through fear",
            "corrupt the oldest town on the road",
            "hide the true boss behind a beloved mask",
        ];
        let mut goals = Vec::new();
        while goals.len() < 2 {
            let goal = goal_bank
                .choose(&mut rng)
                .unwrap_or(&goal_bank[0])
                .to_string();
            if !goals.contains(&goal) {
                goals.push(goal);
            }
        }
        Self {
            identity: identities
                .choose(&mut rng)
                .unwrap_or(&identities[0])
                .to_string(),
            role: roles.choose(&mut rng).unwrap_or(&roles[0]).to_string(),
            purpose: purposes
                .choose(&mut rng)
                .unwrap_or(&purposes[0])
                .to_string(),
            goals,
        }
    }
}

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapState {
    pub seed: u64,
    pub nodes: Vec<MapNode>,
    pub current_node: Option<u32>,
}

#[derive(Resource, Default)]
pub struct EncounterState {
    pub enemies: Vec<Entity>,
    pub turn_order: Vec<Entity>,
    pub turn_index: usize,
    pub surrendered: bool,
}

#[derive(Resource, Default)]
pub struct AssetHandles {
    pub sprites: HashMap<String, Handle<Image>>,
    pub fonts: HashMap<String, Handle<Font>>,
}

// ===== DATA SCHEMA =====
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AbilityMods {
    pub str_: i8,
    pub dex: i8,
    pub con: i8,
    pub int: i8,
    pub wis: i8,
    pub cha: i8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaceData {
    pub id: RaceId,
    pub name: String,
    pub description: String,
    pub ability_mods: AbilityMods,
    pub traits: Vec<TraitId>,
    pub speed: i32,
    pub sprite_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClassData {
    pub id: ClassId,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub ability_mods: AbilityMods,
    pub hit_die: u8,
    #[serde(default)]
    pub base_mana: i32,
    pub primary_abilities: Vec<String>,
    pub saving_throws: Vec<String>,
    pub skill_choices: Vec<SkillId>,
    pub starting_kit: Vec<ItemId>,
    pub class_abilities: Vec<String>,
    #[serde(default)]
    pub ability_mana_costs: HashMap<AbilityId, u32>,
    #[serde(default)]
    pub ability_cooldowns: HashMap<AbilityId, u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillData {
    pub id: SkillId,
    pub name: String,
    pub ability: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraitData {
    pub id: TraitId,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ItemSlot {
    Head,
    Body,
    MainHand,
    OffHand,
    Feet,
    Consumable,
    Treasure,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RarityData {
    pub rarity: Rarity,
    pub name: String,
    pub frame_color: FrameColor,
    pub weight: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AffixKind {
    AttackBonus,
    ArmorBonus,
    DamageBonus,
    MaxHealth,
    MaxMana,
    SkillBonus,
    CooldownReduction,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AffixTemplate {
    pub id: AffixId,
    pub name: String,
    pub kind: AffixKind,
    pub min_value: i32,
    pub max_value: i32,
    pub weight: u32,
    #[serde(default)]
    pub min_level: u32,
    #[serde(default)]
    pub rarity_min: Option<Rarity>,
    #[serde(default)]
    pub allowed_slots: Vec<ItemSlot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Affix {
    pub id: AffixId,
    pub name: String,
    pub kind: AffixKind,
    pub value: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemInstance {
    pub instance_id: ItemInstanceId,
    pub base: ItemId,
    pub rarity: Rarity,
    pub affixes: Vec<Affix>,
    pub level: u32,
    pub value: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsumableCategory {
    Food,
    Potion,
    Scroll,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemData {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub slot: ItemSlot,
    pub armor_bonus: i32,
    pub damage: Option<DiceExpr>,
    pub sprite_key: String,
    pub value: u32,
    #[serde(default)]
    pub consumable: Option<ConsumableCategory>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TalentTreeData {
    pub class: ClassId,
    pub nodes: Vec<TalentNodeData>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TalentNodeData {
    pub id: TalentId,
    pub name: String,
    pub description: String,
    pub rank: u8,
    pub cost: u32,
    #[serde(default)]
    pub requires: Vec<TalentId>,
    #[serde(default)]
    pub unlock_level: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnemyArchetypeData {
    pub id: EnemyArchetypeId,
    pub name: String,
    pub description: String,
    pub level: u32,
    pub abilities: Abilities,
    pub armor_class: i32,
    pub hit_points: i32,
    pub attack_bonus: i32,
    pub damage: DiceExpr,
    pub xp: u32,
    pub sprite_key: String,
}

// ===== DIFFICULTY / DICE =====
#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdvState {
    Normal,
    Advantage,
    Disadvantage,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiceExpr {
    pub count: u32,
    pub sides: u32,
    pub modifier: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollKind {
    Initiative,
    Attack,
    Damage,
    AbilityCheck,
    SavingThrow,
    AbilityScoreGen,
    Generic,
}

// ===== EVENTS =====
pub type EventReader<'w, 's, T> = MessageReader<'w, 's, T>;
pub type EventWriter<'w, T> = MessageWriter<'w, T>;

#[derive(Message)]
pub struct RollRequest {
    pub id: u64,
    pub expr: DiceExpr,
    pub kind: RollKind,
    pub source: Option<Entity>,
    pub advantage: AdvState,
}

#[derive(Message)]
pub struct RollResolved {
    pub id: u64,
    pub rolls: Vec<u32>,
    pub total: i32,
    pub is_nat20: bool,
    pub is_nat1: bool,
    pub kind: RollKind,
}

#[derive(Message)]
pub struct RollAnimationComplete {
    pub id: u64,
}

#[derive(Message)]
pub struct EquipmentChanged {
    pub entity: Entity,
}

#[derive(Message)]
pub struct EncounterStarted {
    pub enemies: Vec<Entity>,
}

#[derive(Message)]
pub struct EncounterEnded {
    pub victory: bool,
}

#[derive(Message)]
pub struct DamageDealt {
    pub target: Entity,
    pub amount: i32,
    pub is_crit: bool,
}

#[derive(Message)]
pub struct UnitDied {
    pub entity: Entity,
}

#[derive(Message)]
pub struct CharacterFinalized {
    pub entity: Entity,
}

#[derive(Message)]
pub struct InventoryChanged;

#[derive(Message, Clone)]
pub struct CompanionRecruited {
    pub entity: Entity,
    pub rank: u8,
    pub class: ClassId,
}

#[derive(Message, Clone, Copy)]
pub struct SurrenderRequested {
    pub actor: Entity,
}

#[derive(Message, Clone, Copy)]
pub struct ReviveAttempt {
    pub entity: Entity,
}

#[derive(Message, Clone)]
pub struct ShopTransactionRequested {
    pub item: ItemInstanceId,
    pub transaction: ShopTransaction,
}

#[derive(Message, Clone)]
pub struct ConsumableUseRequested {
    pub actor: Entity,
    pub item: ItemInstanceId,
}

#[derive(Message, Clone, Copy)]
pub struct CombatActionRequest {
    pub actor: Entity,
    pub target: Entity,
    pub action: CombatAction,
}

#[derive(Message, Clone, Copy)]
pub struct NewGameRequested {
    pub seed: u64,
}

#[derive(Message, Clone, Copy)]
pub struct CreationStepAdvanceRequested;

#[derive(Message, Clone)]
pub struct CharacterBuildRequested {
    pub name: String,
    pub race: RaceId,
    pub class: ClassId,
    pub abilities: Abilities,
    pub skills: Vec<SkillId>,
    pub traits: Vec<TraitId>,
}

#[derive(Message, Clone, Copy)]
pub struct FinishPartyCreationRequested;

#[derive(Message, Clone, Copy)]
pub struct EncounterRequested {
    pub difficulty: MapNodeType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatAction {
    Attack,
    Cast,
    UseItem,
    Surrender,
    RankSwap,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShopTransaction {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CombatSide {
    Party,
    Enemy,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rank(pub u8);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Reach {
    Melee,
    Reach,
    Ranged,
    Any,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankTarget {
    pub entity: Entity,
    pub side: CombatSide,
    pub rank: Rank,
}

#[derive(Resource, Default)]
pub struct PendingRolls {
    pub attack_intents: HashMap<u64, PendingAttackIntent>,
    pub attacks: HashMap<u64, PendingAttack>,
}

#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugDiceOverride {
    pub next: Option<ForcedRoll>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForcedRoll {
    Nat20,
    Nat1,
    Value(u32),
}

#[derive(Clone)]
pub struct PendingAttackIntent {
    pub attacker: Entity,
    pub target: Entity,
    pub damage: DiceExpr,
}

#[derive(Clone)]
pub struct PendingAttack {
    pub attacker: Entity,
    pub target: Entity,
    pub attack_total: i32,
    pub damage: DiceExpr,
    pub is_crit: bool,
}

pub fn parse_dice_expr(input: &str) -> Result<DiceExpr, String> {
    let trimmed = input.trim().replace(' ', "");
    if trimmed.is_empty() {
        return Err("dice expression cannot be empty".to_string());
    }
    let (dice, modifier) = if let Some((left, right)) = trimmed.split_once('+') {
        (
            left,
            right
                .parse::<i32>()
                .map_err(|_| "invalid positive modifier")?,
        )
    } else if let Some(index) = trimmed[1..].find('-') {
        let split = index + 1;
        let (left, right) = trimmed.split_at(split);
        (
            left,
            right
                .parse::<i32>()
                .map_err(|_| "invalid negative modifier")?,
        )
    } else {
        (trimmed.as_str(), 0)
    };

    let (count, sides) = dice
        .split_once('d')
        .or_else(|| dice.split_once('D'))
        .ok_or("dice expression must contain d")?;
    let count = if count.is_empty() {
        1
    } else {
        count.parse::<u32>().map_err(|_| "invalid dice count")?
    };
    let sides = sides.parse::<u32>().map_err(|_| "invalid dice sides")?;

    if count == 0 || sides == 0 {
        return Err("dice count and sides must be positive".to_string());
    }

    Ok(DiceExpr {
        count,
        sides,
        modifier,
    })
}

pub fn roll_dice(expr: &DiceExpr, advantage: AdvState, rng: &mut ChaCha8Rng) -> RollResolvedParts {
    let mut rolls = Vec::new();
    let mut total = expr.modifier;
    let mut is_nat20 = false;
    let mut is_nat1 = false;

    if expr.count == 1 && expr.sides == 20 && advantage != AdvState::Normal {
        let first = rng.random_range(1..=20);
        let second = rng.random_range(1..=20);
        rolls.extend([first, second]);
        let kept = match advantage {
            AdvState::Advantage => first.max(second),
            AdvState::Disadvantage => first.min(second),
            AdvState::Normal => unreachable!(),
        };
        total += kept as i32;
        is_nat20 = kept == 20;
        is_nat1 = kept == 1;
        return RollResolvedParts {
            rolls,
            total,
            is_nat20,
            is_nat1,
        };
    }

    for _ in 0..expr.count {
        let roll = rng.random_range(1..=expr.sides);
        if expr.sides == 20 {
            is_nat20 |= roll == 20;
            is_nat1 |= roll == 1;
        }
        rolls.push(roll);
        total += roll as i32;
    }

    RollResolvedParts {
        rolls,
        total,
        is_nat20,
        is_nat1,
    }
}

pub fn roll_dice_with_difficulty(
    expr: &DiceExpr,
    advantage: AdvState,
    rng: &mut ChaCha8Rng,
    difficulty: Difficulty,
    is_player: bool,
    tuning: DifficultyTuning,
) -> RollResolvedParts {
    if expr.count == 1 && expr.sides == 20 && advantage != AdvState::Normal {
        let first =
            apply_difficulty_to_d20(rng.random_range(1..=20), difficulty, is_player, tuning);
        let second =
            apply_difficulty_to_d20(rng.random_range(1..=20), difficulty, is_player, tuning);
        let kept = match advantage {
            AdvState::Advantage => first.max(second),
            AdvState::Disadvantage => first.min(second),
            AdvState::Normal => unreachable!(),
        };
        return RollResolvedParts {
            rolls: vec![first, second],
            total: kept as i32 + expr.modifier,
            is_nat20: kept == 20,
            is_nat1: kept == 1,
        };
    }

    let mut rolls = Vec::new();
    let mut total = expr.modifier;
    let mut is_nat20 = false;
    let mut is_nat1 = false;
    for _ in 0..expr.count {
        let raw = rng.random_range(1..=expr.sides);
        let roll = if expr.sides == 20 {
            apply_difficulty_to_d20(raw, difficulty, is_player, tuning)
        } else {
            raw
        };
        if expr.sides == 20 {
            is_nat20 |= roll == 20;
            is_nat1 |= roll == 1;
        }
        rolls.push(roll);
        total += roll as i32;
    }
    RollResolvedParts {
        rolls,
        total,
        is_nat20,
        is_nat1,
    }
}

pub fn roll_ability_score_gen(rng: &mut ChaCha8Rng) -> RollResolvedParts {
    let rolls = [
        rng.random_range(1..=6),
        rng.random_range(1..=6),
        rng.random_range(1..=6),
        rng.random_range(1..=6),
    ];
    let mut sorted = rolls;
    sorted.sort_unstable();
    let total = sorted[1..].iter().sum::<u32>() as i32;
    RollResolvedParts {
        rolls: rolls.to_vec(),
        total,
        is_nat20: false,
        is_nat1: false,
    }
}

pub fn forced_roll_parts(expr: &DiceExpr, kind: RollKind, forced: ForcedRoll) -> RollResolvedParts {
    let sides = expr.sides.max(1);
    let value = match forced {
        ForcedRoll::Nat20 => 20.min(sides),
        ForcedRoll::Nat1 => 1,
        ForcedRoll::Value(value) => value.clamp(1, sides),
    };

    if kind == RollKind::AbilityScoreGen {
        return RollResolvedParts {
            rolls: vec![value],
            total: value as i32,
            is_nat20: false,
            is_nat1: false,
        };
    }

    RollResolvedParts {
        rolls: vec![value],
        total: value as i32 + expr.modifier,
        is_nat20: expr.count == 1 && expr.sides == 20 && value == 20,
        is_nat1: expr.count == 1 && expr.sides == 20 && value == 1,
    }
}

pub fn apply_difficulty_to_d20(
    raw: u32,
    difficulty: Difficulty,
    is_player: bool,
    tuning: DifficultyTuning,
) -> u32 {
    if is_player && difficulty == Difficulty::Easy {
        (raw as i32 + tuning.easy_player_d20_bonus).clamp(1, 20) as u32
    } else {
        raw
    }
}

pub fn d20_success_chance(
    dc: i32,
    modifier: i32,
    difficulty: Difficulty,
    is_player: bool,
    tuning: DifficultyTuning,
) -> f32 {
    let successes = (1..=20)
        .filter(|raw| {
            let adjusted = apply_difficulty_to_d20(*raw, difficulty, is_player, tuning) as i32;
            adjusted + modifier >= dc
        })
        .count();
    successes as f32 / 20.0
}

pub fn enemy_tuning_multiplier(difficulty: Difficulty, tuning: DifficultyTuning) -> f32 {
    match difficulty {
        Difficulty::Easy => tuning.easy_enemy_multiplier,
        Difficulty::Normal => tuning.normal_enemy_multiplier,
        Difficulty::Hard => tuning.hard_enemy_multiplier,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollResolvedParts {
    pub rolls: Vec<u32>,
    pub total: i32,
    pub is_nat20: bool,
    pub is_nat1: bool,
}

pub fn ability_modifier(score: u8) -> i32 {
    ((score as i32) - 10).div_euclid(2)
}

pub fn proficiency_bonus(level: u32) -> i32 {
    2 + ((level.saturating_sub(1) as i32) / 4)
}

pub fn armor_class(abilities: Abilities, equipment: &Equipment, data: &GameData) -> i32 {
    let armor = equipment
        .body
        .as_ref()
        .and_then(|id| data.items.get(id))
        .map_or(10, |item| 10 + item.armor_bonus);
    let shield = equipment
        .off_hand
        .as_ref()
        .and_then(|id| data.items.get(id))
        .map_or(0, |item| item.armor_bonus);
    armor + shield + ability_modifier(abilities.dex)
}

pub fn armor_class_with_instances(
    abilities: Abilities,
    equipment: &Equipment,
    data: &GameData,
    instances: &ItemInstances,
) -> i32 {
    let armor = equipment
        .body
        .as_ref()
        .and_then(|id| base_item_for_instance(id, data, instances))
        .map_or(10, |item| {
            10 + item.armor_bonus
                + affix_total(
                    id_affixes(equipment.body.as_ref(), instances),
                    AffixKind::ArmorBonus,
                )
        });
    let shield = equipment
        .off_hand
        .as_ref()
        .and_then(|id| base_item_for_instance(id, data, instances))
        .map_or(0, |item| {
            item.armor_bonus
                + affix_total(
                    id_affixes(equipment.off_hand.as_ref(), instances),
                    AffixKind::ArmorBonus,
                )
        });
    armor + shield + ability_modifier(abilities.dex)
}

fn id_affixes<'a>(
    instance_id: Option<&'a ItemInstanceId>,
    instances: &'a ItemInstances,
) -> &'a [Affix] {
    instance_id
        .and_then(|id| instances.instances.get(id))
        .map(|instance| instance.affixes.as_slice())
        .unwrap_or(&[])
}

pub fn base_item_for_instance<'a>(
    instance_id: &str,
    data: &'a GameData,
    instances: &'a ItemInstances,
) -> Option<&'a ItemData> {
    instances
        .instances
        .get(instance_id)
        .and_then(|instance| data.items.get(&instance.base))
        .or_else(|| data.items.get(instance_id))
}

pub fn affix_total(affixes: &[Affix], kind: AffixKind) -> i32 {
    affixes
        .iter()
        .filter(|affix| affix.kind == kind)
        .map(|affix| affix.value)
        .sum()
}

pub fn initiative_modifier(abilities: Abilities) -> i32 {
    ability_modifier(abilities.dex)
}

pub fn attack_hits(attack_total: i32, target_ac: i32, is_nat20: bool, is_nat1: bool) -> bool {
    if is_nat1 {
        false
    } else if is_nat20 {
        true
    } else {
        attack_total >= target_ac
    }
}

pub fn skill_bonus(abilities: Abilities, skill: &SkillData, skills: &SkillSet, level: u32) -> i32 {
    let ability = ability_score_by_name(abilities, &skill.ability);
    let prof = if skills.proficient.iter().any(|id| id == &skill.id) {
        proficiency_bonus(level)
    } else {
        0
    };
    ability_modifier(ability) + prof
}

pub fn ability_check_bonus(
    abilities: Abilities,
    ability: &str,
    proficient: bool,
    level: u32,
) -> i32 {
    ability_modifier(ability_score_by_name(abilities, ability))
        + if proficient {
            proficiency_bonus(level)
        } else {
            0
        }
}

pub fn saving_throw_bonus(
    abilities: Abilities,
    ability: &str,
    proficient: bool,
    level: u32,
) -> i32 {
    ability_check_bonus(abilities, ability, proficient, level)
}

pub fn total_damage(rolled_total: i32, is_crit: bool, dice: &DiceExpr) -> i32 {
    if is_crit {
        (rolled_total + dice.count as i32).max(0)
    } else {
        rolled_total.max(0)
    }
}

pub fn ability_score_by_name(abilities: Abilities, name: &str) -> u8 {
    match name.to_ascii_lowercase().as_str() {
        "str" | "str_" | "strength" => abilities.str_,
        "dex" | "dexterity" => abilities.dex,
        "con" | "constitution" => abilities.con,
        "int" | "intelligence" => abilities.int,
        "wis" | "wisdom" => abilities.wis,
        "cha" | "charisma" => abilities.cha,
        _ => 10,
    }
}

pub fn standard_array() -> [u8; 6] {
    [15, 14, 13, 12, 10, 8]
}

pub fn point_buy_cost(score: u8) -> Option<u8> {
    match score {
        8 => Some(0),
        9 => Some(1),
        10 => Some(2),
        11 => Some(3),
        12 => Some(4),
        13 => Some(5),
        14 => Some(7),
        15 => Some(9),
        _ => None,
    }
}

pub fn validate_point_buy(scores: [u8; 6]) -> bool {
    scores
        .into_iter()
        .map(point_buy_cost)
        .try_fold(0u8, |sum, cost| cost.map(|cost| sum + cost))
        .is_some_and(|cost| cost <= 27)
}

pub fn roll_4d6_drop_lowest(rng: &mut ChaCha8Rng) -> u8 {
    let mut rolls = [
        rng.random_range(1..=6),
        rng.random_range(1..=6),
        rng.random_range(1..=6),
        rng.random_range(1..=6),
    ];
    rolls.sort_unstable();
    rolls[1..].iter().sum::<u32>() as u8
}

pub fn apply_race_mods(mut abilities: Abilities, mods: &AbilityMods) -> Abilities {
    abilities.str_ = add_mod(abilities.str_, mods.str_);
    abilities.dex = add_mod(abilities.dex, mods.dex);
    abilities.con = add_mod(abilities.con, mods.con);
    abilities.int = add_mod(abilities.int, mods.int);
    abilities.wis = add_mod(abilities.wis, mods.wis);
    abilities.cha = add_mod(abilities.cha, mods.cha);
    abilities
}

pub fn apply_class_mods(abilities: Abilities, class: &ClassData) -> Abilities {
    apply_race_mods(abilities, &class.ability_mods)
}

pub fn build_starting_equipment(class: &ClassData, data: &GameData) -> Equipment {
    let mut equipment = Equipment::default();
    for item_id in &class.starting_kit {
        let Some(item) = data.items.get(item_id) else {
            continue;
        };
        match item.slot {
            ItemSlot::Head => equipment.head = Some(item_id.clone()),
            ItemSlot::Body => equipment.body = Some(item_id.clone()),
            ItemSlot::MainHand => equipment.main_hand = Some(item_id.clone()),
            ItemSlot::OffHand => equipment.off_hand = Some(item_id.clone()),
            ItemSlot::Feet => equipment.feet = Some(item_id.clone()),
            ItemSlot::Consumable | ItemSlot::Treasure => {}
        }
    }
    equipment
}

pub fn finalize_character_bundle(
    name: impl Into<String>,
    race: &RaceData,
    class: &ClassData,
    base_abilities: Abilities,
    selected_skills: Vec<SkillId>,
    selected_traits: Vec<TraitId>,
    data: &GameData,
) -> (
    Character,
    Abilities,
    Derived,
    Health,
    SkillSet,
    Traits,
    Equipment,
    SpriteParts,
) {
    let abilities = apply_class_mods(apply_race_mods(base_abilities, &race.ability_mods), class);
    let equipment = build_starting_equipment(class, data);
    let derived = derived_stats(abilities, 1, class, race, &equipment, data);
    (
        Character {
            name: name.into(),
            race: race.id.clone(),
            class: class.id.clone(),
            subclass: None,
            level: 1,
            xp: 0,
        },
        abilities,
        derived,
        Health {
            current: derived.max_hp,
            max: derived.max_hp,
        },
        SkillSet {
            proficient: selected_skills,
        },
        Traits(selected_traits),
        equipment,
        SpriteParts {
            base_body: race.sprite_key.clone(),
        },
    )
}

pub fn instance_starting_equipment(
    class: &ClassData,
    data: &GameData,
    instances: &mut ItemInstances,
    rng: &mut ChaCha8Rng,
    equipment: &mut Equipment,
    inventory: &mut Inventory,
) {
    for item_id in &class.starting_kit {
        let Some(base) = data.items.get(item_id) else {
            continue;
        };
        let instance = roll_item_instance(base, data, instances, rng, 1);
        match base.slot {
            ItemSlot::Head => equipment.head = Some(instance.instance_id),
            ItemSlot::Body => equipment.body = Some(instance.instance_id),
            ItemSlot::MainHand => equipment.main_hand = Some(instance.instance_id),
            ItemSlot::OffHand => equipment.off_hand = Some(instance.instance_id),
            ItemSlot::Feet => equipment.feet = Some(instance.instance_id),
            ItemSlot::Consumable | ItemSlot::Treasure => {
                let _ = add_item_to_inventory(inventory, instance.instance_id);
            }
        }
    }
}

fn add_mod(score: u8, modifier: i8) -> u8 {
    (score as i16 + modifier as i16).clamp(1, 30) as u8
}

pub fn derived_stats(
    abilities: Abilities,
    level: u32,
    class: &ClassData,
    race: &RaceData,
    equipment: &Equipment,
    data: &GameData,
) -> Derived {
    let con = ability_modifier(abilities.con);
    let max_hp = (class.hit_die as i32 + con).max(1)
        + ((level.saturating_sub(1) as i32) * ((class.hit_die as i32 / 2 + 1) + con).max(1));
    Derived {
        armor_class: armor_class(abilities, equipment, data),
        max_hp,
        initiative_mod: initiative_modifier(abilities),
        proficiency: proficiency_bonus(level),
        speed: race.speed,
    }
}

pub fn xp_for_level(level: u32) -> u32 {
    match level {
        0 | 1 => 0,
        2 => 300,
        3 => 900,
        4 => 2700,
        5 => 6500,
        6 => 14000,
        7 => 23000,
        8 => 34000,
        9 => 48000,
        10 => 64000,
        _ => 64000 + (level - 10) * 20000,
    }
}

pub fn level_for_xp(xp: u32) -> u32 {
    (1..=MAX_LEVEL)
        .rev()
        .find(|level| xp >= xp_for_level(*level))
        .unwrap_or(1)
}

pub const MAX_LEVEL: u32 = 100;
pub const SUBCLASS_UNLOCK_LEVEL: u32 = 10;
pub const INVENTORY_CAPACITY: usize = 20;
pub const REVIVE_MAX_HP_LOSS_PER_STACK: i32 = 2;
pub const REVIVE_GOLD_COST_BASE: u32 = 150;
pub const MAIN_STORY_TARGET_LEVEL: u32 = 35;
pub const POTION_HEAL_AMOUNT: i32 = 10;
pub const FOOD_TEMP_MAX_HP_BONUS: i32 = 2;
pub const SCROLL_MANA_RESTORE: i32 = 5;

pub fn can_unlock_subclass(character: &Character) -> bool {
    character.level >= SUBCLASS_UNLOCK_LEVEL && character.subclass.is_none()
}

pub fn grant_xp(character: &mut Character, amount: u32) -> bool {
    let before = character.level;
    character.xp = character.xp.saturating_add(amount);
    character.level = level_for_xp(character.xp).min(MAX_LEVEL);
    character.level > before
}

pub fn award_talent_points(points: &mut TalentPoints, old_level: u32, new_level: u32) {
    points.0 = points.0.saturating_add(new_level.saturating_sub(old_level));
}

pub fn mana_for_class(class: &ClassData, abilities: Abilities, level: u32) -> Mana {
    let casting = class
        .primary_abilities
        .iter()
        .map(|ability| ability_modifier(ability_score_by_name(abilities, ability)))
        .max()
        .unwrap_or(0);
    let max = (class.base_mana + casting.max(0) + level as i32).max(0);
    Mana { current: max, max }
}

pub fn cooldowns_for_class(class: &ClassData) -> Cooldowns {
    Cooldowns {
        abilities: class
            .ability_cooldowns
            .iter()
            .map(|(ability_id, max)| AbilityCooldown {
                ability_id: ability_id.clone(),
                remaining: 0,
                max: *max,
            })
            .collect(),
    }
}

pub fn inventory_has_room(inventory: &Inventory) -> bool {
    inventory.items.len() < INVENTORY_CAPACITY
}

pub fn add_item_to_inventory(inventory: &mut Inventory, item: ItemInstanceId) -> bool {
    if inventory_has_room(inventory) {
        inventory.items.push(item);
        true
    } else {
        false
    }
}

pub fn buy_price(item: &ItemInstance) -> u32 {
    item.value
}

pub fn sell_price(item: &ItemInstance) -> u32 {
    (item.value / 2).max(1)
}

pub fn can_afford(gold: Gold, price: u32) -> bool {
    gold.0 >= price
}

pub fn apply_revive_cost(
    gold: &mut Gold,
    health: &mut Health,
    penalty: &mut RevivePenalty,
) -> bool {
    let cost = REVIVE_GOLD_COST_BASE.saturating_mul(penalty.stacks + 1);
    if gold.0 < cost {
        return false;
    }
    gold.0 -= cost;
    penalty.stacks += 1;
    health.max = (health.max - REVIVE_MAX_HP_LOSS_PER_STACK).max(1);
    health.current = health.max;
    true
}

pub fn can_reach_rank(attacker_rank: Rank, target_rank: Rank, reach: Reach) -> bool {
    match reach {
        Reach::Melee => attacker_rank.0 <= 1 && target_rank.0 <= 1,
        Reach::Reach => attacker_rank.0 <= 2 && target_rank.0 <= 2,
        Reach::Ranged => target_rank.0 <= 4,
        Reach::Any => true,
    }
}

pub fn reachable_ranks(attacker_rank: Rank, reach: Reach, max_target_rank: u8) -> Vec<Rank> {
    (0..=max_target_rank)
        .map(Rank)
        .filter(|target| can_reach_rank(attacker_rank, *target, reach))
        .collect()
}

pub fn rank_swap(first: &mut Rank, second: &mut Rank) {
    std::mem::swap(first, second);
}

pub fn aoe_targets_by_rank(
    targets: &[RankTarget],
    side: CombatSide,
    center: Rank,
    radius: u8,
) -> Vec<Entity> {
    targets
        .iter()
        .filter(|target| target.side == side)
        .filter(|target| target.rank.0.abs_diff(center.0) <= radius)
        .map(|target| target.entity)
        .collect()
}

pub fn aoe_friendly_fire_risk(
    targets: &[RankTarget],
    friendly_side: CombatSide,
    center: Rank,
    radius: u8,
) -> bool {
    targets
        .iter()
        .any(|target| target.side == friendly_side && target.rank.0.abs_diff(center.0) <= radius)
}

pub fn rarity_rank(rarity: Rarity) -> u8 {
    match rarity {
        Rarity::Common => 0,
        Rarity::Uncommon => 1,
        Rarity::Rare => 2,
        Rarity::Epic => 3,
        Rarity::Legendary => 4,
    }
}

pub fn rarity_frame_color(data: &GameData, rarity: Rarity) -> Option<FrameColor> {
    data.rarities.get(&rarity).map(|row| row.frame_color)
}

pub fn roll_item_instance(
    base: &ItemData,
    data: &GameData,
    instances: &mut ItemInstances,
    rng: &mut ChaCha8Rng,
    level: u32,
) -> ItemInstance {
    let rarity = roll_rarity(data, rng);
    roll_item_instance_with_rarity(base, data, instances, rng, level, rarity)
}

pub fn roll_item_instance_with_rarity(
    base: &ItemData,
    data: &GameData,
    instances: &mut ItemInstances,
    rng: &mut ChaCha8Rng,
    level: u32,
    rarity: Rarity,
) -> ItemInstance {
    instances.next_serial = instances.next_serial.saturating_add(1);
    let affixes = roll_affixes(base, data, rarity, level, rng);
    let affix_value: i32 = affixes.iter().map(|affix| affix.value.abs()).sum();
    let value = ((base.value as f32 * rarity_value_multiplier(rarity)) as u32)
        .saturating_add(affix_value.max(0) as u32 * 5)
        .max(1);
    let instance_id = format!(
        "{}:{}:{}",
        base.id,
        rarity_rank(rarity),
        instances.next_serial
    );
    let instance = ItemInstance {
        instance_id,
        base: base.id.clone(),
        rarity,
        affixes,
        level,
        value,
    };
    instances
        .instances
        .insert(instance.instance_id.clone(), instance.clone());
    instance
}

pub fn roll_loot_instances(
    data: &GameData,
    party_level: u32,
    instances: &mut ItemInstances,
    rng: &mut ChaCha8Rng,
) -> Vec<ItemInstanceId> {
    let mut bases: Vec<_> = data
        .items
        .values()
        .filter(|item| !matches!(item.slot, ItemSlot::Treasure))
        .collect();
    bases.sort_by(|a, b| a.id.cmp(&b.id));
    let count = 1 + usize::from(party_level >= 3);
    let mut rolled = Vec::new();
    for _ in 0..count {
        let Some(base) = bases.choose(rng) else {
            continue;
        };
        rolled.push(roll_item_instance(base, data, instances, rng, party_level).instance_id);
    }
    rolled
}

fn roll_rarity(data: &GameData, rng: &mut ChaCha8Rng) -> Rarity {
    let mut rows: Vec<_> = data.rarities.values().collect();
    rows.sort_by_key(|row| rarity_rank(row.rarity));
    let total_weight: u32 = rows.iter().map(|row| row.weight).sum();
    if rows.is_empty() || total_weight == 0 {
        return Rarity::Common;
    }
    let mut ticket = rng.random_range(0..total_weight);
    for row in rows {
        if ticket < row.weight {
            return row.rarity;
        }
        ticket -= row.weight;
    }
    Rarity::Common
}

fn roll_affixes(
    base: &ItemData,
    data: &GameData,
    rarity: Rarity,
    level: u32,
    rng: &mut ChaCha8Rng,
) -> Vec<Affix> {
    let count = match rarity {
        Rarity::Common => 0,
        Rarity::Uncommon => 1,
        Rarity::Rare => 2,
        Rarity::Epic => 3,
        Rarity::Legendary => 4,
    };
    let mut candidates: Vec<_> = data
        .affixes
        .values()
        .filter(|template| template.min_level <= level)
        .filter(|template| {
            template
                .rarity_min
                .map(|min| rarity_rank(rarity) >= rarity_rank(min))
                .unwrap_or(true)
        })
        .filter(|template| {
            template.allowed_slots.is_empty() || template.allowed_slots.contains(&base.slot)
        })
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    let mut affixes = Vec::new();
    for _ in 0..count {
        let Some(template) = weighted_affix_pick(&candidates, rng) else {
            break;
        };
        let value = if template.min_value <= template.max_value {
            rng.random_range(template.min_value..=template.max_value)
        } else {
            template.min_value
        };
        affixes.push(Affix {
            id: template.id.clone(),
            name: template.name.clone(),
            kind: template.kind,
            value,
        });
    }
    affixes
}

fn weighted_affix_pick<'a>(
    candidates: &'a [&'a AffixTemplate],
    rng: &mut ChaCha8Rng,
) -> Option<&'a AffixTemplate> {
    let total_weight: u32 = candidates.iter().map(|template| template.weight).sum();
    if total_weight == 0 {
        return None;
    }
    let mut ticket = rng.random_range(0..total_weight);
    for template in candidates {
        if ticket < template.weight {
            return Some(template);
        }
        ticket -= template.weight;
    }
    candidates.first().copied()
}

fn rarity_value_multiplier(rarity: Rarity) -> f32 {
    match rarity {
        Rarity::Common => 1.0,
        Rarity::Uncommon => 1.35,
        Rarity::Rare => 1.8,
        Rarity::Epic => 2.6,
        Rarity::Legendary => 4.0,
    }
}

pub fn generate_map(seed: u64, depth: u32) -> MapState {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut nodes = Vec::new();
    let mut next_id = 0;

    for layer in 0..depth {
        let width = if layer == 0 || layer + 1 == depth {
            1
        } else {
            rng.random_range(2..=4)
        };
        for lane in 0..width {
            let node_type = if layer == 0 {
                MapNodeType::Combat
            } else if layer + 1 == depth {
                MapNodeType::Boss
            } else {
                weighted_node_type(&mut rng)
            };
            nodes.push(MapNode {
                id: next_id,
                layer,
                lane,
                node_type,
                next: Vec::new(),
                completed: false,
            });
            next_id += 1;
        }
    }

    for layer in 0..depth.saturating_sub(1) {
        let current: Vec<u32> = nodes
            .iter()
            .filter(|n| n.layer == layer)
            .map(|n| n.id)
            .collect();
        let next: Vec<u32> = nodes
            .iter()
            .filter(|n| n.layer == layer + 1)
            .map(|n| n.id)
            .collect();
        for id in current {
            let edge_count = rng.random_range(1..=next.len().min(2));
            let mut edges = next.clone();
            edges.sort_by_key(|target| target.abs_diff(id));
            let node = nodes.iter_mut().find(|n| n.id == id).expect("node exists");
            node.next = edges.into_iter().take(edge_count).collect();
        }
    }

    MapState {
        seed,
        current_node: Some(0),
        nodes,
    }
}

fn weighted_node_type(rng: &mut ChaCha8Rng) -> MapNodeType {
    match rng.random_range(0..100) {
        0..=42 => MapNodeType::Combat,
        43..=55 => MapNodeType::Elite,
        56..=66 => MapNodeType::Treasure,
        67..=80 => MapNodeType::Event,
        81..=88 => MapNodeType::Shop,
        89..=94 => MapNodeType::Town,
        _ => MapNodeType::Rest,
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MapNodeType {
    Combat,
    Elite,
    Treasure,
    Event,
    Rest,
    Town,
    Shop,
    Boss,
    BonusQuest,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapNode {
    pub id: u32,
    pub layer: u32,
    pub lane: u32,
    pub node_type: MapNodeType,
    pub next: Vec<u32>,
    pub completed: bool,
}

pub fn choose_enemy_archetypes(
    data: &GameData,
    party_level: u32,
    difficulty: MapNodeType,
    rng: &mut ChaCha8Rng,
) -> Vec<EnemyArchetypeId> {
    let count = match difficulty {
        MapNodeType::Boss => 1,
        MapNodeType::Elite => rng.random_range(2..=4),
        _ => rng.random_range(1..=5),
    };
    let mut candidates: Vec<_> = data
        .enemies
        .values()
        .filter(|enemy| enemy.level <= party_level + 2)
        .collect();
    candidates.sort_by_key(|enemy| enemy.level.abs_diff(party_level));
    (0..count)
        .filter_map(|_| candidates.choose(rng).map(|enemy| enemy.id.clone()))
        .collect()
}

pub fn choose_loot(data: &GameData, party_level: u32, rng: &mut ChaCha8Rng) -> Vec<ItemId> {
    let mut ids: Vec<_> = data.items.keys().cloned().collect();
    ids.sort();
    let count = 1 + usize::from(party_level >= 3);
    (0..count)
        .filter_map(|_| ids.choose(rng).cloned())
        .collect()
}

pub fn load_game_data_from_dir(path: impl AsRef<Path>) -> Result<GameData, String> {
    let path = path.as_ref();
    let item_catalog = load_item_catalog(path.join("items.ron"))?;
    Ok(GameData {
        races: load_table(path.join("races.ron"))?,
        classes: load_table(path.join("classes.ron"))?,
        skills: load_table(path.join("skills.ron"))?,
        traits: load_table(path.join("traits.ron"))?,
        items: item_catalog
            .bases
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect(),
        affixes: item_catalog
            .affixes
            .into_iter()
            .map(|row| (row.id.clone(), row))
            .collect(),
        rarities: item_catalog
            .rarities
            .into_iter()
            .map(|row| (row.rarity, row))
            .collect(),
        talent_trees: load_talent_trees(path.join("talents.ron"))?,
        enemies: load_table(path.join("enemies.ron"))?,
    })
}

#[derive(Default, Deserialize)]
struct ItemCatalogData {
    bases: Vec<ItemData>,
    affixes: Vec<AffixTemplate>,
    rarities: Vec<RarityData>,
}

fn load_item_catalog(path: impl AsRef<Path>) -> Result<ItemCatalogData, String> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|error| format!("{}: {error}", path.as_ref().display()))?;
    match ron::from_str::<ItemCatalogData>(&text) {
        Ok(catalog) => Ok(catalog),
        Err(catalog_error) => match ron::from_str::<Vec<ItemData>>(&text) {
            Ok(bases) => Ok(legacy_item_catalog(bases)),
            Err(legacy_error) => Err(format!(
                "{}: catalog parse failed: {catalog_error}; legacy parse failed: {legacy_error}",
                path.as_ref().display()
            )),
        },
    }
}

fn legacy_item_catalog(bases: Vec<ItemData>) -> ItemCatalogData {
    ItemCatalogData {
        bases,
        affixes: Vec::new(),
        rarities: default_rarity_rows(),
    }
}

fn load_talent_trees(path: impl AsRef<Path>) -> Result<HashMap<ClassId, TalentTreeData>, String> {
    if !path.as_ref().exists() {
        return Ok(HashMap::new());
    }
    let text = fs::read_to_string(path.as_ref())
        .map_err(|error| format!("{}: {error}", path.as_ref().display()))?;
    let rows: Vec<TalentTreeData> =
        ron::from_str(&text).map_err(|error| format!("{}: {error}", path.as_ref().display()))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.class.clone(), row))
        .collect())
}

fn default_rarity_rows() -> Vec<RarityData> {
    vec![
        RarityData {
            rarity: Rarity::Common,
            name: "Common".into(),
            frame_color: FrameColor {
                r: 180,
                g: 180,
                b: 170,
                a: 255,
            },
            weight: 700,
        },
        RarityData {
            rarity: Rarity::Uncommon,
            name: "Uncommon".into(),
            frame_color: FrameColor {
                r: 82,
                g: 170,
                b: 93,
                a: 255,
            },
            weight: 220,
        },
        RarityData {
            rarity: Rarity::Rare,
            name: "Rare".into(),
            frame_color: FrameColor {
                r: 70,
                g: 132,
                b: 214,
                a: 255,
            },
            weight: 70,
        },
        RarityData {
            rarity: Rarity::Epic,
            name: "Epic".into(),
            frame_color: FrameColor {
                r: 168,
                g: 86,
                b: 198,
                a: 255,
            },
            weight: 9,
        },
        RarityData {
            rarity: Rarity::Legendary,
            name: "Legendary".into(),
            frame_color: FrameColor {
                r: 226,
                g: 153,
                b: 43,
                a: 255,
            },
            weight: 1,
        },
    ]
}

fn load_table<T>(path: impl AsRef<Path>) -> Result<HashMap<String, T>, String>
where
    T: for<'de> Deserialize<'de> + HasId,
{
    let text = fs::read_to_string(path.as_ref())
        .map_err(|error| format!("{}: {error}", path.as_ref().display()))?;
    let rows: Vec<T> =
        ron::from_str(&text).map_err(|error| format!("{}: {error}", path.as_ref().display()))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.id().to_string(), row))
        .collect())
}

pub trait HasId {
    fn id(&self) -> &str;
}

impl HasId for RaceData {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for ClassData {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for SkillData {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for TraitData {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for ItemData {
    fn id(&self) -> &str {
        &self.id
    }
}
impl HasId for EnemyArchetypeData {
    fn id(&self) -> &str {
        &self.id
    }
}

pub fn serialize_save(save: &SaveGame) -> Result<String, ron::Error> {
    ron::ser::to_string_pretty(save, ron::ser::PrettyConfig::default())
}

pub fn deserialize_save(text: &str) -> Result<SaveGame, ron::error::SpannedError> {
    ron::from_str(text)
}

pub fn validate_game_data(data: &GameData) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if data.races.len() < 6 {
        errors.push(format!(
            "expected at least 6 races, found {}",
            data.races.len()
        ));
    }
    if data.classes.len() < 6 {
        errors.push(format!(
            "expected at least 6 classes, found {}",
            data.classes.len()
        ));
    }
    if data.skills.len() < 18 {
        errors.push(format!(
            "expected at least 18 skills, found {}",
            data.skills.len()
        ));
    }
    if data.enemies.len() < 8 {
        errors.push(format!(
            "expected at least 8 enemy archetypes, found {}",
            data.enemies.len()
        ));
    }
    if data.rarities.len() < 5 {
        errors.push(format!(
            "expected 5 rarity rows, found {}",
            data.rarities.len()
        ));
    }
    if data.affixes.len() < 8 {
        errors.push(format!(
            "expected at least 8 affix templates, found {}",
            data.affixes.len()
        ));
    }
    if data.talent_trees.len() < data.classes.len() {
        errors.push(format!(
            "expected talent trees for all classes, found {} for {} classes",
            data.talent_trees.len(),
            data.classes.len()
        ));
    }

    for (id, race) in &data.races {
        if id != &race.id {
            errors.push(format!("race key {id} does not match row id {}", race.id));
        }
        if race.name.trim().is_empty() || race.description.trim().is_empty() {
            errors.push(format!("race {id} is missing name or description"));
        }
        if race.speed <= 0 {
            errors.push(format!("race {id} has non-positive speed"));
        }
        if race.sprite_key.trim().is_empty() {
            errors.push(format!("race {id} has empty sprite_key"));
        }
        for trait_id in &race.traits {
            if !data.traits.contains_key(trait_id) {
                errors.push(format!("race {id} references missing trait {trait_id}"));
            }
        }
    }

    for (id, class) in &data.classes {
        if id != &class.id {
            errors.push(format!("class key {id} does not match row id {}", class.id));
        }
        if class.name.trim().is_empty() || class.description.trim().is_empty() {
            errors.push(format!("class {id} is missing name or description"));
        }
        if class.hit_die == 0 {
            errors.push(format!("class {id} has invalid hit die"));
        }
        if class.primary_abilities.is_empty() {
            errors.push(format!("class {id} has no primary abilities"));
        }
        if class.class_abilities.is_empty() {
            errors.push(format!("class {id} has no class abilities"));
        }
        if class.base_mana < 0 {
            errors.push(format!("class {id} has negative base_mana"));
        }
        for skill_id in &class.skill_choices {
            if !data.skills.contains_key(skill_id) {
                errors.push(format!("class {id} references missing skill {skill_id}"));
            }
        }
        for item_id in &class.starting_kit {
            if !data.items.contains_key(item_id) {
                errors.push(format!(
                    "class {id} references missing starting item {item_id}"
                ));
            }
        }
        for ability in class
            .primary_abilities
            .iter()
            .chain(class.saving_throws.iter())
        {
            if !is_valid_ability_name(ability) {
                errors.push(format!("class {id} references invalid ability {ability}"));
            }
        }
    }

    for (id, skill) in &data.skills {
        if id != &skill.id {
            errors.push(format!("skill key {id} does not match row id {}", skill.id));
        }
        if !is_valid_ability_name(&skill.ability) {
            errors.push(format!(
                "skill {id} references invalid ability {}",
                skill.ability
            ));
        }
    }

    for (id, item) in &data.items {
        if id != &item.id {
            errors.push(format!("item key {id} does not match row id {}", item.id));
        }
        if item.name.trim().is_empty() || item.description.trim().is_empty() {
            errors.push(format!("item {id} is missing name or description"));
        }
        if item.sprite_key.trim().is_empty() {
            errors.push(format!("item {id} has empty sprite_key"));
        }
        if let Some(damage) = &item.damage {
            validate_dice_expr(damage, &mut errors, format!("item {id} damage"));
        }
        if item.consumable.is_some() && item.slot != ItemSlot::Consumable {
            errors.push(format!(
                "item {id} is consumable but not in Consumable slot"
            ));
        }
    }

    for (rarity, row) in &data.rarities {
        if rarity != &row.rarity {
            errors.push(format!("rarity key {rarity:?} does not match row"));
        }
        if row.weight == 0 {
            errors.push(format!("rarity {rarity:?} has zero weight"));
        }
    }

    for (id, affix) in &data.affixes {
        if id != &affix.id {
            errors.push(format!("affix key {id} does not match row id {}", affix.id));
        }
        if affix.name.trim().is_empty() {
            errors.push(format!("affix {id} is missing name"));
        }
        if affix.weight == 0 {
            errors.push(format!("affix {id} has zero weight"));
        }
        if affix.min_value > affix.max_value {
            errors.push(format!("affix {id} has inverted value range"));
        }
    }

    for (class_id, tree) in &data.talent_trees {
        if !data.classes.contains_key(class_id) {
            errors.push(format!("talent tree references missing class {class_id}"));
        }
        if tree.nodes.is_empty() {
            errors.push(format!("talent tree {class_id} has no nodes"));
        }
        for node in &tree.nodes {
            if node.id.trim().is_empty() || node.name.trim().is_empty() {
                errors.push(format!("talent tree {class_id} has unnamed node"));
            }
            if node.cost == 0 {
                errors.push(format!("talent {} has zero cost", node.id));
            }
            for required in &node.requires {
                if !tree.nodes.iter().any(|candidate| &candidate.id == required) {
                    errors.push(format!(
                        "talent {} requires missing node {required}",
                        node.id
                    ));
                }
            }
        }
    }

    for (id, enemy) in &data.enemies {
        if id != &enemy.id {
            errors.push(format!("enemy key {id} does not match row id {}", enemy.id));
        }
        if enemy.name.trim().is_empty() || enemy.description.trim().is_empty() {
            errors.push(format!("enemy {id} is missing name or description"));
        }
        if enemy.level == 0 {
            errors.push(format!("enemy {id} has zero level"));
        }
        if enemy.armor_class <= 0 || enemy.hit_points <= 0 {
            errors.push(format!("enemy {id} has invalid combat stats"));
        }
        if enemy.sprite_key.trim().is_empty() {
            errors.push(format!("enemy {id} has empty sprite_key"));
        }
        validate_dice_expr(&enemy.damage, &mut errors, format!("enemy {id} damage"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_dice_expr(expr: &DiceExpr, errors: &mut Vec<String>, label: String) {
    if expr.count == 0 || expr.sides == 0 {
        errors.push(format!("{label} has non-positive dice"));
    }
}

fn is_valid_ability_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "str"
            | "str_"
            | "strength"
            | "dex"
            | "dexterity"
            | "con"
            | "constitution"
            | "int"
            | "intelligence"
            | "wis"
            | "wisdom"
            | "cha"
            | "charisma"
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaveGame {
    #[serde(default)]
    pub metadata: CampaignMetadata,
    pub seed: u64,
    #[serde(default)]
    pub difficulty: Difficulty,
    #[serde(default)]
    pub antagonist: Antagonist,
    #[serde(default)]
    pub planned_companions: PlannedCompanions,
    pub party: Vec<SavedCharacter>,
    pub map: MapState,
    pub inventory: Vec<ItemInstanceId>,
    #[serde(default)]
    pub item_instances: HashMap<ItemInstanceId, ItemInstance>,
    #[serde(default)]
    pub gold: u32,
    #[serde(default)]
    pub autosave: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignMetadata {
    pub slot: CampaignSlotId,
    pub name: String,
    pub seed: u64,
    pub difficulty: Difficulty,
    pub autosave: bool,
    pub progress_label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedCharacter {
    pub name: String,
    pub race: RaceId,
    pub class: ClassId,
    #[serde(default)]
    pub subclass: Option<ClassId>,
    pub level: u32,
    pub xp: u32,
    pub abilities: Abilities,
    pub health_current: i32,
    #[serde(default)]
    pub mana_current: i32,
    pub equipment: SavedEquipment,
    pub skills: Vec<SkillId>,
    pub traits: Vec<TraitId>,
    #[serde(default)]
    pub talents: Vec<TalentId>,
    #[serde(default)]
    pub talent_points: u32,
    #[serde(default)]
    pub revive_penalty_stacks: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedEquipment {
    pub head: Option<ItemInstanceId>,
    pub body: Option<ItemInstanceId>,
    pub main_hand: Option<ItemInstanceId>,
    pub off_hand: Option<ItemInstanceId>,
    pub feet: Option<ItemInstanceId>,
}

impl From<Equipment> for SavedEquipment {
    fn from(value: Equipment) -> Self {
        Self {
            head: value.head,
            body: value.body,
            main_hand: value.main_hand,
            off_hand: value.off_hand,
            feet: value.feet,
        }
    }
}

impl From<SavedEquipment> for Equipment {
    fn from(value: SavedEquipment) -> Self {
        Self {
            head: value.head,
            body: value.body,
            main_hand: value.main_hand,
            off_hand: value.off_hand,
            feet: value.feet,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_new_game_requests(
    mut commands: Commands,
    mut requests: EventReader<NewGameRequested>,
    mut campaign_seed: ResMut<CampaignSeed>,
    mut rng: ResMut<GameRng>,
    mut map: ResMut<MapState>,
    mut party: ResMut<PartyRoster>,
    mut inventory: ResMut<Inventory>,
    mut instances: ResMut<ItemInstances>,
    mut gold: ResMut<Gold>,
    mut antagonist: ResMut<Antagonist>,
    mut encounter: ResMut<EncounterState>,
    mut next_state: ResMut<NextState<GameState>>,
    mut next_step: ResMut<NextState<CreationStep>>,
) {
    for request in requests.read() {
        campaign_seed.0 = request.seed;
        rng.0 = ChaCha8Rng::seed_from_u64(request.seed);
        *map = generate_map(request.seed, 10);
        for entity in party.members.drain(..) {
            commands.entity(entity).despawn();
        }
        inventory.items.clear();
        instances.instances.clear();
        instances.next_serial = 0;
        gold.0 = 0;
        *antagonist = Antagonist::generate(request.seed);
        for entity in encounter.enemies.drain(..) {
            commands.entity(entity).despawn();
        }
        encounter.turn_order.clear();
        encounter.turn_index = 0;
        encounter.surrendered = false;
        next_state.set(GameState::CharacterCreation);
        next_step.set(CreationStep::Race);
    }
}

fn handle_creation_step_advance_requests(
    mut requests: EventReader<CreationStepAdvanceRequested>,
    step: Option<Res<State<CreationStep>>>,
    mut next_step: ResMut<NextState<CreationStep>>,
) {
    for _ in requests.read() {
        let Some(step) = step.as_ref() else { continue };
        let next = match step.get() {
            CreationStep::Race => CreationStep::Class,
            CreationStep::Class => CreationStep::AbilityRoll,
            CreationStep::AbilityRoll => CreationStep::SkillsTraits,
            CreationStep::SkillsTraits => CreationStep::Review,
            CreationStep::Review => CreationStep::Companions,
            CreationStep::Companions => CreationStep::Race,
        };
        next_step.set(next);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_character_build_requests(
    mut commands: Commands,
    mut requests: EventReader<CharacterBuildRequested>,
    data: Res<GameData>,
    mut party: ResMut<PartyRoster>,
    mut inventory: ResMut<Inventory>,
    mut instances: ResMut<ItemInstances>,
    mut rng: ResMut<GameRng>,
    mut finalized: EventWriter<CharacterFinalized>,
    mut equipment_changed: EventWriter<EquipmentChanged>,
    mut inventory_changed: EventWriter<InventoryChanged>,
) {
    for request in requests.read() {
        if party.members.len() >= 4 {
            continue;
        }
        let (Some(race), Some(class)) = (
            data.races.get(&request.race),
            data.classes.get(&request.class),
        ) else {
            continue;
        };
        let mut traits = race.traits.clone();
        for trait_id in &request.traits {
            if !traits.contains(trait_id) {
                traits.push(trait_id.clone());
            }
        }
        let (character, abilities, derived, health, skills, traits, mut equipment, sprite) =
            finalize_character_bundle(
                request.name.clone(),
                race,
                class,
                request.abilities,
                request.skills.clone(),
                traits,
                &data,
            );
        instance_starting_equipment(
            class,
            &data,
            &mut instances,
            &mut rng.0,
            &mut equipment,
            &mut inventory,
        );
        let mana = mana_for_class(class, abilities, character.level);
        let cooldowns = cooldowns_for_class(class);
        let is_pc = party.members.is_empty();
        let entity = commands
            .spawn((
                character,
                abilities,
                derived,
                health,
                mana,
                cooldowns,
                skills,
                traits,
                Talents::default(),
                TalentPoints::default(),
                RevivePenalty::default(),
                equipment,
                sprite,
                PartyMember {
                    slot: party.members.len() as u8,
                },
            ))
            .id();
        if is_pc {
            commands.entity(entity).insert(PlayerCharacter);
        }
        party.members.push(entity);
        finalized.write(CharacterFinalized { entity });
        equipment_changed.write(EquipmentChanged { entity });
        inventory_changed.write(InventoryChanged);
    }
}

fn handle_finish_party_creation_requests(
    mut requests: EventReader<FinishPartyCreationRequested>,
    party: Res<PartyRoster>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for _ in requests.read() {
        if !party.members.is_empty() {
            next_state.set(GameState::Exploration);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_encounter_requests(
    mut commands: Commands,
    mut requests: EventReader<EncounterRequested>,
    data: Res<GameData>,
    party: Res<PartyRoster>,
    mut rng: ResMut<GameRng>,
    mut encounter: ResMut<EncounterState>,
    party_derived: Query<&Derived, With<PartyMember>>,
    mut started: EventWriter<EncounterStarted>,
    mut next_state: ResMut<NextState<GameState>>,
    difficulty: Res<GameDifficulty>,
    tuning: Res<DifficultyTuning>,
) {
    for request in requests.read() {
        if party.members.is_empty() || data.enemies.is_empty() {
            continue;
        }
        let party_level = 1;
        let enemy_ids = choose_enemy_archetypes(&data, party_level, request.difficulty, &mut rng.0);
        if enemy_ids.is_empty() {
            continue;
        }
        encounter.surrendered = false;
        begin_encounter(
            &mut commands,
            &data,
            &enemy_ids,
            &mut encounter,
            &mut started,
            difficulty.0,
            *tuning,
        );

        let mut combatants = Vec::new();
        let mut initiatives = Vec::new();
        for entity in &party.members {
            let initiative_mod = party_derived
                .get(*entity)
                .map_or(0, |derived| derived.initiative_mod);
            let roll = rng.0.random_range(1..=20) + initiative_mod;
            commands.entity(*entity).insert(Initiative(roll));
            combatants.push(*entity);
            initiatives.push((*entity, roll));
        }
        for (entity, enemy_id) in encounter.enemies.iter().copied().zip(enemy_ids.iter()) {
            let initiative_mod = data
                .enemies
                .get(enemy_id)
                .map_or(0, |enemy| initiative_modifier(enemy.abilities));
            let roll = rng.0.random_range(1..=20) + initiative_mod;
            commands.entity(entity).insert(Initiative(roll));
            combatants.push(entity);
            initiatives.push((entity, roll));
        }
        build_turn_order(&mut commands, &combatants, &initiatives, &mut encounter);
        next_state.set(GameState::Encounter);
    }
}

fn handle_encounter_ended_state(
    mut commands: Commands,
    mut ended: EventReader<EncounterEnded>,
    mut encounter: ResMut<EncounterState>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for event in ended.read() {
        let surrendered = encounter.surrendered;
        for entity in encounter.enemies.drain(..) {
            commands.entity(entity).despawn();
        }
        encounter.enemies.clear();
        encounter.turn_order.clear();
        encounter.turn_index = 0;
        encounter.surrendered = false;
        next_state.set(if event.victory || surrendered {
            GameState::Exploration
        } else {
            GameState::GameOver
        });
    }
}

fn load_game_data_system(mut commands: Commands) {
    let manifest_assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
    if let Ok(data) =
        load_game_data_from_dir("assets/data").or_else(|_| load_game_data_from_dir(manifest_assets))
    {
        commands.insert_resource(data);
    }
}

fn resolve_roll_requests(
    mut requests: EventReader<RollRequest>,
    mut resolved: EventWriter<RollResolved>,
    mut rng: ResMut<GameRng>,
    mut forced: ResMut<DebugDiceOverride>,
    difficulty: Res<GameDifficulty>,
    tuning: Res<DifficultyTuning>,
    party_members: Query<(), With<PartyMember>>,
) {
    for request in requests.read() {
        let parts = if let Some(forced) = forced.next.take() {
            forced_roll_parts(&request.expr, request.kind, forced)
        } else if request.kind == RollKind::AbilityScoreGen {
            roll_ability_score_gen(&mut rng.0)
        } else {
            let is_player = request
                .source
                .map(|entity| party_members.get(entity).is_ok())
                .unwrap_or(false);
            roll_dice_with_difficulty(
                &request.expr,
                request.advantage,
                &mut rng.0,
                difficulty.0,
                is_player,
                *tuning,
            )
        };
        resolved.write(RollResolved {
            id: request.id,
            rolls: parts.rolls,
            total: parts.total,
            is_nat20: parts.is_nat20,
            is_nat1: parts.is_nat1,
            kind: request.kind,
        });
    }
}

type CombatantActionQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Abilities,
        Option<&'static Equipment>,
        Option<&'static Character>,
        Option<&'static EnemyUnit>,
    ),
>;

fn request_combat_actions(
    mut actions: EventReader<CombatActionRequest>,
    mut requests: EventWriter<RollRequest>,
    mut pending: ResMut<PendingRolls>,
    mut rng: ResMut<GameRng>,
    combatants: CombatantActionQuery,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
) {
    for action in actions.read() {
        if action.action != CombatAction::Attack {
            continue;
        }
        let Ok((abilities, equipment, character, enemy_unit)) = combatants.get(action.actor) else {
            continue;
        };
        let (attack_bonus, damage) = if let Some(enemy_unit) = enemy_unit {
            if let Some(archetype) = data.enemies.get(&enemy_unit.archetype) {
                (archetype.attack_bonus, archetype.damage.clone())
            } else {
                (
                    proficiency_bonus(1),
                    DiceExpr {
                        count: 1,
                        sides: 4,
                        modifier: 0,
                    },
                )
            }
        } else {
            let level = character.map_or(1, |character| character.level);
            let ability_mod = ability_modifier(abilities.str_.max(abilities.dex));
            let attack_bonus = ability_mod + proficiency_bonus(level);
            let damage = equipment
                .and_then(|equipment| equipment.main_hand.as_ref())
                .and_then(|item_id| base_item_for_instance(item_id, &data, &instances))
                .and_then(|item| item.damage.clone())
                .unwrap_or(DiceExpr {
                    count: 1,
                    sides: 4,
                    modifier: ability_mod,
                });
            (attack_bonus, damage)
        };
        let id = rng.0.random::<u64>();
        pending.attack_intents.insert(
            id,
            PendingAttackIntent {
                attacker: action.actor,
                target: action.target,
                damage,
            },
        );
        requests.write(RollRequest {
            id,
            expr: DiceExpr {
                count: 1,
                sides: 20,
                modifier: attack_bonus,
            },
            kind: RollKind::Attack,
            source: Some(action.actor),
            advantage: AdvState::Normal,
        });
    }
}

fn handle_surrender_requests(
    mut requests: EventReader<SurrenderRequested>,
    mut encounter: ResMut<EncounterState>,
    mut ended: EventWriter<EncounterEnded>,
) {
    for _ in requests.read() {
        encounter.surrendered = true;
        ended.write(EncounterEnded { victory: false });
    }
}

fn handle_shop_transactions(
    mut requests: EventReader<ShopTransactionRequested>,
    mut gold: ResMut<Gold>,
    mut inventory: ResMut<Inventory>,
    instances: Res<ItemInstances>,
    mut changed: EventWriter<InventoryChanged>,
) {
    for request in requests.read() {
        let Some(item) = instances.instances.get(&request.item) else {
            continue;
        };
        match request.transaction {
            ShopTransaction::Buy => {
                let price = buy_price(item);
                if can_afford(*gold, price)
                    && add_item_to_inventory(&mut inventory, request.item.clone())
                {
                    gold.0 -= price;
                    changed.write(InventoryChanged);
                }
            }
            ShopTransaction::Sell => {
                let Some(index) = inventory.items.iter().position(|id| id == &request.item) else {
                    continue;
                };
                inventory.items.remove(index);
                gold.0 = gold.0.saturating_add(sell_price(item));
                changed.write(InventoryChanged);
            }
        }
    }
}

fn handle_consumable_use_requests(
    mut requests: EventReader<ConsumableUseRequested>,
    mut inventory: ResMut<Inventory>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    mut actors: Query<(&mut Health, Option<&mut Mana>)>,
    mut changed: EventWriter<InventoryChanged>,
) {
    for request in requests.read() {
        let Some(index) = inventory.items.iter().position(|id| id == &request.item) else {
            continue;
        };
        let Some(base) = base_item_for_instance(&request.item, &data, &instances) else {
            continue;
        };
        let Some(category) = base.consumable else {
            continue;
        };
        let Ok((mut health, mana)) = actors.get_mut(request.actor) else {
            continue;
        };
        match category {
            ConsumableCategory::Potion => {
                health.current = (health.current + POTION_HEAL_AMOUNT).min(health.max);
            }
            ConsumableCategory::Food => {
                health.max = health.max.saturating_add(FOOD_TEMP_MAX_HP_BONUS);
                health.current = health.current.saturating_add(FOOD_TEMP_MAX_HP_BONUS);
            }
            ConsumableCategory::Scroll => {
                if let Some(mut mana) = mana {
                    mana.current = (mana.current + SCROLL_MANA_RESTORE).min(mana.max);
                }
            }
        }
        inventory.items.remove(index);
        changed.write(InventoryChanged);
    }
}

fn handle_revive_attempts(
    mut commands: Commands,
    mut requests: EventReader<ReviveAttempt>,
    mut gold: ResMut<Gold>,
    mut query: Query<(&mut Health, &mut RevivePenalty), With<PlayerCharacter>>,
) {
    for request in requests.read() {
        let Ok((mut health, mut penalty)) = query.get_mut(request.entity) else {
            continue;
        };
        if apply_revive_cost(&mut gold, &mut health, &mut penalty) {
            commands.entity(request.entity).remove::<Downed>();
        }
    }
}

fn capture_attack_roll_results(
    mut resolved: EventReader<RollResolved>,
    mut pending: ResMut<PendingRolls>,
) {
    for roll in resolved.read() {
        if roll.kind != RollKind::Attack {
            continue;
        }
        let Some(intent) = pending.attack_intents.remove(&roll.id) else {
            continue;
        };
        pending.attacks.insert(
            roll.id,
            PendingAttack {
                attacker: intent.attacker,
                target: intent.target,
                attack_total: roll.total,
                damage: intent.damage,
                is_crit: roll.is_nat20,
            },
        );
    }
}

fn complete_pending_roll_actions(
    mut completed: EventReader<RollAnimationComplete>,
    mut pending: ResMut<PendingRolls>,
    mut health: Query<(&mut Health, Option<&Derived>)>,
    mut damage_events: EventWriter<DamageDealt>,
) {
    for event in completed.read() {
        let Some(action) = pending.attacks.remove(&event.id) else {
            continue;
        };
        let Ok((mut target_health, derived)) = health.get_mut(action.target) else {
            continue;
        };
        let target_ac = derived.map_or(10, |derived| derived.armor_class);
        if attack_hits(action.attack_total, target_ac, action.is_crit, false) {
            let mut rng = ChaCha8Rng::seed_from_u64(event.id ^ 0xDADA_600D);
            let parts = roll_dice(&action.damage, AdvState::Normal, &mut rng);
            let amount = total_damage(parts.total, action.is_crit, &action.damage);
            target_health.current = (target_health.current - amount).max(0);
            damage_events.write(DamageDealt {
                target: action.target,
                amount,
                is_crit: action.is_crit,
            });
        }
    }
}

fn detect_dead_units(
    mut died: EventWriter<UnitDied>,
    query: Query<(Entity, &Health), Changed<Health>>,
) {
    for (entity, health) in &query {
        if health.current <= 0 {
            died.write(UnitDied { entity });
        }
    }
}

fn handle_unit_death_outcomes(
    mut commands: Commands,
    mut died: EventReader<UnitDied>,
    party: Query<(Option<&PlayerCharacter>, Option<&PartyMember>)>,
) {
    for event in died.read() {
        let Ok((pc, member)) = party.get(event.entity) else {
            continue;
        };
        if pc.is_some() {
            commands.entity(event.entity).insert(Downed);
        } else if member.is_some() {
            commands.entity(event.entity).despawn();
        }
    }
}

type EncounterResetQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Health,
        Option<&'static Derived>,
        Option<&'static mut Mana>,
        Option<&'static mut Cooldowns>,
    ),
>;

fn reset_between_encounters(
    mut ended: EventReader<EncounterEnded>,
    mut party: EncounterResetQuery,
) {
    if ended.read().next().is_none() {
        return;
    }
    for (mut health, derived, mana, cooldowns) in &mut party {
        let max_hp = derived.map_or(health.max, |derived| derived.max_hp);
        health.max = max_hp;
        health.current = max_hp;
        if let Some(mut mana) = mana {
            mana.current = mana.max;
        }
        if let Some(mut cooldowns) = cooldowns {
            for ability in &mut cooldowns.abilities {
                ability.remaining = 0;
            }
        }
    }
}

fn detect_encounter_end(
    party: Res<PartyRoster>,
    encounter: Res<EncounterState>,
    health: Query<&Health>,
    mut ended: EventWriter<EncounterEnded>,
) {
    if encounter.enemies.is_empty() {
        return;
    }
    let party_alive = party
        .members
        .iter()
        .any(|entity| health.get(*entity).is_ok_and(|hp| hp.current > 0));
    let enemies_alive = encounter
        .enemies
        .iter()
        .any(|entity| health.get(*entity).is_ok_and(|hp| hp.current > 0));
    if !enemies_alive {
        ended.write(EncounterEnded { victory: true });
    } else if !party_alive {
        ended.write(EncounterEnded { victory: false });
    }
}

fn enforce_roster_caps(mut party: ResMut<PartyRoster>, mut encounter: ResMut<EncounterState>) {
    party.members.truncate(4);
    encounter.enemies.truncate(5);
}

pub fn begin_encounter(
    commands: &mut Commands,
    data: &GameData,
    enemy_ids: &[EnemyArchetypeId],
    encounter: &mut EncounterState,
    started: &mut EventWriter<EncounterStarted>,
    difficulty: Difficulty,
    tuning: DifficultyTuning,
) {
    encounter.enemies.clear();
    encounter.turn_order.clear();
    encounter.turn_index = 0;
    encounter.surrendered = false;
    let enemy_multiplier = enemy_tuning_multiplier(difficulty, tuning);

    for (slot, id) in enemy_ids.iter().take(5).enumerate() {
        let Some(enemy) = data.enemies.get(id) else {
            continue;
        };
        let tuned_hp = ((enemy.hit_points as f32) * enemy_multiplier)
            .round()
            .max(1.0) as i32;
        let tuned_ac = ((enemy.armor_class as f32) * enemy_multiplier)
            .round()
            .max(1.0) as i32;
        let entity = commands
            .spawn((
                EnemyUnit {
                    archetype: id.clone(),
                    slot: slot as u8,
                },
                enemy.abilities,
                Derived {
                    armor_class: tuned_ac,
                    max_hp: tuned_hp,
                    initiative_mod: initiative_modifier(enemy.abilities),
                    proficiency: proficiency_bonus(enemy.level),
                    speed: 30,
                },
                Health {
                    current: tuned_hp,
                    max: tuned_hp,
                },
                SpriteParts {
                    base_body: enemy.sprite_key.clone(),
                },
                Equipment::default(),
            ))
            .id();
        encounter.enemies.push(entity);
    }

    started.write(EncounterStarted {
        enemies: encounter.enemies.clone(),
    });
}

pub fn build_turn_order(
    commands: &mut Commands,
    combatants: &[Entity],
    initiatives: &[(Entity, i32)],
    encounter: &mut EncounterState,
) {
    let mut order = initiatives.to_vec();
    order.sort_by_key(|(_, initiative)| -initiative);
    encounter.turn_order = order.into_iter().map(|(entity, _)| entity).collect();
    encounter.turn_index = 0;

    for entity in combatants {
        commands.entity(*entity).remove::<ActiveTurn>();
    }
    if let Some(first) = encounter.turn_order.first().copied() {
        commands.entity(first).insert(ActiveTurn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/data")
    }

    fn fixture_data() -> GameData {
        let data = load_game_data_from_dir(data_dir()).expect("assets/data should parse");
        validate_game_data(&data).unwrap_or_else(|errors| panic!("invalid game data: {errors:#?}"));
        data
    }

    macro_rules! parse_ok_case {
        ($name:ident, $text:literal, $count:expr, $sides:expr, $modifier:expr) => {
            #[test]
            fn $name() {
                assert_eq!(
                    parse_dice_expr($text).unwrap(),
                    DiceExpr {
                        count: $count,
                        sides: $sides,
                        modifier: $modifier
                    }
                );
            }
        };
    }

    macro_rules! parse_err_case {
        ($name:ident, $text:literal) => {
            #[test]
            fn $name() {
                assert!(parse_dice_expr($text).is_err());
            }
        };
    }

    macro_rules! ability_mod_case {
        ($name:ident, $score:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(ability_modifier($score), $expected);
            }
        };
    }

    macro_rules! proficiency_case {
        ($name:ident, $level:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(proficiency_bonus($level), $expected);
            }
        };
    }

    macro_rules! point_buy_case {
        ($name:ident, $score:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(point_buy_cost($score), $expected);
            }
        };
    }

    macro_rules! level_case {
        ($name:ident, $xp:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(level_for_xp($xp), $expected);
            }
        };
    }

    macro_rules! attack_case {
        ($name:ident, $total:expr, $ac:expr, $nat20:expr, $nat1:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(attack_hits($total, $ac, $nat20, $nat1), $expected);
            }
        };
    }

    macro_rules! xp_threshold_case {
        ($name:ident, $level:expr, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(xp_for_level($level), $expected);
            }
        };
    }

    macro_rules! valid_ability_name_case {
        ($name:ident, $text:literal, $expected:expr) => {
            #[test]
            fn $name() {
                assert_eq!(is_valid_ability_name($text), $expected);
            }
        };
    }

    parse_ok_case!(parses_d20, "d20", 1, 20, 0);
    parse_ok_case!(parses_one_d20, "1d20", 1, 20, 0);
    parse_ok_case!(parses_two_d6_plus_three, "2d6+3", 2, 6, 3);
    parse_ok_case!(parses_uppercase_d, "1D8-1", 1, 8, -1);
    parse_ok_case!(parses_spaces, " 4d10 + 12 ", 4, 10, 12);
    parse_ok_case!(parses_negative_modifier, "3d4-2", 3, 4, -2);
    parse_ok_case!(parses_large_pool, "10d12+0", 10, 12, 0);
    parse_ok_case!(parses_percentile, "1d100", 1, 100, 0);
    parse_ok_case!(parses_healing_die, "2d8+2", 2, 8, 2);
    parse_ok_case!(parses_ability_generation, "4d6", 4, 6, 0);
    parse_err_case!(rejects_zero_count, "0d6");
    parse_err_case!(rejects_zero_sides, "2d0");
    parse_err_case!(rejects_missing_d, "hello");
    parse_err_case!(rejects_bad_count, "xd6");
    parse_err_case!(rejects_bad_sides, "2dx");
    parse_err_case!(rejects_bad_positive_modifier, "2d6+x");
    parse_err_case!(rejects_bad_negative_modifier, "2d6-x");
    parse_err_case!(rejects_empty, "");
    parse_err_case!(rejects_only_d, "d");
    parse_err_case!(rejects_only_modifier, "+3");

    ability_mod_case!(ability_mod_01, 1, -5);
    ability_mod_case!(ability_mod_02, 2, -4);
    ability_mod_case!(ability_mod_03, 3, -4);
    ability_mod_case!(ability_mod_04, 4, -3);
    ability_mod_case!(ability_mod_05, 5, -3);
    ability_mod_case!(ability_mod_06, 6, -2);
    ability_mod_case!(ability_mod_07, 7, -2);
    ability_mod_case!(ability_mod_08, 8, -1);
    ability_mod_case!(ability_mod_09, 9, -1);
    ability_mod_case!(ability_mod_10, 10, 0);
    ability_mod_case!(ability_mod_11, 11, 0);
    ability_mod_case!(ability_mod_12, 12, 1);
    ability_mod_case!(ability_mod_13, 13, 1);
    ability_mod_case!(ability_mod_14, 14, 2);
    ability_mod_case!(ability_mod_15, 15, 2);
    ability_mod_case!(ability_mod_16, 16, 3);
    ability_mod_case!(ability_mod_17, 17, 3);
    ability_mod_case!(ability_mod_18, 18, 4);
    ability_mod_case!(ability_mod_19, 19, 4);
    ability_mod_case!(ability_mod_20, 20, 5);
    ability_mod_case!(ability_mod_21, 21, 5);
    ability_mod_case!(ability_mod_22, 22, 6);
    ability_mod_case!(ability_mod_23, 23, 6);
    ability_mod_case!(ability_mod_24, 24, 7);
    ability_mod_case!(ability_mod_25, 25, 7);
    ability_mod_case!(ability_mod_26, 26, 8);
    ability_mod_case!(ability_mod_27, 27, 8);
    ability_mod_case!(ability_mod_28, 28, 9);
    ability_mod_case!(ability_mod_29, 29, 9);
    ability_mod_case!(ability_mod_30, 30, 10);

    proficiency_case!(proficiency_level_01, 1, 2);
    proficiency_case!(proficiency_level_02, 2, 2);
    proficiency_case!(proficiency_level_03, 3, 2);
    proficiency_case!(proficiency_level_04, 4, 2);
    proficiency_case!(proficiency_level_05, 5, 3);
    proficiency_case!(proficiency_level_06, 6, 3);
    proficiency_case!(proficiency_level_07, 7, 3);
    proficiency_case!(proficiency_level_08, 8, 3);
    proficiency_case!(proficiency_level_09, 9, 4);
    proficiency_case!(proficiency_level_10, 10, 4);
    proficiency_case!(proficiency_level_11, 11, 4);
    proficiency_case!(proficiency_level_12, 12, 4);
    proficiency_case!(proficiency_level_13, 13, 5);
    proficiency_case!(proficiency_level_14, 14, 5);
    proficiency_case!(proficiency_level_15, 15, 5);
    proficiency_case!(proficiency_level_16, 16, 5);
    proficiency_case!(proficiency_level_17, 17, 6);
    proficiency_case!(proficiency_level_18, 18, 6);
    proficiency_case!(proficiency_level_19, 19, 6);
    proficiency_case!(proficiency_level_20, 20, 6);

    point_buy_case!(point_buy_07, 7, None);
    point_buy_case!(point_buy_08, 8, Some(0));
    point_buy_case!(point_buy_09, 9, Some(1));
    point_buy_case!(point_buy_10, 10, Some(2));
    point_buy_case!(point_buy_11, 11, Some(3));
    point_buy_case!(point_buy_12, 12, Some(4));
    point_buy_case!(point_buy_13, 13, Some(5));
    point_buy_case!(point_buy_14, 14, Some(7));
    point_buy_case!(point_buy_15, 15, Some(9));
    point_buy_case!(point_buy_16, 16, None);

    level_case!(level_for_xp_0, 0, 1);
    level_case!(level_for_xp_299, 299, 1);
    level_case!(level_for_xp_300, 300, 2);
    level_case!(level_for_xp_899, 899, 2);
    level_case!(level_for_xp_900, 900, 3);
    level_case!(level_for_xp_2700, 2700, 4);
    level_case!(level_for_xp_6500, 6500, 5);
    level_case!(level_for_xp_14000, 14000, 6);
    level_case!(level_for_xp_23000, 23000, 7);
    level_case!(level_for_xp_34000, 34000, 8);
    level_case!(level_for_xp_48000, 48000, 9);
    level_case!(level_for_xp_64000, 64000, 10);
    level_case!(level_for_xp_84000, 84000, 11);

    attack_case!(attack_misses_below_ac, 14, 15, false, false, false);
    attack_case!(attack_hits_equal_ac, 15, 15, false, false, true);
    attack_case!(attack_hits_above_ac, 16, 15, false, false, true);
    attack_case!(attack_nat20_hits_impossible_ac, 1, 99, true, false, true);
    attack_case!(attack_nat1_misses_easy_ac, 99, 1, false, true, false);
    attack_case!(attack_nat1_beats_nat20_flag, 99, 1, true, true, false);
    attack_case!(attack_negative_total_misses, -1, 10, false, false, false);
    attack_case!(attack_zero_ac_edge_hits, 0, 0, false, false, true);

    xp_threshold_case!(xp_threshold_01, 1, 0);
    xp_threshold_case!(xp_threshold_02, 2, 300);
    xp_threshold_case!(xp_threshold_03, 3, 900);
    xp_threshold_case!(xp_threshold_04, 4, 2700);
    xp_threshold_case!(xp_threshold_05, 5, 6500);
    xp_threshold_case!(xp_threshold_06, 6, 14000);
    xp_threshold_case!(xp_threshold_07, 7, 23000);
    xp_threshold_case!(xp_threshold_08, 8, 34000);
    xp_threshold_case!(xp_threshold_09, 9, 48000);
    xp_threshold_case!(xp_threshold_10, 10, 64000);
    xp_threshold_case!(xp_threshold_11, 11, 84000);
    xp_threshold_case!(xp_threshold_12, 12, 104000);
    xp_threshold_case!(xp_threshold_13, 13, 124000);
    xp_threshold_case!(xp_threshold_14, 14, 144000);
    xp_threshold_case!(xp_threshold_15, 15, 164000);
    xp_threshold_case!(xp_threshold_16, 16, 184000);
    xp_threshold_case!(xp_threshold_17, 17, 204000);
    xp_threshold_case!(xp_threshold_18, 18, 224000);
    xp_threshold_case!(xp_threshold_19, 19, 244000);
    xp_threshold_case!(xp_threshold_20, 20, 264000);

    valid_ability_name_case!(valid_ability_str, "str", true);
    valid_ability_name_case!(valid_ability_strength, "strength", true);
    valid_ability_name_case!(valid_ability_dex, "dex", true);
    valid_ability_name_case!(valid_ability_dexterity, "dexterity", true);
    valid_ability_name_case!(valid_ability_con, "con", true);
    valid_ability_name_case!(valid_ability_constitution, "constitution", true);
    valid_ability_name_case!(valid_ability_int, "int", true);
    valid_ability_name_case!(valid_ability_intelligence, "intelligence", true);
    valid_ability_name_case!(valid_ability_wis, "wis", true);
    valid_ability_name_case!(valid_ability_wisdom, "wisdom", true);
    valid_ability_name_case!(valid_ability_cha, "cha", true);
    valid_ability_name_case!(valid_ability_charisma, "charisma", true);
    valid_ability_name_case!(invalid_ability_empty, "", false);
    valid_ability_name_case!(invalid_ability_luck, "luck", false);
    valid_ability_name_case!(invalid_ability_speed, "speed", false);

    #[test]
    fn loads_and_validates_every_ron_file() {
        let data = fixture_data();
        assert_eq!(data.races.len(), 6);
        assert_eq!(data.classes.len(), 6);
        assert!(data.skills.len() >= 18);
        assert!(data.traits.len() >= 14);
        assert!(data.items.len() >= 14);
        assert!(data.enemies.len() >= 8);
    }

    #[test]
    fn roll_dice_stays_within_bounds_for_common_expressions() {
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        for expr in [
            DiceExpr {
                count: 1,
                sides: 4,
                modifier: 0,
            },
            DiceExpr {
                count: 2,
                sides: 6,
                modifier: 3,
            },
            DiceExpr {
                count: 4,
                sides: 8,
                modifier: -2,
            },
            DiceExpr {
                count: 1,
                sides: 20,
                modifier: 7,
            },
        ] {
            for _ in 0..200 {
                let result = roll_dice(&expr, AdvState::Normal, &mut rng);
                let min = expr.count as i32 + expr.modifier;
                let max = (expr.count * expr.sides) as i32 + expr.modifier;
                assert!((min..=max).contains(&result.total));
                assert_eq!(result.rolls.len(), expr.count as usize);
                assert!(
                    result
                        .rolls
                        .iter()
                        .all(|roll| (1..=expr.sides).contains(roll))
                );
            }
        }
    }

    #[test]
    fn advantage_keeps_higher_d20() {
        for seed in 0..64 {
            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            let first = expected_rng.random_range(1..=20);
            let second = expected_rng.random_range(1..=20);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let result = roll_dice(
                &DiceExpr {
                    count: 1,
                    sides: 20,
                    modifier: 5,
                },
                AdvState::Advantage,
                &mut rng,
            );
            assert_eq!(result.total, first.max(second) as i32 + 5);
            assert_eq!(result.rolls, vec![first, second]);
        }
    }

    #[test]
    fn disadvantage_keeps_lower_d20() {
        for seed in 0..64 {
            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            let first = expected_rng.random_range(1..=20);
            let second = expected_rng.random_range(1..=20);
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let result = roll_dice(
                &DiceExpr {
                    count: 1,
                    sides: 20,
                    modifier: -1,
                },
                AdvState::Disadvantage,
                &mut rng,
            );
            assert_eq!(result.total, first.min(second) as i32 - 1);
            assert_eq!(result.rolls, vec![first, second]);
        }
    }

    #[test]
    fn d20_nat_flags_follow_kept_advantage_die() {
        let mut saw_nat20 = false;
        let mut saw_nat1 = false;
        for seed in 0..20_000 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let result = roll_dice(
                &DiceExpr {
                    count: 1,
                    sides: 20,
                    modifier: 0,
                },
                AdvState::Advantage,
                &mut rng,
            );
            saw_nat20 |= result.is_nat20;
            saw_nat1 |= result.is_nat1;
            if saw_nat20 && saw_nat1 {
                break;
            }
        }
        assert!(saw_nat20);
        assert!(saw_nat1);
    }

    #[test]
    fn non_d20_rolls_do_not_set_nat_flags() {
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        for _ in 0..500 {
            let result = roll_dice(
                &DiceExpr {
                    count: 4,
                    sides: 6,
                    modifier: 0,
                },
                AdvState::Normal,
                &mut rng,
            );
            assert!(!result.is_nat20);
            assert!(!result.is_nat1);
        }
    }

    #[test]
    fn easy_difficulty_adds_twenty_five_points_at_baseline_dc() {
        let tuning = DifficultyTuning::default();
        let normal = d20_success_chance(11, 0, Difficulty::Normal, true, tuning);
        let hard = d20_success_chance(11, 0, Difficulty::Hard, true, tuning);
        let easy = d20_success_chance(11, 0, Difficulty::Easy, true, tuning);
        assert!((normal - 0.50).abs() < f32::EPSILON);
        assert!((hard - 0.50).abs() < f32::EPSILON);
        assert!((easy - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn easy_difficulty_applies_before_nat_flags() {
        let tuning = DifficultyTuning::default();
        assert_eq!(
            apply_difficulty_to_d20(15, Difficulty::Easy, true, tuning),
            20
        );
        assert_eq!(
            apply_difficulty_to_d20(1, Difficulty::Normal, true, tuning),
            1
        );
        assert_eq!(
            apply_difficulty_to_d20(1, Difficulty::Easy, false, tuning),
            1
        );
    }

    #[test]
    fn ability_score_gen_total_drops_lowest_die() {
        for seed in 0..128 {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let result = roll_ability_score_gen(&mut rng);
            let mut sorted = result.rolls.clone();
            sorted.sort_unstable();
            assert_eq!(result.total, sorted[1..].iter().sum::<u32>() as i32);
            assert!((3..=18).contains(&(result.total as u8)));
        }
    }

    #[test]
    fn point_buy_validation_handles_limits() {
        assert!(validate_point_buy([15, 15, 15, 8, 8, 8]));
        assert!(!validate_point_buy([15, 15, 15, 15, 8, 8]));
        assert!(!validate_point_buy([16, 10, 10, 10, 10, 10]));
    }

    #[test]
    fn standard_array_is_expected_5e_array() {
        assert_eq!(standard_array(), [15, 14, 13, 12, 10, 8]);
    }

    #[test]
    fn four_d6_drop_lowest_stays_between_three_and_eighteen() {
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        for _ in 0..500 {
            let score = roll_4d6_drop_lowest(&mut rng);
            assert!((3..=18).contains(&score));
        }
    }

    #[test]
    fn ability_lookup_accepts_common_names() {
        let abilities = Abilities {
            str_: 8,
            dex: 10,
            con: 12,
            int: 14,
            wis: 16,
            cha: 18,
        };
        assert_eq!(ability_score_by_name(abilities, "str"), 8);
        assert_eq!(ability_score_by_name(abilities, "strength"), 8);
        assert_eq!(ability_score_by_name(abilities, "dexterity"), 10);
        assert_eq!(ability_score_by_name(abilities, "constitution"), 12);
        assert_eq!(ability_score_by_name(abilities, "intelligence"), 14);
        assert_eq!(ability_score_by_name(abilities, "wisdom"), 16);
        assert_eq!(ability_score_by_name(abilities, "charisma"), 18);
        assert_eq!(ability_score_by_name(abilities, "unknown"), 10);
    }

    #[test]
    fn skill_bonus_adds_proficiency_when_present() {
        let abilities = Abilities {
            str_: 10,
            dex: 16,
            con: 10,
            int: 10,
            wis: 10,
            cha: 10,
        };
        let skill = SkillData {
            id: "stealth".into(),
            name: "Stealth".into(),
            ability: "dex".into(),
            description: String::new(),
        };
        let proficient = SkillSet {
            proficient: vec!["stealth".into()],
        };
        let untrained = SkillSet::default();
        assert_eq!(skill_bonus(abilities, &skill, &proficient, 5), 6);
        assert_eq!(skill_bonus(abilities, &skill, &untrained, 5), 3);
    }

    #[test]
    fn saving_throw_and_ability_check_bonuses_match() {
        let abilities = Abilities {
            str_: 18,
            dex: 8,
            con: 10,
            int: 12,
            wis: 14,
            cha: 16,
        };
        assert_eq!(ability_check_bonus(abilities, "str", false, 1), 4);
        assert_eq!(ability_check_bonus(abilities, "cha", true, 9), 7);
        assert_eq!(saving_throw_bonus(abilities, "wis", true, 5), 5);
        assert_eq!(saving_throw_bonus(abilities, "dex", false, 5), -1);
    }

    #[test]
    fn armor_class_uses_body_shield_and_dex() {
        let data = fixture_data();
        let abilities = Abilities {
            str_: 10,
            dex: 14,
            con: 10,
            int: 10,
            wis: 10,
            cha: 10,
        };
        let equipment = Equipment {
            body: Some("chain_shirt".into()),
            off_hand: Some("wooden_shield".into()),
            ..default()
        };
        assert_eq!(armor_class(abilities, &equipment, &data), 17);
    }

    #[test]
    fn initiative_is_dex_modifier() {
        assert_eq!(
            initiative_modifier(Abilities {
                str_: 10,
                dex: 8,
                con: 10,
                int: 10,
                wis: 10,
                cha: 10,
            }),
            -1
        );
        assert_eq!(
            initiative_modifier(Abilities {
                str_: 10,
                dex: 18,
                con: 10,
                int: 10,
                wis: 10,
                cha: 10,
            }),
            4
        );
    }

    #[test]
    fn damage_total_clamps_and_adds_minimum_crit_bonus() {
        let dice = DiceExpr {
            count: 2,
            sides: 6,
            modifier: 3,
        };
        assert_eq!(total_damage(8, false, &dice), 8);
        assert_eq!(total_damage(8, true, &dice), 10);
        assert_eq!(total_damage(-2, false, &dice), 0);
    }

    #[test]
    fn map_generation_is_reproducible() {
        assert_eq!(generate_map(42, 8), generate_map(42, 8));
        assert_ne!(generate_map(42, 8), generate_map(43, 8));
    }

    #[test]
    fn map_generation_has_valid_node_graph() {
        let map = generate_map(123, 10);
        assert_eq!(map.current_node, Some(0));
        assert!(
            map.nodes
                .iter()
                .any(|node| node.node_type == MapNodeType::Boss)
        );
        for node in &map.nodes {
            if node.node_type != MapNodeType::Boss {
                assert!(
                    !node.next.is_empty(),
                    "non-terminal node {} has no outgoing path",
                    node.id
                );
            }
            for next in &node.next {
                let target = map
                    .nodes
                    .iter()
                    .find(|candidate| candidate.id == *next)
                    .expect("edge target exists");
                assert_eq!(target.layer, node.layer + 1);
            }
        }
    }

    #[test]
    fn encounter_choices_are_deterministic_and_capped() {
        let data = fixture_data();
        for node_type in [MapNodeType::Combat, MapNodeType::Elite, MapNodeType::Boss] {
            let mut first_rng = ChaCha8Rng::seed_from_u64(77);
            let mut second_rng = ChaCha8Rng::seed_from_u64(77);
            let first = choose_enemy_archetypes(&data, 3, node_type, &mut first_rng);
            let second = choose_enemy_archetypes(&data, 3, node_type, &mut second_rng);
            assert_eq!(first, second);
            assert!((1..=5).contains(&first.len()));
            assert!(first.iter().all(|id| data.enemies.contains_key(id)));
        }
    }

    #[test]
    fn loot_choices_are_deterministic_and_valid() {
        let data = fixture_data();
        let mut first_rng = ChaCha8Rng::seed_from_u64(88);
        let mut second_rng = ChaCha8Rng::seed_from_u64(88);
        let first = choose_loot(&data, 4, &mut first_rng);
        let second = choose_loot(&data, 4, &mut second_rng);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|id| data.items.contains_key(id)));
    }

    #[test]
    fn rolled_item_instances_are_seeded_and_stored() {
        let data = fixture_data();
        let base = data.items.get("iron_sword").unwrap();
        let mut first_store = ItemInstances::default();
        let mut second_store = ItemInstances::default();
        let mut first_rng = ChaCha8Rng::seed_from_u64(909);
        let mut second_rng = ChaCha8Rng::seed_from_u64(909);
        let first = roll_item_instance(base, &data, &mut first_store, &mut first_rng, 5);
        let second = roll_item_instance(base, &data, &mut second_store, &mut second_rng, 5);
        assert_eq!(first, second);
        assert_eq!(first_store.instances.get(&first.instance_id), Some(&first));
        assert_eq!(
            base_item_for_instance(&first.instance_id, &data, &first_store),
            Some(base)
        );
    }

    #[test]
    fn rarity_frame_colors_are_data_driven() {
        let data = fixture_data();
        let color = rarity_frame_color(&data, Rarity::Legendary).unwrap();
        assert_eq!(color.r, 230);
        assert!(data.rarities.values().all(|rarity| rarity.weight > 0));
    }

    #[test]
    fn inventory_cap_accepts_twenty_unequipped_instances() {
        let mut inventory = Inventory::default();
        for index in 0..INVENTORY_CAPACITY {
            assert!(add_item_to_inventory(
                &mut inventory,
                format!("item:{index}")
            ));
        }
        assert!(!add_item_to_inventory(&mut inventory, "overflow".into()));
        assert_eq!(inventory.items.len(), INVENTORY_CAPACITY);
    }

    #[test]
    fn rank_helpers_enforce_melee_ranged_and_aoe_rules() {
        assert!(can_reach_rank(Rank(0), Rank(1), Reach::Melee));
        assert!(!can_reach_rank(Rank(3), Rank(2), Reach::Melee));
        assert_eq!(
            reachable_ranks(Rank(3), Reach::Ranged, 4),
            vec![Rank(0), Rank(1), Rank(2), Rank(3), Rank(4)]
        );

        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let c = Entity::from_raw_u32(3).unwrap();
        let targets = vec![
            RankTarget {
                entity: a,
                side: CombatSide::Enemy,
                rank: Rank(0),
            },
            RankTarget {
                entity: b,
                side: CombatSide::Enemy,
                rank: Rank(2),
            },
            RankTarget {
                entity: c,
                side: CombatSide::Party,
                rank: Rank(1),
            },
        ];
        assert_eq!(
            aoe_targets_by_rank(&targets, CombatSide::Enemy, Rank(1), 1),
            vec![a, b]
        );
        assert!(aoe_friendly_fire_risk(
            &targets,
            CombatSide::Party,
            Rank(1),
            0
        ));
    }

    #[test]
    fn encounter_end_resets_hp_mana_and_cooldowns() {
        let mut app = App::new();
        app.add_message::<EncounterEnded>()
            .add_systems(Update, reset_between_encounters);
        let entity = app
            .world_mut()
            .spawn((
                Health { current: 1, max: 8 },
                Derived {
                    armor_class: 10,
                    max_hp: 12,
                    initiative_mod: 0,
                    proficiency: 2,
                    speed: 30,
                },
                Mana { current: 0, max: 7 },
                Cooldowns {
                    abilities: vec![AbilityCooldown {
                        ability_id: "test".into(),
                        remaining: 3,
                        max: 3,
                    }],
                },
            ))
            .id();
        app.world_mut()
            .write_message(EncounterEnded { victory: true });
        app.update();
        assert_eq!(app.world().get::<Health>(entity).unwrap().current, 12);
        assert_eq!(app.world().get::<Mana>(entity).unwrap().current, 7);
        assert_eq!(
            app.world().get::<Cooldowns>(entity).unwrap().abilities[0].remaining,
            0
        );
    }

    #[test]
    fn revive_cost_stacks_gold_and_max_hp_loss() {
        let mut gold = Gold(500);
        let mut health = Health {
            current: 0,
            max: 20,
        };
        let mut penalty = RevivePenalty::default();
        assert!(apply_revive_cost(&mut gold, &mut health, &mut penalty));
        assert_eq!(penalty.stacks, 1);
        assert_eq!(health.max, 18);
        assert_eq!(gold.0, 350);
    }

    #[test]
    fn antagonist_generation_is_seeded() {
        assert_eq!(Antagonist::generate(44), Antagonist::generate(44));
        assert_ne!(Antagonist::generate(44), Antagonist::generate(45));
    }

    #[test]
    fn talent_trees_cover_every_class_and_subclass_unlocks_at_ten() {
        let data = fixture_data();
        for class_id in data.classes.keys() {
            let tree = data.talent_trees.get(class_id).unwrap();
            assert!(tree.nodes.iter().any(|node| node.unlock_level >= 10));
        }
        let mut character = Character {
            name: "Aster".into(),
            race: "human".into(),
            class: "fighter".into(),
            subclass: None,
            level: 9,
            xp: xp_for_level(9),
        };
        assert!(!can_unlock_subclass(&character));
        character.level = 10;
        assert!(can_unlock_subclass(&character));
    }

    #[test]
    fn save_round_trip_is_stable() {
        let save = SaveGame {
            metadata: CampaignMetadata {
                slot: 1,
                name: "Test Campaign".into(),
                seed: 7,
                difficulty: Difficulty::Normal,
                autosave: true,
                progress_label: "Act I".into(),
            },
            seed: 7,
            difficulty: Difficulty::Normal,
            antagonist: Antagonist::generate(7),
            planned_companions: PlannedCompanions::default(),
            party: Vec::new(),
            map: generate_map(7, 3),
            inventory: vec!["iron_sword".into()],
            item_instances: std::collections::HashMap::new(),
            gold: 12,
            autosave: true,
        };
        let text = serialize_save(&save).unwrap();
        assert_eq!(deserialize_save(&text).unwrap(), save);
    }

    #[test]
    fn chargen_finalizes_every_race_class_with_standard_array() {
        let data = fixture_data();
        let base = Abilities {
            str_: 15,
            dex: 14,
            con: 13,
            int: 12,
            wis: 10,
            cha: 8,
        };
        for race in data.races.values() {
            for class in data.classes.values() {
                let (character, abilities, derived, health, skills, traits, equipment, sprite) =
                    finalize_character_bundle(
                        "Tester",
                        race,
                        class,
                        base,
                        class.skill_choices.iter().take(2).cloned().collect(),
                        race.traits.clone(),
                        &data,
                    );
                assert_eq!(character.race, race.id);
                assert_eq!(character.class, class.id);
                assert_eq!(character.level, 1);
                assert!(abilities.str_ >= base.str_);
                assert_eq!(health.max, derived.max_hp);
                assert_eq!(health.current, health.max);
                assert!(skills.proficient.len() <= 2);
                assert_eq!(traits.0, race.traits);
                assert_eq!(sprite.base_body, race.sprite_key);
                assert!(equipment.main_hand.is_some() || class.starting_kit.is_empty());
            }
        }
    }

    #[test]
    fn chargen_accepts_point_buy_for_every_race_class() {
        let data = fixture_data();
        let point_buy = [15, 14, 13, 12, 10, 8];
        assert!(validate_point_buy(point_buy));
        let base = Abilities {
            str_: point_buy[0],
            dex: point_buy[1],
            con: point_buy[2],
            int: point_buy[3],
            wis: point_buy[4],
            cha: point_buy[5],
        };
        for race in data.races.values() {
            for class in data.classes.values() {
                let (_, _, derived, health, _, _, _, _) = finalize_character_bundle(
                    "Point Buy",
                    race,
                    class,
                    base,
                    Vec::new(),
                    Vec::new(),
                    &data,
                );
                assert!(derived.max_hp > 0);
                assert_eq!(health.current, health.max);
            }
        }
    }

    #[test]
    fn chargen_accepts_rolled_scores_for_every_race_class() {
        let data = fixture_data();
        let mut rng = ChaCha8Rng::seed_from_u64(155);
        let base = Abilities {
            str_: roll_4d6_drop_lowest(&mut rng),
            dex: roll_4d6_drop_lowest(&mut rng),
            con: roll_4d6_drop_lowest(&mut rng),
            int: roll_4d6_drop_lowest(&mut rng),
            wis: roll_4d6_drop_lowest(&mut rng),
            cha: roll_4d6_drop_lowest(&mut rng),
        };
        for race in data.races.values() {
            for class in data.classes.values() {
                let (_, abilities, derived, _, _, _, _, _) = finalize_character_bundle(
                    "Rolled",
                    race,
                    class,
                    base,
                    Vec::new(),
                    Vec::new(),
                    &data,
                );
                assert!((1..=30).contains(&abilities.str_));
                assert!(derived.armor_class > 0);
            }
        }
    }

    #[test]
    fn starting_equipment_places_items_in_contract_slots() {
        let data = fixture_data();
        let fighter = data.classes.get("fighter").unwrap();
        let equipment = build_starting_equipment(fighter, &data);
        assert_eq!(equipment.main_hand.as_deref(), Some("iron_sword"));
        assert_eq!(equipment.off_hand.as_deref(), Some("wooden_shield"));
        assert_eq!(equipment.body.as_deref(), Some("chain_shirt"));
    }

    #[test]
    fn derived_stats_scale_with_level() {
        let data = fixture_data();
        let race = data.races.get("human").unwrap();
        let class = data.classes.get("fighter").unwrap();
        let abilities = Abilities {
            str_: 16,
            dex: 12,
            con: 14,
            int: 10,
            wis: 10,
            cha: 10,
        };
        let equipment = build_starting_equipment(class, &data);
        let level_one = derived_stats(abilities, 1, class, race, &equipment, &data);
        let level_five = derived_stats(abilities, 5, class, race, &equipment, &data);
        assert!(level_five.max_hp > level_one.max_hp);
        assert_eq!(level_five.proficiency, 3);
        assert_eq!(level_one.speed, race.speed);
    }

    #[test]
    fn gated_roll_consequence_waits_for_matching_animation_complete() {
        let mut app = App::new();
        app.add_message::<RollAnimationComplete>()
            .add_message::<DamageDealt>()
            .insert_resource(PendingRolls::default())
            .add_systems(Update, complete_pending_roll_actions);

        let target = app
            .world_mut()
            .spawn((
                Health {
                    current: 20,
                    max: 20,
                },
                Derived {
                    armor_class: 10,
                    max_hp: 20,
                    initiative_mod: 0,
                    proficiency: 2,
                    speed: 30,
                },
            ))
            .id();
        let attacker = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PendingRolls>()
            .attacks
            .insert(
                99,
                PendingAttack {
                    attacker,
                    target,
                    attack_total: 99,
                    damage: DiceExpr {
                        count: 1,
                        sides: 1,
                        modifier: 4,
                    },
                    is_crit: false,
                },
            );

        app.update();
        assert_eq!(app.world().get::<Health>(target).unwrap().current, 20);

        app.world_mut()
            .write_message(RollAnimationComplete { id: 100 });
        app.update();
        assert_eq!(app.world().get::<Health>(target).unwrap().current, 20);

        app.world_mut()
            .write_message(RollAnimationComplete { id: 99 });
        app.update();
        assert_eq!(app.world().get::<Health>(target).unwrap().current, 15);
    }

    #[test]
    fn core_plugin_registers_messages_and_seeded_rng() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.add_plugins(StarwoodCorePlugin { seed: 5 });
        assert!(app.world().contains_resource::<GameRng>());
        assert!(app.world().contains_resource::<Messages<RollRequest>>());
        assert!(
            app.world()
                .contains_resource::<Messages<EncounterStarted>>()
        );
    }

    fn flow_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.add_plugins(StarwoodCorePlugin { seed: 1 });
        app.world_mut().insert_resource(fixture_data());
        app
    }

    #[test]
    fn new_game_request_resets_run_resources() {
        let mut app = flow_app();
        app.world_mut()
            .resource_mut::<PartyRoster>()
            .members
            .push(Entity::PLACEHOLDER);
        app.world_mut()
            .resource_mut::<Inventory>()
            .items
            .push("iron_sword".into());
        app.world_mut().write_message(NewGameRequested { seed: 44 });
        app.update();

        assert!(app.world().resource::<PartyRoster>().members.is_empty());
        assert!(app.world().resource::<Inventory>().items.is_empty());
        assert_eq!(app.world().resource::<MapState>().seed, 44);
        assert!(!app.world().resource::<MapState>().nodes.is_empty());
    }

    #[test]
    fn character_build_request_spawns_party_member_and_contract_messages() {
        let mut app = flow_app();
        app.world_mut().write_message(CharacterBuildRequested {
            name: "Aster".into(),
            race: "human".into(),
            class: "fighter".into(),
            abilities: Abilities {
                str_: 15,
                dex: 14,
                con: 13,
                int: 12,
                wis: 10,
                cha: 8,
            },
            skills: vec!["athletics".into()],
            traits: vec![],
        });
        app.update();

        let roster = app.world().resource::<PartyRoster>();
        assert_eq!(roster.members.len(), 1);
        let entity = roster.members[0];
        assert_eq!(app.world().get::<PartyMember>(entity).unwrap().slot, 0);
        assert_eq!(app.world().get::<Character>(entity).unwrap().name, "Aster");
        assert!(
            app.world()
                .get::<Equipment>(entity)
                .unwrap()
                .main_hand
                .is_some()
        );
        assert!(app.world().resource::<Inventory>().items.is_empty());
        assert!(!app.world().resource::<ItemInstances>().instances.is_empty());

        let finalized: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<CharacterFinalized>>()
            .drain()
            .collect();
        assert_eq!(finalized.len(), 1);
        assert_eq!(finalized[0].entity, entity);
    }

    #[test]
    fn encounter_request_spawns_capped_enemies_and_turn_order() {
        let mut app = flow_app();
        let party_entity = app
            .world_mut()
            .spawn((
                PartyMember { slot: 0 },
                Derived {
                    armor_class: 15,
                    max_hp: 12,
                    initiative_mod: 2,
                    proficiency: 2,
                    speed: 30,
                },
                Health {
                    current: 12,
                    max: 12,
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<PartyRoster>()
            .members
            .push(party_entity);
        app.world_mut().write_message(EncounterRequested {
            difficulty: MapNodeType::Combat,
        });
        app.update();

        let encounter = app.world().resource::<EncounterState>();
        assert!((1..=5).contains(&encounter.enemies.len()));
        assert_eq!(encounter.turn_order.len(), encounter.enemies.len() + 1);
        for enemy in &encounter.enemies {
            assert!(app.world().get::<EnemyUnit>(*enemy).is_some());
            assert!(app.world().get::<Health>(*enemy).is_some());
        }
    }
}
