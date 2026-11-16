#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! ASM Test Bench: an educational x86 (32-bit) assembly IDE with a built-in emulator and debugger.

mod asm;
mod cpu;
mod debugger;
mod docs;
mod examples;
mod files;
#[cfg(all(test, target_arch = "x86_64"))]
mod cpu_check;
mod ui;
mod update;

/// The app was called "ASM Garage" before; eframe keeps settings per app name
/// (%APPDATA%\<name>\data\app.ron), so carry them over once.
fn migrate_settings() {
    let Some(appdata) = std::env::var_os("APPDATA").map(std::path::PathBuf::from) else { return };
    let old = appdata.join("ASM Garage").join("data").join("app.ron");
    let new_dir = appdata.join("ASM Test Bench").join("data");
    if old.exists() && !new_dir.join("app.ron").exists() && std::fs::create_dir_all(&new_dir).is_ok() {
        let _ = std::fs::copy(&old, new_dir.join("app.ron"));
    }
}

fn main() -> eframe::Result<()> {
    // `asm_test_bench.exe --version`: the build version (the publish script tags the release with it).
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", update::VERSION);
        return Ok(());
    }
    migrate_settings();
    update::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ASM Test Bench")
            .with_inner_size([1500.0, 950.0])
            .with_min_inner_size([900.0, 600.0])
            // Title bar / taskbar icon; the exe icon itself is embedded by build.rs.
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon_256.png")).unwrap_or_default()),
        ..Default::default()
    };
    let result = eframe::run_native("ASM Test Bench", options, Box::new(|cc| Ok(Box::new(ui::app::App::new(cc)))));
    // The window is gone: install a downloaded update (and start again for "Restart now").
    update::apply_on_exit();
    result
}
