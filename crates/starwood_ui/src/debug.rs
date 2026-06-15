//! Lightweight in-game debug overlay.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::{
    ActiveTurn, Character, EncounterState, EnemyUnit, GameDifficulty, GameState, Gold, Health,
    Inventory, ItemInstances, Mana, PartyMember, RollResolved,
};

use crate::theme;

#[derive(Resource, Default)]
pub struct DebugOverlay {
    pub open: bool,
    pub last_roll: Option<String>,
}

pub fn toggle_debug_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<DebugOverlay>,
) {
    if keyboard.just_pressed(KeyCode::F1) {
        overlay.open = !overlay.open;
    }
}

pub fn record_last_roll(mut rolls: MessageReader<RollResolved>, mut overlay: ResMut<DebugOverlay>) {
    for roll in rolls.read() {
        overlay.last_roll = Some(format!(
            "#{id} {kind:?}: rolls={rolls:?} total={total} nat20={nat20} nat1={nat1}",
            id = roll.id,
            kind = roll.kind,
            rolls = roll.rolls,
            total = roll.total,
            nat20 = roll.is_nat20,
            nat1 = roll.is_nat1
        ));
    }
}

pub fn debug_overlay_ui(
    mut contexts: EguiContexts,
    overlay: Res<DebugOverlay>,
    state: Res<State<GameState>>,
    difficulty: Res<GameDifficulty>,
    encounter: Res<EncounterState>,
    gold: Res<Gold>,
    inventory: Res<Inventory>,
    instances: Res<ItemInstances>,
    party: Query<(&Character, &Health, Option<&Mana>), With<PartyMember>>,
    active: Query<(Option<&Character>, Option<&EnemyUnit>), With<ActiveTurn>>,
) -> Result {
    if !overlay.open {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    egui::Window::new(
        egui::RichText::new("Debug")
            .size(18.0)
            .color(theme::GOLD_BRIGHT),
    )
    .resizable(true)
    .default_width(320.0)
    .frame(theme::panel_frame())
    .show(ctx, |ui| {
        ui.label(theme::flavour("F1 toggles this overlay."));
        ui.separator();
        ui.label(format!("State: {:?}", state.get()));
        ui.label(format!("Difficulty: {:?}", difficulty.0));
        ui.label(format!("Gold: {}", gold.0));
        ui.label(format!("Inventory: {}/20", inventory.items.len()));
        ui.label(format!("Item instances: {}", instances.instances.len()));
        ui.label(format!("Enemies: {}", encounter.enemies.len()));
        ui.label(format!("Turn order: {}", encounter.turn_order.len()));

        ui.separator();
        ui.label(theme::heading("Active"));
        if let Some((character, enemy)) = active.iter().next() {
            let who = character
                .map(|c| c.name.clone())
                .or_else(|| enemy.map(|e| e.archetype.clone()))
                .unwrap_or_else(|| "—".to_string());
            ui.label(who);
        } else {
            ui.label(theme::flavour("none"));
        }

        if party.iter().next().is_some() {
            ui.separator();
            ui.label(theme::heading("Party"));
            for (character, health, mana) in &party {
                let mana_text = mana
                    .map(|m| format!("  MP {}/{}", m.current, m.max))
                    .unwrap_or_default();
                ui.label(format!(
                    "{}: HP {}/{}{mana_text}",
                    character.name, health.current, health.max
                ));
            }
        }

        if let Some(last_roll) = &overlay.last_roll {
            ui.separator();
            ui.label(theme::heading("Last Roll (raw dice + skewed total)"));
            ui.label(theme::flavour(last_roll));
        }
    });

    Ok(())
}
