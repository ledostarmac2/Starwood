//! Campaign save/load: a single autosave slot persisted to disk and surfaced as
//! a campaign slot in the main menu.
//!
//! * `autosave_on_exploration` snapshots the run every time we reach Exploration.
//! * `index_saves` (Startup) reads any existing save so the slot shows on launch.
//! * `process_pending_load` reconstructs the world when the menu's Continue sets
//!   [`PendingLoad`], then transitions to Exploration.

use bevy::prelude::*;
use starwood_core::{
    Abilities, Antagonist, CampaignMetadata, CampaignSaves, CampaignSlot, Character, Equipment,
    GameData, GameDifficulty, GameState, Gold, Health, Inventory, ItemInstances, Mana, MapState,
    PartyMember, PartyRoster, PlannedCompanions, RevivePenalty, SaveGame, SavedCharacter, SkillSet,
    TalentPoints, Talents, Traits, deserialize_save, serialize_save,
};

use crate::menu::SAVE_PATH;

/// Set by the menu's Continue button; consumed by [`process_pending_load`].
#[derive(Resource, Default)]
pub struct PendingLoad(pub bool);

type SavePartyQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Character,
        &'static Abilities,
        &'static Health,
        Option<&'static Mana>,
        &'static Equipment,
        &'static SkillSet,
        &'static Traits,
        Option<&'static Talents>,
        Option<&'static TalentPoints>,
        Option<&'static RevivePenalty>,
        &'static PartyMember,
    ),
>;

/// Snapshot the campaign to disk whenever we (re-)enter Exploration.
pub fn autosave_on_exploration(
    party: SavePartyQuery,
    map: Res<MapState>,
    difficulty: Res<GameDifficulty>,
    antagonist: Res<Antagonist>,
    planned: Res<PlannedCompanions>,
    inventory: Res<Inventory>,
    instances: Res<ItemInstances>,
    gold: Res<Gold>,
    mut saves: ResMut<CampaignSaves>,
) {
    if party.is_empty() {
        return;
    }

    let mut members: Vec<_> = party.iter().collect();
    members.sort_by_key(|(.., member)| member.slot);
    let hero = members
        .first()
        .map(|(character, ..)| character.name.clone())
        .unwrap_or_else(|| "Hero".to_string());
    let saved_party = members
        .into_iter()
        .map(
            |(
                character,
                abilities,
                health,
                mana,
                equipment,
                skills,
                traits,
                talents,
                talent_points,
                revive_penalty,
                _member,
            )| {
                SavedCharacter {
                    name: character.name.clone(),
                    race: character.race.clone(),
                    class: character.class.clone(),
                    subclass: character.subclass.clone(),
                    level: character.level,
                    xp: character.xp,
                    abilities: *abilities,
                    health_current: health.current,
                    mana_current: mana.map_or(0, |mana| mana.current),
                    equipment: equipment.clone().into(),
                    skills: skills.proficient.clone(),
                    traits: traits.0.clone(),
                    talents: talents.map_or_else(Vec::new, |talents| talents.0.clone()),
                    talent_points: talent_points.map_or(0, |points| points.0),
                    revive_penalty_stacks: revive_penalty.map_or(0, |penalty| penalty.stacks),
                }
            },
        )
        .collect();

    let cleared = map.nodes.iter().filter(|n| n.completed).count();
    let metadata = CampaignMetadata {
        slot: 0,
        name: hero,
        seed: map.seed,
        difficulty: difficulty.0,
        autosave: true,
        progress_label: format!("{cleared} nodes cleared"),
    };
    let save = SaveGame {
        metadata: metadata.clone(),
        seed: map.seed,
        difficulty: difficulty.0,
        antagonist: antagonist.clone(),
        planned_companions: planned.clone(),
        party: saved_party,
        map: map.clone(),
        inventory: inventory.items.clone(),
        item_instances: instances.instances.clone(),
        gold: gold.0,
        autosave: true,
    };

    if let Ok(text) = serialize_save(&save) {
        let _ = std::fs::write(SAVE_PATH, text);
    }
    saves.slots[0] = CampaignSlot {
        metadata: Some(metadata),
        autosave: true,
    };
}

/// At startup, surface an existing save as campaign slot 0.
pub fn index_saves(mut saves: ResMut<CampaignSaves>) {
    if let Some(save) = read_save() {
        saves.slots[0] = CampaignSlot {
            metadata: Some(save.metadata),
            autosave: save.autosave,
        };
    }
}

/// Reconstruct the world from the save when the menu requested a load.
pub fn process_pending_load(
    mut pending: ResMut<PendingLoad>,
    mut commands: Commands,
    mut roster: ResMut<PartyRoster>,
    mut map: ResMut<MapState>,
    mut inventory: ResMut<Inventory>,
    mut instances: ResMut<ItemInstances>,
    mut gold: ResMut<Gold>,
    mut difficulty: ResMut<GameDifficulty>,
    mut planned: ResMut<PlannedCompanions>,
    mut antagonist: ResMut<Antagonist>,
    data: Res<GameData>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    if !pending.0 {
        return;
    }
    pending.0 = false;
    let Some(save) = read_save() else {
        return;
    };

    for entity in roster.members.drain(..) {
        commands.entity(entity).despawn();
    }
    *map = save.map.clone();
    inventory.items = save.inventory.clone();
    instances.instances = save.item_instances.clone();
    instances.next_serial = save.item_instances.len() as u64;
    gold.0 = save.gold;
    difficulty.0 = save.difficulty;
    *planned = save.planned_companions.clone();
    *antagonist = save.antagonist.clone();

    for (slot, saved) in save.party.iter().take(4).enumerate() {
        if let Some(entity) = crate::creation::spawn_member_from_saved(
            &mut commands,
            saved,
            slot as u8,
            &data,
            &instances,
        ) {
            roster.members.push(entity);
        }
    }
    if !roster.members.is_empty() {
        next_state.set(GameState::Exploration);
    }
}

/// Delete the autosave file and clear the menu slot.
pub fn delete_save(saves: &mut CampaignSaves) {
    let _ = std::fs::remove_file(SAVE_PATH);
    saves.slots[0] = CampaignSlot::default();
}

pub fn save_exists() -> bool {
    std::path::Path::new(SAVE_PATH).exists()
}

fn read_save() -> Option<SaveGame> {
    let text = std::fs::read_to_string(SAVE_PATH).ok()?;
    deserialize_save(&text).ok()
}
