// Hide the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod classify;
mod telemetry;
mod ui;

use eframe::egui;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // `--dump` prints one telemetry sample as JSON and exits, without a window.
    if args.iter().any(|a| a == "--dump") {
        let mut sampler = telemetry::Sampler::new();
        // CPU and rate figures need two readings a moment apart.
        sleep(Duration::from_millis(500));
        let snap = sampler.sample();
        println!(
            "{}",
            serde_json::to_string_pretty(&snap).expect("snapshot serialises")
        );
        return Ok(());
    }

    // `--screenshot <file.png>` opens the window, saves one picture of it and exits.
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);

    // `--view tiles|heat|tree|mold` picks the view the window opens on.
    let view = args
        .iter()
        .position(|a| a == "--view")
        .and_then(|i| args.get(i + 1))
        .and_then(|name| ui::View::parse(name))
        .unwrap_or(ui::View::Tiles);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Maestro")
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([900.0, 600.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }

    eframe::run_native(
        "Maestro",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, screenshot, view)))),
    )
}
