//! Persistent HUD (party panel + equipped-gear readout), the Exploration map,
//! the encounter turn order and the action bar — plus the small flow systems
//! that turn action-bar clicks into the contract's roll/encounter events and
//! keep the turn order moving.
//!
//! The egui front-end is split into three systems (party panel, exploration,
//! encounter) to stay under Bevy's system-parameter limit and to keep each
//! screen's data needs obvious.
//!
//! Combat correctness lives in `starwood_core`: we only ever *request* rolls and
//! forward their authoritative results into `core`'s `PendingRolls` so that
//! `core` applies damage after the Dice Theater fires `RollAnimationComplete`.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::*;

use crate::theme;

/// Player-side selections that several panels share.
#[derive(Resource, Default)]
pub struct UiSelection {
    pub selected_member: Option<Entity>,
    pub target_enemy: Option<Entity>,
    pub show_sheet: bool,
    pub show_skills: bool,
}

/// In-flight attack we have requested but whose result has not yet resolved.
#[derive(Clone)]
pub struct InFlightAttack {
    pub attacker: Entity,
    pub target: Entity,
    pub damage: DiceExpr,
}

/// UI-side bookkeeping for the encounter loop. Resolution still happens in core.
#[derive(Resource, Default)]
pub struct CombatFlow {
    pub started: bool,
    pub init_pending: HashMap<u64, Entity>,
    pub init_results: Vec<(Entity, i32)>,
    pub init_expected: usize,
    pub attacks: HashMap<u64, InFlightAttack>,
    pub advance_ids: HashSet<u64>,
    pub pending_actor: Option<Entity>,
    pub fled: bool,
    pub next_id: u64,
}

impl CombatFlow {
    fn alloc_id(&mut self) -> u64 {
        if self.next_id < 1_000_000 {
            self.next_id = 1_000_000;
        }
        self.next_id += 1;
        self.next_id
    }

    fn reset_for_new_encounter(&mut self) {
        self.started = false;
        self.init_pending.clear();
        self.init_results.clear();
        self.init_expected = 0;
        self.attacks.clear();
        self.advance_ids.clear();
        self.pending_actor = None;
        self.fled = false;
    }
}

// ===== Query aliases ===============================================

/// Read-only party view used by the always-on party panel.
type PartyView<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Character,
        &'static Health,
        &'static Derived,
        &'static Equipment,
        &'static PartyMember,
    ),
    Without<EnemyUnit>,
>;

/// Mutable party view used inside an encounter (attack reads + heal writes).
type PartyCombat<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Character,
        &'static mut Health,
        &'static Derived,
        &'static Equipment,
        &'static Abilities,
        &'static PartyMember,
    ),
    Without<EnemyUnit>,
>;

type EnemyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static EnemyUnit,
        &'static Health,
        &'static Derived,
    ),
    Without<PartyMember>,
>;

// ===== Party panel (Exploration + Encounter) =======================

pub fn party_panel_ui(
    mut contexts: EguiContexts,
    game_state: Res<State<GameState>>,
    mut inventory_open: ResMut<InventoryOpen>,
    party: PartyView,
    active: Query<Entity, With<ActiveTurn>>,
    mana_view: Query<(&Mana, Option<&Cooldowns>)>,
    mut selection: ResMut<UiSelection>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    gold: Res<Gold>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let in_encounter = *game_state.get() == GameState::Encounter;
    let active_entity = active.iter().next();

    egui::SidePanel::left("party_panel")
        .resizable(false)
        .min_width(252.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.label(theme::heading("Your Party"));
            ui.label(theme::flavour(format!("Gold: {}", gold.0)));
            ui.separator();

            let mut members: Vec<_> = party.iter().collect();
            members.sort_by_key(|(.., member)| member.slot);
            for (entity, character, health, derived, equipment, _member) in members {
                let is_active = Some(entity) == active_entity;
                let selected = selection.selected_member == Some(entity);
                theme::card_frame(is_active).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let title = egui::RichText::new(character.name.as_str())
                            .size(16.0)
                            .color(if is_active {
                                theme::GOLD_BRIGHT
                            } else {
                                theme::INK
                            })
                            .strong();
                        if ui.selectable_label(selected, title).clicked() {
                            selection.selected_member = Some(entity);
                        }
                        if is_active && in_encounter {
                            ui.label(
                                egui::RichText::new("● turn")
                                    .color(theme::GOLD_BRIGHT)
                                    .size(12.0),
                            );
                        }
                    });
                    ui.label(theme::flavour(format!(
                        "L{} {}  ·  AC {}",
                        character.level,
                        class_name(&character.class, &data),
                        derived.armor_class
                    )));
                    hp_bar(ui, health.current, health.max);
                    if let Ok((mana, cooldowns)) = mana_view.get(entity) {
                        mana_bar(ui, mana.current, mana.max);
                        if let Some(cooldowns) = cooldowns {
                            cooldown_line(ui, cooldowns);
                        }
                    }
                    ui.label(theme::flavour(format!(
                        "Gear: {}",
                        equipped_line(equipment, &data, &instances)
                    )));
                });
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Inventory").clicked() {
                    inventory_open.0 = !inventory_open.0;
                }
                if ui.button("Character Sheet").clicked() {
                    selection.show_sheet = !selection.show_sheet;
                }
                if ui.button("Skills").clicked() {
                    selection.show_skills = !selection.show_skills;
                }
            });

            if selection.selected_member.is_none() {
                selection.selected_member =
                    active_entity.or_else(|| party.iter().next().map(|t| t.0));
            }
        });

    Ok(())
}

