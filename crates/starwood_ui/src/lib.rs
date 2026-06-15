#![allow(
    clippy::collapsible_if,
    clippy::manual_contains,
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity
)]

//! STARWOOD — UI crate.
//!
//! Owns the entire egui front-end (menus, difficulty + save slots, character
//! creation, the persistent HUD, inventory/shop, the combat UI, a debug overlay)
//! and the **Dice Theater**.
//!
//! # How the UI drives the live game
//! `starwood_core` is message-driven and authoritative. The UI never mutates
//! game rules; it **fires request messages** and reflects the resulting state:
//!
//! * New campaign → `NewGameRequested`
//! * Creation steps → `CharacterBuildRequested`, `CreationStepAdvanceRequested`,
//!   `FinishPartyCreationRequested`
//! * Enter a fight → `EncounterRequested`
//! * Combat actions → `CombatActionRequest`, `SurrenderRequested`
//! * Items/economy → `ConsumableUseRequested`, `ShopTransactionRequested`
//! * PC revive → `ReviveAttempt`
//!
//! Core resolves rolls (difficulty-aware), builds initiative/turn order, applies
//! damage on `RollAnimationComplete`, and handles death / encounter-end /
//! full-heal. The UI owns only: the Dice Theater, **turn advancement** (core does
//! not move `ActiveTurn`), equip swaps, shop-stock rolling, and presentation.
//!
//! egui draws in [`EguiPrimaryContextPass`]; world/sprite/flow logic in `Update`.

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use starwood_core::GameState;

mod creation;
mod debug;
mod dice;
mod hud;
mod inventory;
mod menu;
mod save;
mod theme;

pub use dice::DiceTheaterPlugin;

/// The single public entry point the binary adds to its `App`.
pub struct StarwoodUiPlugin;

impl Plugin for StarwoodUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // ----- shared UI-local state -----
            .init_resource::<creation::CreationDraft>()
            .init_resource::<hud::UiState>()
            .init_resource::<hud::CombatFlow>()
            .init_resource::<inventory::ShopStock>()
            .init_resource::<debug::DebugOverlay>()
            .init_resource::<save::PendingLoad>()
            // ----- one-time setup -----
            .add_systems(Startup, (setup_ui_camera, save::index_saves))
            .add_systems(
                OnEnter(GameState::Exploration),
                (hud::despawn_stale_enemies, save::autosave_on_exploration),
            )
            // ----- egui screens (must run in the egui primary-context pass) -----
            //
            // All of these borrow the egui context mutably, so Bevy already
            // serialises them; `.chain()` just pins a deterministic order with
            // the theme first and overlays (inventory/shop/debug) drawn last.
            .add_systems(
                EguiPrimaryContextPass,
                (
                    theme::install_theme,
                    menu::main_menu_ui.run_if(in_state(GameState::MainMenu)),
                    menu::game_over_ui.run_if(in_state(GameState::GameOver)),
                    creation::creation_ui.run_if(in_state(GameState::CharacterCreation)),
                    hud::party_panel_ui.run_if(in_exploration_or_encounter),
                    hud::exploration_ui.run_if(in_state(GameState::Exploration)),
                    hud::encounter_ui.run_if(in_state(GameState::Encounter)),
                    inventory::inventory_ui,
                    inventory::shop_ui.run_if(in_state(GameState::Exploration)),
                    debug::debug_overlay_ui,
                )
                    .chain(),
            )
            // ----- flow / event wiring (plain Update) -----
            .add_systems(
                Update,
                (
                    debug::toggle_debug_overlay,
                    debug::record_last_roll,
                    creation::collect_ability_rolls,
                    save::process_pending_load,
                    hud::drive_enemy_turns.run_if(in_state(GameState::Encounter)),
                    hud::advance_turn_after_action.run_if(in_state(GameState::Encounter)),
                ),
            )
            // ----- the Dice Theater (sprites, particles, completion) -----
            .add_plugins(DiceTheaterPlugin);
    }
}

/// Run condition: the party HUD shows both while exploring and in combat.
fn in_exploration_or_encounter(state: Res<State<GameState>>) -> bool {
    matches!(*state.get(), GameState::Exploration | GameState::Encounter)
}

/// Spawn a 2D camera if the world does not already have one.
///
/// Render is the nominal camera owner but ships as a stub, and egui + our dice
/// sprites need a camera. The guard makes this a no-op once render spawns one.
fn setup_ui_camera(mut commands: Commands, cameras: Query<(), With<Camera>>) {
    if cameras.is_empty() {
        commands.spawn((Camera2d, Name::new("StarwoodCamera")));
    }
}
