//! The base view: the PC drawn as a base, with everything running shown as crew.
//!
//! Phase 1 is read-only. Nothing in this file changes the machine.

mod controls;
mod crew;
mod router;
mod timeline;
mod views;

pub use timeline::Page;
pub use views::View;

use crate::classify::{classify, Class};
use crate::history::{History, Reading};
use crate::telemetry::{Proc, Sampler, Snapshot};
use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Rect,
    RichText, Sense, Stroke, Ui,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BG: Color32 = Color32::from_rgb(11, 16, 22);
const PANEL: Color32 = Color32::from_rgb(18, 26, 35);
const PANEL_HI: Color32 = Color32::from_rgb(28, 40, 53);
const LINE: Color32 = Color32::from_rgb(38, 54, 70);
const TEXT: Color32 = Color32::from_rgb(214, 226, 236);
const DIM: Color32 = Color32::from_rgb(120, 140, 158);
const TEAL: Color32 = Color32::from_rgb(70, 214, 190);
const AMBER: Color32 = Color32::from_rgb(240, 200, 110);
const RED: Color32 = Color32::from_rgb(236, 96, 88);
const BLUE: Color32 = Color32::from_rgb(110, 170, 255);
const VIOLET: Color32 = Color32::from_rgb(184, 146, 244);
const SLATE: Color32 = Color32::from_rgb(132, 152, 172);

const ROOM_HEIGHT: f32 = 118.0;

#[derive(PartialEq, Clone, Copy)]
enum Sort {
    Cpu,
    Memory,
    Sockets,
}

struct Shot {
    path: PathBuf,
    requested: bool,
}

/// Counts the window knows and the sampling thread writes into the history.
#[derive(Default)]
struct Live {
    agents: AtomicU32,
    paused: AtomicU32,
}

impl Live {
    fn set(&self, agents: usize, paused: usize) {
        self.agents.store(agents as u32, Ordering::Relaxed);
        self.paused.store(paused as u32, Ordering::Relaxed);
    }
}

pub struct App {
    rx: Receiver<Snapshot>,
    snap: Option<Snapshot>,
    samples: u32,
    selected: Option<String>,
    /// Open the clear-the-machine preview for the selected program at start.
    preview_focus: bool,
    sort: Sort,
    view: View,
    /// Show only programs of this class. `None` shows all of them.
    class_filter: Option<Class>,
    mold: HashMap<String, views::Blob>,
    filter: String,
    /// Phase 2: what Maestro has changed, and the way back.
    ctl: controls::State,
    /// Phase 3: the AI agents, what they report and what they may spend.
    crew: crew::Crew,
    /// Phase 4: what happened over time.
    history: Arc<Mutex<History>>,
    live: Arc<Live>,
    page: Page,
    tl: timeline::Timeline,
    /// Phase 5: the ledger of accounts and the advisor.
    router: router::RouterUi,
    shot: Option<Shot>,
}

