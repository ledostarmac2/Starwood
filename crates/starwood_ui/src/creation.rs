//! Animated character creation, driven by the [`CreationStep`] sub-state.
//!
//! The flow walks Race → Class → AbilityRoll → SkillsTraits → Review and repeats
//! for up to four party members. We collect choices into a [`CreationDraft`],
//! and on Review confirm we build the member entity using `starwood_core`'s
//! *public* rules functions (`apply_race_mods`, `derived_stats`, …) — we never
//! re-implement rules math here — then fire [`CharacterFinalized`].
//!
//! Ability rolls go through the real contract: the button fires
//! [`RollRequest`]`(AbilityScoreGen)`, the Dice Theater animates each die, and
//! [`collect_ability_rolls`] reads the authoritative [`RollResolved`] results.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::*;

use crate::theme;

/// How a member's ability scores are generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AbilityMethod {
    #[default]
    Roll,
    StandardArray,
    PointBuy,
}

/// All in-progress choices for the member currently being created.
#[derive(Resource)]
pub struct CreationDraft {
    pub member_index: u8,
    pub name: String,
    pub race: Option<RaceId>,
    pub class: Option<ClassId>,
    pub method: AbilityMethod,
    /// Pool of generated values (6 entries) for Roll / StandardArray methods.
    pub rolled_pool: Vec<u8>,
    /// Roll ids whose `RollResolved` we are still waiting on (ability gen).
    pub pending_roll_ids: Vec<u64>,
    /// `assignment[i]` = index into the active pool assigned to ability `i`.
    pub assignment: [Option<usize>; 6],
    /// Point-buy scores per ability (used when `method == PointBuy`).
    pub point_buy: [u8; 6],
    pub chosen_skills: Vec<SkillId>,
    pub chosen_traits: Vec<TraitId>,
    /// Monotonic source of roll ids requested by the UI.
    pub next_roll_id: u64,
}

impl Default for CreationDraft {
    fn default() -> Self {
        Self {
            member_index: 0,
            name: "Adventurer".to_string(),
            race: None,
            class: None,
            method: AbilityMethod::Roll,
            rolled_pool: Vec::new(),
            pending_roll_ids: Vec::new(),
            assignment: [None; 6],
            point_buy: [8; 6],
            chosen_skills: Vec::new(),
            chosen_traits: Vec::new(),
            next_roll_id: 1,
        }
    }
}

const ABILITY_LABELS: [&str; 6] = ["STR", "DEX", "CON", "INT", "WIS", "CHA"];

impl CreationDraft {
    fn alloc_roll_id(&mut self) -> u64 {
        let id = self.next_roll_id;
        self.next_roll_id += 1;
        id
    }

    /// The pool of values being assigned, depending on the active method.
    fn active_pool(&self) -> Vec<u8> {
        match self.method {
            AbilityMethod::Roll => self.rolled_pool.clone(),
            AbilityMethod::StandardArray => standard_array().to_vec(),
            AbilityMethod::PointBuy => self.point_buy.to_vec(),
        }
    }

    /// Base scores (pre-race) implied by the current method + assignment.
    fn base_scores(&self) -> [u8; 6] {
        match self.method {
            AbilityMethod::PointBuy => self.point_buy,
            _ => {
                let pool = self.active_pool();
                let mut out = [8u8; 6];
                for (i, slot) in self.assignment.iter().enumerate() {
                    if let Some(idx) = slot {
                        if let Some(value) = pool.get(*idx) {
                            out[i] = *value;
                        }
                    }
                }
                out
            }
        }
    }

    fn skill_budget(&self, data: &GameData) -> usize {
        // Two class skills, plus one if the race grants the "versatile" trait.
        let racial = self
            .race
            .as_ref()
            .and_then(|id| data.races.get(id))
            .map(|race| race.traits.iter().any(|t| t == "versatile"))
            .unwrap_or(false);
        2 + usize::from(racial)
    }

