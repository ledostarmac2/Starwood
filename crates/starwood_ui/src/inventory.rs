//! Inventory overlay, shop, character sheet, skills tab, and talent tree.
//!
//! Items are **rolled instances**: `Equipment` slots and core's `Inventory` hold
//! [`ItemInstanceId`]s resolved through [`ItemInstances`] + [`base_item_for_instance`].
//! Tiles draw rarity-coloured frames and tooltips list the rolled affixes.
//!
//! Equip/unequip mutates the member's [`Equipment`] and fires [`EquipmentChanged`]
//! / [`InventoryChanged`]; consumable use, buying, and selling go through core's
//! [`ConsumableUseRequested`] / [`ShopTransactionRequested`] messages.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use rand_chacha::ChaCha8Rng;
use starwood_core::*;

use crate::hud::UiState;
use crate::theme;

/// The current shop's rolled stock (instances already live in `ItemInstances`).
#[derive(Resource, Default)]
pub struct ShopStock {
    pub items: Vec<ItemInstanceId>,
    pub open: bool,
}

/// Roll `count` shop items into `ItemInstances`, returning their ids.
pub fn roll_shop_stock(
    data: &GameData,
    instances: &mut ItemInstances,
    rng: &mut ChaCha8Rng,
    level: u32,
    count: usize,
) -> Vec<ItemInstanceId> {
    let mut bases: Vec<&ItemData> = data
        .items
        .values()
        .filter(|item| !matches!(item.slot, ItemSlot::Treasure))
        .collect();
    bases.sort_by(|a, b| a.id.cmp(&b.id));
    if bases.is_empty() {
        return Vec::new();
    }
    (0..count)
        .map(|i| {
            let base = bases[i % bases.len()];
            roll_item_instance(base, data, instances, rng, level).instance_id
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EquipSlot {
    Head,
    Body,
    MainHand,
    OffHand,
    Feet,
}

impl EquipSlot {
    fn of(slot: &ItemSlot) -> Option<Self> {
        match slot {
            ItemSlot::Head => Some(Self::Head),
            ItemSlot::Body => Some(Self::Body),
            ItemSlot::MainHand => Some(Self::MainHand),
            ItemSlot::OffHand => Some(Self::OffHand),
            ItemSlot::Feet => Some(Self::Feet),
            ItemSlot::Consumable | ItemSlot::Treasure => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Body => "Body",
            Self::MainHand => "Main Hand",
            Self::OffHand => "Off Hand",
            Self::Feet => "Feet",
        }
    }

    fn get(self, e: &Equipment) -> &Option<ItemInstanceId> {
        match self {
            Self::Head => &e.head,
            Self::Body => &e.body,
            Self::MainHand => &e.main_hand,
            Self::OffHand => &e.off_hand,
            Self::Feet => &e.feet,
        }
    }

    fn slot_mut(self, e: &mut Equipment) -> &mut Option<ItemInstanceId> {
        match self {
            Self::Head => &mut e.head,
            Self::Body => &mut e.body,
            Self::MainHand => &mut e.main_hand,
            Self::OffHand => &mut e.off_hand,
            Self::Feet => &mut e.feet,
        }
    }
}

const ALL_SLOTS: [EquipSlot; 5] = [
    EquipSlot::Head,
    EquipSlot::Body,
    EquipSlot::MainHand,
    EquipSlot::OffHand,
    EquipSlot::Feet,
];

type SheetQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Character,
        &'static mut Equipment,
        &'static Abilities,
        &'static Derived,
        &'static SkillSet,
        &'static Traits,
        &'static mut Talents,
        &'static mut TalentPoints,
        &'static Health,
        &'static PartyMember,
    ),
    Without<EnemyUnit>,
>;

/// Deferred item action so we never mutate while drawing the list.
enum ItemAction {
    Equip(ItemInstanceId),
    Use(ItemInstanceId),
    Unequip(EquipSlot),
}

pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut inventory_open: ResMut<InventoryOpen>,
    mut ui_state: ResMut<UiState>,
    mut party: SheetQuery,
    mut inventory: ResMut<Inventory>,
    instances: Res<ItemInstances>,
    gold: Res<Gold>,
    data: Res<GameData>,
    mut equip_changed: EventWriter<EquipmentChanged>,
    mut inv_changed: EventWriter<InventoryChanged>,
    mut consume: EventWriter<ConsumableUseRequested>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if ui_state.selected_member.is_none() {
        ui_state.selected_member = party.iter().min_by_key(|(.., m)| m.slot).map(|t| t.0);
    }
    let selected = ui_state.selected_member;
    let mut roster: Vec<(Entity, String, u8)> = party
        .iter()
        .map(|(e, c, .., m)| (e, c.name.clone(), m.slot))
        .collect();
    roster.sort_by_key(|(_, _, slot)| *slot);

    // ---- Inventory overlay ----
    if inventory_open.0 {
        let mut open = true;
        let mut new_selected = selected;
        let mut action: Option<ItemAction> = None;

        egui::Window::new(
            egui::RichText::new("Inventory & Gear")
                .size(22.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .resizable(true)
        .default_width(620.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                member_tabs(ui, &roster, selected, &mut new_selected);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} / {} slots   ·   {} gold",
                            inventory.items.len(),
                            INVENTORY_CAPACITY,
                            gold.0
                        ))
                        .color(theme::GOLD),
                    );
                });
            });
            ui.separator();

            if let Some(member) = selected.and_then(|m| party.get(m).ok()) {
                let (_, _, equipment, _, _, _, _, _, _, _, _) = member;
                ui.columns(2, |cols| {
                    cols[0].label(theme::heading("Equipped"));
                    for slot in ALL_SLOTS {
                        let current = slot.get(equipment).clone();
                        theme::card_frame(false).show(&mut cols[0], |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(slot.label())
                                        .color(theme::GOLD)
                                        .strong(),
                                );
                                match &current {
                                    Some(id) => {
                                        item_label(ui, id, &instances, &data);
                                        if ui.small_button("Unequip").clicked() {
                                            action = Some(ItemAction::Unequip(slot));
                                        }
                                    }
                                    None => {
                                        ui.label(theme::flavour("— empty —"));
                                    }
                                }
                            });
                        });
                    }

                    cols[1].label(theme::heading("Stash"));
                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .show(&mut cols[1], |ui| {
                            for item_id in inventory.items.iter() {
                                let Some(base) = base_item_for_instance(item_id, &data, &instances)
                                else {
                                    continue;
                                };
                                let frame = rarity_frame(item_id, &instances, &data);
                                frame
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            item_label(ui, item_id, &instances, &data);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if base.consumable.is_some() {
                                                        if ui.small_button("Use").clicked() {
                                                            action = Some(ItemAction::Use(
                                                                item_id.clone(),
                                                            ));
                                                        }
                                                    } else if EquipSlot::of(&base.slot).is_some()
                                                        && ui.small_button("Equip").clicked()
                                                    {
                                                        action = Some(ItemAction::Equip(
                                                            item_id.clone(),
                                                        ));
                                                    }
                                                },
                                            );
                                        });
                                    })
                                    .response
                                    .on_hover_ui(|ui| item_tooltip(ui, item_id, &instances, &data));
                            }
                            if inventory.items.is_empty() {
                                ui.label(theme::flavour("The stash is empty."));
                            }
                        });
                });
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });

        // Apply the deferred action.
        if let (Some(member), Some(action)) = (selected, action) {
            match action {
                ItemAction::Use(item) => {
                    consume.write(ConsumableUseRequested {
                        actor: member,
                        item,
                    });
                }
                ItemAction::Unequip(slot) => {
                    if let Ok((_, _, mut equipment, ..)) = party.get_mut(member) {
                        if let Some(prev) = slot.slot_mut(&mut equipment).take() {
                            if add_item_to_inventory(&mut inventory, prev.clone()) {
                                equip_changed.write(EquipmentChanged { entity: member });
                                inv_changed.write(InventoryChanged);
                            } else {
                                // No room — put it back on.
                                *slot.slot_mut(&mut equipment) = Some(prev);
                            }
                        }
                    }
                }
                ItemAction::Equip(item) => {
                    let target_slot = data
                        .items
                        .get(
                            instances
                                .instances
                                .get(&item)
                                .map(|i| &i.base)
                                .unwrap_or(&item),
                        )
                        .and_then(|base| EquipSlot::of(&base.slot));
                    if let (Some(target), Ok((_, _, mut equipment, ..))) =
                        (target_slot, party.get_mut(member))
                    {
                        if let Some(index) = inventory.items.iter().position(|id| id == &item) {
                            inventory.items.remove(index);
                            let previous = target.slot_mut(&mut equipment).replace(item);
                            if let Some(previous) = previous {
                                let _ = add_item_to_inventory(&mut inventory, previous);
                            }
                            equip_changed.write(EquipmentChanged { entity: member });
                            inv_changed.write(InventoryChanged);
                        }
                    }
                }
            }
        }

        ui_state.selected_member = new_selected;
        inventory_open.0 = open;
    }

    // ---- Character sheet ----
    if ui_state.show_sheet {
        let mut open = true;
        egui::Window::new(
            egui::RichText::new("Character Sheet")
                .size(20.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .default_width(420.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            if let Some((_, character, _, abilities, derived, _, traits, _, _, health, _)) =
                selected.and_then(|m| party.get(m).ok())
            {
                character_sheet(ui, character, *abilities, *derived, *health, traits, &data);
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });
        ui_state.show_sheet = open;
    }

    // ---- Skills tab ----
    if ui_state.show_skills {
        let mut open = true;
        egui::Window::new(
            egui::RichText::new("Skills")
                .size(20.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .default_width(440.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            if let Some((_, character, _, abilities, _, skills, _, _, _, _, _)) =
                selected.and_then(|m| party.get(m).ok())
            {
                skills_tab(ui, *abilities, skills, character.level, &data);
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });
        ui_state.show_skills = open;
    }

    // ---- Talent tree + subclass ----
    if ui_state.show_talents {
        let mut open = true;
        let mut pick: Option<TalentId> = None;
        let mut subclass_pick: Option<ClassId> = None;
        egui::Window::new(
            egui::RichText::new("Talents")
                .size(20.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .default_width(460.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            if let Some((_, character, _, _, _, _, _, talents, points, _, _)) =
                selected.and_then(|m| party.get(m).ok())
            {
                talent_tree(
                    ui,
                    character,
                    talents,
                    *points,
                    &data,
                    &mut pick,
                    &mut subclass_pick,
                );
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });
        if let Some(member) = selected {
            if let Ok((_, mut character, _, _, _, _, _, mut talents, mut points, _, _)) =
                party.get_mut(member)
            {
                if let Some(node_id) = pick {
                    if let Some(node) = talent_node(&character.class, &node_id, &data) {
                        let prereqs = node.requires.iter().all(|r| talents.0.contains(r));
                        if points.0 >= node.cost
                            && prereqs
                            && !talents.0.contains(&node_id)
                            && character.level >= node.unlock_level
                        {
                            talents.0.push(node_id);
                            points.0 -= node.cost;
                        }
                    }
                }
                if let Some(class_id) = subclass_pick {
                    if can_unlock_subclass(&character) {
                        character.subclass = Some(class_id);
                    }
                }
            }
        }
        ui_state.show_talents = open;
    }

    Ok(())
}

// ===== Shop ========================================================

pub fn shop_ui(
    mut contexts: EguiContexts,
    mut shop: ResMut<ShopStock>,
    inventory: Res<Inventory>,
    instances: Res<ItemInstances>,
    gold: Res<Gold>,
    data: Res<GameData>,
    mut shop_tx: EventWriter<ShopTransactionRequested>,
) -> Result {
    if !shop.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let mut open = true;
    egui::Window::new(
        egui::RichText::new("Merchant")
            .size(22.0)
            .color(theme::GOLD_BRIGHT),
    )
    .open(&mut open)
    .resizable(true)
    .default_width(620.0)
    .frame(theme::hero_frame())
    .show(ctx, |ui| {
        ui.label(
            egui::RichText::new(format!("Your gold: {}", gold.0))
                .color(theme::GOLD)
                .strong(),
        );
        ui.separator();
        ui.columns(2, |cols| {
            cols[0].label(theme::heading("For Sale"));
            egui::ScrollArea::vertical()
                .id_salt("shop_buy")
                .max_height(360.0)
                .show(&mut cols[0], |ui| {
                    for item_id in shop.items.iter() {
                        let Some(item) = instances.instances.get(item_id) else {
                            continue;
                        };
                        let price = buy_price(item);
                        rarity_frame(item_id, &instances, &data)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    item_label(ui, item_id, &instances, &data);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let afford = gold.0 >= price;
                                            if ui
                                                .add_enabled(
                                                    afford,
                                                    egui::Button::new(format!("Buy {price}g")),
                                                )
                                                .clicked()
                                            {
                                                shop_tx.write(ShopTransactionRequested {
                                                    item: item_id.clone(),
                                                    transaction: ShopTransaction::Buy,
                                                });
                                            }
                                        },
                                    );
                                });
                            })
                            .response
                            .on_hover_ui(|ui| item_tooltip(ui, item_id, &instances, &data));
                    }
                });

            cols[1].label(theme::heading("Your Stash"));
            egui::ScrollArea::vertical()
                .id_salt("shop_sell")
                .max_height(360.0)
                .show(&mut cols[1], |ui| {
                    for item_id in inventory.items.iter() {
                        let Some(item) = instances.instances.get(item_id) else {
                            continue;
                        };
                        let price = sell_price(item);
                        rarity_frame(item_id, &instances, &data)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    item_label(ui, item_id, &instances, &data);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button(format!("Sell {price}g")).clicked() {
                                                shop_tx.write(ShopTransactionRequested {
                                                    item: item_id.clone(),
                                                    transaction: ShopTransaction::Sell,
                                                });
                                            }
                                        },
                                    );
                                });
                            })
                            .response
                            .on_hover_ui(|ui| item_tooltip(ui, item_id, &instances, &data));
                    }
                });
        });
        ui.add_space(6.0);
        if ui.button("Leave").clicked() {
            shop.open = false;
        }
    });
    if !open {
        shop.open = false;
    }
    Ok(())
}

