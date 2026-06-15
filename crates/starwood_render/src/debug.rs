//! Visual debug overlay for rank/slot layout and paper-doll anchor points.
//!
//! Toggle with **F3** during play. Shows party rank slots (left), enemy rank
//! slots (right), and per-unit anchor crosshairs that track live positions.

use crate::{UNIT_SCALE, enemy_slot_position, party_slot_position};
use bevy::prelude::*;
use starwood_core::{EnemyUnit, PartyMember};

/// When true, rank boundaries and anchor markers are drawn over the world.
#[derive(Resource, Default)]
pub struct RenderDebugOverlay {
    pub enabled: bool,
}

#[derive(Component)]
pub(crate) struct DebugRankMarker;

#[derive(Component)]
pub(crate) struct DebugUnitMarker;

/// Toggle the overlay with F3.
pub fn toggle_debug_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<RenderDebugOverlay>,
) {
    if keyboard.just_pressed(KeyCode::F3) {
        overlay.enabled = !overlay.enabled;
        info!(
            "starwood_render: debug overlay {}",
            if overlay.enabled {
                "ON (F3 to hide)"
            } else {
                "OFF"
            }
        );
    }
}

/// Spawn static rank-slot outlines when debug mode turns on; remove all markers when off.
pub fn sync_debug_rank_markers(
    mut commands: Commands,
    overlay: Res<RenderDebugOverlay>,
    ranks: Query<Entity, With<DebugRankMarker>>,
    units: Query<Entity, With<DebugUnitMarker>>,
) {
    if !overlay.is_changed() && !overlay.enabled {
        return;
    }

    if !overlay.enabled {
        for entity in ranks.iter().chain(units.iter()) {
            commands.entity(entity).despawn();
        }
        return;
    }

    if !ranks.is_empty() {
        return;
    }

    for slot in 0u8..4 {
        let pos = party_slot_position(slot);
        spawn_rank_marker(
            &mut commands,
            pos,
            Color::srgba(0.35, 0.75, 0.95, 0.45),
            slot,
            "P",
        );
    }
    for slot in 0u8..5 {
        let pos = enemy_slot_position(slot);
        spawn_rank_marker(
            &mut commands,
            pos,
            Color::srgba(0.95, 0.45, 0.40, 0.45),
            slot,
            "E",
        );
    }
}

/// Refresh per-unit crosshairs and rank labels every frame while debug is on.
pub fn update_debug_unit_markers(
    mut commands: Commands,
    overlay: Res<RenderDebugOverlay>,
    existing: Query<Entity, With<DebugUnitMarker>>,
    party_units: Query<(&Transform, &PartyMember), With<PartyMember>>,
    enemy_units: Query<(&Transform, &EnemyUnit), With<EnemyUnit>>,
) {
    if !overlay.enabled {
        return;
    }

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for (transform, member) in &party_units {
        spawn_unit_markers(
            &mut commands,
            transform.translation,
            Color::srgba(0.4, 0.9, 1.0, 0.9),
            format!("P{}", member.slot),
        );
    }
    for (transform, enemy) in &enemy_units {
        spawn_unit_markers(
            &mut commands,
            transform.translation,
            Color::srgba(1.0, 0.55, 0.45, 0.9),
            format!("E{}", enemy.slot),
        );
    }
}

fn spawn_rank_marker(commands: &mut Commands, center: Vec3, color: Color, slot: u8, side: &str) {
    commands.spawn((
        Sprite::from_color(color, Vec2::new(90.0, 120.0)),
        Transform::from_translation(center + Vec3::new(0.0, 10.0, 5.0)),
        DebugRankMarker,
        Name::new(format!("debug_rank_{side}{slot}")),
    ));
}

fn spawn_unit_markers(commands: &mut Commands, center: Vec3, color: Color, label: String) {
    let anchor = center + Vec3::new(0.0, -40.0, 6.0);
    let arm = 14.0;
    commands.spawn((
        Sprite::from_color(color, Vec2::new(arm * 2.0, 3.0)),
        Transform::from_translation(anchor),
        DebugUnitMarker,
    ));
    commands.spawn((
        Sprite::from_color(color, Vec2::new(3.0, arm * 2.0)),
        Transform::from_translation(anchor),
        DebugUnitMarker,
    ));
    commands.spawn((
        Text2d::new(label),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(color),
        Transform::from_translation(center + Vec3::new(0.0, 48.0 * UNIT_SCALE, 7.0)),
        DebugUnitMarker,
    ));
}
