#![allow(
    clippy::collapsible_if,
    clippy::manual_contains,
    clippy::needless_range_loop,
    clippy::too_many_arguments,
    clippy::type_complexity
)]

//! STARWOOD — UI crate.
//!
//! This crate owns the entire egui front-end (main menu, animated character
//! creation, the persistent party HUD + action bar, the inventory overlay,
//! the skills tab and character sheet) and the **Dice Theater** — the 2D
//! sprite-driven dice animation that listens for [`RollResolved`], animates a
//! die toward the already-decided value, plays the nat-20 / nat-1 flourishes,
//! and finally fires [`RollAnimationComplete`] so `starwood_core` can apply the
//! consequences of the roll.
//!
//! # Boundaries (per the Shared Contract)
//! * We **only ever request** rolls and **animate the result**. Core is the
//!   sole authority for dice math — we never compute a result here.
//! * Character / enemy / item *world* sprites and the paper-doll belong to
//!   `starwood_render`. We draw menus, panels and the dice/effect sprites only.
//! * We never edit `starwood_core` or `starwood_render`. Anything we wish the
//!   contract exposed is recorded in `NEEDS_FROM_CORE.md`; new shared
//!   dependencies go in `WORKSPACE_DEPS_TODO.md`.
//!
//! The egui drawing systems run in [`EguiPrimaryContextPass`] (required by
//! bevy_egui 0.39's multipass primary context). All world/sprite/animation
//! logic runs in [`Update`].

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use starwood_core::GameState;

mod creation;
mod dice;
mod hud;
mod inventory;
mod menu;
mod theme;

pub use dice::DiceTheaterPlugin;

/// The single public entry point the binary adds to its `App`.
pub struct StarwoodUiPlugin;

impl Plugin for StarwoodUiPlugin {
    fn build(&self, app: &mut App) {
        app
            // ----- shared UI-local state -----
            .init_resource::<creation::CreationDraft>()
            .init_resource::<inventory::PartyInventory>()
            .init_resource::<hud::UiSelection>()
            .init_resource::<hud::CombatFlow>()
            // ----- one-time setup -----
            .add_systems(Startup, setup_ui_camera)
            .add_systems(
                OnEnter(GameState::Exploration),
                inventory::ensure_starter_stash,
            )
            // ----- egui screens (must run in the egui primary-context pass) -----
            //
            // All of these borrow the egui context mutably, so Bevy already
            // serialises them; `.chain()` just pins a deterministic order with
            // the theme applied first and the inventory overlay drawn last.
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
                )
                    .chain(),
            )
            // ----- flow / event wiring (plain Update) -----
            .add_systems(
                Update,
                (
                    creation::collect_ability_rolls,
                    hud::handle_encounter_started,
                    hud::collect_initiative_rolls,
                    hud::register_pending_attacks,
                    hud::advance_turn_on_complete,
                    hud::drive_enemy_turns.run_if(in_state(GameState::Encounter)),
                    hud::handle_encounter_ended,
                    hud::apply_rest,
                ),
            )
            // ----- the Dice Theater (sprites, particles, completion) -----
            .add_plugins(DiceTheaterPlugin);
    }
}

/// Run condition: the party HUD is shown both while exploring and in combat.
fn in_exploration_or_encounter(state: Res<State<GameState>>) -> bool {
    matches!(*state.get(), GameState::Exploration | GameState::Encounter)
}

/// Spawn a 2D camera if the world does not already have one.
///
/// The blueprint nominally gives camera ownership to `starwood_render`, but that
/// crate currently ships as an empty stub, and egui + our dice sprites need a
/// camera to render against. The guard means that once `starwood_render` spawns
/// its own camera we silently defer to it (and the human integrator can decide
/// who owns it — noted in `NEEDS_FROM_CORE.md`).
fn setup_ui_camera(mut commands: Commands, cameras: Query<(), With<Camera>>) {
    if cameras.is_empty() {
        commands.spawn((Camera2d, Name::new("StarwoodCamera")));
    }
}
