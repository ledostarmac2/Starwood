use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use starwood_core::*;
use starwood_render::StarwoodRenderPlugin;
use starwood_ui::StarwoodUiPlugin;

#[derive(Clone, Debug)]
pub struct StarwoodAppOptions {
    pub seed: u64,
    pub debug: StarwoodDebugConfig,
}

impl Default for StarwoodAppOptions {
    fn default() -> Self {
        Self {
            seed: StarwoodCorePlugin::default().seed,
            debug: StarwoodDebugConfig::default(),
        }
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct StarwoodDebugConfig {
    pub headless_smoke: bool,
    pub scripted_encounter: bool,
    pub auto_complete_rolls: bool,
    pub auto_drive_combat: bool,
    pub force_next_roll: Option<ForcedRoll>,
    pub force_every_roll: Option<ForcedRoll>,
    pub spawn_item: Option<DebugItemSpawn>,
    pub difficulty: Option<Difficulty>,
    pub enemies: Vec<EnemyArchetypeId>,
}

#[derive(Clone, Debug)]
pub struct DebugItemSpawn {
    pub base: ItemId,
    pub rarity: Rarity,
}

#[derive(Resource, Default, Debug)]
pub struct DebugHarnessState {
    pub bootstrapped: bool,
    pub item_spawned: bool,
    pub completed_victory: bool,
}

pub struct StarwoodDebugHarnessPlugin {
    pub config: StarwoodDebugConfig,
}

#[derive(SystemParam)]
struct DebugBootstrapResources<'w> {
    data: Res<'w, GameData>,
    rng: ResMut<'w, GameRng>,
    difficulty: Res<'w, GameDifficulty>,
    tuning: Res<'w, DifficultyTuning>,
    party: ResMut<'w, PartyRoster>,
    planned: ResMut<'w, PlannedCompanions>,
    inventory: ResMut<'w, Inventory>,
    instances: ResMut<'w, ItemInstances>,
    gold: ResMut<'w, Gold>,
    encounter: ResMut<'w, EncounterState>,
    next_state: ResMut<'w, NextState<GameState>>,
}

#[derive(SystemParam)]
struct DebugBootstrapEvents<'w> {
    started: EventWriter<'w, EncounterStarted>,
    finalized: EventWriter<'w, CharacterFinalized>,
    equipment_changed: EventWriter<'w, EquipmentChanged>,
    inventory_changed: EventWriter<'w, InventoryChanged>,
}

impl Plugin for StarwoodDebugHarnessPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config.clone())
            .init_resource::<DebugHarnessState>()
            .add_systems(Startup, apply_debug_startup_options)
            .add_systems(
                Update,
                (
                    debug_force_rolls,
                    debug_start_scripted_encounter,
                    debug_auto_complete_rolls,
                    debug_auto_drive_combat,
                    debug_track_encounter_end,
                ),
            );
    }
}

pub fn build_starwood_app(options: StarwoodAppOptions) -> App {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Starwood".to_string(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .add_plugins(StarwoodCorePlugin { seed: options.seed })
    .add_plugins(StarwoodDebugHarnessPlugin {
        config: options.debug,
    })
    .add_plugins(StarwoodRenderPlugin)
    .add_plugins(StarwoodUiPlugin);
    app
}

pub fn build_headless_app(options: StarwoodAppOptions) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::state::app::StatesPlugin)
        .add_plugins(StarwoodCorePlugin { seed: options.seed })
        .add_plugins(StarwoodDebugHarnessPlugin {
            config: options.debug,
        });
    app
}

