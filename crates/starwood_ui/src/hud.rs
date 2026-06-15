//! Persistent HUD, the Exploration map, and the encounter combat UI.
//!
//! Combat is **message-driven**: the UI fires `CombatActionRequest` /
//! `SurrenderRequested` / `ConsumableUseRequested`; core builds the attack roll,
//! resolves it (difficulty-aware), and — after the Dice Theater fires
//! `RollAnimationComplete` — applies damage and detects death / encounter end.
//!
//! Core does *not* move `ActiveTurn`, so the one combat job the UI owns is
//! **turn advancement**: after an action resolves (a roll completes, or a no-roll
//! action ends the turn) we move `ActiveTurn` to the next living combatant.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::*;

use crate::theme;

/// Player-side selections + which overlay windows are open.
#[derive(Resource, Default)]
pub struct UiState {
    pub selected_member: Option<Entity>,
    pub target_enemy: Option<Entity>,
    pub show_sheet: bool,
    pub show_skills: bool,
    pub show_talents: bool,
}

/// UI-side combat bookkeeping (core owns resolution).
#[derive(Resource, Default)]
pub struct CombatFlow {
    /// An attack is mid-flight; advance the turn when its roll animation ends.
    pub pending_actor: Option<Entity>,
    /// A no-roll action (item/move-as-action) asked to end the turn next frame.
    pub end_turn_now: bool,
}

// ===== Query aliases ===============================================

type PartyView<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Character,
        &'static Health,
        Option<&'static Mana>,
        &'static Derived,
        &'static Equipment,
        &'static PartyMember,
        Option<&'static PlayerCharacter>,
        Option<&'static Downed>,
    ),
    Without<EnemyUnit>,
>;

type PartyCombat<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Character,
        &'static Health,
        Option<&'static mut Mana>,
        &'static Equipment,
        &'static mut PartyMember,
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
    mut ui_state: ResMut<UiState>,
    gold: Res<Gold>,
    data: Res<GameData>,
    mut revive: EventWriter<ReviveAttempt>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let in_encounter = *game_state.get() == GameState::Encounter;
    let active_entity = active.iter().next();

    egui::SidePanel::left("party_panel")
        .resizable(false)
        .min_width(264.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::heading("Your Party"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("⦿ {} gold", gold.0))
                            .color(theme::GOLD_BRIGHT)
                            .strong(),
                    );
                });
            });
            ui.separator();

            let mut members: Vec<_> = party.iter().collect();
            members.sort_by_key(|(.., member, _, _)| member.slot);
            for (entity, character, health, mana, derived, _equipment, member, pc, downed) in
                members
            {
                let is_active = Some(entity) == active_entity;
                let selected = ui_state.selected_member == Some(entity);
                theme::card_frame(is_active).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let title_color = if downed.is_some() {
                            theme::BLOOD
                        } else if is_active {
                            theme::GOLD_BRIGHT
                        } else {
                            theme::INK
                        };
                        let mark = if pc.is_some() { "★ " } else { "" };
                        let title = egui::RichText::new(format!("{mark}{}", character.name))
                            .size(16.0)
                            .color(title_color)
                            .strong();
                        if ui.selectable_label(selected, title).clicked() {
                            ui_state.selected_member = Some(entity);
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
                        "rank {}  ·  L{} {}  ·  AC {}",
                        member.slot,
                        character.level,
                        class_name(&character.class, &data),
                        derived.armor_class
                    )));
                    hp_bar(ui, health.current, health.max);
                    if let Some(mana) = mana {
                        if mana.max > 0 {
                            mana_bar(ui, mana.current, mana.max);
                        }
                    }
                    if downed.is_some() {
                        let cost = REVIVE_GOLD_COST_BASE;
                        if ui
                            .add_enabled(
                                gold.0 >= cost,
                                egui::Button::new(format!("Revive ({cost}g)")),
                            )
                            .clicked()
                        {
                            revive.write(ReviveAttempt { entity });
                        }
                    }
                });
            }

            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Inventory").clicked() {
                    inventory_open.0 = !inventory_open.0;
                }
                if ui.button("Sheet").clicked() {
                    ui_state.show_sheet = !ui_state.show_sheet;
                }
                if ui.button("Skills").clicked() {
                    ui_state.show_skills = !ui_state.show_skills;
                }
                if ui.button("Talents").clicked() {
                    ui_state.show_talents = !ui_state.show_talents;
                }
            });

            if ui_state.selected_member.is_none() {
                ui_state.selected_member =
                    active_entity.or_else(|| party.iter().next().map(|t| t.0));
            }
        });

    Ok(())
}