// ===== Exploration map =============================================

#[allow(clippy::too_many_arguments)]
pub fn exploration_ui(
    mut contexts: EguiContexts,
    party: Query<&Character, With<PartyMember>>,
    mut map: ResMut<MapState>,
    data: Res<GameData>,
    difficulty: Res<GameDifficulty>,
    tuning: Res<DifficultyTuning>,
    mut rng: ResMut<GameRng>,
    mut encounter: ResMut<EncounterState>,
    mut combat: ResMut<CombatFlow>,
    mut commands: Commands,
    mut started: EventWriter<EncounterStarted>,
    mut next_game: ResMut<NextState<GameState>>,
    mut inventory: ResMut<crate::inventory::PartyInventory>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let party_level = party.iter().map(|c| c.level).max().unwrap_or(1);

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(theme::BG_DEEP)
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.label(theme::title("The Starwood"));
            ui.label(theme::flavour(
                "Choose your path. Each road forward is a choice you cannot unmake.",
            ));
            ui.add_space(10.0);

            if map.nodes.is_empty() {
                ui.label("The map has not yet been drawn.");
                return;
            }

            let current = map.current_node;
            let reachable: Vec<u32> = current
                .and_then(|id| map.nodes.iter().find(|n| n.id == id))
                .map(|n| n.next.clone())
                .unwrap_or_default();
            let depth = map.nodes.iter().map(|n| n.layer).max().unwrap_or(0);

            let mut clicked: Option<u32> = None;
            for layer in 0..=depth {
                ui.horizontal(|ui| {
                    ui.label(theme::flavour(format!("Tier {layer}")));
                    for node in map.nodes.iter().filter(|n| n.layer == layer) {
                        let is_current = current == Some(node.id);
                        let is_reachable = reachable.contains(&node.id);
                        let (glyph, color) = node_glyph(node.node_type, node.completed, is_current);
                        let button =
                            egui::Button::new(egui::RichText::new(glyph).size(16.0).color(color))
                                .min_size(egui::vec2(118.0, 34.0))
                                .fill(if is_current {
                                    theme::PANEL_LIGHT
                                } else {
                                    theme::PANEL
                                });
                        let response = ui.add_enabled(is_reachable, button);
                        if response.on_hover_text(node_hint(node.node_type)).clicked() {
                            clicked = Some(node.id);
                        }
                    }
                });
                ui.add_space(4.0);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.label(theme::flavour(
                "◆ combat   ◈ elite   ✦ treasure   ❂ event   ☾ rest   ☠ boss   ✓ cleared",
            ));

            if let Some(node_id) = clicked {
                enter_node(
                    node_id,
                    &mut map,
                    party_level,
                    &data,
                    &mut rng,
                    &mut encounter,
                    &mut combat,
                    &mut commands,
                    &mut started,
                    &mut next_game,
                    &mut inventory,
                    difficulty.0,
                    *tuning,
                );
            }
        });

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enter_node(
    node_id: u32,
    map: &mut MapState,
    party_level: u32,
    data: &GameData,
    rng: &mut GameRng,
    encounter: &mut EncounterState,
    combat: &mut CombatFlow,
    commands: &mut Commands,
    started: &mut EventWriter<EncounterStarted>,
    next_game: &mut NextState<GameState>,
    inventory: &mut crate::inventory::PartyInventory,
    difficulty: Difficulty,
    tuning: DifficultyTuning,
) {
    if let Some(from) = map.current_node {
        if let Some(node) = map.nodes.iter_mut().find(|n| n.id == from) {
            node.completed = true;
        }
    }
    map.current_node = Some(node_id);

    let Some(node_type) = map
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .map(|n| n.node_type)
    else {
        return;
    };

    match node_type {
        MapNodeType::Combat | MapNodeType::Elite | MapNodeType::Boss => {
            let enemy_ids = choose_enemy_archetypes(data, party_level, node_type, &mut rng.0);
            combat.reset_for_new_encounter();
            begin_encounter(
                commands, data, &enemy_ids, encounter, started, difficulty, tuning,
            );
            next_game.set(GameState::Encounter);
        }
        MapNodeType::BonusQuest => {
            let enemy_ids =
                choose_enemy_archetypes(data, party_level.max(10), MapNodeType::Boss, &mut rng.0);
            combat.reset_for_new_encounter();
            begin_encounter(
                commands, data, &enemy_ids, encounter, started, difficulty, tuning,
            );
            next_game.set(GameState::Encounter);
        }
        MapNodeType::Treasure => {
            inventory
                .items
                .extend(choose_loot(data, party_level, &mut rng.0));
            mark_current_completed(map);
        }
        MapNodeType::Town | MapNodeType::Shop => {
            inventory.shop_open = matches!(node_type, MapNodeType::Shop);
            mark_current_completed(map);
        }
        MapNodeType::Rest => {
            inventory.rest_requested = true;
            mark_current_completed(map);
        }
        MapNodeType::Event => {
            inventory.items.push("healing_draught".to_string());
            mark_current_completed(map);
        }
    }
}