/// Processes that share a name, shown as one crew tile.
struct Group<'a> {
    name: &'a str,
    procs: Vec<&'a Proc>,
    cpu: f32,
    mem: u64,
    sockets: usize,
    /// True when Maestro has paused any of its processes.
    paused: bool,
    /// The most notable class among the program's processes.
    class: Class,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        screenshot: Option<PathBuf>,
        view: View,
        page: Page,
        select: Option<String>,
        preview_focus: bool,
    ) -> Self {
        apply_style(&cc.egui_ctx);

        // Sample on a background thread so reading the machine never stalls the window.
        // Room for one: while the window is minimised and not drawing, new
        // samples are dropped instead of piling up.
        let (tx, rx) = sync_channel(1);
        let ctx = cc.egui_ctx.clone();
        let history = Arc::new(Mutex::new(History::load()));
        let live = Arc::new(Live::default());
        let (history_thread, live_thread) = (history.clone(), live.clone());
        std::thread::spawn(move || {
            let mut sampler = Sampler::new();
            loop {
                std::thread::sleep(Duration::from_millis(1000));
                let snap = sampler.sample();
                // The history is kept here, not in the window, so it carries
                // on while the window is minimised and drawing nothing.
                if let Ok(mut history) = history_thread.lock() {
                    let reading = Reading {
                        at_ms: snap.taken_ms,
                        cpu: snap.cpu.total_percent,
                        mem: ratio(snap.memory.used_bytes, snap.memory.total_bytes) * 100.0,
                        down: snap.networks.iter().map(|n| n.down_bytes_per_sec).sum(),
                        up: snap.networks.iter().map(|n| n.up_bytes_per_sec).sum(),
                        agents: live_thread.agents.load(Ordering::Relaxed),
                        paused: live_thread.paused.load(Ordering::Relaxed),
                    };
                    if history.record(reading) {
                        history.save();
                    }
                }
                if let Err(TrySendError::Disconnected(_)) = tx.try_send(snap) {
                    break;
                }
                ctx.request_repaint();
            }
        });

        Self {
            rx,
            snap: None,
            samples: 0,
            preview_focus: preview_focus && select.is_some(),
            selected: select,
            sort: Sort::Cpu,
            view,
            class_filter: None,
            mold: HashMap::new(),
            filter: String::new(),
            ctl: controls::State::load(),
            crew: crew::Crew::start(&cc.egui_ctx),
            history,
            live,
            page,
            tl: timeline::Timeline::new(),
            router: router::RouterUi::load(),
            shot: screenshot.map(|path| Shot {
                path,
                requested: false,
            }),
        }
    }

    /// Screenshot mode: wait for a few samples, save one picture, then close.
    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else { return };
        if !shot.requested && self.samples >= 3 {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let [w, h] = image.size;
            if let Err(err) = image::save_buffer(
                &shot.path,
                image.as_raw(),
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("could not save screenshot: {err}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(snap) = self.rx.try_recv() {
            self.snap = Some(snap);
            self.samples += 1;
        }
        self.handle_screenshot(ctx);

        let Some(snap) = self.snap.take() else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Reading the machine…").color(DIM).size(18.0));
                });
            });
            return;
        };

        let sockets_by_pid = count_sockets(&snap);
        let classes = classify(&snap);
        self.ctl.poll();
        self.ctl.sync(&snap);
        let paused = self.ctl.paused_pids();
        let mut groups = group_processes(&snap, &sockets_by_pid, &classes, &paused);
        match self.sort {
            Sort::Cpu => groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
            Sort::Memory => groups.sort_by(|a, b| b.mem.cmp(&a.mem)),
            Sort::Sockets => groups.sort_by(|a, b| b.sockets.cmp(&a.sockets)),
        }

        if std::mem::take(&mut self.preview_focus) {
            if let Some(task) = self.selected.clone() {
                self.open_focus(task);
            }
        }
        let found = self.crew.recogniser.find(&snap);
        self.keep_history(&found);
        hud(ctx, &snap, groups.len(), &mut self.page);
        self.action_log(ctx);

        if let Some(name) = self.selected.clone() {
            match groups.iter().find(|g| g.name == name) {
                Some(group) => {
                    let mut open = true;
                    egui::SidePanel::right("detail")
                        .resizable(true)
                        .default_width(420.0)
                        .show(ctx, |ui| {
                            detail(ui, &snap, group, &mut open, |ui| self.actions_ui(ui, group))
                        });
                    if !open {
                        self.selected = None;
                    }
                }
                // Every process with that name has exited.
                None => self.selected = None,
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.page {
                    Page::Base => {
                        if let Some(adapter) = rooms(ui, &snap) {
                            self.open_adapter(adapter);
                        }
                        ui.add_space(4.0);
                        self.crew_ui(ui, &found);
                        ui.add_space(14.0);
                        self.crew(ui, &snap, &groups);
                    }
                    Page::Timeline => self.timeline_ui(ui),
                    Page::Router => self.router_ui(ui),
                });
        });

        self.control_windows(ctx, &snap, &groups, &classes);

        self.snap = Some(snap);
    }
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(7, 11, 15);
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = Color32::from_rgb(30, 92, 84);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    ctx.set_visuals(visuals);
}

