mod game;
mod player;
mod rendering;
mod settings;
mod ui;
mod visual_test;
mod world;

fn main() {
    let visual_test = std::env::args().any(|arg| arg == "--visual-test");
    if visual_test {
        // Keep the development screenshot harness from touching the real
        // save file: seed, edits and player position stay isolated.
        std::env::set_var("RUNEWILD_SAVE_FILE", "visual_tests/session_save.json");
        std::env::set_var(
            "RUNEWILD_SETTINGS_FILE",
            "visual_tests/session_settings.json",
        );
    }
    game::run(visual_test);
}
