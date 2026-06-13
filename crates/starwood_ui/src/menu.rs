//! Main menu and game-over screens.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::{
    CampaignSaves, Difficulty, EventWriter, GameData, GameDifficulty, GameState, MapState,
    PartyRoster,
};

use crate::creation::{CreationDraft, reset_draft_for_new_game};
use crate::theme;

/// Where the single save slot lives, relative to the working directory.
pub const SAVE_PATH: &str = "starwood_save.ron";

#[allow(clippy::too_many_arguments)]
pub fn main_menu_ui(
    mut contexts: EguiContexts,
    mut next_state: ResMut<NextState<GameState>>,
    mut exit: EventWriter<AppExit>,
    mut commands: Commands,
    mut draft: ResMut<CreationDraft>,
    mut roster: ResMut<PartyRoster>,
    mut map: ResMut<MapState>,
    mut difficulty: ResMut<GameDifficulty>,
    campaign_saves: Res<CampaignSaves>,
    data: Res<GameData>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_DEEP))
        .show(ctx, |ui| {
            ui.add_space(ui.available_height() * 0.18);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("STARWOOD")
                        .size(64.0)
                        .color(theme::GOLD_BRIGHT)
                        .strong(),
                );
                ui.label(theme::flavour(
                    "A tactical tale spun from seed and starlight",
                ));
                ui.add_space(40.0);

                ui.horizontal(|ui| {
                    ui.label(theme::heading("Difficulty"));
                    difficulty_button(ui, &mut difficulty, Difficulty::Easy, "Easy");
                    difficulty_button(ui, &mut difficulty, Difficulty::Normal, "Normal");
                    difficulty_button(ui, &mut difficulty, Difficulty::Hard, "Hard");
                });
                ui.label(theme::flavour(match difficulty.0 {
                    Difficulty::Easy => "Easy quietly bends the hero's d20s toward hope.",
                    Difficulty::Normal => "Normal keeps honest d20 odds and standard enemy tuning.",
                    Difficulty::Hard => "Hard keeps honest d20 odds with sharper enemy tuning.",
                }));
                ui.add_space(18.0);

                ui.label(theme::heading("Campaign Slots"));
                for (index, slot) in campaign_saves.slots.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let label = slot
                            .metadata
                            .as_ref()
                            .map(|metadata| {
                                format!(
                                    "Slot {}: {} ({:?}){}",
                                    index + 1,
                                    metadata.name,
                                    metadata.difficulty,
                                    if slot.autosave { " autosave" } else { "" }
                                )
                            })
                            .unwrap_or_else(|| format!("Slot {}: Empty", index + 1));
                        ui.label(theme::flavour(label));
                        let delete_enabled = slot.metadata.is_some();
                        if ui
                            .add_enabled(delete_enabled, egui::Button::new("Delete"))
                            .clicked()
                        {
                            let _ = std::fs::remove_file(SAVE_PATH);
                        }
                    });
                }
                ui.add_space(18.0);

                let button = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                    ui.add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new(label).size(22.0).color(theme::INK))
                            .min_size(egui::vec2(260.0, 48.0)),
                    )
                };

                if button(ui, "New Game", true).clicked() {
                    // Fresh run: clear any prior party and roll a new map seed.
                    for entity in roster.members.drain(..) {
                        commands.entity(entity).despawn();
                    }
                    *map = MapState::default();
                    reset_draft_for_new_game(&mut draft, &data);
                    next_state.set(GameState::CharacterCreation);
                }

                let has_save = std::path::Path::new(SAVE_PATH).exists();
                let continue_btn = button(ui, "Continue", has_save);
                if !has_save {
                    continue_btn.on_hover_text("No saved expedition found.");
                } else if continue_btn.clicked() {
                    if try_load_save(&mut commands, &mut roster, &mut map, &data) {
                        next_state.set(GameState::Exploration);
                    }
                }

                if button(ui, "Quit", true).clicked() {
                    exit.write(AppExit::Success);
                }
            });
        });
    Ok(())
}

fn difficulty_button(
    ui: &mut egui::Ui,
    difficulty: &mut GameDifficulty,
    value: Difficulty,
    label: &str,
) {
    if ui
        .selectable_label(difficulty.0 == value, label)
        .on_hover_text(match value {
            Difficulty::Easy => "Hidden d20 bonus for player rolls.",
            Difficulty::Normal => "Uniform d20 rolls.",
            Difficulty::Hard => "Uniform d20 rolls, tougher enemies.",
        })
        .clicked()
    {
        difficulty.0 = value;
    }
}

pub fn game_over_ui(
    mut contexts: EguiContexts,
    mut next_state: ResMut<NextState<GameState>>,
    mut exit: EventWriter<AppExit>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_DEEP))
        .show(ctx, |ui| {
            ui.add_space(ui.available_height() * 0.30);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("YOUR STORY ENDS")
                        .size(48.0)
                        .color(theme::BLOOD)
                        .strong(),
                );
                ui.label(theme::flavour(
                    "The Starwood keeps its secrets a while longer.",
                ));
                ui.add_space(36.0);
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Return to Menu").size(22.0))
                            .min_size(egui::vec2(260.0, 48.0)),
                    )
                    .clicked()
                {
                    next_state.set(GameState::MainMenu);
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("Quit").size(20.0))
                            .min_size(egui::vec2(260.0, 44.0)),
                    )
                    .clicked()
                {
                    exit.write(AppExit::Success);
                }
            });
        });
    Ok(())
}

/// Best-effort load of the save slot, reconstructing party entities and the map.
/// Returns `true` only if the save parsed and a party was spawned.
fn try_load_save(
    commands: &mut Commands,
    roster: &mut PartyRoster,
    map: &mut MapState,
    data: &GameData,
) -> bool {
    let Ok(text) = std::fs::read_to_string(SAVE_PATH) else {
        return false;
    };
    let Ok(save) = starwood_core::deserialize_save(&text) else {
        return false;
    };

    for entity in roster.members.drain(..) {
        commands.entity(entity).despawn();
    }
    *map = save.map.clone();

    for (slot, saved) in save.party.iter().take(4).enumerate() {
        if let Some(entity) =
            crate::creation::spawn_member_from_saved(commands, saved, slot as u8, data)
        {
            roster.members.push(entity);
        }
    }
    !roster.members.is_empty()
}
