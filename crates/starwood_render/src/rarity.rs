//! Rarity frame visuals keyed off core's `Rarity` enum via `rarity_rank`.

use bevy::prelude::*;
use starwood_core::{FrameColor, Rarity, rarity_rank};

/// Number of published rarity tiers in core (`Common`..=`Legendary`).
pub const RARITY_TIERS: u8 = 5;

/// Render-only visual styling for one rarity tier.
#[derive(Clone, Copy, Debug)]
pub struct RarityStyle {
    pub name: &'static str,
    pub frame: Color,
    pub fill: Color,
    pub glow: f32,
}

/// Build a style from core's data-driven frame color, keeping tier glow from the
/// fallback table for consistency.
pub fn rarity_style_from_frame(frame: FrameColor, tier: u8) -> RarityStyle {
    let mut style = rarity_style(tier);
    style.frame = Color::srgba(
        frame.r as f32 / 255.0,
        frame.g as f32 / 255.0,
        frame.b as f32 / 255.0,
        frame.a as f32 / 255.0,
    );
    style.fill = Color::srgba(
        frame.r as f32 / 255.0 * 0.55,
        frame.g as f32 / 255.0 * 0.55,
        frame.b as f32 / 255.0 * 0.55,
        0.38,
    );
    style
}

/// Fallback cosmetic table when core data is unavailable (tests / early boot).
pub fn rarity_style(tier: u8) -> RarityStyle {
    match tier.min(RARITY_TIERS - 1) {
        0 => RarityStyle {
            name: "Common",
            frame: Color::srgb(0.69, 0.68, 0.63),
            fill: Color::srgba(0.62, 0.66, 0.72, 0.30),
            glow: 0.0,
        },
        1 => RarityStyle {
            name: "Uncommon",
            frame: Color::srgb(0.33, 0.67, 0.37),
            fill: Color::srgba(0.30, 0.66, 0.30, 0.34),
            glow: 0.15,
        },
        2 => RarityStyle {
            name: "Rare",
            frame: Color::srgb(0.29, 0.52, 0.85),
            fill: Color::srgba(0.24, 0.48, 0.85, 0.36),
            glow: 0.30,
        },
        3 => RarityStyle {
            name: "Epic",
            frame: Color::srgb(0.65, 0.34, 0.80),
            fill: Color::srgba(0.55, 0.30, 0.82, 0.38),
            glow: 0.45,
        },
        _ => RarityStyle {
            name: "Legendary",
            frame: Color::srgb(0.90, 0.61, 0.18),
            fill: Color::srgba(0.86, 0.56, 0.18, 0.40),
            glow: 0.65,
        },
    }
}

/// Sprite-manifest key for a rarity frame (tier = `rarity_rank` output).
pub fn rarity_frame_key(tier: u8) -> String {
    format!("rarity_frame_{}", tier.min(RARITY_TIERS - 1))
}

/// Sprite-manifest key for a core `Rarity` variant.
pub fn rarity_frame_key_for(rarity: Rarity) -> String {
    rarity_frame_key(rarity_rank(rarity))
}
