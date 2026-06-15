//! Programmatic placeholder pixel-art generation.
//!
//! Every sprite key referenced by `GameData` (race base bodies, equipment
//! items, and enemy archetypes) is turned into a small, distinct, palette
//! consistent texture at runtime. Real art can later replace these by dropping
//! files into `assets/sprites/` keyed by the same sprite key — no code change.
//!
//! Bodies/enemies are drawn on a 64x64 canvas; items on a 32x32 canvas. All
//! generation is deterministic: a given sprite key always produces the same
//! placeholder, so visuals are stable across runs.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use starwood_core::{FrameColor, ItemData, ItemSlot};

pub const BODY_SIZE: u32 = 64;
pub const ITEM_SIZE: u32 = 32;
/// Side length of a rarity-frame placeholder (sits behind a 32px item icon).
pub const FRAME_SIZE: u32 = 48;

type Rgba = [u8; 4];

const OUTLINE: Rgba = [26, 22, 34, 255];
const SHADOW: Rgba = [0, 0, 0, 70];

/// A cohesive, limited dark-/high-fantasy palette. Primary colors for sprites
/// are chosen deterministically from this list so the whole cast reads as one
/// art set.
const PALETTE: [Rgba; 12] = [
    [201, 173, 119, 255], // worn gold
    [122, 161, 92, 255],  // moss
    [86, 124, 173, 255],  // steel blue
    [161, 78, 86, 255],   // oxblood
    [120, 96, 150, 255],  // arcane violet
    [196, 150, 66, 255],  // brass
    [88, 145, 142, 255],  // verdigris
    [158, 110, 70, 255],  // tanned leather
    [206, 205, 214, 255], // bone
    [74, 92, 112, 255],   // slate
    [158, 70, 138, 255],  // royal magenta
    [104, 120, 78, 255],  // olive
];

/// A tiny CPU canvas used to compose placeholder sprites pixel by pixel.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; (w * h * 4) as usize],
        }
    }

    fn blend(&mut self, x: i32, y: i32, c: Rgba) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || c[3] == 0 {
            return;
        }
        let idx = ((y as u32 * self.w + x as u32) * 4) as usize;
        if c[3] == 255 {
            self.px[idx..idx + 4].copy_from_slice(&c);
            return;
        }
        // Simple "over" alpha blend onto the existing pixel.
        let a = c[3] as f32 / 255.0;
        for (k, channel) in c.iter().take(3).enumerate() {
            let dst = self.px[idx + k] as f32;
            self.px[idx + k] = (*channel as f32 * a + dst * (1.0 - a)).round() as u8;
        }
        let dst_a = self.px[idx + 3] as f32 / 255.0;
        self.px[idx + 3] = (((a + dst_a * (1.0 - a)) * 255.0).round() as u8).max(self.px[idx + 3]);
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, c);
            }
        }
    }

    /// Filled rectangle with a 1px dark outline drawn just outside it.
    fn panel(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, fill: Rgba) {
        self.rect(x0 - 1, y0 - 1, x1 + 1, y1 + 1, OUTLINE);
        self.rect(x0, y0, x1, y1, fill);
    }

    fn disc(&mut self, cx: i32, cy: i32, r: i32, c: Rgba) {
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= r * r {
                    self.blend(x, y, c);
                }
            }
        }
    }

    fn disc_outlined(&mut self, cx: i32, cy: i32, r: i32, fill: Rgba) {
        self.disc(cx, cy, r + 1, OUTLINE);
        self.disc(cx, cy, r, fill);
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32, c: Rgba) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let x = (x0 as f32 + (x1 - x0) as f32 * t).round() as i32;
            let y = (y0 as f32 + (y1 - y0) as f32 * t).round() as i32;
            let h = thickness / 2;
            self.rect(x - h, y - h, x - h + thickness, y - h + thickness, c);
        }
    }

    fn into_image(self) -> Image {
        let mut image = Image::new(
            Extent3d {
                width: self.w,
                height: self.h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.px,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        // Crisp pixel-art scaling: never bilinear-blur the placeholders.
        image.sampler = ImageSampler::nearest();
        image
    }
}

fn fnv1a(key: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn primary_for(key: &str) -> Rgba {
    PALETTE[(fnv1a(key) % PALETTE.len() as u64) as usize]
}

fn shade(c: Rgba, factor: f32) -> Rgba {
    [
        (c[0] as f32 * factor).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * factor).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * factor).clamp(0.0, 255.0) as u8,
        c[3],
    ]
}

