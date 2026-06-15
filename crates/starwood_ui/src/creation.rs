//! Animated character creation, driven by the [`CreationStep`] sub-state.
//!
//! New design: the player builds **one full PC** (Race → Class → AbilityRoll →
//! SkillsTraits → Review) and then picks the **classes of the three companions**
//! who will join later (Companions step). We collect choices into a
//! [`CreationDraft`] and drive the live game purely through messages:
//!
//! * forward navigation → `CreationStepAdvanceRequested`
//! * Review confirm → `CharacterBuildRequested` (core spawns the PC entity)
//! * Companions "Begin" → set `PlannedCompanions` + `FinishPartyCreationRequested`
//!
//! Ability rolls go through the contract: the button fires
//! `RollRequest(AbilityScoreGen)`; core now applies the 4d6-drop-lowest itself,
//! so `RollResolved.total` is the authoritative score and the Dice Theater
//! animates it.

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

/// All in-progress choices for the PC being created, plus the companion plan.
#[derive(Resource)]
pub struct CreationDraft {
    pub name: String,
    pub race: Option<RaceId>,
    pub class: Option<ClassId>,
    pub method: AbilityMethod,
    pub rolled_pool: Vec<u8>,
    pub pending_roll_ids: Vec<u64>,
    pub assignment: [Option<usize>; 6],
    pub point_buy: [u8; 6],
    pub chosen_skills: Vec<SkillId>,
    pub chosen_traits: Vec<TraitId>,
    pub companion_classes: [ClassId; 3],
    /// Whether the PC has already been submitted via `CharacterBuildRequested`.
    pub built: bool,
    pub next_roll_id: u64,
}

impl Default for CreationDraft {
    fn default() -> Self {
        Self {
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
            companion_classes: ["fighter".into(), "cleric".into(), "rogue".into()],
            built: false,
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

    fn active_pool(&self) -> Vec<u8> {
        match self.method {
            AbilityMethod::Roll => self.rolled_pool.clone(),
            AbilityMethod::StandardArray => standard_array().to_vec(),
            AbilityMethod::PointBuy => self.point_buy.to_vec(),
        }
    }

    /// Base scores (pre-race/class mods) implied by the method + assignment.
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

    fn base_abilities(&self) -> Abilities {
        let s = self.base_scores();
        Abilities {
            str_: s[0],
            dex: s[1],
            con: s[2],
            int: s[3],
            wis: s[4],
            cha: s[5],
        }
    }

    fn skill_budget(&self, data: &GameData) -> usize {
        let versatile = self
            .race
            .as_ref()
            .and_then(|id| data.races.get(id))
            .map(|race| race.traits.iter().any(|t| t == "versatile"))
            .unwrap_or(false);
        2 + usize::from(versatile)
    }

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

/// Reset for a brand-new campaign.
pub fn reset_draft_for_new_game(draft: &mut CreationDraft) {
    *draft = CreationDraft::default();
}

pub fn spawn_member_from_saved(
    commands: &mut Commands,
    saved: &SavedCharacter,
    slot: u8,
    data: &GameData,
    instances: &ItemInstances,
) -> Option<Entity> {
    let race = data.races.get(&saved.race)?;
    let class = data.classes.get(&saved.class)?;
    let equipment: Equipment = saved.equipment.clone().into();
    let mut derived = derived_stats(saved.abilities, saved.level, class, race, &equipment, data);
    derived.armor_class = armor_class_with_instances(saved.abilities, &equipment, data, instances);
    let max_hp = derived.max_hp.max(1);
    let health = Health {
        current: saved.health_current.clamp(0, max_hp),
        max: max_hp,
    };
    let mut mana = mana_for_class(class, saved.abilities, saved.level);
    mana.current = saved.mana_current.clamp(0, mana.max);

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
            health,
            mana,
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
            equipment,
            SpriteParts {
                base_body: race.sprite_key.clone(),
            },
            PartyMember { slot },
        ))
        .id();
    if slot == 0 {
        commands.entity(entity).insert(PlayerCharacter);
    }
    if health.current <= 0 && slot == 0 {
        commands.entity(entity).insert(Downed);
    }
    Some(entity)
}

// ===== The screen ==================================================

pub fn creation_ui(
    mut contexts: EguiContexts,
    mut draft: ResMut<CreationDraft>,
    step_state: Res<State<CreationStep>>,
    mut next_step: ResMut<NextState<CreationStep>>,
    mut planned: ResMut<PlannedCompanions>,
    data: Res<GameData>,
    mut rolls: EventWriter<RollRequest>,
    mut advance: EventWriter<CreationStepAdvanceRequested>,
    mut build: EventWriter<CharacterBuildRequested>,
    mut finish: EventWriter<FinishPartyCreationRequested>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let step = step_state.get().clone();

    let appear = ctx.animate_bool_with_time(
        egui::Id::new(("creation_step", std::mem::discriminant(&step))),
        true,
        0.35,
    );
    let slide = (1.0 - appear) * 26.0;

    egui::TopBottomPanel::top("creation_progress")
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::heading("Forge Your Hero"));
                ui.add_space(16.0);
                step_breadcrumbs(ui, &step);
            });
        });

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
                            let label = if draft.built {
                                "Companions ▶"
                            } else {
                                "✓ Create Hero"
                            };
                            if ui.add(primary_button(label)).clicked() {
                                if !draft.built {
                                    if let Some(request) = build_request(&draft) {
                                        build.write(request);
                                        draft.built = true;
                                    }
                                }
                                // Advance once the PC exists (also re-advances if the
                                // player stepped Back here after building).
                                if draft.built {
                                    advance.write(CreationStepAdvanceRequested);
                                }
                            }
                        }
                        CreationStep::Companions => {
                            if ui.add(primary_button("Begin Expedition »")).clicked() {
                                planned.classes = draft.companion_classes.clone();
                                finish.write(FinishPartyCreationRequested);
                            }
                        }
                        _ => {
                            let label = if matches!(step, CreationStep::SkillsTraits) {
                                "Review ▶"
                            } else {
                                "Continue ▶"
                            };
                            if ui.add_enabled(can, primary_button(label)).clicked() {
                                advance.write(CreationStepAdvanceRequested);
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
                CreationStep::Companions => companions_step(ui, &mut draft, &data),
            });
        });

    Ok(())
}

