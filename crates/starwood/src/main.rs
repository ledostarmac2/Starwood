fn main() {
    let options = starwood::parse_options_from_env_and_args(std::env::args().skip(1));
    if options.debug.headless_smoke {
        let mut app = starwood::build_headless_app(options);
        for _ in 0..160 {
            app.update();
            if app
                .world()
                .resource::<starwood::DebugHarnessState>()
                .completed_victory
            {
                println!("Starwood headless smoke completed.");
                return;
            }
        }
        eprintln!("Starwood headless smoke did not complete.");
        std::process::exit(1);
    }
    starwood::build_starwood_app(options).run();
}