/// Generate a humanoid base-body placeholder (used for party race sprites).
pub fn generate_body(key: &str) -> Image {
    let mut c = Canvas::new(BODY_SIZE, BODY_SIZE);
    let primary = primary_for(key);
    let dark = shade(primary, 0.7);
    let skin = shade(primary, 1.15);

    // Ground shadow.
    c.disc(32, 60, 11, SHADOW);

    // Legs.
    c.panel(25, 42, 31, 58, dark);
    c.panel(33, 42, 39, 58, dark);
    // Torso.
    c.panel(23, 24, 41, 44, primary);
    // Arms.
    c.panel(17, 25, 23, 41, dark);
    c.panel(41, 25, 47, 41, dark);
    // Head.
    c.disc_outlined(32, 15, 8, skin);
    // Eyes for a bit of character.
    c.rect(29, 14, 31, 16, OUTLINE);
    c.rect(34, 14, 36, 16, OUTLINE);

    c.into_image()
}

/// Generate an enemy placeholder. The silhouette varies by archetype so the
/// stage reads as a varied group rather than identical blobs.
pub fn generate_enemy(key: &str) -> Image {
    let mut c = Canvas::new(BODY_SIZE, BODY_SIZE);
    let primary = primary_for(key);
    let dark = shade(primary, 0.65);
    let eye: Rgba = [240, 210, 120, 255];

    c.disc(32, 60, 13, SHADOW);

    match fnv1a(key) % 4 {
        0 => {
            // Hulking humanoid brute.
            c.panel(22, 40, 30, 58, dark);
            c.panel(34, 40, 42, 58, dark);
            c.panel(20, 22, 44, 42, primary);
            c.panel(13, 24, 20, 44, dark);
            c.panel(44, 24, 51, 44, dark);
            c.disc_outlined(32, 14, 9, primary);
            c.rect(28, 13, 31, 16, eye);
            c.rect(34, 13, 37, 16, eye);
        }
        1 => {
            // Quadruped beast.
            c.panel(18, 30, 46, 46, primary);
            c.panel(20, 46, 25, 57, dark);
            c.panel(39, 46, 44, 57, dark);
            c.panel(42, 22, 54, 36, primary); // head
            c.rect(48, 26, 51, 29, eye);
            c.line(18, 32, 8, 26, 3, dark); // tail
        }
        2 => {
            // Amorphous blob.
            c.disc_outlined(32, 38, 18, primary);
            c.disc(26, 34, 4, eye);
            c.disc(38, 34, 4, eye);
            c.rect(26, 33, 28, 35, OUTLINE);
            c.rect(38, 33, 40, 35, OUTLINE);
        }
        _ => {
            // Winged horror.
            c.line(30, 24, 8, 14, 3, dark); // left wing
            c.line(34, 24, 56, 14, 3, dark); // right wing
            c.panel(27, 22, 37, 50, primary);
            c.disc_outlined(32, 16, 7, dark);
            c.rect(29, 15, 31, 17, eye);
            c.rect(33, 15, 35, 17, eye);
        }
    }

    c.into_image()
}

/// Generate a 32x32 item icon placeholder, shaped by the item's slot so it is
/// recognizable both as an inventory icon and as a paper-doll layer.
pub fn generate_item(item: &ItemData) -> Image {
    let mut c = Canvas::new(ITEM_SIZE, ITEM_SIZE);
    let primary = primary_for(&item.sprite_key);
    let metal: Rgba = [196, 200, 210, 255];
    let dark_metal = shade(metal, 0.6);
    let wood: Rgba = [150, 104, 62, 255];

    match item.slot {
        ItemSlot::MainHand => match fnv1a(&item.sprite_key) % 4 {
            0 => {
                // Sword.
                c.line(16, 4, 16, 22, 4, metal);
                c.line(16, 4, 16, 22, 2, shade(metal, 1.1));
                c.rect(10, 22, 22, 25, dark_metal); // crossguard
                c.rect(15, 25, 17, 30, wood); // hilt
            }
            1 => {
                // Dagger.
                c.line(16, 9, 16, 21, 3, metal);
                c.rect(12, 21, 20, 23, dark_metal);
                c.rect(15, 23, 17, 28, wood);
            }
            2 => {
                // Staff with orb.
                c.line(16, 8, 16, 30, 2, wood);
                c.disc_outlined(16, 7, 4, primary);
            }
            _ => {
                // Bow.
                c.line(12, 5, 12, 27, 2, wood);
                c.line(12, 5, 20, 9, 2, wood);
                c.line(12, 27, 20, 23, 2, wood);
                c.line(20, 9, 20, 23, 1, [230, 230, 230, 200]); // string
            }
        },
        ItemSlot::OffHand => {
            // Shield.
            c.disc_outlined(16, 15, 11, primary);
            c.disc(16, 15, 4, shade(primary, 0.7));
            c.rect(15, 6, 17, 24, shade(primary, 1.2));
        }
        ItemSlot::Body => {
            // Chestplate.
            c.panel(8, 8, 24, 26, primary);
            c.rect(15, 8, 17, 26, shade(primary, 0.7));
            c.panel(6, 9, 9, 18, shade(primary, 0.8)); // pauldron
            c.panel(23, 9, 26, 18, shade(primary, 0.8));
        }
        ItemSlot::Head => {
            // Helm.
            c.disc(16, 16, 9, primary);
            c.rect(7, 16, 25, 22, primary); // brim
            c.rect(15, 12, 17, 22, dark_metal); // nasal
            c.rect(7, 22, 25, 24, OUTLINE);
        }
        ItemSlot::Feet => {
            // Boots.
            c.panel(8, 10, 14, 22, primary);
            c.panel(8, 20, 18, 24, shade(primary, 0.7));
            c.panel(18, 10, 24, 22, primary);
            c.panel(18, 20, 28, 24, shade(primary, 0.7));
        }
        ItemSlot::Consumable => {
            // Potion flask.
            c.rect(14, 5, 18, 10, dark_metal); // neck
            c.disc_outlined(16, 19, 8, [220, 220, 230, 230]); // glass
            c.disc(16, 21, 6, [190, 60, 70, 255]); // liquid
        }
        ItemSlot::Treasure => {
            // Gem.
            c.line(16, 6, 26, 16, 1, OUTLINE);
            for y in 6..26 {
                let half = (8 - (y - 16_i32).abs() / 2).max(0);
                c.rect(16 - half, y, 16 + half, y + 1, primary);
            }
        }
    }

    c.into_image()
}

