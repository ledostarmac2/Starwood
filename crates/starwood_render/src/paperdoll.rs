//! Paper-doll composition: turn a logical unit (`SpriteParts` + `Equipment`)
//! into a layered stack of sprites.
//!
//! Each rendered unit is a small hierarchy of "pivot" entities so that several
//! animations can run without fighting over the same `Transform`:
//!
//! ```text
//! core entity (root)  -- world position by slot, UNIT_SCALE
//!   └─ bob_pivot       -- idle breathing bob (bevy_tweening)
//!        └─ lunge_pivot -- attack lunge (bevy_tweening, on demand)
//!             └─ fx_pivot -- hurt shake (timer system)
//!                  ├─ base body layer
//!                  ├─ feet / body / off_hand / head / main_hand layers
//!                  └─ ...
//! ```
//!
//! Party members have fully swappable equipment (re-composed on
//! `EquipmentChanged`). Enemies carry an empty `Equipment`, so only their baked
//! archetype body is drawn.

use crate::effects::idle_bob_anim;
use crate::{RenderRng, SpriteManifest, UNIT_SCALE};
use crate::{enemy_slot_position, party_slot_position, rank_scale};
use bevy::prelude::*;
use bevy_tweening::lens::TransformPositionLens;
use bevy_tweening::{Tween, TweenAnim};
use starwood_core::{
    base_item_for_instance, EnemyUnit, Equipment, EquipmentChanged, GameData, ItemInstances,
    PartyMember, SpriteParts,
};
use std::time::Duration;

/// Stored on a rendered unit's root entity; references its animation pivots.
#[derive(Component)]
pub struct UnitVisual {
    pub bob_pivot: Entity,
    pub lunge_pivot: Entity,
    pub fx_pivot: Entity,
}

/// Marks a core entity whose visuals have been built (prevents re-spawning).
#[derive(Component)]
pub(crate) struct RenderedUnit;

/// Marks a unit whose visuals have been torn down (death). Excluded from
/// re-rendering.
#[derive(Component)]
pub(crate) struct Defeated;

/// A single paper-doll sprite layer, tagged with the unit it belongs to so the
/// whole stack can be found, recomposed, flashed, or faded together.
#[derive(Component)]
pub struct UnitLayer {
    pub owner: Entity,
}

#[derive(Component)]
pub(crate) struct BobPivot;
#[derive(Component)]
pub(crate) struct LungePivot;
#[derive(Component)]
pub(crate) struct FxPivot;

/// Last rank (`slot`) a unit's root was positioned for. When the logical
/// `slot` changes (a rank-swap "move"), [`apply_rank_slide`] animates the root
/// to the new rank instead of teleporting.
#[derive(Component)]
pub(crate) struct RankAnchor {
    slot: u8,
}

/// Resolve a unit's root translation + scale from its rank (`slot`).
fn unit_layout(party: Option<&PartyMember>, enemy: Option<&EnemyUnit>) -> (Vec3, f32) {
    match (party, enemy) {
        (Some(member), _) => (party_slot_position(member.slot), UNIT_SCALE * rank_scale(member.slot)),
        (_, Some(unit)) => (enemy_slot_position(unit.slot), UNIT_SCALE * rank_scale(unit.slot)),
        _ => (Vec3::ZERO, UNIT_SCALE),
    }
}

fn unit_slot(party: Option<&PartyMember>, enemy: Option<&EnemyUnit>) -> u8 {
    party.map(|m| m.slot).or_else(|| enemy.map(|u| u.slot)).unwrap_or(0)
}

/// Anchor offset (in 64x64 body-local units) and scale for an equipment layer.
/// `z` orders the layers front-to-back within the body.
struct Anchor {
    translation: Vec3,
    scale: f32,
}

const BASE_ANCHOR: Anchor = Anchor { translation: Vec3::new(0.0, 0.0, 0.0), scale: 1.0 };
const FEET_ANCHOR: Anchor = Anchor { translation: Vec3::new(0.0, -22.0, 0.1), scale: 0.85 };
const BODY_ANCHOR: Anchor = Anchor { translation: Vec3::new(0.0, 0.0, 0.2), scale: 0.95 };
const OFF_HAND_ANCHOR: Anchor = Anchor { translation: Vec3::new(-18.0, -2.0, 0.3), scale: 0.8 };
const HEAD_ANCHOR: Anchor = Anchor { translation: Vec3::new(0.0, 20.0, 0.4), scale: 0.8 };
const MAIN_HAND_ANCHOR: Anchor = Anchor { translation: Vec3::new(18.0, -2.0, 0.5), scale: 0.8 };