// ===== Sub-views ===================================================

fn member_tabs(
    ui: &mut egui::Ui,
    roster: &[(Entity, String, u8)],
    selected: Option<Entity>,
    new_selected: &mut Option<Entity>,
) {
    for (entity, name, _) in roster {
        if ui
            .selectable_label(selected == Some(*entity), name.as_str())
            .clicked()
        {
            *new_selected = Some(*entity);
        }
    }
}

fn character_sheet(
    ui: &mut egui::Ui,
    character: &Character,
    abilities: Abilities,
    derived: Derived,
    health: Health,
    traits: &Traits,
    data: &GameData,
) {
    ui.label(
        egui::RichText::new(character.name.as_str())
            .size(22.0)
            .color(theme::GOLD_BRIGHT)
            .strong(),
    );
    let subclass = character
        .subclass
        .as_ref()
        .map(|s| format!(" / {}", class_display(s, data)))
        .unwrap_or_default();
    ui.label(format!(
        "Level {} {} {}{subclass}",
        character.level,
        race_name(&character.race, data),
        class_display(&character.class, data)
    ));
    ui.label(theme::flavour(format!("XP {}", character.xp)));
    ui.add_space(8.0);

    egui::Grid::new("sheet_abilities")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Ability").strong());
            ui.label(egui::RichText::new("Score").strong());
            ui.label(egui::RichText::new("Mod").strong());
            ui.end_row();
            for (label, score) in ability_rows(abilities) {
                ui.label(label);
                ui.label(score.to_string());
                ui.label(
                    egui::RichText::new(theme::signed(ability_modifier(score))).color(theme::GOLD),
                );
                ui.end_row();
            }
        });

    ui.add_space(8.0);
    ui.label(theme::heading("Derived"));
    ui.label(format!(
        "HP {}/{}   ·   AC {}   ·   Init {}   ·   Prof {}   ·   Speed {}",
        health.current,
        derived.max_hp,
        derived.armor_class,
        theme::signed(derived.initiative_mod),
        theme::signed(derived.proficiency),
        derived.speed
    ));

    ui.add_space(8.0);
    ui.label(theme::heading("Traits"));
    if traits.0.is_empty() {
        ui.label(theme::flavour("None"));
    } else {
        for id in &traits.0 {
            if let Some(tr) = data.traits.get(id) {
                ui.label(egui::RichText::new(tr.name.as_str()).strong());
                ui.label(theme::flavour(tr.description.as_str()));
            }
        }
    }
}

