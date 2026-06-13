//! World animations and combat-reaction effects.
//!
//! - Idle "breathing" bob: an infinite mirrored translate on `bob_pivot`
//!   (bevy_tweening), desynced per unit so the party doesn't pulse in lockstep.
//! - Attack lunge: a forward-then-back translate on `lunge_pivot` triggered by
//!   `CombatActionRequest` (bevy_tweening sequence).
//! - Hurt flash + shake: on `DamageDealt`, the target's layers tint red and its
//!   `fx_pivot` jitters (timer systems, using the render-only RNG).
//! - Two-tier death: on `UnitDied`, companions/enemies dissolve (layers fade,
//!   hierarchy torn down, entity marked `Defeated`), while the player character
//!   instead enters a distinct **downed** state — a pulsing pale tint that is
//!   cleared automatically when its health returns above 0 (a revive).
//! - Stage clear: on `EncounterEnded`, enemy entities are despawned.
//!
//! Hurt-flash is keyed to each `DamageDealt` target individually, so an AoE
//! that damages your own ranks (friendly fire) flashes those allies too — the
//! lane model needs no special-casing here.

use crate::RenderRng;
use crate::paperdoll::{Defeated, RenderedUnit, UnitLayer, UnitVisual};
use bevy::prelude::*;
use bevy_tweening::lens::TransformPositionLens;
use bevy_tweening::{RepeatCount, RepeatStrategy, Tween, TweenAnim};
use rand::RngCore;
use rand_chacha::ChaCha8Rng;
use starwood_core::{
    CombatAction, CombatActionRequest, DamageDealt, Downed, EncounterEnded, EnemyUnit, Health,
    PlayerCharacter, UnitDied,
};
use std::time::Duration;

#[derive(Component)]
pub(crate) struct ShakeEffect {
    timer: Timer,
    magnitude: f32,
}

#[derive(Component)]
pub(crate) struct FlashEffect {
    timer: Timer,
}

#[derive(Component)]
pub(crate) struct DeathFade {
    timer: Timer,
}

/// Marks a unit shown in the "downed" state (the revivable PC at 0 HP) rather
/// than dissolving like a permanently-dead companion. Render applies it when
/// core's `PlayerCharacter` receives `UnitDied`; it is cleared when core
/// removes `Downed` or the unit's health returns above 0.
#[derive(Component)]
pub struct DownedVisual;

/// Build the looping idle-bob animation for a `bob_pivot`, desynced via the
/// render RNG so units breathe independently.
pub(crate) fn idle_bob_anim(rng: &mut ChaCha8Rng) -> TweenAnim {
    let duration = 1200 + (rng.next_u32() % 700) as u64;
    let tween = Tween::new(
        EaseFunction::SineInOut,
        Duration::from_millis(duration),
        TransformPositionLens { start: Vec3::new(0.0, -1.5, 0.0), end: Vec3::new(0.0, 1.5, 0.0) },
    )
    .with_repeat_count(RepeatCount::Infinite)
    .with_repeat_strategy(RepeatStrategy::MirroredRepeat);
    TweenAnim::new(tween)
}

/// Lunge the acting unit toward its target when it requests an attack.
pub fn lunge_on_action(
    mut commands: Commands,
    mut actions: MessageReader<CombatActionRequest>,
    units: Query<&UnitVisual>,
    transforms: Query<&Transform>,
) {
    for action in actions.read() {
        if action.action != CombatAction::Attack {
            continue;
        }
        let Ok(visual) = units.get(action.actor) else {
            continue;
        };
        let actor_x = transforms.get(action.actor).map(|t| t.translation.x).unwrap_or(0.0);
        let target_x = transforms.get(action.target).map(|t| t.translation.x).unwrap_or(actor_x + 1.0);
        let reach = if target_x >= actor_x { 16.0 } else { -16.0 };

        let forward = Tween::new(
            EaseFunction::QuadraticOut,
            Duration::from_millis(120),
            TransformPositionLens { start: Vec3::ZERO, end: Vec3::new(reach, 0.0, 0.0) },
        );
        let back = Tween::new(
            EaseFunction::QuadraticIn,
            Duration::from_millis(170),
            TransformPositionLens { start: Vec3::new(reach, 0.0, 0.0), end: Vec3::ZERO },
        );
        commands.entity(visual.lunge_pivot).insert(TweenAnim::new(forward.then(back)));
    }
}

/// On damage, shake the target and flash all of its layers red.
pub fn react_to_damage(
    mut commands: Commands,
    mut damage: MessageReader<DamageDealt>,
    units: Query<&UnitVisual>,
    layers: Query<(Entity, &UnitLayer)>,
) {
    for hit in damage.read() {
        if let Ok(visual) = units.get(hit.target) {
            commands.entity(visual.fx_pivot).insert(ShakeEffect {
                timer: Timer::from_seconds(0.28, TimerMode::Once),
                magnitude: if hit.is_crit { 7.0 } else { 4.0 },
            });
        }
        for (layer_entity, layer) in &layers {
            if layer.owner == hit.target {
                commands.entity(layer_entity).insert(FlashEffect { timer: Timer::from_seconds(0.25, TimerMode::Once) });
            }
        }
    }
}

