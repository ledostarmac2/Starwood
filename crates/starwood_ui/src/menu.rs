//! Main menu (difficulty + campaign slots) and game-over screen.
//!
//! New Game fires [`NewGameRequested`] so core resets the campaign, generates the
//! map, and moves to creation. Continue defers to `save::process_pending_load`.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::{
    CampaignSaves, CampaignSeed, Difficulty, EventWriter, GameDifficulty, GameState,
    NewGameRequested,
};

use crate::creation::{CreationDraft, reset_draft_for_new_game};
use crate::save::{self, PendingLoad};
use crate::theme;

/// Where the single autosave slot lives, relative to the working directory.
pub const SAVE_PATH: &str = "starwood_save.ron";

pub fn main_menu_ui(
    mut contexts: EguiContexts,
    mut exit: EventWriter<AppExit>,
    mut draft: ResMut<CreationDraft>,
    mut difficulty: ResMut<GameDifficulty>,
    mut saves: ResMut<CampaignSaves>,
    mut new_game: EventWriter<NewGameRequested>,
    mut pending: ResMut<PendingLoad>,
    seed: Res<CampaignSeed>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(theme::BG_DEEP))
        .show(ctx, |ui| {
            ui.add_space(ui.available_height() * 0.14);
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
                ui.add_space(34.0);

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
                ui.add_space(16.0);

                ui.label(theme::heading("Campaign Slots"));
                let mut delete_request = false;
                for (index, slot) in saves.slots.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let label = slot
                            .metadata
                            .as_ref()
                            .map(|m| {
                                format!(
                                    "Slot {}: {} · {:?} · {}{}",
                                    index + 1,
                                    m.name,
                                    m.difficulty,
                                    m.progress_label,
                                    if slot.autosave { " (autosave)" } else { "" }
                                )
                            })
                            .unwrap_or_else(|| format!("Slot {}: Empty", index + 1));
                        ui.label(theme::flavour(label));
                        if index == 0
                            && ui
                                .add_enabled(slot.metadata.is_some(), egui::Button::new("Delete"))
                                .clicked()
                        {
                            delete_request = true;
                        }
                    });
                }
                if delete_request {
                    save::delete_save(&mut saves);
                }
                ui.add_space(16.0);

                let button = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                    ui.add_enabled(
                        enabled,
                        egui::Button::new(egui::RichText::new(label).size(22.0).color(theme::INK))
                            .min_size(egui::vec2(280.0, 48.0)),
                    )
                };

                if button(ui, "New Game", true).clicked() {
                    reset_draft_for_new_game(&mut draft);
                    new_game.write(NewGameRequested { seed: seed.0 });
                }

                let has_save = save::save_exists();
                let continue_btn = button(ui, "Continue", has_save);
                if !has_save {
                    continue_btn.on_hover_text("No saved expedition found.");
                } else if continue_btn.clicked() {
                    pending.0 = true;
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