fn build_request(draft: &CreationDraft) -> Option<CharacterBuildRequested> {
    Some(CharacterBuildRequested {
        name: draft.name.clone(),
        race: draft.race.clone()?,
        class: draft.class.clone()?,
        abilities: draft.base_abilities(),
        skills: draft.chosen_skills.clone(),
        traits: draft.chosen_traits.clone(),
    })
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
                ui.label(theme::flavour(format!(
                    "d{} hit die  ·  {} mana",
                    class.hit_die, class.base_mana
                )));
            });
            ui.label(class.description.as_str());
            ui.label(
                egui::RichText::new(format!("Abilities: {}", class.class_abilities.join(", ")))
                    .color(theme::ARCANE)
                    .size(13.0),
            );
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
        AbilityMethod::PointBuy => point_buy_grid(ui, draft),
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
            let final_abilities = apply_class_mods(
                apply_race_mods(draft.base_abilities(), &race.ability_mods),
                class,
            );
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
        }
        ui.add_space(4.0);
        ui.label(theme::flavour(
            "Confirm to forge your hero, then choose your companions.",
        ));
    });
}

fn companions_step(ui: &mut egui::Ui, draft: &mut CreationDraft, data: &GameData) {
    ui.label(theme::title("Your Companions"));
    ui.label(theme::flavour(
        "You start alone. Choose the calling of the three who will join you on the road — \
         their names and stories are written when you meet them.",
    ));
    ui.add_space(10.0);

    let mut classes: Vec<&ClassData> = data.classes.values().collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));

    for slot in 0..3 {
        theme::card_frame(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Companion {}", slot + 1))
                        .color(theme::GOLD)
                        .strong(),
                );
                let current = draft.companion_classes[slot].clone();
                let current_name = data
                    .classes
                    .get(&current)
                    .map(|c| c.name.clone())
                    .unwrap_or(current);
                egui::ComboBox::from_id_salt(("companion", slot))
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for class in &classes {
                            ui.selectable_value(
                                &mut draft.companion_classes[slot],
                                class.id.clone(),
                                class.name.as_str(),
                            );
                        }
                    });
            });
            if let Some(class) = data.classes.get(&draft.companion_classes[slot]) {
                ui.label(theme::flavour(class.description.as_str()));
            }
        });
    }
}

// ===== Roll collection =============================================

/// Reads authoritative `RollResolved` ability-gen events. Core already applies
/// 4d6-drop-lowest, so `total` is the score — we no longer compute dice here.
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
            draft.rolled_pool.push(event.total.clamp(3, 18) as u8);
        }
    }
}

// ===== Small widgets & helpers =====================================

fn primary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_owned())
            .size(16.0)
            .color(theme::INK),
    )
    .fill(theme::GOLD.gamma_multiply(0.30))
    .min_size(egui::vec2(160.0, 32.0))
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
                            let taken = snapshot
                                .iter()
                                .enumerate()
                                .any(|(j, a)| j != i && *a == Some(idx));
                            if !taken {
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
                let affordable = point_buy_cost(score + 1)
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
    let class = draft.class.as_ref().and_then(|id| data.classes.get(id));
    let mut final_abilities = apply_race_mods(draft.base_abilities(), &race.ability_mods);
    if let Some(class) = class {
        final_abilities = apply_class_mods(final_abilities, class);
    }
    ui.label(theme::heading("With lineage & calling bonuses"));
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

fn join_names(ids: &[String], lookup: impl Fn(&String) -> Option<String>) -> String {
    ids.iter()
        .map(|id| lookup(id).unwrap_or_else(|| id.clone()))
        .collect::<Vec<_>>()
        .join(", ")
}