// ===== Exploration map =============================================

pub fn exploration_ui(
    mut contexts: EguiContexts,
    party: Query<&Character, With<PartyMember>>,
    mut map: ResMut<MapState>,
    data: Res<GameData>,
    mut rng: ResMut<GameRng>,
    mut inventory: ResMut<Inventory>,
    mut instances: ResMut<ItemInstances>,
    mut gold: ResMut<Gold>,
    mut shop: ResMut<crate::inventory::ShopStock>,
    antagonist: Res<Antagonist>,
    mut encounter_req: EventWriter<EncounterRequested>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let party_level = party.iter().map(|c| c.level).max().unwrap_or(1);

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_DEEP).inner_margin(egui::Margin::same(16)))
        .show(ctx, |ui| {
            ui.label(theme::title("The Starwood"));
            ui.label(theme::flavour(format!(
                "{} — a {} who would {}.",
                antagonist.identity, antagonist.role, antagonist.purpose
            )));
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
                        let button = egui::Button::new(egui::RichText::new(glyph).size(15.0).color(color))
                            .min_size(egui::vec2(120.0, 32.0))
                            .fill(if is_current { theme::PANEL_LIGHT } else { theme::PANEL });
                        if ui.add_enabled(is_reachable, button).on_hover_text(node_hint(node.node_type)).clicked() {
                            clicked = Some(node.id);
                        }
                    }
                });
                ui.add_space(3.0);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.label(theme::flavour(
                "◆ combat  ◈ elite  ✦ treasure  ❂ event  ☾ rest  ⌂ town  ⚖ shop  ☠ boss  ✪ quest  ✓ cleared",
            ));

            if let Some(node_id) = clicked {
                enter_node(
                    node_id, &mut map, party_level, &data, &mut rng, &mut inventory, &mut instances,
                    &mut gold, &mut shop, &mut encounter_req,
                );
            }
        });

    Ok(())
}

fn enter_node(
    node_id: u32,
    map: &mut MapState,
    party_level: u32,
    data: &GameData,
    rng: &mut GameRng,
    inventory: &mut Inventory,
    instances: &mut ItemInstances,
    gold: &mut Gold,
    shop: &mut crate::inventory::ShopStock,
    encounter_req: &mut EventWriter<EncounterRequested>,
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
        MapNodeType::Combat | MapNodeType::Elite | MapNodeType::Boss | MapNodeType::BonusQuest => {
            // Core spawns enemies, rolls initiative, and switches to Encounter.
            encounter_req.write(EncounterRequested {
                difficulty: node_type,
            });
        }
        MapNodeType::Treasure => {
            for id in roll_loot_instances(data, party_level, instances, &mut rng.0) {
                let _ = add_item_to_inventory(inventory, id);
            }
            mark_completed(map, node_id);
        }
        MapNodeType::Shop | MapNodeType::Town => {
            shop.items =
                crate::inventory::roll_shop_stock(data, instances, &mut rng.0, party_level, 6);
            shop.open = true;
            mark_completed(map, node_id);
        }
        MapNodeType::Event => {
            // A small choose-your-path boon: a purse of gold scaled to depth.
            gold.0 = gold.0.saturating_add(20 + party_level * 10);
            mark_completed(map, node_id);
        }
        MapNodeType::Rest => {
            mark_completed(map, node_id);
        }
    }
}

fn mark_completed(map: &mut MapState, node_id: u32) {
    if let Some(node) = map.nodes.iter_mut().find(|n| n.id == node_id) {
        node.completed = true;
    }
}

// ===== Encounter (turn order, foes, action bar) ====================

