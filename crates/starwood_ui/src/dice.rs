//! The Dice Theater — the marquee feature.
//!
//! # Event flow (the only contract we touch)
//! ```text
//!   (anyone) RollRequest ──▶ core resolves ──▶ RollResolved {id,total,is_nat20,is_nat1}
//!                                                     │
//!                          this module animates a die │ toward the decided value
//!                                                     ▼
//!                          ... tumble, land, flourish ...
//!                                                     │
//!                              RollAnimationComplete {id}  ◀── fired here, once
//!                                                     │
//!                          core applies the roll's consequences (damage, etc.)
//! ```
//!
//! We are **never** authoritative: we read `total` / `is_nat20` / `is_nat1` from
//! the event and animate toward them. The nat-20 / nat-1 effects are purely
//! cosmetic reactions to those flags. When the animation (and its flourish)
//! finishes we fire [`RollAnimationComplete`] with the same `id`.
//!
//! Rendering is hand-rolled from `bevy` sprites + `Text2d` (the blueprint's
//! sprite-particle fallback), so it carries no version-sensitive particle or
//! tweening dependency.

use std::collections::VecDeque;

use bevy::prelude::*;
use starwood_core::{EventReader, EventWriter, RollAnimationComplete, RollResolved};

pub struct DiceTheaterPlugin;

impl Plugin for DiceTheaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DiceTheater>()
            .init_resource::<CameraShake>()
            .add_systems(
                Update,
                (
                    enqueue_resolved,
                    run_theater,
                    update_particles,
                    update_screen_flash,
                    apply_camera_shake,
                )
                    .chain(),
            );
    }
}

// ===== Timeline (seconds) ==========================================
const POP_IN: f32 = 0.16;
const TUMBLE: f32 = 0.85;
const UNWIND: f32 = 0.22;
const HOLD: f32 = 0.55;
const DONE: f32 = TUMBLE + HOLD;

const DIE_SIZE: f32 = 92.0;
const Z_FLASH: f32 = 90.0;
const Z_DIE: f32 = 100.0;
const Z_LABEL: f32 = 101.0;
const Z_PARTICLE: f32 = 102.0;

// ===== Palette (bevy colors mirroring the egui theme) ==============
const GOLD: Color = Color::srgb(0.84, 0.70, 0.40);
const GOLD_BRIGHT: Color = Color::srgb(0.96, 0.84, 0.55);
const BLOOD: Color = Color::srgb(0.69, 0.23, 0.23);
const INK: Color = Color::srgb(0.89, 0.86, 0.80);

// ===== Components & resources ======================================
#[derive(Component)]
struct DiceDie;

#[derive(Component)]
struct DiceLabel;

#[derive(Component)]
struct DiceParticle {
    velocity: Vec2,
    life: f32,
    max_life: f32,
    color: Color,
}

#[derive(Component)]
struct ScreenFlash {
    life: f32,
    max_life: f32,
    color: Color,
}

#[derive(Resource, Default)]
struct CameraShake {
    remaining: f32,
    magnitude: f32,
}

struct DiceJob {
    id: u64,
    total: i32,
    is_nat20: bool,
    is_nat1: bool,
}

struct ActiveDie {
    job: DiceJob,
    elapsed: f32,
    die: Entity,
    label: Entity,
    revealed: bool,
}

#[derive(Resource, Default)]
pub struct DiceTheater {
    queue: VecDeque<DiceJob>,
    active: Option<ActiveDie>,
}

// ===== Systems =====================================================

/// Turn each authoritative `RollResolved` into an animation job.
fn enqueue_resolved(mut resolved: EventReader<RollResolved>, mut theater: ResMut<DiceTheater>) {
    for event in resolved.read() {
        theater.queue.push_back(DiceJob {
            id: event.id,
            total: event.total,
            is_nat20: event.is_nat20,
            is_nat1: event.is_nat1,
        });
    }
}