fn mark_current_completed(map: &mut MapState) {
    if let Some(id) = map.current_node {
        if let Some(node) = map.nodes.iter_mut().find(|n| n.id == id) {
            node.completed = true;
        }
    }
}

// ===== Encounter (turn order, foes, action bar) ====================

#[allow(clippy::too_many_arguments)]
pub fn encounter_ui(
    mut contexts: EguiContexts,
    mut party: PartyCombat,
    enemies: EnemyQuery,
    active: Query<Entity, With<ActiveTurn>>,
    mana_view: Query<(&Mana, Option<&Cooldowns>)>,
    mut selection: ResMut<UiSelection>,
    mut combat: ResMut<CombatFlow>,
    mut encounter: ResMut<EncounterState>,
    mut inventory: ResMut<crate::inventory::PartyInventory>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    mut commands: Commands,
    mut rolls: EventWriter<RollRequest>,
    mut surrender: EventWriter<SurrenderRequested>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let active_entity = active.iter().next();

    // Turn order (top).
    egui::TopBottomPanel::top("turn_order")
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::heading("Initiative"));
                ui.add_space(8.0);
                if encounter.turn_order.is_empty() {
                    ui.label(theme::flavour("rolling for initiative…"));
                }
                for entity in &encounter.turn_order {
                    let active_now = Some(*entity) == active_entity;
                    let name = combatant_name(*entity, &party, &enemies, &data);
                    let color = if active_now {
                        theme::GOLD_BRIGHT
                    } else {
                        theme::INK_DIM
                    };
                    ui.label(egui::RichText::new(name).color(color).strong());
                    ui.label(egui::RichText::new("›").color(theme::INK_DIM));
                }
            });
        });

    // Foes (right).
    egui::SidePanel::right("enemy_panel")
        .resizable(false)
        .min_width(230.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.label(theme::heading("Foes"));
            ui.separator();
            let mut foes: Vec<_> = enemies.iter().collect();
            foes.sort_by_key(|(.., unit, _, _)| unit.slot);
            for (entity, unit, health, _derived) in foes {
                if health.current <= 0 {
                    continue;
                }
                let targeted = selection.target_enemy == Some(entity);
                theme::card_frame(targeted).show(ui, |ui| {
                    if ui
                        .selectable_label(
                            targeted,
                            egui::RichText::new(enemy_name(&unit.archetype, &data))
                                .size(15.0)
                                .strong(),
                        )
                        .clicked()
                    {
                        selection.target_enemy = Some(entity);
                    }
                    hp_bar(ui, health.current, health.max);
                });
            }
        });

    // Action bar (bottom).
    egui::TopBottomPanel::bottom("action_bar")
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            let active_party = active_entity.filter(|e| party.get(*e).is_ok());
            let locked = combat.pending_actor.is_some();

            let Some(actor) = active_party else {
                ui.horizontal(|ui| {
                    ui.label(theme::heading("Foe's turn"));
                    ui.label(theme::flavour("the enemy moves against you…"));
                });
                return;
            };

            // Keep a valid, living target selected.
            let target_invalid = selection
                .target_enemy
                .map(|t| {
                    enemies
                        .get(t)
                        .map(|(.., h, _)| h.current <= 0)
                        .unwrap_or(true)
                })
                .unwrap_or(true);
            if target_invalid {
                selection.target_enemy = enemies
                    .iter()
                    .find(|(.., h, _)| h.current > 0)
                    .map(|(e, ..)| e);
            }
            let target = selection.target_enemy;
            let has_target = target.is_some();
            let has_potion = inventory.items.iter().any(|i| i == "healing_draught");

            ui.horizontal(|ui| {
                ui.label(theme::heading("Your move"));
                ui.add_space(8.0);
                if let Ok((mana, cooldowns)) = mana_view.get(actor) {
                    ui.label(theme::flavour(format!(
                        "{}/{} mana",
                        mana.current, mana.max
                    )));
                    if let Some(cooldowns) = cooldowns {
                        let active_cooldowns = cooldowns
                            .abilities
                            .iter()
                            .filter(|ability| ability.remaining > 0)
                            .count();
                        if active_cooldowns > 0 {
                            ui.label(theme::flavour(format!("{active_cooldowns} cooling")));
                        }
                    }
                }
                if ui
                    .add_enabled(!locked && has_target, action_button("⚔ Attack"))
                    .clicked()
                {
                    if let Some(target) = target {
                        request_attack(
                            actor,
                            target,
                            AttackKind::Weapon,
                            &party,
                            &data,
                            &instances,
                            &mut combat,
                            &mut rolls,
                        );
                    }
                }
                if ui
                    .add_enabled(!locked && has_target, action_button("✦ Cast"))
                    .clicked()
                {
                    if let Some(target) = target {
                        request_attack(
                            actor,
                            target,
                            AttackKind::Spell,
                            &party,
                            &data,
                            &instances,
                            &mut combat,
                            &mut rolls,
                        );
                    }
                }
                if ui
                    .add_enabled(!locked && has_potion, action_button("🜂 Use Item"))
                    .clicked()
                {
                    use_healing_draught(
                        actor,
                        &mut party,
                        &mut inventory,
                        &mut combat,
                        &mut commands,
                        &mut encounter,
                    );
                }
                if ui
                    .add_enabled(!locked, action_button("Surrender"))
                    .clicked()
                {
                    combat.fled = true;
                    surrender.write(SurrenderRequested { actor });
                }
            });
            if locked {
                ui.label(theme::flavour("the dice decide…"));
            }
        });

    Ok(())
}