fn skills_tab(
    ui: &mut egui::Ui,
    abilities: Abilities,
    skills: &SkillSet,
    level: u32,
    data: &GameData,
) {
    ui.label(theme::flavour(
        "d20 + ability modifier + proficiency (if trained).",
    ));
    ui.add_space(6.0);
    let mut all: Vec<&SkillData> = data.skills.values().collect();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    egui::Grid::new("skills_grid")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Skill").strong());
            ui.label(egui::RichText::new("Bonus").strong());
            ui.label(egui::RichText::new("Trained").strong());
            ui.end_row();
            for skill in all {
                let proficient = skills.proficient.iter().any(|id| id == &skill.id);
                let bonus = skill_bonus(abilities, skill, skills, level);
                ui.label(format!(
                    "{}  ({})",
                    skill.name,
                    skill.ability.to_uppercase()
                ))
                .on_hover_text(skill.description.as_str());
                ui.label(
                    egui::RichText::new(theme::signed(bonus))
                        .color(theme::GOLD)
                        .strong(),
                );
                ui.label(if proficient { "●" } else { "" });
                ui.end_row();
            }
        });
}

fn talent_tree(
    ui: &mut egui::Ui,
    character: &Character,
    talents: &Talents,
    points: TalentPoints,
    data: &GameData,
    pick: &mut Option<TalentId>,
    subclass_pick: &mut Option<ClassId>,
) {
    ui.label(
        egui::RichText::new(format!("{} — {} talent points", character.name, points.0))
            .color(theme::GOLD)
            .strong(),
    );
    ui.add_space(6.0);

    if can_unlock_subclass(character) {
        ui.label(theme::heading("Choose a Subclass (level 10)"));
        let mut classes: Vec<&ClassData> = data.classes.values().collect();
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        ui.horizontal_wrapped(|ui| {
            for class in classes {
                if class.id != character.class && ui.button(class.name.as_str()).clicked() {
                    *subclass_pick = Some(class.id.clone());
                }
            }
        });
        ui.separator();
    }

    let Some(tree) = data.talent_trees.get(&character.class) else {
        ui.label(theme::flavour("No talent tree for this class yet."));
        return;
    };
    let mut nodes: Vec<&TalentNodeData> = tree.nodes.iter().collect();
    nodes.sort_by_key(|n| (n.rank, n.id.clone()));
    for node in nodes {
        let taken = talents.0.contains(&node.id);
        let prereqs = node.requires.iter().all(|r| talents.0.contains(r));
        let affordable =
            points.0 >= node.cost && character.level >= node.unlock_level && prereqs && !taken;
        theme::card_frame(taken).show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = if taken {
                    theme::GOLD_BRIGHT
                } else {
                    theme::INK
                };
                ui.label(egui::RichText::new(&node.name).color(color).strong());
                ui.label(theme::flavour(format!(
                    "rank {} · cost {}",
                    node.rank, node.cost
                )));
                if taken {
                    ui.label(
                        egui::RichText::new("learned")
                            .color(theme::VERDANT)
                            .size(12.0),
                    );
                } else if ui
                    .add_enabled(affordable, egui::Button::new("Learn"))
                    .clicked()
                {
                    *pick = Some(node.id.clone());
                }
            });
            ui.label(theme::flavour(node.description.as_str()));
            if node.unlock_level > character.level {
                ui.label(
                    egui::RichText::new(format!("requires level {}", node.unlock_level))
                        .color(theme::BLOOD)
                        .size(12.0),
                );
            }
        });
    }
}