    /// Whether the current step's choices are complete enough to advance.
    fn can_advance(&self, step: &CreationStep, data: &GameData) -> bool {
        match step {
            CreationStep::Race => self.race.is_some(),
            CreationStep::Class => self.class.is_some(),
            CreationStep::AbilityRoll => match self.method {
                AbilityMethod::PointBuy => validate_point_buy(self.point_buy),
                _ => !self.active_pool().is_empty() && self.assignment.iter().all(Option::is_some),
            },
            CreationStep::SkillsTraits => self.chosen_skills.len() <= self.skill_budget(data),
            CreationStep::Review => true,
            CreationStep::Companions => true,
        }
    }
}

/// Reset for a brand-new run (first member).
pub fn reset_draft_for_new_game(draft: &mut CreationDraft, _data: &GameData) {
    *draft = CreationDraft::default();
}

fn reset_draft_for_next_member(draft: &mut CreationDraft) {
    let next_index = draft.member_index + 1;
    let next_roll_id = draft.next_roll_id;
    let method = draft.method;
    *draft = CreationDraft {
        member_index: next_index,
        name: format!("Adventurer {}", next_index + 1),
        method,
        next_roll_id,
        ..Default::default()
    };
}

// ===== The screen ==================================================

#[allow(clippy::too_many_arguments)]
pub fn creation_ui(
    mut contexts: EguiContexts,
    mut draft: ResMut<CreationDraft>,
    step_state: Res<State<CreationStep>>,
    mut next_step: ResMut<NextState<CreationStep>>,
    mut next_game: ResMut<NextState<GameState>>,
    mut roster: ResMut<PartyRoster>,
    mut planned: ResMut<PlannedCompanions>,
    mut map: ResMut<MapState>,
    data: Res<GameData>,
    mut rolls: EventWriter<RollRequest>,
    mut finalized: EventWriter<CharacterFinalized>,
    mut commands: Commands,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let step = step_state.get().clone();

    // Gentle slide-in whenever the step changes (animated, not snappy).
    let appear = ctx.animate_bool_with_time(
        egui::Id::new(("creation_step", std::mem::discriminant(&step))),
        true,
        0.35,
    );
    let slide = (1.0 - appear) * 26.0;

    // Top: progress through the creation steps.
    egui::TopBottomPanel::top("creation_progress")
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::heading("Forge Your Hero"));
                ui.add_space(16.0);
                step_breadcrumbs(ui, &step);
            });
        });

    // Bottom: navigation.
    egui::TopBottomPanel::bottom("creation_nav")
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(prev) = previous_step(&step) {
                    if ui
                        .button(egui::RichText::new("◀ Back").size(16.0))
                        .clicked()
                    {
                        next_step.set(prev);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can = draft.can_advance(&step, &data);
                    match &step {
                        CreationStep::Review => {
                            // Confirm finalises the member.
                            if ui.add(primary_button("✓ Confirm Member")).clicked() {
                                if let Some(entity) = finalize_member(
                                    &mut commands,
                                    &draft,
                                    &data,
                                    roster.members.len() as u8,
                                ) {
                                    roster.members.push(entity);
                                    finalized.write(CharacterFinalized { entity });
                                    if !roster.members.is_empty() {
                                        next_step.set(CreationStep::Companions);
                                    } else {
                                        reset_draft_for_next_member(&mut draft);
                                        next_step.set(CreationStep::Race);
                                    }
                                }
                            }
                            if !roster.members.is_empty()
                                && ui
                                    .button(egui::RichText::new("Begin Expedition »").size(16.0))
                                    .clicked()
                            {
                                // Finalise this member too, then march.
                                if let Some(entity) = finalize_member(
                                    &mut commands,
                                    &draft,
                                    &data,
                                    roster.members.len() as u8,
                                ) {
                                    roster.members.push(entity);
                                    finalized.write(CharacterFinalized { entity });
                                }
                                begin_expedition(&mut next_game, &mut map);
                            }
                        }
                        CreationStep::Companions => {
                            let can_begin = !roster.members.is_empty()
                                && planned
                                    .classes
                                    .iter()
                                    .all(|id| data.classes.contains_key(id));
                            if ui
                                .add_enabled(can_begin, primary_button("Begin Expedition"))
                                .clicked()
                            {
                                begin_expedition(&mut next_game, &mut map);
                            }
                        }
                        _ => {
                            let label = if matches!(step, CreationStep::SkillsTraits) {
                                "Review ▶"
                            } else {
                                "Continue ▶"
                            };
                            if ui.add_enabled(can, primary_button(label)).clicked() {
                                if let Some(next) = next_step_of(&step) {
                                    next_step.set(next);
                                }
                            }
                        }
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.add_space(slide);
            egui::ScrollArea::vertical().show(ui, |ui| match &step {
                CreationStep::Race => race_step(ui, &mut draft, &data),
                CreationStep::Class => class_step(ui, &mut draft, &data),
                CreationStep::AbilityRoll => ability_step(ui, &mut draft, &data, &mut rolls),
                CreationStep::SkillsTraits => skills_step(ui, &mut draft, &data),
                CreationStep::Review => review_step(ui, &draft, &data),
                CreationStep::Companions => companions_step(ui, &mut planned, &data),
            });
        });

    Ok(())
}