pub fn encounter_ui(
    mut contexts: EguiContexts,
    mut party: PartyCombat,
    enemies: EnemyQuery,
    active: Query<Entity, With<ActiveTurn>>,
    mut ui_state: ResMut<UiState>,
    mut combat: ResMut<CombatFlow>,
    encounter: Res<EncounterState>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    inventory: Res<Inventory>,
    mut action_req: EventWriter<CombatActionRequest>,
    mut surrender: EventWriter<SurrenderRequested>,
    mut consume: EventWriter<ConsumableUseRequested>,
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

    // Active actor reach (for rank-valid targeting).
    let actor = active_entity.filter(|e| party.get(*e).is_ok());
    let (actor_rank, actor_reach) = actor
        .and_then(|e| party.get(e).ok())
        .map(|(_, _, _, _, equipment, member)| {
            (member.slot, weapon_reach(equipment, &instances, &data))
        })
        .unwrap_or((0, Reach::Melee));

    // The frontmost living foe is always targetable so a melee party can never
    // soft-lock once the front rank dies (core does not rank-collapse enemies).
    let front_rank = enemies
        .iter()
        .filter(|(_, _, h, _)| h.current > 0)
        .map(|(_, unit, _, _)| unit.slot)
        .min();

    // Foes (right) — reachable or frontmost living foes are selectable.
    egui::SidePanel::right("enemy_panel")
        .resizable(false)
        .min_width(238.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.label(theme::heading("Foes"));
            ui.separator();
            let mut foes: Vec<_> = enemies.iter().collect();
            foes.sort_by_key(|(.., unit, _, _)| unit.slot);
            for (entity, unit, health, _) in foes {
                if health.current <= 0 {
                    continue;
                }
                let reachable = can_reach_rank(Rank(actor_rank), Rank(unit.slot), actor_reach)
                    || Some(unit.slot) == front_rank;
                let targeted = ui_state.target_enemy == Some(entity);
                theme::card_frame(targeted).show(ui, |ui| {
                    let label = egui::RichText::new(format!(
                        "R{} {}",
                        unit.slot,
                        enemy_name(&unit.archetype, &data)
                    ))
                    .size(15.0)
                    .strong()
                    .color(if reachable {
                        theme::INK
                    } else {
                        theme::INK_DIM
                    });
                    if ui
                        .add_enabled(reachable, egui::Button::selectable(targeted, label))
                        .clicked()
                    {
                        ui_state.target_enemy = Some(entity);
                    }
                    hp_bar(ui, health.current, health.max);
                });
            }
        });

    // Action bar (bottom).
    egui::TopBottomPanel::bottom("action_bar")
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            let Some(actor) = actor else {
                ui.horizontal(|ui| {
                    ui.label(theme::heading("Foe's turn"));
                    ui.label(theme::flavour("the enemy moves against you…"));
                });
                return;
            };
            let locked = combat.pending_actor.is_some();

            // Validate / default the target to a reachable, living foe.
            let can_target = |slot: u8, current: i32| {
                current > 0
                    && (can_reach_rank(Rank(actor_rank), Rank(slot), actor_reach)
                        || Some(slot) == front_rank)
            };
            let target_valid = ui_state.target_enemy.is_some_and(|t| {
                enemies
                    .get(t)
                    .map(|(_, unit, h, _)| can_target(unit.slot, h.current))
                    .unwrap_or(false)
            });
            if !target_valid {
                ui_state.target_enemy = enemies
                    .iter()
                    .filter(|(_, unit, h, _)| can_target(unit.slot, h.current))
                    .min_by_key(|(_, unit, _, _)| unit.slot)
                    .map(|(e, ..)| e);
            }
            let target = ui_state.target_enemy;
            let has_target = target.is_some();

            // Mana available for a Cast?
            let mana_left = party
                .get(actor)
                .ok()
                .and_then(|(_, _, _, mana, _, _)| mana.map(|m| m.current))
                .unwrap_or(0);
            let potion = find_potion(&inventory, &instances, &data);

            ui.horizontal_wrapped(|ui| {
                ui.label(theme::heading("Your move"));
                ui.add_space(8.0);

                if ui
                    .add_enabled(!locked && has_target, action_button("⚔ Attack"))
                    .clicked()
                {
                    if let Some(target) = target {
                        action_req.write(CombatActionRequest {
                            actor,
                            target,
                            action: CombatAction::Attack,
                        });
                        combat.pending_actor = Some(actor);
                    }
                }
                if ui
                    .add_enabled(
                        !locked && has_target && mana_left >= 2,
                        action_button("✦ Cast"),
                    )
                    .clicked()
                {
                    if let Some(target) = target {
                        if let Ok((_, _, _, Some(mut mana), _, _)) = party.get_mut(actor) {
                            mana.current = (mana.current - 2).max(0);
                        }
                        action_req.write(CombatActionRequest {
                            actor,
                            target,
                            action: CombatAction::Attack,
                        });
                        combat.pending_actor = Some(actor);
                    }
                }
                if ui.add_enabled(!locked, action_button("⇄ Move")).clicked() {
                    rank_swap_forward(actor, &mut party);
                }
                if ui
                    .add_enabled(!locked && potion.is_some(), action_button("🜂 Potion"))
                    .clicked()
                {
                    if let Some(item) = potion {
                        consume.write(ConsumableUseRequested { actor, item });
                        combat.end_turn_now = true;
                    }
                }
                if ui
                    .add_enabled(!locked, action_button("🏳 Surrender"))
                    .clicked()
                {
                    surrender.write(SurrenderRequested { actor });
                }
            });

            // AoE friendly-fire awareness for the current target's rank.
            if let Some(target) = target {
                if let Ok((_, unit, _, _)) = enemies.get(target) {
                    let targets = rank_targets(&party, &enemies);
                    if aoe_friendly_fire_risk(&targets, CombatSide::Party, Rank(unit.slot), 1) {
                        ui.label(
                            egui::RichText::new("⚠ An AoE at this rank could catch your own.")
                                .color(theme::BLOOD)
                                .size(12.0),
                        );
                    }
                }
            }
            if locked {
                ui.label(theme::flavour("the dice decide…"));
            }
        });

    Ok(())
}