// ===== Action helpers ==============================================

#[derive(Clone, Copy)]
enum AttackKind {
    Weapon,
    Spell,
}

fn request_attack(
    attacker: Entity,
    target: Entity,
    kind: AttackKind,
    party: &PartyCombat,
    data: &GameData,
    instances: &ItemInstances,
    combat: &mut CombatFlow,
    rolls: &mut EventWriter<RollRequest>,
) {
    let Ok((_, _, _, derived, equipment, abilities, _)) = party.get(attacker) else {
        return;
    };

    let attack_mod = match kind {
        AttackKind::Weapon => {
            derived.proficiency
                + ability_modifier(abilities.str_).max(ability_modifier(abilities.dex))
        }
        AttackKind::Spell => {
            derived.proficiency
                + ability_modifier(abilities.int).max(ability_modifier(abilities.cha))
        }
    };
    let damage = match kind {
        AttackKind::Weapon => equipment
            .main_hand
            .as_ref()
            .and_then(|id| base_item_for_instance(id, data, instances))
            .and_then(|item| item.damage.clone())
            .unwrap_or(DiceExpr {
                count: 1,
                sides: 4,
                modifier: 0,
            }),
        AttackKind::Spell => DiceExpr {
            count: 1,
            sides: 8,
            modifier: 0,
        },
    };

    let id = combat.alloc_id();
    rolls.write(RollRequest {
        id,
        expr: DiceExpr {
            count: 1,
            sides: 20,
            modifier: attack_mod,
        },
        kind: RollKind::Attack,
        source: Some(attacker),
        advantage: AdvState::Normal,
    });
    combat.attacks.insert(
        id,
        InFlightAttack {
            attacker,
            target,
            damage,
        },
    );
    combat.pending_actor = Some(attacker);
}