fn hud(ctx: &egui::Context, snap: &Snapshot, programs: usize, page: &mut Page) {
    egui::TopBottomPanel::top("hud")
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .inner_margin(Margin::symmetric(14, 10)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MAESTRO").color(TEAL).strong().size(18.0));
                ui.add_space(8.0);
                ui.label(RichText::new("LIVE").color(AMBER).size(11.0));
                ui.add_space(12.0);
                ui.selectable_value(page, Page::Base, "Base");
                ui.selectable_value(page, Page::Timeline, "Timeline")
                    .on_hover_text("The last day, what happened, what agents spent, and achievements.");
                ui.selectable_value(page, Page::Router, "Router")
                    .on_hover_text("Your AI accounts, and the cheapest way to do a task with them.");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let os = format!(
                        "{} {}",
                        snap.system.os_name.as_deref().unwrap_or("Unknown OS"),
                        snap.system.os_version.as_deref().unwrap_or("")
                    );
                    ui.label(RichText::new(os).color(DIM));
                    ui.label(RichText::new("·").color(LINE));
                    ui.label(
                        RichText::new(format!("up {}", duration(snap.system.uptime_secs)))
                            .color(DIM),
                    );
                    ui.label(RichText::new("·").color(LINE));
                    ui.label(
                        RichText::new(format!(
                            "{} processes in {} programs",
                            snap.processes.len(),
                            programs
                        ))
                        .color(DIM),
                    );
                });
            });
        });
}

/// The base's rooms: one per part of the machine.
/// Returns the network adapter the user clicked, if any.
fn rooms(ui: &mut Ui, snap: &Snapshot) -> Option<String> {
    let picked = std::cell::RefCell::new(None);
    type Body<'a> = &'a dyn Fn(&mut Ui);
    let all: [(&str, &str, Body<'_>); 5] = [

        ("REACTOR", "processor", &|ui: &mut Ui| {
            big_number(ui, &format!("{:.0}%", snap.cpu.total_percent));
            ui.label(RichText::new(&snap.cpu.brand).color(DIM).size(11.0));
            ui.add_space(4.0);
            core_cells(ui, &snap.cpu.core_percent);
        }),

        ("STORES", "memory", &|ui: &mut Ui| {
            let m = &snap.memory;
            let frac = ratio(m.used_bytes, m.total_bytes);
            big_number(ui, &format!("{:.0}%", frac * 100.0));
            ui.label(
                RichText::new(format!(
                    "{} of {} in use",
                    bytes(m.used_bytes),
                    bytes(m.total_bytes)
                ))
                .color(DIM)
                .size(11.0),
            );
            bar(ui, frac, heat(frac * 100.0));
            if m.swap_total_bytes > 0 {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "page file {} of {}",
                        bytes(m.swap_used_bytes),
                        bytes(m.swap_total_bytes)
                    ))
                    .color(DIM)
                    .size(11.0),
                );
                bar(ui, ratio(m.swap_used_bytes, m.swap_total_bytes), DIM);
            }
        }),

        ("DOCKS", "disks", &|ui: &mut Ui| {
            if snap.disks.is_empty() {
                ui.label(RichText::new("No disks reported").color(DIM));
            }
            for d in snap.disks.iter().take(5) {
                let used = d.total_bytes.saturating_sub(d.free_bytes);
                let frac = ratio(used, d.total_bytes);
                ui.label(
                    RichText::new(format!(
                        "{}  {} free of {}",
                        d.mount,
                        bytes(d.free_bytes),
                        bytes(d.total_bytes)
                    ))
                    .size(11.0),
                );
                bar(ui, frac, heat(frac * 100.0));
            }
            if snap.disks.len() > 5 {
                ui.label(
                    RichText::new(format!("and {} more", snap.disks.len() - 5))
                        .color(DIM)
                        .size(11.0),
                );
            }
        }),

        ("COMMS", "network", &|ui: &mut Ui| {
            let down: f64 = snap.networks.iter().map(|n| n.down_bytes_per_sec).sum();
            let up: f64 = snap.networks.iter().map(|n| n.up_bytes_per_sec).sum();
            ui.horizontal(|ui| {
                ui.label(RichText::new("down").color(DIM).size(11.0));
                ui.label(RichText::new(rate(down)).color(TEAL).size(18.0));
                ui.add_space(8.0);
                ui.label(RichText::new("up").color(DIM).size(11.0));
                ui.label(RichText::new(rate(up)).color(AMBER).size(18.0));
            });
            let remote = snap
                .connections
                .iter()
                .filter(|c| c.state.as_deref() == Some("ESTABLISHED"))
                .count();
            ui.label(
                RichText::new(format!(
                    "{} open sockets, {} established connections",
                    snap.connections.len(),
                    remote
                ))
                .color(DIM)
                .size(11.0),
            );
            ui.add_space(4.0);
            let mut active: Vec<_> = snap
                .networks
                .iter()
                .filter(|n| n.total_down_bytes + n.total_up_bytes > 0)
                .collect();
            active.sort_by(|a, b| {
                (b.down_bytes_per_sec + b.up_bytes_per_sec)
                    .total_cmp(&(a.down_bytes_per_sec + a.up_bytes_per_sec))
            });
            for n in active.iter().take(3) {
                let text = RichText::new(format!(
                    "{}  {} down, {} up",
                    clip(&n.name, 22),
                    rate(n.down_bytes_per_sec),
                    rate(n.up_bytes_per_sec)
                ))
                .size(11.0);
                if ui.link(text).on_hover_text("Adapter actions").clicked() {
                    *picked.borrow_mut() = Some(n.name.clone());
                }
            }
        }),

        ("FORGE", "graphics", &|ui: &mut Ui| {
            ui.label(RichText::new("Not read yet").color(DIM).size(18.0));
            ui.label(
                RichText::new("Graphics load and memory are not part of this build.")
                    .color(DIM)
                    .size(11.0),
            );
        }),
    ];

    // Lay the rooms out in rows that fit the space, wrapping by hand: nested
    // frames do not wrap on their own.
    const GAP: f32 = 10.0;
    let space = ui.available_width();
    let per_row = ((space / 190.0).floor() as usize).clamp(1, all.len());
    // 26 is the frame's own margin and border on both sides.
    let width = (space - GAP * (per_row as f32 - 1.0)) / per_row as f32 - 26.0;
    for row in all.chunks(per_row) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for (title, subtitle, body) in row {
                room(ui, width, title, subtitle, body);
            }
        });
        ui.add_space(GAP);
    }
    picked.into_inner()
}