// ===== Item rendering helpers ======================================

fn item_label(ui: &mut egui::Ui, id: &str, instances: &ItemInstances, data: &GameData) {
    let name = item_display_name(id, instances, data);
    let color = rarity_color(id, instances, data);
    ui.label(egui::RichText::new(name).color(color).strong());
}

fn item_tooltip(ui: &mut egui::Ui, id: &str, instances: &ItemInstances, data: &GameData) {
    let Some(base) = base_item_for_instance(id, data, instances) else {
        return;
    };
    let instance = instances.instances.get(id);
    let rarity = instance.map(|i| i.rarity).unwrap_or(Rarity::Common);
    let rarity_name = data
        .rarities
        .get(&rarity)
        .map(|r| r.name.clone())
        .unwrap_or_else(|| format!("{rarity:?}"));
    ui.label(
        egui::RichText::new(base.name.as_str())
            .color(rarity_color(id, instances, data))
            .strong()
            .size(15.0),
    );
    ui.label(theme::flavour(rarity_name));
    ui.label(base.description.as_str());
    if base.armor_bonus != 0 {
        ui.label(format!("Armor +{}", base.armor_bonus));
    }
    if let Some(dmg) = &base.damage {
        ui.label(format!(
            "Damage {}d{}{}",
            dmg.count,
            dmg.sides,
            theme::signed(dmg.modifier)
        ));
    }
    if let Some(cat) = base.consumable {
        ui.label(theme::flavour(format!("Consumable ({cat:?})")));
    }
    if let Some(instance) = instance {
        for affix in &instance.affixes {
            ui.label(
                egui::RichText::new(format!("{} {}", affix.name, theme::signed(affix.value)))
                    .color(theme::ARCANE),
            );
        }
        ui.label(theme::flavour(format!("Value {}g", instance.value)));
    }
}