pub fn parse_options_from_env_and_args<I, S>(args: I) -> StarwoodAppOptions
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut options = StarwoodAppOptions {
        seed: std::env::var("STARWOOD_SEED")
            .ok()
            .and_then(|seed| seed.parse().ok())
            .unwrap_or_else(|| StarwoodCorePlugin::default().seed),
        debug: StarwoodDebugConfig::default(),
    };

    let mut args = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = args.next() {
        let (key, inline_value) = split_arg(&arg);
        match key {
            "--seed" => {
                if let Some(value) = inline_value.or_else(|| args.next())
                    && let Ok(seed) = value.parse()
                {
                    options.seed = seed;
                }
            }
            "--difficulty" => {
                if let Some(value) = inline_value.or_else(|| args.next()) {
                    options.debug.difficulty = parse_difficulty(&value);
                }
            }
            "--debug-encounter" => {
                options.debug.scripted_encounter = true;
                options.debug.auto_drive_combat = true;
                options.debug.force_every_roll = Some(ForcedRoll::Nat20);
            }
            "--headless-smoke" => {
                options.debug.headless_smoke = true;
                options.debug.scripted_encounter = true;
                options.debug.auto_complete_rolls = true;
                options.debug.auto_drive_combat = true;
                options.debug.force_every_roll = Some(ForcedRoll::Nat20);
                if options.debug.enemies.is_empty() {
                    options.debug.enemies = vec!["goblin_cutpurse".to_string()];
                }
            }
            "--enemy" => {
                if let Some(value) = inline_value.or_else(|| args.next()) {
                    options.debug.enemies = value
                        .split(',')
                        .filter(|id| !id.trim().is_empty())
                        .map(|id| id.trim().to_string())
                        .collect();
                }
            }
            "--force-roll" => {
                if let Some(value) = inline_value.or_else(|| args.next()) {
                    options.debug.force_next_roll = parse_forced_roll(&value);
                }
            }
            "--spawn-item" => {
                if let Some(value) = inline_value.or_else(|| args.next()) {
                    options.debug.spawn_item = parse_item_spawn(&value);
                }
            }
            _ => {}
        }
    }

    options
}

fn split_arg(arg: &str) -> (&str, Option<String>) {
    if let Some((key, value)) = arg.split_once('=') {
        (key, Some(value.to_string()))
    } else {
        (arg, None)
    }
}

fn parse_difficulty(value: &str) -> Option<Difficulty> {
    match value.to_ascii_lowercase().as_str() {
        "easy" => Some(Difficulty::Easy),
        "normal" => Some(Difficulty::Normal),
        "hard" => Some(Difficulty::Hard),
        _ => None,
    }
}

fn parse_forced_roll(value: &str) -> Option<ForcedRoll> {
    match value.to_ascii_lowercase().as_str() {
        "nat20" | "20" => Some(ForcedRoll::Nat20),
        "nat1" | "1" => Some(ForcedRoll::Nat1),
        other => other.parse().ok().map(ForcedRoll::Value),
    }
}

fn parse_item_spawn(value: &str) -> Option<DebugItemSpawn> {
    let (base, rarity) = value
        .split_once(':')
        .or_else(|| value.split_once('='))
        .unwrap_or((value, "rare"));
    Some(DebugItemSpawn {
        base: base.trim().to_string(),
        rarity: parse_rarity(rarity.trim())?,
    })
}

fn parse_rarity(value: &str) -> Option<Rarity> {
    match value.to_ascii_lowercase().as_str() {
        "common" => Some(Rarity::Common),
        "uncommon" => Some(Rarity::Uncommon),
        "rare" => Some(Rarity::Rare),
        "epic" => Some(Rarity::Epic),
        "legendary" => Some(Rarity::Legendary),
        _ => None,
    }
}

fn apply_debug_startup_options(
    config: Res<StarwoodDebugConfig>,
    mut difficulty: ResMut<GameDifficulty>,
    mut dice_override: ResMut<DebugDiceOverride>,
) {
    if let Some(value) = config.difficulty {
        difficulty.0 = value;
    }
    if let Some(value) = config.force_next_roll {
        dice_override.next = Some(value);
    }
}