fn room(ui: &mut Ui, width: f32, title: &str, subtitle: &str, body: &dyn Fn(&mut Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.set_min_height(ROOM_HEIGHT);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(title).color(TEAL).strong().size(12.0));
                    ui.label(RichText::new(subtitle).color(DIM).size(11.0));
                });
                ui.add_space(4.0);
                body(ui);
            });
        });
}

fn section_title(ui: &mut Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).color(TEAL).strong().size(13.0));
    ui.label(RichText::new(subtitle).color(DIM).size(11.0));
}

fn big_number(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(28.0).strong());
}

/// One small cell per logical core, coloured by its load.
fn core_cells(ui: &mut Ui, cores: &[f32]) {
    const CELL: f32 = 16.0;
    const GAP: f32 = 4.0;
    let width = ui.available_width();
    let per_row = ((width + GAP) / (CELL + GAP)).floor().max(1.0) as usize;
    let rows = cores.len().div_ceil(per_row).max(1);
    let size = vec2(width, rows as f32 * (CELL + GAP) - GAP);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    for (i, load) in cores.iter().enumerate() {
        let col = (i % per_row) as f32;
        let row = (i / per_row) as f32;
        let min = rect.min + vec2(col * (CELL + GAP), row * (CELL + GAP));
        let cell = Rect::from_min_size(min, vec2(CELL, CELL));
        painter.rect_filled(cell, CornerRadius::same(3), Color32::from_rgb(24, 34, 45));
        let level = (load / 100.0).clamp(0.0, 1.0);
        let fill = Rect::from_min_max(
            pos2(cell.min.x, cell.max.y - CELL * level),
            cell.max,
        );
        painter.rect_filled(fill, CornerRadius::same(3), heat(*load));
    }
}

fn bar(ui: &mut Ui, frac: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(24, 34, 45));
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    painter.rect_filled(fill, CornerRadius::same(3), color);
}