/// Swap the active member one rank forward (toward the front) with a neighbour.
fn rank_swap_forward(actor: Entity, party: &mut PartyCombat) {
    let mut roster: Vec<(Entity, u8)> =
        party.iter().map(|(e, _, _, _, _, m)| (e, m.slot)).collect();
    roster.sort_by_key(|(_, slot)| *slot);
    let Some(pos) = roster.iter().position(|(e, _)| *e == actor) else {
        return;
    };
    // Prefer swapping with the member just ahead; otherwise just behind.
    let other = if pos > 0 {
        roster.get(pos - 1)
    } else {
        roster.get(pos + 1)
    };
    let Some(&(other_entity, other_slot)) = other else {
        return;
    };
    let actor_slot = roster[pos].1;
    if let Ok((.., mut member)) = party.get_mut(actor) {
        member.slot = other_slot;
    }
    if let Ok((.., mut member)) = party.get_mut(other_entity) {
        member.slot = actor_slot;
    }
}

// ===== Flow systems (Update, Encounter) ============================

/// Despawn leftover foe entities when we return to Exploration. Core clears the
/// encounter's enemy list but never despawns the entities (dead foes have no
/// `PartyMember`/`PlayerCharacter`, so its death handler skips them), so we tidy
/// the stage here. Safe because no encounter is active in Exploration.
pub fn despawn_stale_enemies(mut commands: Commands, enemies: Query<Entity, With<EnemyUnit>>) {
    for entity in &enemies {
        commands.entity(entity).despawn();
    }
}

/// Drive enemy turns by firing an attack request against the first living PC.
pub fn drive_enemy_turns(
    active_enemy: Query<(Entity, &EnemyUnit), With<ActiveTurn>>,
    roster: Res<PartyRoster>,
    health: Query<&Health>,
    mut combat: ResMut<CombatFlow>,
    mut action_req: EventWriter<CombatActionRequest>,
) {
    if combat.pending_actor.is_some() {
        return;
    }
    let Ok((enemy, _)) = active_enemy.single() else {
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
    action_req.write(CombatActionRequest {
        actor: enemy,
        target,
        action: CombatAction::Attack,
    });
    combat.pending_actor = Some(enemy);
}

/// Advance the turn once an action resolves (a roll animation completed, or a
/// no-roll action ended the turn). This is the only turn-mover in the game.
pub fn advance_turn_after_action(
    mut completed: EventReader<RollAnimationComplete>,
    mut combat: ResMut<CombatFlow>,
    mut encounter: ResMut<EncounterState>,
    health: Query<&Health>,
    mut commands: Commands,
) {
    let mut advance = false;
    let roll_done = completed.read().count() > 0;
    if roll_done && combat.pending_actor.is_some() {
        combat.pending_actor = None;
        advance = true;
    }
    if combat.end_turn_now {
        combat.end_turn_now = false;
        advance = true;
    }
    if advance {
        advance_turn(&mut commands, &mut encounter, &health);
    }
}

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
            .desired_width(210.0)
            .fill(color)
            .corner_radius(egui::CornerRadius::same(3))
            .text(egui::RichText::new(format!("{}/{} HP", current.max(0), max)).size(12.0)),
    );
}

