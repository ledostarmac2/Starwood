//! `starwood_render` — the visual world layer for Starwood.
//!
//! Owns the 2D scene (party area + enemy stage), the paper-doll composition of
//! characters and enemies from `SpriteParts` + `Equipment`, world animations
//! (idle bob, attack lunge, hurt flash/shake, death dissolve), item-icon
//! rendering for the UI crate, and programmatic placeholder pixel-art.
//!
//! This crate depends only on `starwood_core` and obeys its Shared Contract.
//! It never touches the UI or core crates. The Dice Theater and egui panels
//! belong to `starwood_ui`; gameplay logic and dice results belong to
//! `starwood_core`.

mod art;
mod debug;
mod effects;
mod icons;
mod paperdoll;
mod rarity;

pub use art::{BODY_SIZE, FRAME_SIZE, ITEM_SIZE};
pub use debug::RenderDebugOverlay;
pub use effects::DownedVisual;
pub use icons::{
    ItemIcon, instance_icon_handle, item_icon_handle, rarity_frame_handle, rarity_frame_handle_tier,
};
pub use paperdoll::{UnitLayer, UnitVisual};
pub use rarity::{RARITY_TIERS, RarityStyle, rarity_frame_key, rarity_frame_key_for, rarity_style};

use bevy::prelude::*;
use bevy_tweening::TweeningPlugin;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use starwood_core::{AssetHandles, GameData, rarity_rank};
use std::collections::{HashMap, HashSet};

/// World-space scale applied to every 64x64 unit so placeholder art reads at a
/// comfortable size on screen.
pub const UNIT_SCALE: f32 = 2.4;
/// Default on-screen size (pixels) of a standalone item icon.
pub const ICON_SIZE: f32 = 40.0;

const BG_Z: f32 = -50.0;

// ===== LANE / RANK LAYOUT =====
// Units stand in ranks (Darkest-Dungeon style). `slot` is the rank: 0 = front
// (closest to the enemy line at screen center), higher = further back. The
// party occupies the left half, enemies the right half. Back ranks step up and
// shrink slightly so the formation reads with depth, and they render behind the
// front rank (lower, i.e. more negative, z).
const RANK_DX: f32 = 145.0; // horizontal gap between adjacent ranks
const RANK_DY: f32 = 30.0; // vertical step so back ranks peek over front ranks
const RANK_BASE_Y: f32 = -40.0; // front-rank baseline
const PARTY_FRONT_X: f32 = -150.0; // front party rank, just left of center
const ENEMY_FRONT_X: f32 = 150.0; // front enemy rank, just right of center
const RANK_DEPTH_FALLOFF: f32 = 0.06; // per-rank shrink for depth cueing

/// System set grouping all render work, so it can be ordered/observed as a unit.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderSet;

/// The render plugin. Add it to the Bevy app to bring the world to life.
pub struct StarwoodRenderPlugin;

impl Plugin for StarwoodRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TweeningPlugin)
            .init_resource::<SpriteManifest>()
            .init_resource::<RenderDebugOverlay>()
            .insert_resource(RenderRng(ChaCha8Rng::seed_from_u64(0x5757_0D17_u64)))
            .add_systems(Startup, setup_scene)
            .add_systems(
                Update,
                (
                    generate_placeholders,
                    paperdoll::spawn_unit_visuals,
                    paperdoll::recompose_on_equipment_change,
                    paperdoll::apply_rank_slide,
                    icons::resolve_item_icons,
                    effects::lunge_on_action,
                    effects::react_to_damage,
                    effects::run_shake,
                    effects::run_flash,
                    effects::sync_downed_from_core,
                    effects::route_unit_death,
                    effects::run_death_fade,
                    effects::apply_downed_visual,
                    effects::clear_downed_on_revive,
                    effects::clear_stage_on_encounter_end,
                )
                    .chain()
                    .in_set(RenderSet),
            )
            .add_systems(
                Update,
                (
                    debug::toggle_debug_overlay,
                    debug::sync_debug_rank_markers,
                    debug::update_debug_unit_markers,
                ),
            );
    }
}

/// Maps every sprite key from `GameData` to a generated (or, later, authored)
/// texture handle. The UI crate can resolve item icons through this resource or
/// through the shared `AssetHandles` resource, which mirrors the same handles.
#[derive(Resource, Default)]
pub struct SpriteManifest {
    pub handles: HashMap<String, Handle<Image>>,
    pub ready: bool,
}

impl SpriteManifest {
    pub fn get(&self, key: &str) -> Option<Handle<Image>> {
        self.handles.get(key).cloned()
    }

    /// Insert a generated body placeholder if `key` is not yet registered.
    pub fn ensure_body(
        &mut self,
        images: &mut Assets<Image>,
        asset_handles: &mut AssetHandles,
        key: &str,
    ) -> Option<Handle<Image>> {
        if let Some(handle) = self.get(key) {
            return Some(handle);
        }
        Some(register(
            self,
            images,
            asset_handles,
            key,
            art::generate_body(key),
        ))
    }

