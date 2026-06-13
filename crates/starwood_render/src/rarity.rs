//! Rarity *visuals* (frame/background colors) — a render-only concern.
//!
//! Core owns gameplay `Rarity`; callers map it to a tier via `rarity_rank` and
//! pick frame colors from this table. These styles are cosmetic only.
//!
//! Palette is classic high-fantasy and bright/earnest (not grim): the familiar
//! gray → green → blue → purple → gold → red ladder.

use bevy::prelude::*;

/// How many rarity tiers render generates frames/styles for.
pub const RARITY_TIERS: u8 = 6;

/// Render-only visual styling for one rarity tier.
#[derive(Clone, Copy, Debug)]
pub struct RarityStyle {
    pub name: &'static str,
    /// Bright border color drawn around the icon.
    pub frame: Color,
    /// Soft translucent background behind the icon.
    pub fill: Color,
    /// 0..1 emphasis used for frame brightness / future bloom.
    pub glow: f32,
}

/// The visual style for a rarity tier (tiers at/above the table clamp to the
/// top tier).
pub fn rarity_style(tier: u8) -> RarityStyle {
    match tier {
        0 => RarityStyle {
            name: "Common",
            frame: Color::srgb(0.80, 0.82, 0.86),
            fill: Color::srgba(0.62, 0.66, 0.72, 0.30),
            glow: 0.0,
        },
        1 => RarityStyle {
            name: "Uncommon",
            frame: Color::srgb(0.40, 0.83, 0.38),
            fill: Color::srgba(0.30, 0.66, 0.30, 0.34),
            glow: 0.15,
        },
        2 => RarityStyle {
            name: "Rare",
            frame: Color::srgb(0.32, 0.62, 0.98),
            fill: Color::srgba(0.24, 0.48, 0.85, 0.36),
            glow: 0.30,
        },
        3 => RarityStyle {
            name: "Epic",
            frame: Color::srgb(0.74, 0.42, 0.96),
            fill: Color::srgba(0.55, 0.30, 0.82, 0.38),
            glow: 0.45,
        },
        4 => RarityStyle {
            name: "Legendary",
            frame: Color::srgb(0.98, 0.72, 0.26),
            fill: Color::srgba(0.86, 0.56, 0.18, 0.40),
            glow: 0.65,
        },
        _ => RarityStyle {
            name: "Mythic",
            frame: Color::srgb(0.98, 0.38, 0.34),
            fill: Color::srgba(0.86, 0.26, 0.26, 0.42),
            glow: 0.85,
        },
    }
}

/// Sprite-manifest key for a rarity frame placeholder texture.
pub fn rarity_frame_key(tier: u8) -> String {
    format!("rarity_frame_{}", tier.min(RARITY_TIERS - 1))
}
