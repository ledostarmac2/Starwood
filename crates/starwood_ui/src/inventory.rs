//! Inventory overlay, character sheet, and skills tab.
//!
//! The inventory is an overlay toggled by core's [`InventoryOpen`] flag, so the
//! world keeps rendering underneath. Equipping / unequipping mutates the
//! member's [`Equipment`] and fires [`EquipmentChanged`] (which the render crate
//! listens for to re-compose the paper-doll) and [`InventoryChanged`].
//!
//! Item icons are *owned by the render crate*; until it exposes textures we
//! show simple lettered tiles as a stand-in (a UI affordance, not a world
//! sprite). Swapping to `egui::Image` later needs no structural change here.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use starwood_core::*;

use crate::hud::UiSelection;
use crate::theme;

/// The party's shared stash of unequipped items, plus a couple of flow flags.
#[derive(Resource, Default)]
pub struct PartyInventory {
    pub items: Vec<ItemId>,
    /// Set by a Rest map node; consumed by `hud::apply_rest`.
    pub rest_requested: bool,
    /// Set by a Shop map node; consumed by the shop overlay.
    pub shop_open: bool,
    /// Whether the starter stash has been seeded for this run.
    pub seeded: bool,
}

/// Seed a few swappable items the first time the party reaches Exploration, so
/// the inventory has something to equip in the first milestone.
pub fn ensure_starter_stash(mut inventory: ResMut<PartyInventory>) {
    if inventory.seeded {
        return;
    }
    inventory.seeded = true;
    inventory.items.extend(
        [
            "iron_helm",
            "leather_armor",
            "scale_mail",
            "wooden_shield",
            "dagger",
            "mace",
            "soft_boots",
            "healing_draught",
        ]
        .into_iter()
        .map(str::to_string),
    );
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

    fn slot_mut(self, equipment: &mut Equipment) -> &mut Option<ItemId> {
        match self {
            Self::Head => &mut equipment.head,
            Self::Body => &mut equipment.body,
            Self::MainHand => &mut equipment.main_hand,
            Self::OffHand => &mut equipment.off_hand,
            Self::Feet => &mut equipment.feet,
        }
    }

    fn get(self, equipment: &Equipment) -> &Option<ItemId> {
        match self {
            Self::Head => &equipment.head,
            Self::Body => &equipment.body,
            Self::MainHand => &equipment.main_hand,
            Self::OffHand => &equipment.off_hand,
            Self::Feet => &equipment.feet,
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
        &'static Character,
        &'static mut Equipment,
        &'static Abilities,
        &'static Derived,
        &'static SkillSet,
        &'static Traits,
        &'static PartyMember,
    ),
    Without<EnemyUnit>,
>;

#[allow(clippy::too_many_arguments)]
pub fn inventory_ui(
    mut contexts: EguiContexts,
    mut inventory_open: ResMut<InventoryOpen>,
    mut selection: ResMut<UiSelection>,
    mut party: SheetQuery,
    mut inventory: ResMut<PartyInventory>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    mut gold: ResMut<Gold>,
    mut equip_changed: EventWriter<EquipmentChanged>,
    mut inv_changed: EventWriter<InventoryChanged>,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    // Make sure something is selected if any window is open.
    if selection.selected_member.is_none() {
        selection.selected_member = party.iter().min_by_key(|(.., m)| m.slot).map(|t| t.0);
    }
    let selected = selection.selected_member;

    // Roster list for the member selector (drop the query borrow before get_mut).
    let mut roster: Vec<(Entity, String, u8)> = party
        .iter()
        .map(|(e, c, .., m)| (e, c.name.clone(), m.slot))
        .collect();
    roster.sort_by_key(|(_, _, slot)| *slot);

    // ---- Inventory overlay ----
    if inventory_open.0 {
        let mut open = true;
        let mut new_selected = selected;
        let mut to_equip: Option<usize> = None;
        let mut to_unequip: Option<EquipSlot> = None;

        egui::Window::new(
            egui::RichText::new("Inventory & Gear")
                .size(22.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            member_selector(ui, &roster, selected, &mut new_selected);
            ui.label(theme::flavour(format!(
                "Gold: {}   Stash: {}/20",
                gold.0,
                inventory.items.len().min(INVENTORY_CAPACITY)
            )));
            ui.separator();

            if let Some(member) = selected {
                if let Ok((_, _, equipment, _, _, _, _, _)) = party.get(member) {
                    ui.columns(2, |cols| {
                        // Equipped column.
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
                                            ui.label(item_name(id, &data, &instances));
                                            if ui.small_button("Unequip").clicked() {
                                                to_unequip = Some(slot);
                                            }
                                        }
                                        None => {
                                            ui.label(theme::flavour("— empty —"));
                                        }
                                    }
                                });
                            });
                        }

                        // Stash column.
                        cols[1].label(theme::heading("Stash"));
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(&mut cols[1], |ui| {
                                for (index, item_id) in inventory.items.iter().enumerate() {
                                    let Some(item) =
                                        base_item_for_instance(item_id, &data, &instances)
                                    else {
                                        continue;
                                    };
                                    let instance = instances.instances.get(item_id);
                                    theme::card_frame(false).show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            item_tile(
                                                ui,
                                                &item.name,
                                                rarity_color(instance, &data),
                                            );
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(item.name.as_str())
                                                        .strong(),
                                                );
                                                ui.label(theme::flavour(item_stat_line(
                                                    item, instance,
                                                )));
                                            });
                                            if EquipSlot::of(&item.slot).is_some()
                                                && ui.small_button("Equip").clicked()
                                            {
                                                to_equip = Some(index);
                                            }
                                        })
                                        .response
                                        .on_hover_text(item_tooltip(item, instance, &data));
                                    });
                                }
                                if inventory.items.is_empty() {
                                    ui.label(theme::flavour("The stash is empty."));
                                }
                            });
                    });
                }
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });

        // Apply the deferred equip/unequip, then propagate the changes.
        if let Some(member) = selected {
            if let Ok((_, _, mut equipment, _, _, _, _, _)) = party.get_mut(member) {
                let mut changed = false;
                if let Some(slot) = to_unequip {
                    if let Some(prev) = slot.slot_mut(&mut equipment).take() {
                        inventory.items.push(prev);
                        changed = true;
                    }
                }
                if let Some(index) = to_equip {
                    if index < inventory.items.len() {
                        let item_id = inventory.items[index].clone();
                        if let Some(target) = base_item_for_instance(&item_id, &data, &instances)
                            .and_then(|i| EquipSlot::of(&i.slot))
                        {
                            let dest = target.slot_mut(&mut equipment);
                            let previous = dest.replace(item_id);
                            inventory.items.remove(index);
                            if let Some(previous) = previous {
                                inventory.items.push(previous);
                            }
                            changed = true;
                        }
                    }
                }
                if changed {
                    equip_changed.write(EquipmentChanged { entity: member });
                    inv_changed.write(InventoryChanged);
                }
            }
        }

        selection.selected_member = new_selected;
        inventory_open.0 = open;
    }

    // ---- Character sheet ----
    if selection.show_sheet {
        let mut open = true;
        egui::Window::new(
            egui::RichText::new("Character Sheet")
                .size(20.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .resizable(true)
        .default_width(420.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            if let Some((_, character, _, abilities, derived, _, traits, _)) =
                selected.and_then(|m| party.get(m).ok())
            {
                character_sheet(ui, character, *abilities, *derived, traits, &data);
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });
        selection.show_sheet = open;
    }

    // ---- Skills tab ----
    if selection.show_skills {
        let mut open = true;
        egui::Window::new(
            egui::RichText::new("Skills")
                .size(20.0)
                .color(theme::GOLD_BRIGHT),
        )
        .open(&mut open)
        .resizable(true)
        .default_width(440.0)
        .frame(theme::hero_frame())
        .show(ctx, |ui| {
            if let Some((_, character, _, abilities, _, skills, _, _)) =
                selected.and_then(|m| party.get(m).ok())
            {
                skills_tab(ui, *abilities, skills, character.level, &data);
            } else {
                ui.label(theme::flavour("No party member selected."));
            }
        });
        selection.show_skills = open;
    }

    if inventory.shop_open {
        shop_ui(ctx, &mut inventory, &data, &mut gold, &mut inv_changed);
    }

    Ok(())
}

// ===== Sub-views ===================================================

fn member_selector(
    ui: &mut egui::Ui,
    roster: &[(Entity, String, u8)],
    selected: Option<Entity>,
    new_selected: &mut Option<Entity>,
) {
    ui.horizontal_wrapped(|ui| {
        for (entity, name, _) in roster {
            if ui
                .selectable_label(selected == Some(*entity), name.as_str())
                .clicked()
            {
                *new_selected = Some(*entity);
            }
        }
    });
}

fn character_sheet(
    ui: &mut egui::Ui,
    character: &Character,
    abilities: Abilities,
    derived: Derived,
    traits: &Traits,
    data: &GameData,
) {
    ui.label(
        egui::RichText::new(character.name.as_str())
            .size(22.0)
            .color(theme::GOLD_BRIGHT)
            .strong(),
    );
    ui.label(format!(
        "Level {} {} {}",
        character.level,
        race_name(&character.race, data),
        class_display(&character.class, data),
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
        "HP {}   ·   AC {}   ·   Initiative {}   ·   Proficiency {}   ·   Speed {}",
        derived.max_hp,
        derived.armor_class,
        theme::signed(derived.initiative_mod),
        theme::signed(derived.proficiency),
        derived.speed,
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
                let name = ui.label(format!(
                    "{}  ({})",
                    skill.name,
                    skill.ability.to_uppercase()
                ));
                name.on_hover_text(skill.description.as_str());
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

fn shop_ui(
    ctx: &egui::Context,
    inventory: &mut PartyInventory,
    data: &GameData,
    gold: &mut Gold,
    inv_changed: &mut EventWriter<InventoryChanged>,
) {
    let mut open = inventory.shop_open;
    egui::Window::new(
        egui::RichText::new("Shop")
            .size(20.0)
            .color(theme::GOLD_BRIGHT),
    )
    .open(&mut open)
    .resizable(true)
    .default_width(520.0)
    .frame(theme::hero_frame())
    .show(ctx, |ui| {
        ui.label(theme::heading(format!("Gold {}", gold.0)));
        ui.columns(2, |cols| {
            cols[0].label(theme::heading("Buy"));
            let mut stock: Vec<&ItemData> = data.items.values().collect();
            stock.sort_by(|a, b| a.name.cmp(&b.name));
            for item in stock.into_iter().take(8) {
                let can_buy = gold.0 >= item.value && inventory.items.len() < INVENTORY_CAPACITY;
                cols[0].horizontal(|ui| {
                    ui.label(item.name.as_str());
                    ui.label(theme::flavour(format!("{}g", item.value)));
                    if ui.add_enabled(can_buy, egui::Button::new("Buy")).clicked() {
                        gold.0 -= item.value;
                        inventory.items.push(item.id.clone());
                        inv_changed.write(InventoryChanged);
                    }
                });
            }

            cols[1].label(theme::heading("Sell"));
            let mut sold_index = None;
            for (index, item_id) in inventory.items.iter().enumerate() {
                let Some(item) = data.items.get(item_id) else {
                    continue;
                };
                let price = (item.value / 2).max(1);
                cols[1].horizontal(|ui| {
                    ui.label(item.name.as_str());
                    ui.label(theme::flavour(format!("{}g", price)));
                    if ui.button("Sell").clicked() {
                        sold_index = Some((index, price));
                    }
                });
            }
            if let Some((index, price)) = sold_index {
                inventory.items.remove(index);
                gold.0 = gold.0.saturating_add(price);
                inv_changed.write(InventoryChanged);
            }
        });
    });
    inventory.shop_open = open;
}

// ===== Small helpers ===============================================

fn item_tile(ui: &mut egui::Ui, name: &str, frame_color: egui::Color32) {
    let initial = name.chars().next().unwrap_or('?').to_ascii_uppercase();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(4), theme::PANEL_LIGHT);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(4),
        egui::Stroke::new(2.0, frame_color),
        egui::StrokeKind::Outside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        egui::FontId::proportional(16.0),
        theme::GOLD_BRIGHT,
    );
}

fn item_stat_line(item: &ItemData, instance: Option<&ItemInstance>) -> String {
    let mut parts = Vec::new();
    if item.armor_bonus != 0 {
        parts.push(format!("+{} armor", item.armor_bonus));
    }
    if let Some(dmg) = &item.damage {
        parts.push(format!(
            "{}d{}{}",
            dmg.count,
            dmg.sides,
            theme::signed(dmg.modifier)
        ));
    }
    if let Some(instance) = instance {
        parts.push(format!("{:?}", instance.rarity));
        parts.push(format!("{}g", instance.value));
    } else {
        parts.push("Common".to_string());
        parts.push(format!("{}g", item.value));
    }
    parts.join("  ·  ")
}

fn item_tooltip(item: &ItemData, instance: Option<&ItemInstance>, data: &GameData) -> String {
    let mut lines = vec![item.description.clone()];
    if let Some(instance) = instance {
        let rarity = data
            .rarities
            .get(&instance.rarity)
            .map(|rarity| rarity.name.as_str())
            .unwrap_or("Unknown");
        lines.push(format!("Rarity: {rarity}"));
        for affix in &instance.affixes {
            lines.push(format!("{} {:+}", affix.name, affix.value));
        }
    } else {
        lines.push("Rarity: Common".to_string());
    }
    if let Some(category) = item.consumable {
        lines.push(format!("Consumable: {category:?}"));
    }
    lines.join("\n")
}

fn rarity_color(instance: Option<&ItemInstance>, data: &GameData) -> egui::Color32 {
    let rarity = instance.map(|item| item.rarity).unwrap_or(Rarity::Common);
    rarity_frame_color(data, rarity)
        .map(|color| egui::Color32::from_rgba_premultiplied(color.r, color.g, color.b, color.a))
        .unwrap_or(theme::INK_DIM)
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

fn item_name(id: &str, data: &GameData, instances: &ItemInstances) -> String {
    base_item_for_instance(id, data, instances)
        .map(|i| i.name.clone())
        .unwrap_or_else(|| id.to_string())
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
