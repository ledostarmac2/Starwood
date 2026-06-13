# Starwood UI fonts

This folder is owned by the **UI crate** (`starwood_ui`). The UI looks for a
fantasy display font here at startup and, if it finds one, installs it as egui's
primary proportional font. If none is present, the UI falls back to egui's
built-in font — the theme still applies, so the game looks intentional either
way.

## Drop-in a font

Place a `.ttf` or `.otf` file here under any one of these names (searched in
order):

```
starwood.ttf
fantasy.ttf
display.ttf
starwood.otf
```

That's it — no code change required. The loader lives in
`crates/starwood_ui/src/theme.rs` (`try_install_font`).

## Suggested free fonts

Any open-licensed fantasy/serif display face works well. Good fits for the dark
high-fantasy palette include **Cinzel**, **IM Fell**, **MedievalSharp**, or
**Uncial Antiqua** (all OFL-licensed). Download the `.ttf`, rename it to
`starwood.ttf`, and drop it in this folder.

> Licensing: ship only fonts whose license permits redistribution. The repo
> intentionally does **not** vendor a font binary.