fn item_display_name(id: &str, instances: &ItemInstances, data: &GameData) -> String {
    base_item_for_instance(id, data, instances)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn rarity_color(id: &str, instances: &ItemInstances, data: &GameData) -> egui::Color32 {
    instances
        .instances
        .get(id)
        .and_then(|i| rarity_frame_color(data, i.rarity))
        .map(theme::frame_color)
        .unwrap_or(theme::INK)
}

fn rarity_frame(id: &str, instances: &ItemInstances, data: &GameData) -> egui::Frame {
    theme::rarity_card(rarity_color(id, instances, data), false)
}

fn ability_rows(a: Abilities) -> [(&'static str, u8); 6] {
    [
        ("Strength", a.str_),
        ("Dexterity", a.dex),
        ("Constitution", a.con),
        ("Intelligence", a.int),
        ("Wisdom", a.wis),
        ("Charisma", a.cha),
    ]
}

fn talent_node<'a>(class: &str, node_id: &str, data: &'a GameData) -> Option<&'a TalentNodeData> {
    data.talent_trees
        .get(class)
        .and_then(|tree| tree.nodes.iter().find(|n| n.id == node_id))
}

fn race_name(id: &str, data: &GameData) -> String {
    data.races
        .get(id)
        .map(|r| r.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn class_display(id: &str, data: &GameData) -> String {
    data.classes
        .get(id)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| id.to_string())
}