// ===== Steps =======================================================

fn race_step(ui: &mut egui::Ui, draft: &mut CreationDraft, data: &GameData) {
    ui.label(theme::title("Choose a Lineage"));
    ui.label(theme::flavour(
        "Your blood shapes your gifts and your hungers.",
    ));
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut draft.name);
        ui.label(theme::flavour("name"));
    });
    ui.add_space(6.0);

    let mut races: Vec<&RaceData> = data.races.values().collect();
    races.sort_by(|a, b| a.name.cmp(&b.name));

    for race in races {
        let selected = draft.race.as_deref() == Some(race.id.as_str());
        theme::card_frame(selected).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        selected,
                        egui::RichText::new(race.name.as_str()).size(18.0).strong(),
                    )
                    .clicked()
                {
                    draft.race = Some(race.id.clone());
                }
                ui.label(theme::flavour(format!("speed {}", race.speed)));
            });
            ui.label(race.description.as_str());
            ui.label(
                egui::RichText::new(ability_mods_line(&race.ability_mods))
                    .color(theme::GOLD)
                    .size(13.0),
            );
            if !race.traits.is_empty() {
                ui.label(theme::flavour(format!(
                    "Traits: {}",
                    trait_names(&race.traits, data)
                )));
            }
        });
    }
}

fn class_step(ui: &mut egui::Ui, draft: &mut CreationDraft, data: &GameData) {
    ui.label(theme::title("Choose a Calling"));
    ui.label(theme::flavour(
        "How you meet the dark — with steel, spell, or guile.",
    ));
    ui.add_space(8.0);

    let mut classes: Vec<&ClassData> = data.classes.values().collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));

    for class in classes {
        let selected = draft.class.as_deref() == Some(class.id.as_str());
        theme::card_frame(selected).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        selected,
                        egui::RichText::new(class.name.as_str()).size(18.0).strong(),
                    )
                    .clicked()
                {
                    draft.class = Some(class.id.clone());
                }
                ui.label(theme::flavour(format!("d{} hit die", class.hit_die)));
            });
            ui.label(class.description.as_str());
            ui.label(
                egui::RichText::new(format!("Abilities: {}", class.class_abilities.join(", ")))
                    .color(theme::ARCANE)
                    .size(13.0),
            );
            ui.label(theme::flavour(format!(
                "Starting kit: {}",
                item_names(&class.starting_kit, data)
            )));
        });
    }
}