/// Detect logical units that have no visuals yet and build their paper-doll.
pub fn spawn_unit_visuals(
    mut commands: Commands,
    manifest: Res<SpriteManifest>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    mut rng: ResMut<RenderRng>,
    query: Query<
        (Entity, &SpriteParts, &Equipment, Option<&PartyMember>, Option<&EnemyUnit>),
        (Without<RenderedUnit>, Without<Defeated>),
    >,
) {
    if !manifest.ready {
        return;
    }

    for (entity, parts, equipment, party, enemy) in &query {
        let (position, scale) = unit_layout(party, enemy);

        commands.entity(entity).insert((
            Transform::from_translation(position).with_scale(Vec3::splat(scale)),
            Visibility::Visible,
            RenderedUnit,
            RankAnchor { slot: unit_slot(party, enemy) },
        ));

        let bob = commands
            .spawn((Transform::default(), Visibility::Inherited, BobPivot, idle_bob_anim(&mut rng.0)))
            .id();
        let lunge = commands.spawn((Transform::default(), Visibility::Inherited, LungePivot)).id();
        let fx = commands.spawn((Transform::default(), Visibility::Inherited, FxPivot)).id();

        commands.entity(entity).add_child(bob);
        commands.entity(bob).add_child(lunge);
        commands.entity(lunge).add_child(fx);

        compose_layers(&mut commands, fx, entity, parts, equipment, &manifest, &data, &instances);

        commands.entity(entity).insert(UnitVisual { bob_pivot: bob, lunge_pivot: lunge, fx_pivot: fx });
    }
}

/// Animate a unit's root to its new rank when its `slot` changes (a rank-swap
/// "move"). Until core drives rank changes at runtime this simply stays put;
/// the slide is ready the moment a `slot` is reassigned.
pub fn apply_rank_slide(
    mut commands: Commands,
    mut units: Query<
        (Entity, &mut Transform, &mut RankAnchor, Option<&PartyMember>, Option<&EnemyUnit>),
        With<UnitVisual>,
    >,
) {
    for (entity, mut transform, mut anchor, party, enemy) in &mut units {
        let slot = unit_slot(party, enemy);
        if slot == anchor.slot {
            continue;
        }
        anchor.slot = slot;
        let (target, scale) = unit_layout(party, enemy);
        let slide = Tween::new(
            EaseFunction::QuadraticInOut,
            Duration::from_millis(280),
            TransformPositionLens { start: transform.translation, end: target },
        );
        commands.entity(entity).insert(TweenAnim::new(slide));
        // Depth scale snaps immediately; the slide animates position only.
        transform.scale = Vec3::splat(scale);
    }
}

/// Rebuild a unit's layers when its equipment changes.
pub fn recompose_on_equipment_change(
    mut commands: Commands,
    mut events: MessageReader<EquipmentChanged>,
    manifest: Res<SpriteManifest>,
    data: Res<GameData>,
    instances: Res<ItemInstances>,
    units: Query<(&UnitVisual, &SpriteParts, &Equipment)>,
    layers: Query<(Entity, &UnitLayer)>,
) {
    for event in events.read() {
        let Ok((visual, parts, equipment)) = units.get(event.entity) else {
            continue;
        };
        for (layer_entity, layer) in &layers {
            if layer.owner == event.entity {
                commands.entity(layer_entity).despawn();
            }
        }
        compose_layers(
            &mut commands,
            visual.fx_pivot,
            event.entity,
            parts,
            equipment,
            &manifest,
            &data,
            &instances,
        );
    }
}

fn compose_layers(
    commands: &mut Commands,
    fx_pivot: Entity,
    owner: Entity,
    parts: &SpriteParts,
    equipment: &Equipment,
    manifest: &SpriteManifest,
    data: &GameData,
    instances: &ItemInstances,
) {
    if let Some(handle) = manifest.get(&parts.base_body) {
        spawn_layer(commands, fx_pivot, owner, handle, &BASE_ANCHOR);
    }

    // Ordered back-to-front so z-fighting never matters even if z values tie.
    let slots: [(&Option<String>, &Anchor); 5] = [
        (&equipment.feet, &FEET_ANCHOR),
        (&equipment.body, &BODY_ANCHOR),
        (&equipment.off_hand, &OFF_HAND_ANCHOR),
        (&equipment.head, &HEAD_ANCHOR),
        (&equipment.main_hand, &MAIN_HAND_ANCHOR),
    ];

    for (slot, anchor) in slots {
        let Some(instance_id) = slot else { continue };
        let Some(item) = base_item_for_instance(instance_id, data, instances) else { continue };
        let Some(handle) = manifest.get(&item.sprite_key) else { continue };
        spawn_layer(commands, fx_pivot, owner, handle, anchor);
    }
}

fn spawn_layer(commands: &mut Commands, fx_pivot: Entity, owner: Entity, handle: Handle<Image>, anchor: &Anchor) {
    let layer = commands
        .spawn((
            Sprite { image: handle, ..default() },
            Transform::from_translation(anchor.translation).with_scale(Vec3::splat(anchor.scale)),
            Visibility::Inherited,
            UnitLayer { owner },
        ))
        .id();
    commands.entity(fx_pivot).add_child(layer);
}