pub fn run_shake(
    mut commands: Commands,
    time: Res<Time>,
    mut rng: ResMut<RenderRng>,
    mut query: Query<(Entity, &mut Transform, &mut ShakeEffect)>,
) {
    for (entity, mut transform, mut shake) in &mut query {
        shake.timer.tick(time.delta());
        if shake.timer.is_finished() {
            transform.translation.x = 0.0;
            transform.translation.y = 0.0;
            commands.entity(entity).remove::<ShakeEffect>();
        } else {
            let amplitude = shake.magnitude * shake.timer.fraction_remaining();
            transform.translation.x = signed_unit(&mut rng.0) * amplitude;
            transform.translation.y = signed_unit(&mut rng.0) * amplitude;
        }
    }
}

pub fn run_flash(mut commands: Commands, time: Res<Time>, mut query: Query<(Entity, &mut Sprite, &mut FlashEffect)>) {
    for (entity, mut sprite, mut flash) in &mut query {
        flash.timer.tick(time.delta());
        if flash.timer.is_finished() {
            sprite.color = Color::WHITE;
            commands.entity(entity).remove::<FlashEffect>();
        } else {
            // Strong red at impact, easing back to the untinted sprite.
            sprite.color = mix_color(Color::srgb(1.0, 0.25, 0.25), Color::WHITE, flash.timer.fraction());
        }
    }
}

/// Route a unit's death: the player character (revivable) enters the downed
/// state; everyone else (companions, enemies) dissolves permanently.
pub fn route_unit_death(
    mut commands: Commands,
    mut died: MessageReader<UnitDied>,
    pc: Query<&PlayerCharacter>,
    units: Query<(), (With<UnitVisual>, Without<Defeated>, Without<DownedVisual>)>,
) {
    for death in died.read() {
        if units.get(death.entity).is_err() {
            continue;
        }
        if pc.get(death.entity).is_ok() {
            commands.entity(death.entity).insert(DownedVisual);
        } else {
            commands.entity(death.entity).insert(DeathFade { timer: Timer::from_seconds(0.6, TimerMode::Once) });
        }
    }
}

/// Render the downed state: a slow pulsing pale-blue tint over the unit's
/// layers — clearly distinct from the red hurt-flash and the death dissolve,
/// reading as "incapacitated, awaiting revive."
pub fn apply_downed_visual(
    time: Res<Time>,
    downed: Query<Entity, With<DownedVisual>>,
    mut layers: Query<(&UnitLayer, &mut Sprite)>,
) {
    if downed.is_empty() {
        return;
    }
    let pulse = 0.45 + 0.18 * ((time.elapsed_secs() * 3.2).sin() * 0.5 + 0.5);
    for (layer, mut sprite) in &mut layers {
        if downed.contains(layer.owner) {
            sprite.color = Color::srgba(0.55, 0.66, 0.92, pulse);
        }
    }
}

/// Clear the downed visual once core removes `Downed` or the PC's health is
/// restored (a revive), returning its layers to full color.
pub fn clear_downed_on_revive(
    mut commands: Commands,
    revived: Query<(Entity, &Health, Option<&Downed>), With<DownedVisual>>,
    mut layers: Query<(&UnitLayer, &mut Sprite)>,
) {
    for (entity, health, downed) in &revived {
        if health.current > 0 || downed.is_none() {
            commands.entity(entity).remove::<DownedVisual>();
            for (layer, mut sprite) in &mut layers {
                if layer.owner == entity {
                    sprite.color = Color::WHITE;
                }
            }
        }
    }
}

pub fn run_death_fade(
    mut commands: Commands,
    time: Res<Time>,
    mut units: Query<(Entity, &UnitVisual, &mut DeathFade)>,
    mut layers: Query<(&UnitLayer, &mut Sprite)>,
) {
    for (entity, visual, mut fade) in &mut units {
        fade.timer.tick(time.delta());
        let alpha = fade.timer.fraction_remaining();
        for (layer, mut sprite) in &mut layers {
            if layer.owner == entity {
                let mut color = sprite.color.to_srgba();
                color.alpha = alpha;
                sprite.color = color.into();
            }
        }
        if fade.timer.is_finished() {
            commands.entity(visual.bob_pivot).despawn();
            commands
                .entity(entity)
                .remove::<DeathFade>()
                .remove::<UnitVisual>()
                .remove::<RenderedUnit>()
                .insert(Defeated)
                .insert(Visibility::Hidden);
        }
    }
}

/// Clear the enemy stage when an encounter ends.
pub fn clear_stage_on_encounter_end(
    mut commands: Commands,
    mut ended: MessageReader<EncounterEnded>,
    enemies: Query<Entity, With<EnemyUnit>>,
) {
    for _ in ended.read() {
        for entity in &enemies {
            commands.entity(entity).despawn();
        }
    }
}

fn signed_unit(rng: &mut ChaCha8Rng) -> f32 {
    (rng.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let a = a.to_srgba();
    let b = b.to_srgba();
    let t = t.clamp(0.0, 1.0);
    Color::srgba(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
        a.alpha + (b.alpha - a.alpha) * t,
    )
}