    /// Insert a generated item placeholder if the item's sprite key is missing.
    pub fn ensure_item(
        &mut self,
        images: &mut Assets<Image>,
        asset_handles: &mut AssetHandles,
        item: &starwood_core::ItemData,
    ) -> Option<Handle<Image>> {
        if let Some(handle) = self.get(&item.sprite_key) {
            return Some(handle);
        }
        Some(register(
            self,
            images,
            asset_handles,
            &item.sprite_key,
            art::generate_item(item),
        ))
    }

    /// Last-resort fallback for unknown keys (never returns None once images exist).
    pub fn ensure_fallback(
        &mut self,
        images: &mut Assets<Image>,
        asset_handles: &mut AssetHandles,
        key: &str,
        width: u32,
        height: u32,
    ) -> Handle<Image> {
        if let Some(handle) = self.get(key) {
            return handle;
        }
        register(
            self,
            images,
            asset_handles,
            key,
            art::generate_fallback_sprite(key, width, height),
        )
    }
}

/// A render-only RNG used for purely cosmetic randomness (shake direction, bob
/// desync). Kept separate from `core`'s authoritative `GameRng` so visuals can
/// never desynchronize deterministic gameplay.
#[derive(Resource)]
pub struct RenderRng(pub ChaCha8Rng);

fn setup_scene(mut commands: Commands) {
    commands.spawn((Camera2d, Name::new("starwood_camera")));

    // Two faint backdrop halves hint at the persistent lane layout: the party
    // musters on the left, the enemy line forms on the right, meeting at center.
    commands.spawn((
        Sprite::from_color(Color::srgba(0.10, 0.12, 0.18, 1.0), Vec2::new(640.0, 720.0)),
        Transform::from_xyz(-320.0, 0.0, BG_Z),
        Name::new("party_band"),
    ));
    commands.spawn((
        Sprite::from_color(Color::srgba(0.18, 0.11, 0.13, 1.0), Vec2::new(640.0, 720.0)),
        Transform::from_xyz(320.0, 0.0, BG_Z),
        Name::new("enemy_band"),
    ));
}

/// World position for party rank `slot` (0 = front, saturating at 3). Ranks
/// march left and step up as they go back.
pub fn party_slot_position(slot: u8) -> Vec3 {
    let s = slot.min(3) as f32;
    Vec3::new(PARTY_FRONT_X - s * RANK_DX, RANK_BASE_Y + s * RANK_DY, -s)
}

/// World position for enemy rank `slot` (0 = front, saturating at 4). Mirrors
/// the party layout on the right half.
pub fn enemy_slot_position(slot: u8) -> Vec3 {
    let s = slot.min(4) as f32;
    Vec3::new(ENEMY_FRONT_X + s * RANK_DX, RANK_BASE_Y + s * RANK_DY, -s)
}

/// Per-rank scale multiplier: back ranks shrink slightly for depth cueing.
pub fn rank_scale(slot: u8) -> f32 {
    1.0 - RANK_DEPTH_FALLOFF * slot.min(4) as f32
}

/// Collect every sprite key referenced by loaded `GameData`.
pub fn collect_sprite_keys(data: &GameData) -> HashSet<String> {
    let mut keys = HashSet::new();
    for race in data.races.values() {
        keys.insert(race.sprite_key.clone());
    }
    for enemy in data.enemies.values() {
        keys.insert(enemy.sprite_key.clone());
    }
    for item in data.items.values() {
        keys.insert(item.sprite_key.clone());
    }
    for tier in 0..RARITY_TIERS {
        keys.insert(rarity::rarity_frame_key(tier));
    }
    keys
}

/// Build all placeholder textures once `GameData` has loaded.
fn generate_placeholders(
    mut manifest: ResMut<SpriteManifest>,
    mut images: ResMut<Assets<Image>>,
    mut handles: ResMut<AssetHandles>,
    data: Res<GameData>,
) {
    if manifest.ready {
        return;
    }
    if data.races.is_empty() && data.items.is_empty() && data.enemies.is_empty() {
        return;
    }

    for race in data.races.values() {
        register(
            &mut manifest,
            &mut images,
            &mut handles,
            &race.sprite_key,
            art::generate_body(&race.sprite_key),
        );
    }
    for enemy in data.enemies.values() {
        register(
            &mut manifest,
            &mut images,
            &mut handles,
            &enemy.sprite_key,
            art::generate_enemy(&enemy.sprite_key),
        );
    }
    for item in data.items.values() {
        register(
            &mut manifest,
            &mut images,
            &mut handles,
            &item.sprite_key,
            art::generate_item(item),
        );
    }
    for (rarity, row) in &data.rarities {
        let tier = rarity_rank(*rarity);
        let key = rarity::rarity_frame_key(tier);
        register(
            &mut manifest,
            &mut images,
            &mut handles,
            &key,
            art::generate_rarity_frame_from_color(row.frame_color, tier),
        );
    }

    let expected = collect_sprite_keys(&data);
    let missing: Vec<_> = expected
        .iter()
        .filter(|k| !manifest.handles.contains_key(k.as_str()))
        .collect();
    if !missing.is_empty() {
        warn!(
            "starwood_render: {} sprite keys still missing after generation: {:?}",
            missing.len(),
            missing
        );
    }

    manifest.ready = true;
    info!(
        "starwood_render: generated {} placeholder sprites ({} expected from GameData)",
        manifest.handles.len(),
        expected.len()
    );
}