fn ability_step(
    ui: &mut egui::Ui,
    draft: &mut CreationDraft,
    data: &GameData,
    rolls: &mut EventWriter<RollRequest>,
) {
    ui.label(theme::title("Forge the Body & Mind"));
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Method:");
        method_radio(ui, draft, AbilityMethod::Roll, "4d6 drop lowest");
        method_radio(ui, draft, AbilityMethod::StandardArray, "Standard array");
        method_radio(ui, draft, AbilityMethod::PointBuy, "Point buy");
    });
    ui.add_space(8.0);

    match draft.method {
        AbilityMethod::Roll => {
            ui.horizontal(|ui| {
                if ui.add(primary_button("🎲 Roll Abilities")).clicked() {
                    draft.rolled_pool.clear();
                    draft.pending_roll_ids.clear();
                    draft.assignment = [None; 6];
                    for _ in 0..6 {
                        let id = draft.alloc_roll_id();
                        draft.pending_roll_ids.push(id);
                        rolls.write(RollRequest {
                            id,
                            expr: DiceExpr {
                                count: 4,
                                sides: 6,
                                modifier: 0,
                            },
                            kind: RollKind::AbilityScoreGen,
                            source: None,
                            advantage: AdvState::Normal,
                        });
                    }
                }
                if !draft.pending_roll_ids.is_empty() {
                    ui.label(theme::flavour("the dice are still tumbling…"));
                }
            });
            ui.add_space(6.0);
            pool_chips(ui, &draft.rolled_pool, &draft.assignment);
            ui.add_space(6.0);
            if !draft.active_pool().is_empty() {
                assignment_grid(ui, draft);
            }
        }
        AbilityMethod::StandardArray => {
            ui.label(theme::flavour("Assign 15, 14, 13, 12, 10, 8 as you like."));
            ui.add_space(6.0);
            pool_chips(ui, &standard_array(), &draft.assignment);
            ui.add_space(6.0);
            assignment_grid(ui, draft);
        }
        AbilityMethod::PointBuy => {
            point_buy_grid(ui, draft);
        }
    }

    ui.add_space(10.0);
    ui.separator();
    ability_preview(ui, draft, data);
}

fn skills_step(ui: &mut egui::Ui, draft: &mut CreationDraft, data: &GameData) {
    ui.label(theme::title("Talents & Boons"));
    let budget = draft.skill_budget(data);
    let class_skills: Vec<SkillId> = draft
        .class
        .as_ref()
        .and_then(|id| data.classes.get(id))
        .map(|c| c.skill_choices.clone())
        .unwrap_or_default();

    ui.label(theme::flavour(format!(
        "Choose up to {budget} skill{} ({} selected).",
        if budget == 1 { "" } else { "s" },
        draft.chosen_skills.len()
    )));
    ui.add_space(6.0);

    for skill_id in &class_skills {
        let Some(skill) = data.skills.get(skill_id) else {
            continue;
        };
        let mut checked = draft.chosen_skills.contains(skill_id);
        let at_budget = draft.chosen_skills.len() >= budget;
        let enabled = checked || !at_budget;
        let response = ui.add_enabled(
            enabled,
            egui::Checkbox::new(
                &mut checked,
                format!("{}  ({})", skill.name, skill.ability.to_uppercase()),
            ),
        );
        response.on_hover_text(skill.description.as_str());
        if checked && !draft.chosen_skills.contains(skill_id) {
            draft.chosen_skills.push(skill_id.clone());
        } else if !checked {
            draft.chosen_skills.retain(|s| s != skill_id);
        }
    }

    ui.add_space(12.0);
    ui.label(theme::heading("Background Boon (optional, pick one)"));
    let mut traits: Vec<&TraitData> = data.traits.values().collect();
    traits.sort_by(|a, b| a.name.cmp(&b.name));
    for tr in traits {
        let selected = draft.chosen_traits.first().map(String::as_str) == Some(tr.id.as_str());
        let response = ui.selectable_label(selected, tr.name.as_str());
        if response.on_hover_text(tr.description.as_str()).clicked() {
            if selected {
                draft.chosen_traits.clear();
            } else {
                draft.chosen_traits = vec![tr.id.clone()];
            }
        }
    }
}