#[allow(clippy::too_many_arguments)]
fn debug_start_scripted_encounter(
    mut commands: Commands,
    config: Res<StarwoodDebugConfig>,
    mut state: ResMut<DebugHarnessState>,
    mut res: DebugBootstrapResources,
    mut events: DebugBootstrapEvents,
) {
    if !config.scripted_encounter || state.bootstrapped || res.data.races.is_empty() {
        return;
    }

    for entity in res.party.members.drain(..) {
        commands.entity(entity).despawn();
    }
    for entity in res.encounter.enemies.drain(..) {
        commands.entity(entity).despawn();
    }
    res.inventory.items.clear();
    res.instances.instances.clear();
    res.instances.next_serial = 0;
    res.gold.0 = 250;
    *res.planned = PlannedCompanions {
        classes: ["cleric".into(), "rogue".into(), "wizard".into()],
    };

    let party_plan = [
        ("Aster", "human", "fighter"),
        ("Mira", "elf", "cleric"),
        ("Bram", "halfling", "rogue"),
        ("Sable", "tiefling", "wizard"),
    ];
    for (slot, (name, race_id, class_id)) in party_plan.iter().enumerate() {
        let Some(race) = res.data.races.get(*race_id) else {
            continue;
        };
        let Some(class) = res.data.classes.get(*class_id) else {
            continue;
        };
        let (character, abilities, derived, health, skills, traits, mut equipment, sprite) =
            finalize_character_bundle(
                *name,
                race,
                class,
                Abilities {
                    str_: 16,
                    dex: 14,
                    con: 14,
                    int: 12,
                    wis: 12,
                    cha: 10,
                },
                class.skill_choices.iter().take(2).cloned().collect(),
                race.traits.clone(),
                &res.data,
            );
        instance_starting_equipment(
            class,
            &res.data,
            &mut res.instances,
            &mut res.rng.0,
            &mut equipment,
            &mut res.inventory,
        );
        let entity = commands
            .spawn((
                character,
                abilities,
                derived,
                health,
                mana_for_class(class, abilities, 1),
                cooldowns_for_class(class),
                skills,
                traits,
                Talents::default(),
                TalentPoints::default(),
                RevivePenalty::default(),
                equipment,
                sprite,
                PartyMember { slot: slot as u8 },
            ))
            .id();
        if slot == 0 {
            commands.entity(entity).insert(PlayerCharacter);
        }
        res.party.members.push(entity);
        events.finalized.write(CharacterFinalized { entity });
        events.equipment_changed.write(EquipmentChanged { entity });
    }

    if let Some(spawn) = &config.spawn_item {
        spawn_debug_item(
            spawn,
            &res.data,
            &mut res.instances,
            &mut res.inventory,
            &mut res.rng.0,
        );
        state.item_spawned = true;
        events.inventory_changed.write(InventoryChanged);
    }

    let enemy_ids = if config.enemies.is_empty() {
        vec!["goblin_cutpurse".to_string()]
    } else {
        config.enemies.clone()
    };
    begin_encounter(
        &mut commands,
        &res.data,
        &enemy_ids,
        &mut res.encounter,
        &mut events.started,
        res.difficulty.0,
        *res.tuning,
    );

    let mut initiatives = Vec::new();
    let mut combatants = Vec::new();
    for (index, entity) in res.party.members.iter().copied().enumerate() {
        let initiative = 30 - index as i32;
        commands.entity(entity).insert(Initiative(initiative));
        initiatives.push((entity, initiative));
        combatants.push(entity);
    }
    for (index, entity) in res.encounter.enemies.iter().copied().enumerate() {
        let initiative = 10 - index as i32;
        commands.entity(entity).insert(Initiative(initiative));
        initiatives.push((entity, initiative));
        combatants.push(entity);
    }
    build_turn_order(&mut commands, &combatants, &initiatives, &mut res.encounter);
    res.next_state.set(GameState::Encounter);
    state.bootstrapped = true;
}

fn spawn_debug_item(
    spawn: &DebugItemSpawn,
    data: &GameData,
    instances: &mut ItemInstances,
    inventory: &mut Inventory,
    rng: &mut rand_chacha::ChaCha8Rng,
) {
    let Some(base) = data.items.get(&spawn.base) else {
        return;
    };
    let item = roll_item_instance_with_rarity(base, data, instances, rng, 1, spawn.rarity);
    let _ = add_item_to_inventory(inventory, item.instance_id);
}