fn color_to_rgba(color: Color) -> Rgba {
    let s = color.to_srgba();
    [
        (s.red.clamp(0.0, 1.0) * 255.0).round() as u8,
        (s.green.clamp(0.0, 1.0) * 255.0).round() as u8,
        (s.blue.clamp(0.0, 1.0) * 255.0).round() as u8,
        (s.alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

/// Generate a rarity-frame placeholder using core's frame color.
pub fn generate_rarity_frame_from_color(frame: FrameColor, tier: u8) -> Image {
    build_rarity_frame_image(&crate::rarity::rarity_style_from_frame(frame, tier))
}

/// Generate a rarity-frame placeholder from a tier (fallback table).
#[allow(dead_code)] // used by unit tests when core RarityData is unavailable
pub fn generate_rarity_frame(tier: u8) -> Image {
    build_rarity_frame_image(&crate::rarity::rarity_style(tier))
}

/// Generic fallback for any missing sprite key — distinct hash-colored silhouette.
pub fn generate_fallback_sprite(key: &str, width: u32, height: u32) -> Image {
    let mut c = Canvas::new(width, height);
    let primary = primary_for(key);
    c.panel(2, 2, width as i32 - 2, height as i32 - 2, primary);
    c.rect(
        4,
        4,
        width as i32 - 4,
        height as i32 - 4,
        shade(primary, 1.2),
    );
    c.into_image()
}

fn build_rarity_frame_image(style: &crate::rarity::RarityStyle) -> Image {
    let frame = color_to_rgba(style.frame);
    let fill = color_to_rgba(style.fill);

    // Corner accents lerp the frame color toward white by the tier's glow.
    let lift = 0.35 + 0.55 * style.glow;
    let accent: Rgba = [
        (frame[0] as f32 + (255.0 - frame[0] as f32) * lift) as u8,
        (frame[1] as f32 + (255.0 - frame[1] as f32) * lift) as u8,
        (frame[2] as f32 + (255.0 - frame[2] as f32) * lift) as u8,
        255,
    ];

    let mut c = Canvas::new(FRAME_SIZE, FRAME_SIZE);
    let n = FRAME_SIZE as i32;
    let t = 3; // border thickness
    let a = 8; // corner accent length

    // Soft background inside the border.
    c.rect(t, t, n - t, n - t, fill);

    // Border ring.
    c.rect(0, 0, n, t, frame);
    c.rect(0, n - t, n, n, frame);
    c.rect(0, 0, t, n, frame);
    c.rect(n - t, 0, n, n, frame);

    // Bright corner accents (top-left, top-right, bottom-left, bottom-right).
    c.rect(0, 0, a, t, accent);
    c.rect(0, 0, t, a, accent);
    c.rect(n - a, 0, n, t, accent);
    c.rect(n - t, 0, n, a, accent);
    c.rect(0, n - t, a, n, accent);
    c.rect(0, n - a, t, n, accent);
    c.rect(n - a, n - t, n, n, accent);
    c.rect(n - t, n - a, n, n, accent);

    c.into_image()
}