fn use_healing_draught(
    actor: Entity,
    party: &mut PartyCombat,
    inventory: &mut crate::inventory::PartyInventory,
    combat: &mut CombatFlow,
    commands: &mut Commands,
    encounter: &mut EncounterState,
) {
    if let Some(pos) = inventory.items.iter().position(|i| i == "healing_draught") {
        inventory.items.remove(pos);
    } else {
        return;
    }
    if let Ok((_, _, mut health, _, _, _, _)) = party.get_mut(actor) {
        health.current = (health.current + 8).min(health.max);
    }
    advance_turn_now(commands, encounter);
    combat.pending_actor = None;
}

// ===== Flow systems (Update) =======================================

pub fn handle_encounter_started(
    mut events: EventReader<EncounterStarted>,
    roster: Res<PartyRoster>,
    combatants: Query<(&Derived, &Health)>,
    mut combat: ResMut<CombatFlow>,
    mut rolls: EventWriter<RollRequest>,
) {
    for event in events.read() {
        combat.reset_for_new_encounter();
        combat.started = true;

        let all = roster
            .members
            .iter()
            .copied()
            .chain(event.enemies.iter().copied());
        for entity in all {
            let Ok((derived, health)) = combatants.get(entity) else {
                continue;
            };
            if health.current <= 0 {
                continue;
            }
            let id = combat.alloc_id();
            combat.init_pending.insert(id, entity);
            combat.init_expected += 1;
            rolls.write(RollRequest {
                id,
                expr: DiceExpr {
                    count: 1,
                    sides: 20,
                    modifier: derived.initiative_mod,
                },
                kind: RollKind::Initiative,
                source: Some(entity),
                advantage: AdvState::Normal,
            });
        }
    }
}

pub fn collect_initiative_rolls(
    mut resolved: EventReader<RollResolved>,
    mut combat: ResMut<CombatFlow>,
    mut encounter: ResMut<EncounterState>,
    mut commands: Commands,
) {
    for event in resolved.read() {
        if event.kind != RollKind::Initiative {
            continue;
        }
        if let Some(entity) = combat.init_pending.remove(&event.id) {
            combat.init_results.push((entity, event.total));
        }
    }

    if combat.init_expected > 0 && combat.init_results.len() >= combat.init_expected {
        let initiatives = std::mem::take(&mut combat.init_results);
        let order: Vec<Entity> = initiatives.iter().map(|(e, _)| *e).collect();
        build_turn_order(&mut commands, &order, &initiatives, &mut encounter);
        combat.init_expected = 0;
    }
}

pub fn register_pending_attacks(
    mut resolved: EventReader<RollResolved>,
    mut combat: ResMut<CombatFlow>,
    mut pending: ResMut<PendingRolls>,
) {
    for event in resolved.read() {
        if let Some(attack) = combat.attacks.remove(&event.id) {
            pending.attacks.insert(
                event.id,
                PendingAttack {
                    attacker: attack.attacker,
                    target: attack.target,
                    attack_total: event.total,
                    damage: attack.damage,
                    is_crit: event.is_nat20,
                },
            );
            combat.advance_ids.insert(event.id);
        }
    }
}

pub fn advance_turn_on_complete(
    mut completed: EventReader<RollAnimationComplete>,
    mut combat: ResMut<CombatFlow>,
    mut encounter: ResMut<EncounterState>,
    health: Query<&Health>,
    mut commands: Commands,
) {
    for event in completed.read() {
        if combat.advance_ids.remove(&event.id) {
            combat.pending_actor = None;
            advance_turn(&mut commands, &mut encounter, &health);
        }
    }
}