/// One program's tile in the crew grid.
fn tile(ui: &mut Ui, group: &Group<'_>, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(178.0, 52.0), Sense::click());
    let fill = if selected || response.hovered() {
        PANEL_HI
    } else {
        PANEL
    };
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    // Accent bar: brighter and warmer the more processor the program is using.
    let accent = Rect::from_min_size(rect.min, vec2(if selected { 5.0 } else { 3.0 }, rect.height()));
    painter.rect_filled(accent, CornerRadius::same(2), heat(group.cpu * 4.0));

    painter.text(
        rect.min + vec2(12.0, 8.0),
        Align2::LEFT_TOP,
        clip(group.name, 20),
        FontId::proportional(13.0),
        TEXT,
    );
    if group.procs.len() > 1 {
        painter.text(
            pos2(rect.max.x - 8.0, rect.min.y + 9.0),
            Align2::RIGHT_TOP,
            format!("x{}", group.procs.len()),
            FontId::proportional(11.0),
            DIM,
        );
    }
    let mut line = format!("{:.1}%  {}", group.cpu, bytes(group.mem));
    if group.paused {
        line.insert_str(0, "paused  ");
    }
    if group.sockets > 0 {
        line.push_str(&format!("  net {}", group.sockets));
    }
    painter.text(
        rect.min + vec2(12.0, 30.0),
        Align2::LEFT_TOP,
        line,
        FontId::proportional(11.0),
        DIM,
    );
    painter.circle_filled(
        pos2(rect.max.x - 11.0, rect.max.y - 12.0),
        3.5,
        class_colour(group.class),
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(summary(group))
}

/// Everything known about the selected program.
fn detail(
    ui: &mut Ui,
    snap: &Snapshot,
    group: &Group<'_>,
    open: &mut bool,
    actions: impl FnOnce(&mut Ui),
) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(group.name).strong().size(17.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Close").clicked() {
                *open = false;
            }
        });
    });
    ui.label(
        RichText::new(format!(
            "{} running  ·  {:.1}% processor  ·  {} memory  ·  {} sockets",
            group.procs.len(),
            group.cpu,
            bytes(group.mem),
            group.sockets
        ))
        .color(DIM)
        .size(11.0),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(group.class.label())
            .color(class_colour(group.class))
            .size(12.0),
    );
    ui.label(RichText::new(group.class.explain()).color(DIM).size(11.0));
    ui.label(
        RichText::new("Judged from its name, file location and what started it. Signatures are not checked yet.")
            .color(DIM)
            .italics()
            .size(10.0),
    );
    actions(ui);
    ui.separator();

    let names: HashMap<u32, &str> = snap
        .processes
        .iter()
        .map(|p| (p.pid, p.name.as_str()))
        .collect();
    let mut procs = group.procs.clone();
    procs.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // The file on disk responsible for the program.
            let mut paths: Vec<&str> = procs.iter().filter_map(|p| p.exe.as_deref()).collect();
            paths.sort_unstable();
            paths.dedup();
            ui.label(RichText::new("FILE ON DISK").color(TEAL).size(11.0));
            if paths.is_empty() {
                ui.label(
                    RichText::new("The system does not let this app read the path.")
                        .color(DIM)
                        .size(11.0),
                );
            }
            for path in paths {
                ui.label(RichText::new(path).monospace().size(11.0));
            }

            ui.add_space(10.0);
            ui.label(RichText::new("PROCESSES").color(TEAL).size(11.0));
            egui::Grid::new("procs")
                .striped(true)
                .spacing(vec2(14.0, 4.0))
                .show(ui, |ui| {
                    for head in ["PID", "CPU", "Memory", "Disk write", "Started by"] {
                        ui.label(RichText::new(head).color(DIM).size(11.0));
                    }
                    ui.end_row();
                    for p in &procs {
                        ui.label(RichText::new(p.pid.to_string()).monospace().size(11.0));
                        ui.label(RichText::new(format!("{:.1}%", p.cpu_percent)).size(11.0));
                        ui.label(RichText::new(bytes(p.memory_bytes)).size(11.0));
                        ui.label(RichText::new(rate(p.disk_write_bytes_per_sec)).size(11.0));
                        let parent = p
                            .parent_pid
                            .and_then(|pid| names.get(&pid).copied())
                            .unwrap_or("-");
                        ui.label(RichText::new(clip(parent, 22)).size(11.0));
                        ui.end_row();
                    }
                });

            ui.add_space(10.0);
            ui.label(RichText::new("NETWORK").color(TEAL).size(11.0));
            let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
            let mut conns: Vec<_> = snap
                .connections
                .iter()
                .filter(|c| c.pid.is_some_and(|pid| pids.contains(&pid)))
                .collect();
            // Connections to somewhere first, then sockets that are only listening.
            conns.sort_by_key(|c| c.state.as_deref() != Some("ESTABLISHED"));
            if conns.is_empty() {
                ui.label(RichText::new("No open sockets.").color(DIM).size(11.0));
            }
            egui::Grid::new("conns")
                .striped(true)
                .spacing(vec2(14.0, 4.0))
                .show(ui, |ui| {
                    for c in conns {
                        ui.label(RichText::new(c.protocol).color(DIM).size(11.0));
                        ui.label(
                            RichText::new(format!(":{}", c.local_port))
                                .monospace()
                                .size(11.0),
                        );
                        let remote = match (&c.remote_addr, c.remote_port) {
                            (Some(addr), Some(port)) if port != 0 => format!("{addr}:{port}"),
                            _ => "-".to_string(),
                        };
                        ui.label(RichText::new(remote).monospace().size(11.0));
                        ui.label(
                            RichText::new(c.state.as_deref().unwrap_or("-").to_lowercase())
                                .color(DIM)
                                .size(11.0),
                        );
                        ui.end_row();
                    }
                });
        });
}

