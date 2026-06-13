use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use starwood_core::StarwoodCorePlugin;
use starwood_render::StarwoodRenderPlugin;
use starwood_ui::StarwoodUiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Starwood".to_string(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(StarwoodCorePlugin {
            seed: starwood_seed(),
        })
        .add_plugins(StarwoodRenderPlugin)
        .add_plugins(StarwoodUiPlugin)
        .run();
}

fn starwood_seed() -> u64 {
    std::env::var("STARWOOD_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or_else(|| StarwoodCorePlugin::default().seed)
}