pub fn drive_enemy_turns(
    active_enemy: Query<(Entity, &EnemyUnit), With<ActiveTurn>>,
    roster: Res<PartyRoster>,
    health: Query<&Health>,
    data: Res<GameData>,
    mut combat: ResMut<CombatFlow>,
    mut rolls: EventWriter<RollRequest>,
) {
    if combat.pending_actor.is_some() {
        return;
    }
    let Ok((enemy, unit)) = active_enemy.single() else {
        return;
    };
    let Some(archetype) = data.enemies.get(&unit.archetype) else {
        return;
    };
    let Some(target) = roster
        .members
        .iter()
        .copied()
        .find(|e| health.get(*e).map(|h| h.current > 0).unwrap_or(false))
    else {
        return;
    };

    let id = combat.alloc_id();
    rolls.write(RollRequest {
        id,
        expr: DiceExpr {
            count: 1,
            sides: 20,
            modifier: archetype.attack_bonus,
        },
        kind: RollKind::Attack,
        source: Some(enemy),
        advantage: AdvState::Normal,
    });
    combat.attacks.insert(
        id,
        InFlightAttack {
            attacker: enemy,
            target,
            damage: archetype.damage.clone(),
        },
    );
    combat.pending_actor = Some(enemy);
}

#[allow(clippy::too_many_arguments)]
pub fn handle_encounter_ended(
    mut events: EventReader<EncounterEnded>,
    mut next_game: ResMut<NextState<GameState>>,
    mut encounter: ResMut<EncounterState>,
    mut combat: ResMut<CombatFlow>,
    mut map: ResMut<MapState>,
    mut inventory: ResMut<crate::inventory::PartyInventory>,
    data: Res<GameData>,
    mut rng: ResMut<GameRng>,
    mut commands: Commands,
) {
    let mut handled = false;
    for event in events.read() {
        if handled || (encounter.enemies.is_empty() && !combat.fled) {
            continue;
        }
        handled = true;
        let fled = combat.fled;

        for entity in encounter.turn_order.drain(..) {
            commands.entity(entity).remove::<ActiveTurn>();
        }
        for entity in encounter.enemies.drain(..) {
            commands.entity(entity).despawn();
        }
        encounter.turn_index = 0;
        combat.reset_for_new_encounter();

        if let Some(id) = map.current_node {
            if let Some(node) = map.nodes.iter_mut().find(|n| n.id == id) {
                node.completed = true;
            }
        }

        if event.victory {
            inventory.items.extend(choose_loot(&data, 1, &mut rng.0));
            next_game.set(GameState::Exploration);
        } else if fled {
            next_game.set(GameState::Exploration);
        } else {
            next_game.set(GameState::GameOver);
        }
    }
}

/// Apply a Rest node's full heal to the whole party.
pub fn apply_rest(
    mut inventory: ResMut<crate::inventory::PartyInventory>,
    mut party: Query<&mut Health, With<PartyMember>>,
) {
    if !inventory.rest_requested {
        return;
    }
    inventory.rest_requested = false;
    for mut health in &mut party {
        health.current = health.max;
    }
}

// ===== Turn-order helpers ==========================================

fn advance_turn(commands: &mut Commands, encounter: &mut EncounterState, health: &Query<&Health>) {
    if encounter.turn_order.is_empty() {
        return;
    }
    if let Some(current) = encounter.turn_order.get(encounter.turn_index).copied() {
        commands.entity(current).remove::<ActiveTurn>();
    }
    let count = encounter.turn_order.len();
    for step in 1..=count {
        let idx = (encounter.turn_index + step) % count;
        let candidate = encounter.turn_order[idx];
        if health
            .get(candidate)
            .map(|h| h.current > 0)
            .unwrap_or(false)
        {
            encounter.turn_index = idx;
            commands.entity(candidate).insert(ActiveTurn);
            return;
        }
    }
}

fn advance_turn_now(commands: &mut Commands, encounter: &mut EncounterState) {
    if encounter.turn_order.is_empty() {
        return;
    }
    if let Some(current) = encounter.turn_order.get(encounter.turn_index).copied() {
        commands.entity(current).remove::<ActiveTurn>();
    }
    let count = encounter.turn_order.len();
    encounter.turn_index = (encounter.turn_index + 1) % count;
    commands
        .entity(encounter.turn_order[encounter.turn_index])
        .insert(ActiveTurn);
}

// ===== Small view helpers ==========================================