/// Spawn, animate, and retire the active die; fire `RollAnimationComplete` when
/// its animation finishes.
#[allow(clippy::too_many_arguments)]
fn run_theater(
    time: Res<Time>,
    mut commands: Commands,
    mut theater: ResMut<DiceTheater>,
    mut dice_q: Query<(&mut Transform, &mut Sprite), With<DiceDie>>,
    mut label_q: Query<(&mut Text2d, &mut TextColor), (With<DiceLabel>, Without<DiceDie>)>,
    mut complete: EventWriter<RollAnimationComplete>,
    mut shake: ResMut<CameraShake>,
) {
    // Start the next job if idle.
    if theater.active.is_none() {
        let Some(job) = theater.queue.pop_front() else {
            return;
        };
        let die = commands
            .spawn((
                Sprite::from_color(GOLD, Vec2::splat(DIE_SIZE)),
                Transform::from_xyz(0.0, 24.0, Z_DIE).with_scale(Vec3::splat(0.01)),
                DiceDie,
                Name::new("DiceDie"),
            ))
            .id();
        let label = commands
            .spawn((
                Text2d::new(""),
                TextFont {
                    font_size: 48.0,
                    ..default()
                },
                TextColor(INK),
                Transform::from_xyz(0.0, 24.0, Z_LABEL),
                DiceLabel,
                Name::new("DiceLabel"),
            ))
            .id();
        theater.active = Some(ActiveDie {
            job,
            elapsed: 0.0,
            die,
            label,
            revealed: false,
        });
    }

    let dt = time.delta_secs();
    let Some(active) = theater.active.as_mut() else {
        return;
    };
    active.elapsed += dt;
    let e = active.elapsed;

    // --- Die transform & color ---
    if let Ok((mut tf, mut sprite)) = dice_q.get_mut(active.die) {
        let scale = ease_out((e / POP_IN).clamp(0.0, 1.0));
        let pulse = if active.revealed && active.job.is_nat20 {
            1.0 + 0.12 * (e * 14.0).sin().max(0.0)
        } else {
            1.0
        };
        tf.scale = Vec3::splat(scale * pulse);

        let spin = TUMBLE * 17.0;
        let angle = if e < TUMBLE {
            e * 17.0
        } else {
            // Ease the spin to rest at the nearest quarter-turn with a small tilt.
            let t = ((e - TUMBLE) / UNWIND).clamp(0.0, 1.0);
            let target =
                (spin / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2 + 0.16;
            lerp(spin, target, ease_out(t))
        };
        tf.rotation = Quat::from_rotation_z(angle);

        sprite.color = if active.revealed {
            if active.job.is_nat20 {
                GOLD_BRIGHT
            } else if active.job.is_nat1 {
                BLOOD
            } else {
                GOLD
            }
        } else {
            GOLD
        };
    }

    // --- Reveal the value and play the flourish, exactly once ---
    if !active.revealed && e >= TUMBLE {
        active.revealed = true;
        if let Ok((mut text, mut color)) = label_q.get_mut(active.label) {
            text.0 = active.job.total.to_string();
            color.0 = if active.job.is_nat20 {
                GOLD_BRIGHT
            } else if active.job.is_nat1 {
                BLOOD
            } else {
                INK
            };
        }
        if active.job.is_nat20 {
            spawn_burst(&mut commands, GOLD_BRIGHT, 28, 360.0, active.job.id);
            spawn_flash(&mut commands, Color::srgba(0.96, 0.84, 0.55, 0.45));
        } else if active.job.is_nat1 {
            spawn_burst(&mut commands, BLOOD, 16, 220.0, active.job.id);
            spawn_flash(&mut commands, Color::srgba(0.0, 0.0, 0.0, 0.55));
            shake.remaining = 0.35;
            shake.magnitude = 9.0;
        }
    }

    // --- Finish: fire completion and retire the die ---
    if e >= DONE {
        // Copy out the Copy fields so the `active` borrow ends before we clear it.
        let (id, die, label) = (active.job.id, active.die, active.label);
        complete.write(RollAnimationComplete { id });
        commands.entity(die).despawn();
        commands.entity(label).despawn();
        theater.active = None;
    }
}

/// Move, fade, and retire burst particles.
fn update_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut particles: Query<(Entity, &mut Transform, &mut Sprite, &mut DiceParticle)>,
) {
    let dt = time.delta_secs();
    for (entity, mut tf, mut sprite, mut particle) in &mut particles {
        particle.life -= dt;
        if particle.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= 520.0 * dt; // gravity
        tf.translation.x += particle.velocity.x * dt;
        tf.translation.y += particle.velocity.y * dt;
        let alpha = (particle.life / particle.max_life).clamp(0.0, 1.0);
        sprite.color = particle.color.with_alpha(alpha);
    }
}

/// Fade and retire the full-screen flash overlay.
fn update_screen_flash(
    time: Res<Time>,
    mut commands: Commands,
    mut flashes: Query<(Entity, &mut Sprite, &mut ScreenFlash)>,
) {
    let dt = time.delta_secs();
    for (entity, mut sprite, mut flash) in &mut flashes {
        flash.life -= dt;
        if flash.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = (flash.life / flash.max_life).clamp(0.0, 1.0);
        sprite.color = flash.color.with_alpha(flash.color.alpha() * alpha);
    }
}

/// Decay a one-shot camera shake (used for nat-1).
fn apply_camera_shake(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut cameras: Query<&mut Transform, With<Camera2d>>,
) {
    let Ok(mut tf) = cameras.single_mut() else {
        return;
    };
    if shake.remaining <= 0.0 {
        // Rest at the origin (the camera does not otherwise move).
        tf.translation.x = 0.0;
        tf.translation.y = 0.0;
        return;
    }
    shake.remaining -= time.delta_secs();
    let strength = shake.magnitude * (shake.remaining / 0.35).clamp(0.0, 1.0);
    let t = time.elapsed_secs() * 47.0;
    tf.translation.x = strength * t.sin();
    tf.translation.y = strength * (t * 1.3).cos();
}

// ===== Spawning helpers ============================================

/// Spawn a radial burst of small sprite particles. Directions are derived
/// deterministically from the roll id so no RNG dependency is needed here.
fn spawn_burst(commands: &mut Commands, color: Color, count: u32, speed: f32, id: u64) {
    let phase = (id as f32) * 0.61803;
    for i in 0..count {
        let frac = i as f32 / count as f32;
        let angle = frac * std::f32::consts::TAU + phase;
        let speed = speed * (0.6 + 0.4 * ((i * 7) % 5) as f32 / 4.0);
        let velocity = Vec2::new(angle.cos(), angle.sin()) * speed + Vec2::Y * 60.0;
        let size = 6.0 + ((i * 3) % 4) as f32 * 2.0;
        commands.spawn((
            Sprite::from_color(color, Vec2::splat(size)),
            Transform::from_xyz(0.0, 24.0, Z_PARTICLE),
            DiceParticle {
                velocity,
                life: 0.7,
                max_life: 0.7,
                color,
            },
        ));
    }
}

/// Spawn a fading full-screen overlay (oversized so it covers any window size).
fn spawn_flash(commands: &mut Commands, color: Color) {
    commands.spawn((
        Sprite::from_color(color, Vec2::new(6000.0, 4000.0)),
        Transform::from_xyz(0.0, 0.0, Z_FLASH),
        ScreenFlash {
            life: 0.5,
            max_life: 0.5,
            color,
        },
    ));
}

// ===== Math helpers ================================================
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}