fn review_step(ui: &mut egui::Ui, draft: &CreationDraft, data: &GameData) {
    ui.label(theme::title("Review"));
    ui.add_space(6.0);

    let race = draft.race.as_ref().and_then(|id| data.races.get(id));
    let class = draft.class.as_ref().and_then(|id| data.classes.get(id));

    theme::hero_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(draft.name.as_str())
                .size(24.0)
                .color(theme::GOLD_BRIGHT)
                .strong(),
        );
        ui.label(format!(
            "{} {}",
            race.map(|r| r.name.as_str()).unwrap_or("?"),
            class.map(|c| c.name.as_str()).unwrap_or("?"),
        ));
        ui.add_space(8.0);

        if let (Some(race), Some(class)) = (race, class) {
            let base = abilities_from(draft.base_scores());
            let final_abilities = apply_race_mods(base, &race.ability_mods);
            let equipment = equipment_from_kit(&class.starting_kit, data);
            let derived = derived_stats(final_abilities, 1, class, race, &equipment, data);

            egui::Grid::new("review_abilities")
                .striped(true)
                .show(ui, |ui| {
                    for (i, label) in ABILITY_LABELS.iter().enumerate() {
                        let score = ability_at(final_abilities, i);
                        ui.label(*label);
                        ui.label(egui::RichText::new(score.to_string()).strong());
                        ui.label(theme::signed(ability_modifier(score)));
                        ui.end_row();
                    }
                });
            ui.add_space(8.0);
            ui.label(format!(
                "HP {}   AC {}   Init {}   Prof {}   Speed {}",
                derived.max_hp,
                derived.armor_class,
                theme::signed(derived.initiative_mod),
                theme::signed(derived.proficiency),
                derived.speed,
            ));
            ui.add_space(4.0);
            if !draft.chosen_skills.is_empty() {
                ui.label(theme::flavour(format!(
                    "Skills: {}",
                    skill_names(&draft.chosen_skills, data)
                )));
            }
            let mut all_traits = race.traits.clone();
            all_traits.extend(draft.chosen_traits.clone());
            if !all_traits.is_empty() {
                ui.label(theme::flavour(format!(
                    "Traits: {}",
                    trait_names(&all_traits, data)
                )));
            }
            ui.label(theme::flavour(format!(
                "Kit: {}",
                item_names(&class.starting_kit, data)
            )));
        }
    });
}

fn companions_step(ui: &mut egui::Ui, planned: &mut PlannedCompanions, data: &GameData) {
    ui.label(theme::title("Plan Your Companions"));
    ui.label(theme::flavour(
        "Choose the three classes that will join your story later.",
    ));
    ui.add_space(8.0);

    let mut classes: Vec<&ClassData> = data.classes.values().collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    for index in 0..planned.classes.len() {
        theme::card_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::heading(format!("Companion {}", index + 1)));
                egui::ComboBox::from_id_salt(("planned_companion", index))
                    .selected_text(class_name(&planned.classes[index], data))
                    .show_ui(ui, |ui| {
                        for class in &classes {
                            ui.selectable_value(
                                &mut planned.classes[index],
                                class.id.clone(),
                                class.name.as_str(),
                            );
                        }
                    });
            });
            if let Some(class) = data.classes.get(&planned.classes[index]) {
                ui.label(theme::flavour(class.description.as_str()));
                ui.label(theme::flavour(format!(
                    "Abilities: {}",
                    class.class_abilities.join(", ")
                )));
            }
        });
    }

    ui.add_space(10.0);
    ui.label(theme::heading("Planned Party"));
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::flavour("Hero"));
        for class_id in &planned.classes {
            ui.label(egui::RichText::new(class_name(class_id, data)).color(theme::GOLD));
        }
    });
}

// ===== Roll collection =============================================

/// Reads authoritative `RollResolved` events for our pending ability-gen rolls
/// and turns each into a score (sum of the top three of the four dice — the
/// "drop lowest" the contract's resolver doesn't apply itself).
pub fn collect_ability_rolls(
    mut resolved: EventReader<RollResolved>,
    mut draft: ResMut<CreationDraft>,
) {
    for event in resolved.read() {
        if event.kind != RollKind::AbilityScoreGen {
            continue;
        }
        if let Some(pos) = draft.pending_roll_ids.iter().position(|id| *id == event.id) {
            draft.pending_roll_ids.remove(pos);
            draft.rolled_pool.push((event.total as u8).clamp(3, 18));
        }
    }
}