fn debug_force_rolls(
    config: Res<StarwoodDebugConfig>,
    mut dice_override: ResMut<DebugDiceOverride>,
) {
    if dice_override.next.is_none()
        && let Some(value) = config.force_every_roll
    {
        dice_override.next = Some(value);
    }
}

fn debug_auto_complete_rolls(
    config: Res<StarwoodDebugConfig>,
    mut resolved: EventReader<RollResolved>,
    mut completed: EventWriter<RollAnimationComplete>,
) {
    if !config.auto_complete_rolls {
        return;
    }
    for roll in resolved.read() {
        completed.write(RollAnimationComplete { id: roll.id });
    }
}

type DebugPartyQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static Health), (With<PartyMember>, Without<EnemyUnit>)>;

type DebugEnemyQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static Health), (With<EnemyUnit>, Without<PartyMember>)>;

fn debug_auto_drive_combat(
    config: Res<StarwoodDebugConfig>,
    pending: Res<PendingRolls>,
    active: Query<Entity, With<ActiveTurn>>,
    party: DebugPartyQuery,
    enemies: DebugEnemyQuery,
    mut actions: EventWriter<CombatActionRequest>,
) {
    if !config.auto_drive_combat
        || !pending.attack_intents.is_empty()
        || !pending.attacks.is_empty()
    {
        return;
    }
    let Some(target) = enemies
        .iter()
        .find(|(_, health)| health.current > 0)
        .map(|(entity, _)| entity)
    else {
        return;
    };
    let actor = active
        .iter()
        .find(|entity| {
            party
                .get(*entity)
                .is_ok_and(|(_, health)| health.current > 0)
        })
        .or_else(|| {
            party
                .iter()
                .find(|(_, health)| health.current > 0)
                .map(|(entity, _)| entity)
        });
    if let Some(actor) = actor {
        actions.write(CombatActionRequest {
            actor,
            target,
            action: CombatAction::Attack,
        });
    }
}

fn debug_track_encounter_end(
    mut ended: EventReader<EncounterEnded>,
    mut state: ResMut<DebugHarnessState>,
) {
    for event in ended.read() {
        if event.victory {
            state.completed_victory = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_scripted_encounter_reaches_victory_and_heals() {
        let mut app = build_headless_app(StarwoodAppOptions {
            seed: 123,
            debug: StarwoodDebugConfig {
                scripted_encounter: true,
                auto_complete_rolls: true,
                auto_drive_combat: true,
                force_every_roll: Some(ForcedRoll::Nat20),
                spawn_item: Some(DebugItemSpawn {
                    base: "iron_sword".to_string(),
                    rarity: Rarity::Rare,
                }),
                enemies: vec!["goblin_cutpurse".to_string()],
                ..default()
            },
        });

        for _ in 0..1_000 {
            app.update();
            if app
                .world()
                .resource::<DebugHarnessState>()
                .completed_victory
                && app.world().resource::<State<GameState>>().get() == &GameState::Exploration
            {
                break;
            }
        }

        for _ in 0..3 {
            app.update();
        }

        let debug_state = app.world().resource::<DebugHarnessState>();
        let game_state = app.world().resource::<State<GameState>>().get().clone();
        let inventory = app.world().resource::<Inventory>().items.clone();
        let enemy_count = app.world().resource::<EncounterState>().enemies.len();
        let pending_intents = app.world().resource::<PendingRolls>().attack_intents.len();
        let pending_attacks = app.world().resource::<PendingRolls>().attacks.len();

        assert!(
            debug_state.completed_victory,
            "headless smoke did not complete victory: state={game_state:?}, \
             enemy_count={enemy_count}, inventory={inventory:?}, \
             pending_intents={pending_intents}, pending_attacks={pending_attacks}, \
             debug={debug_state:?}"
        );
        assert_eq!(game_state, GameState::Exploration);
        assert!(inventory.iter().any(|id| id.starts_with("iron_sword:2:")));

        let mut party = app
            .world_mut()
            .query_filtered::<&Health, With<PartyMember>>();
        for health in party.iter(app.world()) {
            assert_eq!(health.current, health.max);
        }
    }
}