fn mana_bar(ui: &mut egui::Ui, current: i32, max: i32) {
    let max = max.max(1);
    let fraction = (current.max(0) as f32 / max as f32).clamp(0.0, 1.0);
    ui.add(
        egui::ProgressBar::new(fraction)
            .desired_width(210.0)
            .fill(theme::ARCANE)
            .corner_radius(egui::CornerRadius::same(3))
            .text(egui::RichText::new(format!("{}/{} MP", current.max(0), max)).size(11.0)),
    );
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

fn weapon_reach(equipment: &Equipment, instances: &ItemInstances, data: &GameData) -> Reach {
    let tags = equipment
        .main_hand
        .as_ref()
        .and_then(|id| base_item_for_instance(id, data, instances))
        .map(|item| item.tags.clone())
        .unwrap_or_default();
    if tags.iter().any(|t| t == "ranged") {
        Reach::Ranged
    } else if tags.iter().any(|t| t == "reach") {
        Reach::Reach
    } else {
        Reach::Melee
    }
}

fn rank_targets(party: &PartyCombat, enemies: &EnemyQuery) -> Vec<RankTarget> {
    let mut targets = Vec::new();
    for (entity, _, _, _, _, member) in party.iter() {
        targets.push(RankTarget {
            entity,
            side: CombatSide::Party,
            rank: Rank(member.slot),
        });
    }
    for (entity, unit, _, _) in enemies.iter() {
        targets.push(RankTarget {
            entity,
            side: CombatSide::Enemy,
            rank: Rank(unit.slot),
        });
    }
    targets
}

fn find_potion(
    inventory: &Inventory,
    instances: &ItemInstances,
    data: &GameData,
) -> Option<ItemInstanceId> {
    inventory
        .items
        .iter()
        .find(|id| {
            base_item_for_instance(id, data, instances)
                .and_then(|base| base.consumable)
                .map(|cat| cat == ConsumableCategory::Potion)
                .unwrap_or(false)
        })
        .cloned()
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
        MapNodeType::Town => ("⌂ Town", theme::VERDANT),
        MapNodeType::Shop => ("⚖ Shop", theme::GOLD),
        MapNodeType::Boss => ("☠ Boss", theme::BLOOD),
        MapNodeType::BonusQuest => ("✪ Quest", theme::ARCANE),
    }
}

fn node_hint(node_type: MapNodeType) -> &'static str {
    match node_type {
        MapNodeType::Combat => "A fight against a small band.",
        MapNodeType::Elite => "A harder fight — and better spoils.",
        MapNodeType::Treasure => "Rolled loot, no fight.",
        MapNodeType::Event => "Something happens on the road.",
        MapNodeType::Rest => "A safe place to pause.",
        MapNodeType::Town => "A town with a merchant.",
        MapNodeType::Shop => "Buy and sell with gold.",
        MapNodeType::Boss => "The Starwood's guardian waits here.",
        MapNodeType::BonusQuest => "An optional post-boss trial.",
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

fn combatant_name(
    entity: Entity,
    party: &PartyCombat,
    enemies: &EnemyQuery,
    data: &GameData,
) -> String {
    if let Ok((_, character, ..)) = party.get(entity) {
        character.name.clone()
    } else if let Ok((_, unit, _, _)) = enemies.get(entity) {
        format!("R{} {}", unit.slot, enemy_name(&unit.archetype, data))
    } else {
        "—".to_string()
    }
}
