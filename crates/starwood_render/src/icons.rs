//! Item-icon rendering for the UI crate (per `ItemInstance`).

use crate::{ICON_SIZE, SpriteManifest};
use bevy::prelude::*;
use starwood_core::{
    GameData, ItemId, ItemInstance, ItemInstanceId, ItemInstances, Rarity, base_item_for_instance,
    rarity_rank,
};

/// Attach to an entity to render a rarity-framed icon for a rolled item instance.
#[derive(Component, Clone, Debug)]
pub struct ItemIcon(pub ItemInstanceId);

impl ItemIcon {
    pub fn new(instance_id: impl Into<ItemInstanceId>) -> Self {
        Self(instance_id.into())
    }

    pub fn from_instance(instance: &ItemInstance) -> Self {
        Self(instance.instance_id.clone())
    }

    pub fn from_instance_id(
        instance_id: &ItemInstanceId,
        instances: &ItemInstances,
    ) -> Option<Self> {
        instances
            .instances
            .contains_key(instance_id)
            .then(|| Self(instance_id.clone()))
    }
}

#[derive(Component)]
pub(crate) struct IconResolved;

/// Resolve a base item template icon (egui / legacy callers without an instance).
pub fn item_icon_handle(
    manifest: &SpriteManifest,
    data: &GameData,
    item_id: &ItemId,
) -> Option<Handle<Image>> {
    let item = data.items.get(item_id)?;
    manifest.get(&item.sprite_key)
}

/// Resolve an instance's base-template icon handle.
pub fn instance_icon_handle(
    manifest: &SpriteManifest,
    data: &GameData,
    instances: &ItemInstances,
    instance_id: &ItemInstanceId,
) -> Option<Handle<Image>> {
    let item = base_item_for_instance(instance_id, data, instances)?;
    manifest.get(&item.sprite_key)
}

/// Resolve the rarity-frame texture for a core `Rarity`.
pub fn rarity_frame_handle(manifest: &SpriteManifest, rarity: Rarity) -> Option<Handle<Image>> {
    manifest.get(&crate::rarity::rarity_frame_key_for(rarity))
}

/// Resolve the rarity-frame texture for a tier (`rarity_rank` output).
pub fn rarity_frame_handle_tier(manifest: &SpriteManifest, tier: u8) -> Option<Handle<Image>> {
    manifest.get(&crate::rarity::rarity_frame_key(tier))
}

/// Build framed sprite stacks for unresolved [`ItemIcon`] entities.
pub(crate) fn resolve_item_icons(
    mut commands: Commands,
    manifest: Res<SpriteManifest>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    query: Query<(Entity, &ItemIcon), Without<IconResolved>>,
) {
    if !manifest.ready {
        return;
    }
    for (entity, icon) in &query {
        let Some(instance) = instances.instances.get(&icon.0) else {
            continue;
        };
        let Some(item_handle) = instance_icon_handle(&manifest, &data, &instances, &icon.0) else {
            continue;
        };
        let tier = rarity_rank(instance.rarity);

        commands
            .entity(entity)
            .insert_if_new(Transform::default())
            .insert_if_new(Visibility::Visible)
            .insert(IconResolved);

        if let Some(frame_handle) = rarity_frame_handle(&manifest, instance.rarity)
            .or_else(|| rarity_frame_handle_tier(&manifest, tier))
        {
            let frame = commands
                .spawn((
                    Sprite {
                        image: frame_handle,
                        custom_size: Some(Vec2::splat(ICON_SIZE + 12.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.0),
                    Visibility::Inherited,
                ))
                .id();
            commands.entity(entity).add_child(frame);
        }

        let icon_sprite = commands
            .spawn((
                Sprite {
                    image: item_handle,
                    custom_size: Some(Vec2::splat(ICON_SIZE)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 1.0),
                Visibility::Inherited,
            ))
            .id();
        commands.entity(entity).add_child(icon_sprite);
    }
}
