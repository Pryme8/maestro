// Hide the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agents;
mod classify;
mod control;
mod receiver;
mod telemetry;
mod ui;
#[cfg(all(windows, target_pointer_width = "64"))]
mod winproc;

use eframe::egui;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    attach_console();

    // Command-line controls. They follow the same rules as the buttons.
    for action in ["--pause", "--resume", "--end"] {
        if let Some(pid) = arg_after(&args, action).and_then(|v| v.parse::<u32>().ok()) {
            std::process::exit(command_line_action(action, pid));
        }
    }
    // `--agents` lists the AI agents found running. It changes nothing.
    if args.iter().any(|a| a == "--agents") {
        let mut sampler = telemetry::Sampler::new();
        sleep(Duration::from_millis(500));
        let snap = sampler.sample();
        for agent in agents::Recogniser::default().find(&snap) {
            println!(
                "{} ({}, {}): {} running, {} processes, {} helpers, {:.1}% processor, {} MB",
                agent.kind.label,
                agent.kind.vendor,
                agent.kind.what,
                agent.sessions,
                agent.procs.len(),
                agent.helpers.len(),
                agent.cpu,
                agent.mem / 1_048_576
            );
        }
        return Ok(());
    }
    // `--receive` runs only the usage receiver and prints the totals as reports arrive.
    if args.iter().any(|a| a == "--receive") {
        use std::sync::{Arc, Mutex};
        let store = Arc::new(Mutex::new(receiver::Store::load()));
        let (tx, rx) = std::sync::mpsc::channel();
        if let Err(problem) = receiver::start(store.clone(), move || {
            let _ = tx.send(());
        }) {
            eprintln!("{problem}");
            std::process::exit(1);
        }
        println!("listening on 127.0.0.1:{}", receiver::PORT);
        while rx.recv().is_ok() {
            if let Ok(store) = store.lock() {
                for (agent, t) in &store.today {
                    println!(
                        "{agent}: ${:.4} today, {} in, {} out, {} cache read, {} cache write",
                        t.cost_usd, t.tokens_in, t.tokens_out, t.cache_read, t.cache_write
                    );
                }
            }
        }
        return Ok(());
    }
    // `--focus-plan <program>` lists what clearing the machine would pause. It changes nothing.
    if let Some(task) = arg_after(&args, "--focus-plan") {
        let snap = telemetry::Sampler::new().sample();
        let classes = classify::classify(&snap);
        let keep = control::Journal::load().keep;
        let plan = control::focus_plan(&snap, &classes, task, &keep);
        println!("left running: {}", plan.kept.join(", "));
        println!("apps that would be paused: {}", plan.apps.join(", "));
        println!("services offered, not paused by default: {}", plan.services.join(", "));
        return Ok(());
    }

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

    // `--profile` times each part of a sample, to see what the monitor itself costs.
    if args.iter().any(|a| a == "--profile") {
        let mut sampler = telemetry::Sampler::new();
        for round in 1..=3 {
            sleep(Duration::from_millis(1000));
            println!("round {round}");
            for (name, ms) in sampler.profile() {
                println!("  {name:<14}{ms:>8.1} ms");
            }
        }
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

    // `--select <program>` opens with that program's details showing, and
    // `--preview-focus` also opens its clear-the-machine preview. Neither changes anything.
    let select = arg_after(&args, "--select").map(str::to_string);
    let preview_focus = args.iter().any(|a| a == "--preview-focus");

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
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, screenshot, view, select, preview_focus)))),
    )
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Pause, resume or end one process from the command line. Returns the exit code.
fn command_line_action(action: &str, pid: u32) -> i32 {
    let snap = telemetry::Sampler::new().sample();
    let classes = classify::classify(&snap);
    let Some(process) = snap.processes.iter().find(|p| p.pid == pid) else {
        eprintln!("No process has the number {pid}.");
        return 2;
    };
    let class = classes.get(&pid).copied().unwrap_or(classify::Class::App);
    if let Err(reason) = control::check(class, pid) {
        eprintln!("{} is locked. {reason}", process.name);
        return 3;
    }
    let mut journal = control::Journal::load();
    let entry = control::Paused::of(process);
    let result = match action {
        "--pause" if journal.paused.contains(&entry) => Err("It is already paused.".to_string()),
        "--pause" => control::suspend(pid).map(|()| journal.paused.push(entry)),
        "--resume" => control::resume(pid).map(|()| journal.paused.retain(|p| p != &entry)),
        _ => control::end(pid).map(|()| journal.paused.retain(|p| p != &entry)),
    };
    match result {
        Ok(()) => {
            journal.save();
            println!("{} {} ({pid})", &action[2..], process.name);
            0
        }
        Err(reason) => {
            eprintln!("Could not {} {}. {reason}", &action[2..], process.name);
            1
        }
    }
}

/// A release build has no console of its own, so borrow the one it was
/// started from. That lets the command-line flags print their results.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process: u32) -> i32;
    }
    const PARENT: u32 = u32::MAX;
    // SAFETY: a plain call that fails harmlessly when there is no parent console.
    unsafe {
        AttachConsole(PARENT);
    }
}

#[cfg(not(windows))]
fn attach_console() {}