fn register(
    manifest: &mut SpriteManifest,
    images: &mut Assets<Image>,
    handles: &mut AssetHandles,
    key: &str,
    image: Image,
) -> Handle<Image> {
    let handle = images.add(image);
    handles.sprites.insert(key.to_string(), handle.clone());
    manifest.handles.insert(key.to_string(), handle.clone());
    handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use starwood_core::{DiceExpr, ItemData, ItemSlot};

    #[test]
    fn party_ranks_march_front_to_back_on_the_left() {
        for slot in 0u8..4 {
            assert!(
                party_slot_position(slot).x < 0.0,
                "party rank {slot} should be left of center"
            );
        }
        // Back ranks sit further from center and render behind the front rank.
        assert!(party_slot_position(3).x < party_slot_position(0).x);
        assert!(party_slot_position(3).z < party_slot_position(0).z);
        assert!(rank_scale(0) >= rank_scale(3));
        // Ranks saturate at 3 so an out-of-range slot never flies off screen.
        assert_eq!(party_slot_position(3), party_slot_position(9));
    }

    #[test]
    fn enemy_ranks_march_front_to_back_on_the_right() {
        for slot in 0u8..5 {
            assert!(
                enemy_slot_position(slot).x > 0.0,
                "enemy rank {slot} should be right of center"
            );
        }
        assert!(enemy_slot_position(4).x > enemy_slot_position(0).x);
        assert_eq!(enemy_slot_position(4), enemy_slot_position(12));
    }

    #[test]
    fn rarity_frames_render_at_frame_size_for_every_tier() {
        for tier in 0..RARITY_TIERS {
            let frame = art::generate_rarity_frame(tier);
            assert_eq!(frame.width(), FRAME_SIZE);
            assert_eq!(frame.height(), FRAME_SIZE);
        }
        // Higher tiers must produce visibly different frames (color differs).
        assert_ne!(
            art::generate_rarity_frame(0).data,
            art::generate_rarity_frame(RARITY_TIERS - 1).data
        );
    }

    #[test]
    fn placeholder_dimensions_match_spec() {
        let body = art::generate_body("race_human");
        assert_eq!(body.width(), BODY_SIZE);
        assert_eq!(body.height(), BODY_SIZE);

        let item = ItemData {
            id: "iron_sword".into(),
            name: "Iron Sword".into(),
            description: String::new(),
            slot: ItemSlot::MainHand,
            armor_bonus: 0,
            damage: Some(DiceExpr {
                count: 1,
                sides: 8,
                modifier: 0,
            }),
            sprite_key: "item_iron_sword".into(),
            value: 15,
            consumable: None,
            tags: vec![],
        };
        let icon = art::generate_item(&item);
        assert_eq!(icon.width(), ITEM_SIZE);
        assert_eq!(icon.height(), ITEM_SIZE);
    }

    #[test]
    fn all_game_data_sprite_keys_have_placeholders() {
        let assets_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
        let data = starwood_core::load_game_data_from_dir(assets_dir.to_str().expect("utf-8 path"))
            .expect("assets/data should parse");
        let keys = collect_sprite_keys(&data);
        assert!(
            keys.len() >= 6 + 8 + 16 + RARITY_TIERS as usize,
            "expected full catalog coverage"
        );

        let mut manifest = SpriteManifest::default();
        let mut images = Assets::<Image>::default();
        let mut handles = AssetHandles::default();

        for race in data.races.values() {
            register(
                &mut manifest,
                &mut images,
                &mut handles,
                &race.sprite_key,
                art::generate_body(&race.sprite_key),
            );
        }
        for enemy in data.enemies.values() {
            register(
                &mut manifest,
                &mut images,
                &mut handles,
                &enemy.sprite_key,
                art::generate_enemy(&enemy.sprite_key),
            );
        }
        for item in data.items.values() {
            register(
                &mut manifest,
                &mut images,
                &mut handles,
                &item.sprite_key,
                art::generate_item(item),
            );
        }
        for (rarity, row) in &data.rarities {
            let tier = rarity_rank(*rarity);
            register(
                &mut manifest,
                &mut images,
                &mut handles,
                &rarity::rarity_frame_key(tier),
                art::generate_rarity_frame_from_color(row.frame_color, tier),
            );
        }

        for key in &keys {
            assert!(
                manifest.handles.contains_key(key),
                "missing placeholder for sprite key {key}"
            );
        }
    }

    #[test]
    fn placeholder_generation_is_deterministic() {
        let a = art::generate_enemy("enemy_wolf");
        let b = art::generate_enemy("enemy_wolf");
        assert_eq!(a.data, b.data);
        let c = art::generate_enemy("enemy_ogre_brute");
        assert_ne!(a.data, c.data);
    }
}