// ===== Member construction (shared with the save loader) ===========

fn equipment_from_kit(kit: &[ItemId], data: &GameData) -> Equipment {
    let mut eq = Equipment::default();
    for id in kit {
        let Some(item) = data.items.get(id) else {
            continue;
        };
        match item.slot {
            ItemSlot::Head => eq.head = Some(id.clone()),
            ItemSlot::Body => eq.body = Some(id.clone()),
            ItemSlot::MainHand => eq.main_hand = Some(id.clone()),
            ItemSlot::OffHand => eq.off_hand = Some(id.clone()),
            ItemSlot::Feet => eq.feet = Some(id.clone()),
            ItemSlot::Consumable | ItemSlot::Treasure => {}
        }
    }
    eq
}

fn finalize_member(
    commands: &mut Commands,
    draft: &CreationDraft,
    data: &GameData,
    slot: u8,
) -> Option<Entity> {
    let race = data.races.get(draft.race.as_ref()?)?;
    let class = data.classes.get(draft.class.as_ref()?)?;

    let base = abilities_from(draft.base_scores());
    let abilities = apply_race_mods(base, &race.ability_mods);
    let equipment = equipment_from_kit(&class.starting_kit, data);
    let derived = derived_stats(abilities, 1, class, race, &equipment, data);

    let mut traits = race.traits.clone();
    traits.extend(draft.chosen_traits.clone());

    let entity = commands
        .spawn((
            Character {
                name: draft.name.clone(),
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
            mana_for_class(class, abilities, 1),
            cooldowns_for_class(class),
            SkillSet {
                proficient: draft.chosen_skills.clone(),
            },
            Traits(traits),
            Talents::default(),
            TalentPoints::default(),
            RevivePenalty::default(),
            PlayerCharacter,
            PartyMember { slot },
            equipment,
            SpriteParts {
                base_body: race.sprite_key.clone(),
            },
        ))
        .id();
    Some(entity)
}

/// Rebuild a member entity from a saved record (used by the Continue flow).
pub fn spawn_member_from_saved(
    commands: &mut Commands,
    saved: &SavedCharacter,
    slot: u8,
    data: &GameData,
) -> Option<Entity> {
    let race = data.races.get(&saved.race)?;
    let class = data.classes.get(&saved.class)?;
    let equipment: Equipment = saved.equipment.clone().into();
    let derived = derived_stats(saved.abilities, saved.level, class, race, &equipment, data);

    let entity = commands
        .spawn((
            Character {
                name: saved.name.clone(),
                race: saved.race.clone(),
                class: saved.class.clone(),
                subclass: saved.subclass.clone(),
                level: saved.level,
                xp: saved.xp,
            },
            saved.abilities,
            derived,
            Health {
                current: saved.health_current,
                max: derived.max_hp,
            },
            Mana {
                current: saved.mana_current,
                max: mana_for_class(class, saved.abilities, saved.level).max,
            },
            cooldowns_for_class(class),
            SkillSet {
                proficient: saved.skills.clone(),
            },
            Traits(saved.traits.clone()),
            Talents(saved.talents.clone()),
            TalentPoints(saved.talent_points),
            RevivePenalty {
                stacks: saved.revive_penalty_stacks,
            },
            PartyMember { slot },
            equipment,
            SpriteParts {
                base_body: race.sprite_key.clone(),
            },
        ))
        .id();
    if slot == 0 {
        commands.entity(entity).insert(PlayerCharacter);
    }
    Some(entity)
}

fn begin_expedition(next_game: &mut NextState<GameState>, map: &mut MapState) {
    if map.nodes.is_empty() {
        // A fresh, run-specific seed. It is captured in the map (and the save),
        // so the run stays reproducible from that point on.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x57A2_0D0D);
        *map = generate_map(seed, 8);
    }
    next_game.set(GameState::Exploration);
}

// ===== Small widgets & helpers =====================================

fn primary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_owned())
            .size(16.0)
            .color(theme::INK),
    )
    .fill(theme::GOLD.gamma_multiply(0.30))
    .min_size(egui::vec2(150.0, 32.0))
}

