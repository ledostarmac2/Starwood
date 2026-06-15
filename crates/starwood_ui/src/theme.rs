//! Fantasy theming for the whole egui front-end.
//!
//! The goal is for the UI to read as a polished game, *not* a debug tool: a
//! dark, cohesive high-fantasy palette, framed parchment-style panels, generous
//! spacing, and (optionally) a fantasy display font dropped into
//! `assets/fonts/`. Everything is applied once, the first frame the egui
//! primary context exists.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

// ===== Palette =====================================================
// One limited, cohesive dark-/high-fantasy palette shared by every screen.

/// Deep near-black background with a faint blue cast (the night between stars).
pub const BG_DEEP: egui::Color32 = egui::Color32::from_rgb(18, 16, 26);
/// Panel fill — slightly lifted from the background, like aged parchment in gloom.
pub const PANEL: egui::Color32 = egui::Color32::from_rgb(34, 30, 44);
/// A raised/hovered panel or widget.
pub const PANEL_LIGHT: egui::Color32 = egui::Color32::from_rgb(48, 42, 60);
/// Frame/inactive widget borders — tarnished gold.
pub const EDGE: egui::Color32 = egui::Color32::from_rgb(92, 76, 48);
/// The signature accent — warm starwood gold.
pub const GOLD: egui::Color32 = egui::Color32::from_rgb(214, 178, 102);
/// Bright gold for active/selected accents and nat-20 text.
pub const GOLD_BRIGHT: egui::Color32 = egui::Color32::from_rgb(245, 214, 140);
/// Primary readable text — warm parchment white.
pub const INK: egui::Color32 = egui::Color32::from_rgb(226, 219, 204);
/// Muted secondary text.
pub const INK_DIM: egui::Color32 = egui::Color32::from_rgb(160, 150, 138);
/// Danger / damage / nat-1.
pub const BLOOD: egui::Color32 = egui::Color32::from_rgb(176, 58, 58);
/// Health / positive.
pub const VERDANT: egui::Color32 = egui::Color32::from_rgb(108, 158, 96);
/// Arcane / mana accent.
pub const ARCANE: egui::Color32 = egui::Color32::from_rgb(120, 132, 196);

/// Candidate filenames searched (in order) under `assets/fonts/` for a fantasy
/// display font. Drop any TTF/OTF here under one of these names and it is used
/// automatically; otherwise the styled egui default font is used.
const FONT_CANDIDATES: &[&str] = &[
    "assets/fonts/starwood.ttf",
    "assets/fonts/fantasy.ttf",
    "assets/fonts/display.ttf",
    "assets/fonts/starwood.otf",
];

/// A framed "parchment" panel frame used by the side/top/overlay panels.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.5, EDGE))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(12))
        .outer_margin(egui::Margin::same(4))
}

/// A heavier framed panel for hero moments (menu, review, game over).
pub fn hero_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(2.0, GOLD))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(20))
        .outer_margin(egui::Margin::same(8))
}

/// A lighter inset card for list rows / item tiles.
pub fn card_frame(selected: bool) -> egui::Frame {
    let (fill, stroke) = if selected {
        (PANEL_LIGHT, egui::Stroke::new(1.5, GOLD_BRIGHT))
    } else {
        (PANEL, egui::Stroke::new(1.0, EDGE))
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(8))
        .outer_margin(egui::Margin::symmetric(0, 3))
}

/// A large gold title.
pub fn title(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .size(30.0)
        .color(GOLD_BRIGHT)
        .strong()
}

/// A section heading.
pub fn heading(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(20.0).color(GOLD).strong()
}

/// Dimmed helper / flavour text.
pub fn flavour(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .size(13.0)
        .italics()
        .color(INK_DIM)
}

/// Format a signed modifier the D&D way: `+3`, `0`, `-1`.
pub fn signed(value: i32) -> String {
    if value >= 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}

/// Convert a core `FrameColor` (rarity frame) into an egui colour.
pub fn frame_color(color: starwood_core::FrameColor) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// A card frame tinted by a rarity frame colour (for item tiles/tooltips).
pub fn rarity_card(color: egui::Color32, selected: bool) -> egui::Frame {
    let width = if selected { 2.5 } else { 1.5 };
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(width, color))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(7))
        .outer_margin(egui::Margin::symmetric(0, 3))
}

/// Apply the theme exactly once, after the egui primary context exists.
///
/// `installed` is a per-system `Local` flag so the (slightly expensive) font
/// load and style construction only happen on the first frame; egui retains the
/// applied style/fonts for the rest of the session.
pub fn install_theme(mut contexts: EguiContexts, mut installed: Local<bool>) -> Result {
    if *installed {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;

    try_install_font(ctx);

    let mut style = egui::Style::default();
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(INK);
    v.panel_fill = BG_DEEP;
    v.window_fill = PANEL;
    v.window_stroke = egui::Stroke::new(1.5, EDGE);
    v.extreme_bg_color = BG_DEEP;
    v.faint_bg_color = PANEL_LIGHT;
    v.hyperlink_color = GOLD_BRIGHT;
    v.selection.bg_fill = GOLD.gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, GOLD_BRIGHT);
    v.window_corner_radius = egui::CornerRadius::same(8);

    // Widget states all share the parchment-and-gold look.
    let w = &mut v.widgets;
    style_widget(&mut w.noninteractive, PANEL, EDGE, INK);
    style_widget(&mut w.inactive, PANEL_LIGHT, EDGE, INK);
    style_widget(&mut w.hovered, PANEL_LIGHT, GOLD, GOLD_BRIGHT);
    style_widget(
        &mut w.active,
        GOLD.gamma_multiply(0.30),
        GOLD_BRIGHT,
        GOLD_BRIGHT,
    );
    style_widget(&mut w.open, PANEL_LIGHT, GOLD, INK);

    // Roomy, deliberate spacing so panels don't feel like a settings dialog.
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 8.0);
    s.button_padding = egui::vec2(10.0, 6.0);
    s.window_margin = egui::Margin::same(10);
    s.indent = 18.0;
    s.interact_size.y = 26.0;

    ctx.set_style(style);
    *installed = true;
    Ok(())
}

fn style_widget(
    w: &mut egui::style::WidgetVisuals,
    bg: egui::Color32,
    edge: egui::Color32,
    fg: egui::Color32,
) {
    w.bg_fill = bg;
    w.weak_bg_fill = bg;
    w.bg_stroke = egui::Stroke::new(1.0, edge);
    w.fg_stroke = egui::Stroke::new(1.0, fg);
    w.corner_radius = egui::CornerRadius::same(5);
}

/// Best-effort load of a fantasy display font from `assets/fonts/`. Silently
/// does nothing (keeping egui's default font) if no candidate file is present.
fn try_install_font(ctx: &egui::Context) {
    let Some(bytes) = FONT_CANDIDATES
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "starwood".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // Make it the preferred proportional font, and a fallback for monospace.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "starwood".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("starwood".to_owned());
    ctx.set_fonts(fonts);
}
