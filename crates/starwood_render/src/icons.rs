//! Item-icon rendering for the UI crate.
//!
//! The render crate owns all sprites, including item art. The UI crate should
//! not draw item bodies itself; instead it can either:
//!
//! 1. Resolve a `Handle<Image>` for any item via [`item_icon_handle`] or
//!    [`instance_icon_handle`], a rarity frame via [`rarity_frame_handle`] (or
//!    directly through the shared `AssetHandles` resource, keyed by sprite key),
//!    and register them with egui for inventory/HUD icons; or
//! 2. Spawn a world entity carrying an [`ItemIcon`] component, which this
//!    module resolves into a rarity-framed sprite stack.
//!
//! ## Per-instance rendering
//!
//! Item rendering is driven per *instance*: the base item sprite (resolved from
//! the instance's base item template) is drawn on top of a rarity-colored frame
//! chosen via `rarity_rank(instance.rarity)`. Prefer [`ItemIcon::from_instance`]
//! or [`ItemIcon::from_instance_id`] when you have an `ItemInstance`.

use crate::{ICON_SIZE, SpriteManifest};
use bevy::prelude::*;
use starwood_core::{
    base_item_for_instance, GameData, ItemId, ItemInstance, ItemInstanceId, ItemInstances,
    rarity_rank,
};

/// Attach to an entity to render it as a rarity-framed item icon.
///
/// `item` is the **base** item id (its sprite key resolves the icon art).
/// `rarity_tier` selects the frame color behind the icon; `None` draws a plain
/// unframed icon.
#[derive(Component, Clone, Debug)]
pub struct ItemIcon {
    pub item: ItemId,
    pub rarity_tier: Option<u8>,
}

impl ItemIcon {
    /// A plain (unframed) icon for a base item.
    pub fn new(item: impl Into<ItemId>) -> Self {
        Self { item: item.into(), rarity_tier: None }
    }

    /// An icon for a base item with a rarity frame behind it.
    pub fn with_rarity(item: impl Into<ItemId>, rarity_tier: u8) -> Self {
        Self { item: item.into(), rarity_tier: Some(rarity_tier) }
    }

    /// Build an icon from a rolled item instance (base sprite + rarity frame).
    pub fn from_instance(instance: &ItemInstance) -> Self {
        Self { item: instance.base.clone(), rarity_tier: Some(rarity_rank(instance.rarity)) }
    }

    /// Resolve an instance id from the shared store into an icon descriptor.
    pub fn from_instance_id(
        instance_id: &ItemInstanceId,
        instances: &ItemInstances,
    ) -> Option<Self> {
        instances.instances.get(instance_id).map(Self::from_instance)
    }
}

/// Marks an [`ItemIcon`] entity whose sprite stack has been built.
#[derive(Component)]
struct IconResolved;

/// Resolve the texture handle for an item's icon, going `ItemId` -> item data
/// -> base sprite key -> generated handle.
pub fn item_icon_handle(manifest: &SpriteManifest, data: &GameData, item_id: &ItemId) -> Option<Handle<Image>> {
    let item = data.items.get(item_id)?;
    manifest.get(&item.sprite_key)
}

/// Resolve the texture handle for an item instance's icon (base template sprite).
pub fn instance_icon_handle(
    manifest: &SpriteManifest,
    data: &GameData,
    instances: &ItemInstances,
    instance_id: &ItemInstanceId,
) -> Option<Handle<Image>> {
    let item = base_item_for_instance(instance_id, data, instances)?;
    manifest.get(&item.sprite_key)
}

/// Resolve the rarity-frame texture for a tier (drawn behind an item icon).
pub fn rarity_frame_handle(manifest: &SpriteManifest, tier: u8) -> Option<Handle<Image>> {
    manifest.get(&crate::rarity::rarity_frame_key(tier))
}

/// Build the framed sprite stack for every unresolved [`ItemIcon`] once art
/// exists: a rarity frame child behind the item-icon child.
pub fn resolve_item_icons(
    mut commands: Commands,
    manifest: Res<SpriteManifest>,
    data: Res<GameData>,
    query: Query<(Entity, &ItemIcon), Without<IconResolved>>,
) {
    if !manifest.ready {
        return;
    }
    for (entity, icon) in &query {
        let Some(item_handle) = item_icon_handle(&manifest, &data, &icon.item) else {
            continue; // unknown item id — leave for a later frame / real art
        };

        // The icon entity becomes a container; only fill in a transform/
        // visibility if the caller didn't already place it.
        commands
            .entity(entity)
            .insert_if_new(Transform::default())
            .insert_if_new(Visibility::Visible)
            .insert(IconResolved);

        // Rarity frame sits behind the icon (z = 0).
        if let Some(tier) = icon.rarity_tier {
            if let Some(frame_handle) = rarity_frame_handle(&manifest, tier) {
                let frame = commands
                    .spawn((
                        Sprite { image: frame_handle, custom_size: Some(Vec2::splat(ICON_SIZE + 12.0)), ..default() },
                        Transform::from_xyz(0.0, 0.0, 0.0),
                        Visibility::Inherited,
                    ))
                    .id();
                commands.entity(entity).add_child(frame);
            }
        }

        // Item icon in front of the frame (z = 1).
        let icon_sprite = commands
            .spawn((
                Sprite { image: item_handle, custom_size: Some(Vec2::splat(ICON_SIZE)), ..default() },
                Transform::from_xyz(0.0, 0.0, 1.0),
                Visibility::Inherited,
            ))
            .id();
        commands.entity(entity).add_child(icon_sprite);
    }
}