fn hp_bar(ui: &mut egui::Ui, current: i32, max: i32) {
    let max = max.max(1);
    let fraction = (current.max(0) as f32 / max as f32).clamp(0.0, 1.0);
    let color = if fraction > 0.5 {
        theme::VERDANT
    } else if fraction > 0.25 {
        theme::GOLD
    } else {
        theme::BLOOD
    };
    ui.add(
        egui::ProgressBar::new(fraction)
            .desired_width(200.0)
            .fill(color)
            .corner_radius(egui::CornerRadius::same(3))
            .text(egui::RichText::new(format!("{}/{} HP", current.max(0), max)).size(12.0)),
    );
}

fn mana_bar(ui: &mut egui::Ui, current: i32, max: i32) {
    if max <= 0 {
        return;
    }
    let fraction = (current.max(0) as f32 / max as f32).clamp(0.0, 1.0);
    ui.add(
        egui::ProgressBar::new(fraction)
            .desired_width(200.0)
            .fill(theme::ARCANE)
            .corner_radius(egui::CornerRadius::same(3))
            .text(egui::RichText::new(format!("{}/{} Mana", current.max(0), max)).size(12.0)),
    );
}

fn cooldown_line(ui: &mut egui::Ui, cooldowns: &Cooldowns) {
    let active: Vec<String> = cooldowns
        .abilities
        .iter()
        .filter(|ability| ability.remaining > 0)
        .map(|ability| format!("{} {}", ability.ability_id, ability.remaining))
        .collect();
    if !active.is_empty() {
        ui.label(theme::flavour(format!("Cooldowns: {}", active.join(", "))));
    }
}

fn action_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_owned())
            .size(16.0)
            .color(theme::INK),
    )
    .min_size(egui::vec2(118.0, 38.0))
    .fill(theme::PANEL_LIGHT)
}

fn node_glyph(
    node_type: MapNodeType,
    completed: bool,
    current: bool,
) -> (&'static str, egui::Color32) {
    if completed && !current {
        return ("✓ Cleared", theme::INK_DIM);
    }
    match node_type {
        MapNodeType::Combat => ("◆ Combat", theme::INK),
        MapNodeType::Elite => ("◈ Elite", theme::ARCANE),
        MapNodeType::Treasure => ("✦ Treasure", theme::GOLD_BRIGHT),
        MapNodeType::Event => ("❂ Event", theme::INK),
        MapNodeType::Rest => ("☾ Rest", theme::VERDANT),
        MapNodeType::Town => ("Town", theme::VERDANT),
        MapNodeType::Shop => ("Shop", theme::GOLD_BRIGHT),
        MapNodeType::Boss => ("☠ Boss", theme::BLOOD),
        MapNodeType::BonusQuest => ("Bonus Quest", theme::ARCANE),
    }
}

fn node_hint(node_type: MapNodeType) -> &'static str {
    match node_type {
        MapNodeType::Combat => "A fight against a small band.",
        MapNodeType::Elite => "A harder fight — and better spoils.",
        MapNodeType::Treasure => "Loot, no fight.",
        MapNodeType::Event => "Something happens on the road.",
        MapNodeType::Rest => "Catch your breath and recover.",
        MapNodeType::Town => "A safe settlement with story and trade.",
        MapNodeType::Shop => "Buy and sell before the next road.",
        MapNodeType::Boss => "The Starwood's guardian waits here.",
        MapNodeType::BonusQuest => "A final optional challenge after the main boss.",
    }
}

fn class_name(id: &str, data: &GameData) -> String {
    data.classes
        .get(id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn enemy_name(id: &str, data: &GameData) -> String {
    data.enemies
        .get(id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn equipped_line(equipment: &Equipment, data: &GameData, instances: &ItemInstances) -> String {
    let mut parts = Vec::new();
    for id in [
        &equipment.main_hand,
        &equipment.body,
        &equipment.off_hand,
        &equipment.head,
        &equipment.feet,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(item) = base_item_for_instance(id, data, instances) {
            parts.push(item.name.clone());
        }
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(", ")
    }
}

fn combatant_name(
    entity: Entity,
    party: &PartyCombat,
    enemies: &EnemyQuery,
    data: &GameData,
) -> String {
    if let Ok((_, character, ..)) = party.get(entity) {
        character.name.clone()
    } else if let Ok((_, unit, _, _)) = enemies.get(entity) {
        enemy_name(&unit.archetype, data)
    } else {
        "—".to_string()
    }
}