fn method_radio(ui: &mut egui::Ui, draft: &mut CreationDraft, method: AbilityMethod, label: &str) {
    if ui.selectable_label(draft.method == method, label).clicked() {
        draft.method = method;
        draft.assignment = [None; 6];
    }
}

fn pool_chips(ui: &mut egui::Ui, pool: &[u8], assignment: &[Option<usize>; 6]) {
    if pool.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::flavour("rolled:"));
        for (idx, value) in pool.iter().enumerate() {
            let used = assignment.iter().any(|a| *a == Some(idx));
            let color = if used {
                theme::INK_DIM
            } else {
                theme::GOLD_BRIGHT
            };
            ui.label(
                egui::RichText::new(format!("[{value}]"))
                    .color(color)
                    .size(18.0)
                    .strong(),
            );
        }
    });
}

fn assignment_grid(ui: &mut egui::Ui, draft: &mut CreationDraft) {
    let pool = draft.active_pool();
    let snapshot = draft.assignment;
    egui::Grid::new("assign_grid")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for i in 0..6 {
                ui.label(egui::RichText::new(ABILITY_LABELS[i]).strong());
                let current_text = snapshot[i]
                    .and_then(|idx| pool.get(idx))
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".to_string());
                egui::ComboBox::from_id_salt(("assign", i))
                    .selected_text(current_text)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut draft.assignment[i], None, "—");
                        for (idx, value) in pool.iter().enumerate() {
                            let taken_elsewhere = snapshot
                                .iter()
                                .enumerate()
                                .any(|(j, a)| j != i && *a == Some(idx));
                            if !taken_elsewhere {
                                ui.selectable_value(
                                    &mut draft.assignment[i],
                                    Some(idx),
                                    value.to_string(),
                                );
                            }
                        }
                    });
                let modifier = snapshot[i]
                    .and_then(|idx| pool.get(idx))
                    .map(|v| theme::signed(ability_modifier(*v)))
                    .unwrap_or_default();
                ui.label(egui::RichText::new(modifier).color(theme::GOLD));
                ui.end_row();
            }
        });
}

fn point_buy_grid(ui: &mut egui::Ui, draft: &mut CreationDraft) {
    let spent: u32 = draft
        .point_buy
        .iter()
        .map(|s| point_buy_cost(*s).unwrap_or(0) as u32)
        .sum();
    let remaining = 27i32 - spent as i32;
    ui.label(theme::flavour(format!(
        "Points remaining: {remaining} / 27   (scores 8–15)"
    )));
    ui.add_space(4.0);
    egui::Grid::new("pointbuy_grid")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            for i in 0..6 {
                ui.label(egui::RichText::new(ABILITY_LABELS[i]).strong());
                let score = draft.point_buy[i];
                if ui.add_enabled(score > 8, egui::Button::new("−")).clicked() {
                    draft.point_buy[i] -= 1;
                }
                ui.label(egui::RichText::new(score.to_string()).size(16.0).strong());
                let next_cost = point_buy_cost(score + 1);
                let affordable = next_cost
                    .map(|c| {
                        (spent + c as u32).saturating_sub(point_buy_cost(score).unwrap_or(0) as u32)
                            <= 27
                    })
                    .unwrap_or(false);
                if ui
                    .add_enabled(score < 15 && affordable, egui::Button::new("+"))
                    .clicked()
                {
                    draft.point_buy[i] += 1;
                }
                ui.label(
                    egui::RichText::new(theme::signed(ability_modifier(score))).color(theme::GOLD),
                );
                ui.end_row();
            }
        });
}

fn ability_preview(ui: &mut egui::Ui, draft: &CreationDraft, data: &GameData) {
    let Some(race) = draft.race.as_ref().and_then(|id| data.races.get(id)) else {
        return;
    };
    let base = abilities_from(draft.base_scores());
    let final_abilities = apply_race_mods(base, &race.ability_mods);
    ui.label(theme::heading("With lineage bonuses"));
    egui::Grid::new("ability_preview")
        .num_columns(6)
        .striped(true)
        .show(ui, |ui| {
            for label in ABILITY_LABELS {
                ui.label(egui::RichText::new(label).color(theme::GOLD).strong());
            }
            ui.end_row();
            for i in 0..6 {
                let score = ability_at(final_abilities, i);
                ui.label(format!(
                    "{score} ({})",
                    theme::signed(ability_modifier(score))
                ));
            }
            ui.end_row();
        });
}