fn count_sockets(snap: &Snapshot) -> HashMap<u32, usize> {
    let mut counts = HashMap::new();
    for pid in snap.connections.iter().filter_map(|c| c.pid) {
        *counts.entry(pid).or_insert(0) += 1;
    }
    counts
}

fn group_processes<'a>(
    snap: &'a Snapshot,
    sockets: &HashMap<u32, usize>,
    classes: &HashMap<u32, Class>,
    paused: &std::collections::HashSet<u32>,
) -> Vec<Group<'a>> {
    let mut by_name: HashMap<&str, Group<'a>> = HashMap::new();
    for p in &snap.processes {
        let group = by_name.entry(p.name.as_str()).or_insert_with(|| Group {
            name: p.name.as_str(),
            procs: Vec::new(),
            cpu: 0.0,
            mem: 0,
            sockets: 0,
            paused: false,
            class: Class::App,
        });
        group.cpu += p.cpu_percent;
        group.mem += p.memory_bytes;
        group.sockets += sockets.get(&p.pid).copied().unwrap_or(0);
        group.class = group
            .class
            .max(classes.get(&p.pid).copied().unwrap_or(Class::App));
        group.paused |= paused.contains(&p.pid);
        group.procs.push(p);
    }
    let mut groups: Vec<_> = by_name.into_values().collect();
    // A stable base order, so equal values do not shuffle between samples.
    groups.sort_by(|a, b| a.name.cmp(b.name));
    groups
}

fn class_colour(class: Class) -> Color32 {
    match class {
        Class::App => TEAL,
        Class::Service => VIOLET,
        Class::System => SLATE,
        Class::Critical => BLUE,
        Class::Suspect => RED,
    }
}

fn summary(group: &Group<'_>) -> String {
    format!(
        "{}\n{}\n{} running, {:.1}% processor, {} memory, {} sockets",
        group.name,
        group.class.label(),
        group.procs.len(),
        group.cpu,
        bytes(group.mem),
        group.sockets
    )
}

/// Teal when idle, amber when busy, red when saturated.
fn heat(percent: f32) -> Color32 {
    let t = (percent / 100.0).clamp(0.0, 1.0);
    if t < 0.6 {
        lerp(TEAL, AMBER, t / 0.6)
    } else {
        lerp(AMBER, RED, (t - 0.6) / 0.4)
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn ratio(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        part as f32 / whole as f32
    }
}

fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn rate(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes(bytes_per_sec.max(0.0) as u64))
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, (secs % 86_400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Shorten long names so they fit their tile, counting characters, not bytes.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}