fn step_breadcrumbs(ui: &mut egui::Ui, current: &CreationStep) {
    let steps = [
        ("Race", CreationStep::Race),
        ("Class", CreationStep::Class),
        ("Abilities", CreationStep::AbilityRoll),
        ("Talents", CreationStep::SkillsTraits),
        ("Review", CreationStep::Review),
        ("Companions", CreationStep::Companions),
    ];
    ui.horizontal(|ui| {
        for (i, (label, step)) in steps.iter().enumerate() {
            let active = step == current;
            let color = if active {
                theme::GOLD_BRIGHT
            } else {
                theme::INK_DIM
            };
            ui.label(egui::RichText::new(*label).color(color).strong());
            if i + 1 < steps.len() {
                ui.label(egui::RichText::new("›").color(theme::INK_DIM));
            }
        }
    });
}

fn next_step_of(step: &CreationStep) -> Option<CreationStep> {
    match step {
        CreationStep::Race => Some(CreationStep::Class),
        CreationStep::Class => Some(CreationStep::AbilityRoll),
        CreationStep::AbilityRoll => Some(CreationStep::SkillsTraits),
        CreationStep::SkillsTraits => Some(CreationStep::Review),
        CreationStep::Review => Some(CreationStep::Companions),
        CreationStep::Companions => None,
    }
}

fn previous_step(step: &CreationStep) -> Option<CreationStep> {
    match step {
        CreationStep::Race => None,
        CreationStep::Class => Some(CreationStep::Race),
        CreationStep::AbilityRoll => Some(CreationStep::Class),
        CreationStep::SkillsTraits => Some(CreationStep::AbilityRoll),
        CreationStep::Review => Some(CreationStep::SkillsTraits),
        CreationStep::Companions => Some(CreationStep::Review),
    }
}

fn abilities_from(scores: [u8; 6]) -> Abilities {
    Abilities {
        str_: scores[0],
        dex: scores[1],
        con: scores[2],
        int: scores[3],
        wis: scores[4],
        cha: scores[5],
    }
}

fn ability_at(a: Abilities, i: usize) -> u8 {
    [a.str_, a.dex, a.con, a.int, a.wis, a.cha][i]
}

fn ability_mods_line(mods: &AbilityMods) -> String {
    let parts = [
        ("STR", mods.str_),
        ("DEX", mods.dex),
        ("CON", mods.con),
        ("INT", mods.int),
        ("WIS", mods.wis),
        ("CHA", mods.cha),
    ];
    let shown: Vec<String> = parts
        .iter()
        .filter(|(_, v)| *v != 0)
        .map(|(name, v)| format!("{name} {}", theme::signed(*v as i32)))
        .collect();
    if shown.is_empty() {
        "No ability bonuses".to_string()
    } else {
        shown.join(", ")
    }
}

fn trait_names(ids: &[TraitId], data: &GameData) -> String {
    join_names(ids, |id| data.traits.get(id).map(|t| t.name.clone()))
}

fn skill_names(ids: &[SkillId], data: &GameData) -> String {
    join_names(ids, |id| data.skills.get(id).map(|s| s.name.clone()))
}

fn item_names(ids: &[ItemId], data: &GameData) -> String {
    join_names(ids, |id| data.items.get(id).map(|i| i.name.clone()))
}

fn class_name(id: &str, data: &GameData) -> String {
    data.classes
        .get(id)
        .map(|class| class.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn join_names(ids: &[String], lookup: impl Fn(&String) -> Option<String>) -> String {
    ids.iter()
        .map(|id| lookup(id).unwrap_or_else(|| id.clone()))
        .collect::<Vec<_>>()
        .join(", ")
}
