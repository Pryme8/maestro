//! Phase 2 in the window: the buttons that act on the machine, the log of
//! what was done, and the way back from each action.

use super::*;
use crate::control::{self, Journal, Paused};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

/// How to reverse an action.
pub(super) enum Undo {
    Resume(Vec<Paused>),
    Unblock(String),
    EnableAdapter(String),
}

/// Something that just happened, waiting to be written into the history.
pub(super) enum Happened {
    /// A line that went into the action log.
    Note { text: String, ok: bool },
    Paused(usize),
    Ended(usize),
    Blocked,
    AdapterOff,
    Cleared,
    QuestDone,
}

/// One line in the action log.
struct Entry {
    at: Instant,
    text: String,
    ok: bool,
    undo: Option<Undo>,
}

/// What a finished permission-gated job changed.
enum Effect {
    Blocked(String),
    Unblocked(String),
    AdapterOff(String),
    AdapterOn(String),
}

struct Outcome {
    done: String,
    failed: String,
    result: Result<(), String>,
    effect: Effect,
}

/// One program offered for pausing when clearing the machine.
struct Pick {
    name: String,
    service: bool,
    on: bool,
}

/// The "clear the machine" preview, open until applied or cancelled.
struct Focus {
    task: String,
    kept: Vec<String>,
    picks: Option<Vec<Pick>>,
}

pub(super) struct State {
    pub(super) journal: Journal,
    log: Vec<Entry>,
    /// Program waiting for a second click before it is ended.
    confirm_end: Option<String>,
    focus: Option<Focus>,
    /// Adapter whose window is open, and whether "switch off" was clicked once.
    adapter: Option<String>,
    adapter_confirm: bool,
    tx: Sender<Outcome>,
    rx: Receiver<Outcome>,
    /// Jobs waiting on the system's permission prompt.
    waiting: usize,
    /// Emptied into the history every frame.
    pub(super) happened: Vec<Happened>,
}

impl State {
    pub(super) fn load() -> Self {
        let journal = Journal::load();
        let (tx, rx) = channel();
        let mut state = Self {
            journal,
            log: Vec::new(),
            confirm_end: None,
            focus: None,
            adapter: None,
            adapter_confirm: false,
            tx,
            rx,
            waiting: 0,
            happened: Vec::new(),
        };
        if !state.journal.paused.is_empty() {
            let left = state.journal.paused.clone();
            state.note(
                format!("{} processes were left paused from last time", left.len()),
                true,
                Some(Undo::Resume(left)),
            );
        }
        state
    }

    pub(super) fn paused_pids(&self) -> HashSet<u32> {
        self.journal.paused.iter().map(|p| p.pid).collect()
    }

    /// Forget paused processes that have since exited.
    pub(super) fn sync(&mut self, snap: &Snapshot) {
        let before = self.journal.paused.len();
        self.journal.drop_gone(snap);
        if self.journal.paused.len() != before {
            self.journal.save();
        }
    }

    pub(super) fn note(&mut self, text: String, ok: bool, undo: Option<Undo>) {
        self.happened.push(Happened::Note {
            text: text.clone(),
            ok,
        });
        self.show(text, ok, undo);
    }

    /// A line for the action log that the history already has.
    pub(super) fn announce(&mut self, text: String) {
        self.show(text, true, None);
    }

    fn show(&mut self, text: String, ok: bool, undo: Option<Undo>) {
        self.log.push(Entry {
            at: Instant::now(),
            text,
            ok,
            undo,
        });
        // Keep the log short; old entries have nothing left to undo.
        if self.log.len() > 50 {
            self.log.remove(0);
        }
    }

    fn pause(&mut self, label: &str, list: Vec<Paused>) {
        let mut done = Vec::new();
        let mut problem = None;
        for p in list {
            // Pausing twice would need resuming twice, so skip what is already paused.
            if self.journal.paused.contains(&p) {
                continue;
            }
            match control::suspend(p.pid) {
                Ok(()) => done.push(p),
                Err(e) => problem = Some(e),
            }
        }
        self.journal.paused.extend(done.iter().cloned());
        self.journal.save();
        if !done.is_empty() {
            self.happened.push(Happened::Paused(done.len()));
            self.note(
                format!("Paused {label} ({} processes)", done.len()),
                true,
                Some(Undo::Resume(done)),
            );
        }
        if let Some(e) = problem {
            self.note(format!("Could not pause all of {label}. {e}"), false, None);
        }
    }

    fn resume(&mut self, label: &str, list: Vec<Paused>) {
        let mut count = 0;
        for p in &list {
            if self.journal.paused.contains(p) {
                // A process that exited while paused needs nothing.
                if control::resume(p.pid).is_ok() {
                    count += 1;
                }
            }
        }
        self.journal.paused.retain(|p| !list.contains(p));
        self.journal.save();
        // Their undo entries have nothing left to do.
        for entry in &mut self.log {
            if let Some(Undo::Resume(items)) = &mut entry.undo {
                items.retain(|p| !list.contains(p));
                if items.is_empty() {
                    entry.undo = None;
                }
            }
        }
        if count > 0 {
            self.note(format!("Resumed {label} ({count} processes)"), true, None);
        }
    }

    fn end(&mut self, label: &str, list: Vec<Paused>) {
        let mut count = 0;
        let mut problem = None;
        for p in &list {
            match control::end(p.pid) {
                Ok(()) => count += 1,
                Err(e) => problem = Some(e),
            }
        }
        self.journal.paused.retain(|p| !list.contains(p));
        self.journal.save();
        if count > 0 {
            self.happened.push(Happened::Ended(count));
            self.note(format!("Ended {label} ({count} processes)"), true, None);
        }
        if let Some(e) = problem {
            self.note(format!("Could not end all of {label}. {e}"), false, None);
        }
    }

    /// Start a job that the system will ask permission for. The answer arrives
    /// through `poll`, so the window never freezes while the prompt is up.
    fn elevated(
        &mut self,
        script: Result<String, String>,
        done: String,
        failed: String,
        effect: Effect,
        ctx: &egui::Context,
    ) {
        let script = match script {
            Ok(script) => script,
            Err(e) => {
                self.note(format!("{failed} {e}"), false, None);
                return;
            }
        };
        self.waiting += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = control::run_elevated(&script);
            let _ = tx.send(Outcome {
                done,
                failed,
                result,
                effect,
            });
            ctx.request_repaint();
        });
    }

    pub(super) fn poll(&mut self) {
        while let Ok(outcome) = self.rx.try_recv() {
            self.waiting = self.waiting.saturating_sub(1);
            match outcome.result {
                Ok(()) => {
                    let undo = match outcome.effect {
                        Effect::Blocked(program) => {
                            self.journal.blocked.push(program.clone());
                            self.happened.push(Happened::Blocked);
                            Some(Undo::Unblock(program))
                        }
                        Effect::Unblocked(program) => {
                            self.journal.blocked.retain(|p| p != &program);
                            None
                        }
                        Effect::AdapterOff(adapter) => {
                            self.journal.adapters_off.push(adapter.clone());
                            self.happened.push(Happened::AdapterOff);
                            Some(Undo::EnableAdapter(adapter))
                        }
                        Effect::AdapterOn(adapter) => {
                            self.journal.adapters_off.retain(|a| a != &adapter);
                            None
                        }
                    };
                    self.journal.save();
                    self.note(outcome.done, true, undo);
                }
                Err(e) => self.note(format!("{} {e}", outcome.failed), false, None),
            }
        }
    }

    fn unblock(&mut self, program: String, ctx: &egui::Context) {
        self.elevated(
            control::firewall_unblock_script(&program),
            format!("Unblocked {program} on the network"),
            format!("Could not unblock {program}."),
            Effect::Unblocked(program),
            ctx,
        );
    }

    fn enable_adapter(&mut self, adapter: String, ctx: &egui::Context) {
        self.elevated(
            control::adapter_script(&adapter, true),
            format!("Switched adapter {adapter} back on"),
            format!("Could not switch adapter {adapter} on."),
            Effect::AdapterOn(adapter),
            ctx,
        );
    }

    fn undo(&mut self, undo: Undo, ctx: &egui::Context) {
        match undo {
            Undo::Resume(list) => self.resume("what was paused", list),
            Undo::Unblock(program) => self.unblock(program, ctx),
            Undo::EnableAdapter(adapter) => self.enable_adapter(adapter, ctx),
        }
    }
}

impl Drop for State {
    /// Closing Maestro must never leave programs frozen with nothing on
    /// screen to resume them, so everything paused is resumed on the way out.
    fn drop(&mut self) {
        for p in std::mem::take(&mut self.journal.paused) {
            let _ = control::resume(p.pid);
        }
        self.journal.save();
    }
}

impl App {
    pub(super) fn open_focus(&mut self, task: String) {
        self.ctl.focus = Some(Focus {
            task,
            kept: Vec::new(),
            picks: None,
        });
    }

    pub(super) fn open_adapter(&mut self, name: String) {
        self.ctl.adapter = Some(name);
        self.ctl.adapter_confirm = false;
    }

    /// The action buttons for the selected program, in the detail panel.
    pub(super) fn actions_ui(&mut self, ui: &mut Ui, group: &Group<'_>) {
        ui.add_space(8.0);
        ui.label(RichText::new("ACTIONS").color(TEAL).size(11.0));

        let locked = group
            .procs
            .iter()
            .find_map(|p| control::check(group.class, p.pid).err());
        if let Some(reason) = locked {
            ui.label(RichText::new(reason).color(DIM).size(11.0));
            return;
        }

        let ctl = &mut self.ctl;
        let ctx = ui.ctx().clone();
        let name = group.name.to_string();
        let procs: Vec<Paused> = group.procs.iter().map(|p| Paused::of(p)).collect();

        let mut open_focus = false;
        ui.horizontal_wrapped(|ui| {
            if group.paused {
                if ui.button("Resume").clicked() {
                    ctl.resume(&name, procs.clone());
                }
            } else if ui
                .button("Pause")
                .on_hover_text("Freezes the program so it uses no processor. Nothing is lost.")
                .clicked()
            {
                ctl.pause(&name, procs.clone());
            }

            if ctl.confirm_end.as_deref() == Some(group.name) {
                let confirm = RichText::new(format!("Confirm: end {} processes", procs.len()))
                    .color(RED);
                if ui.button(confirm).clicked() {
                    ctl.confirm_end = None;
                    ctl.end(&name, procs.clone());
                }
                if ui.button("Cancel").clicked() {
                    ctl.confirm_end = None;
                }
            } else if ui
                .button("End…")
                .on_hover_text("Closes the program at once. Unsaved work in it is lost.")
                .clicked()
            {
                ctl.confirm_end = Some(name.clone());
            }

            if ctl.journal.blocked.contains(&name) {
                if ui.button("Unblock network").clicked() {
                    ctl.unblock(name.clone(), &ctx);
                }
            } else if let Some(exe) = group.procs.iter().find_map(|p| p.exe.as_deref()) {
                if ui
                    .button("Block network")
                    .on_hover_text(control::BLOCK_HINT)
                    .clicked()
                {
                    ctl.elevated(
                        control::firewall_block_script(&name, exe),
                        format!("Blocked {name} on the network"),
                        format!("Could not block {name}."),
                        Effect::Blocked(name.clone()),
                        &ctx,
                    );
                }
            }

            if ui
                .button("Clear the machine for this")
                .on_hover_text("Shows what could be paused so this program gets the machine.")
                .clicked()
            {
                open_focus = true;
            }
        });

        if open_focus {
            ctl.focus = Some(Focus {
                task: name.clone(),
                kept: Vec::new(),
                picks: None,
            });
        }

        let mut keep = ctl.journal.keep.contains(&name);
        if ui
            .checkbox(&mut keep, "Always keep running when clearing the machine")
            .changed()
        {
            if keep {
                ctl.journal.keep.push(name);
            } else {
                ctl.journal.keep.retain(|k| k != &name);
            }
            ctl.journal.save();
        }
    }

    /// The bar along the bottom: what Maestro has changed, and the way back.
    pub(super) fn action_log(&mut self, ctx: &egui::Context) {
        let ctl = &mut self.ctl;
        let standing = !ctl.journal.paused.is_empty()
            || !ctl.journal.blocked.is_empty()
            || !ctl.journal.adapters_off.is_empty();
        if ctl.log.is_empty() && !standing && ctl.waiting == 0 {
            return;
        }

        let mut undo_at = None;
        let mut resume_all = false;
        let mut unblock = None;
        let mut enable = None;
        egui::TopBottomPanel::bottom("actions")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("ACTIONS").color(TEAL).strong().size(12.0));
                    if !ctl.journal.paused.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{} processes paused",
                                ctl.journal.paused.len()
                            ))
                            .color(AMBER)
                            .size(11.0),
                        );
                        resume_all = ui.small_button("Resume all").clicked();
                    }
                    for program in &ctl.journal.blocked {
                        if ui
                            .small_button(format!("Unblock {program}"))
                            .on_hover_text("Blocked on the network by Maestro.")
                            .clicked()
                        {
                            unblock = Some(program.clone());
                        }
                    }
                    for adapter in &ctl.journal.adapters_off {
                        if ui
                            .small_button(format!("Switch {adapter} back on"))
                            .on_hover_text("Network adapter switched off by Maestro.")
                            .clicked()
                        {
                            enable = Some(adapter.clone());
                        }
                    }
                    if ctl.waiting > 0 {
                        ui.label(
                            RichText::new("Waiting for permission…")
                                .color(AMBER)
                                .size(11.0),
                        );
                    }
                });
                for (i, entry) in ctl.log.iter().enumerate().rev().take(4) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(ago(entry.at)).color(DIM).size(11.0));
                        let colour = if entry.ok { TEXT } else { RED };
                        ui.label(RichText::new(&entry.text).color(colour).size(11.0));
                        if entry.undo.is_some() && ui.small_button("Undo").clicked() {
                            undo_at = Some(i);
                        }
                    });
                }
            });

        if resume_all {
            let all = ctl.journal.paused.clone();
            ctl.resume("everything", all);
        }
        if let Some(program) = unblock {
            ctl.unblock(program, ctx);
        }
        if let Some(adapter) = enable {
            ctl.enable_adapter(adapter, ctx);
        }
        if let Some(undo) = undo_at.and_then(|i| ctl.log[i].undo.take()) {
            ctl.undo(undo, ctx);
        }
    }

    /// The floating windows: the clear-the-machine preview and adapter actions.
    pub(super) fn control_windows(
        &mut self,
        ctx: &egui::Context,
        snap: &Snapshot,
        groups: &[Group<'_>],
        classes: &HashMap<u32, Class>,
    ) {
        self.focus_window(ctx, snap, groups, classes);
        self.adapter_window(ctx);
    }

    fn focus_window(
        &mut self,
        ctx: &egui::Context,
        snap: &Snapshot,
        groups: &[Group<'_>],
        classes: &HashMap<u32, Class>,
    ) {
        let ctl = &mut self.ctl;
        let Some(focus) = &mut ctl.focus else { return };
        let picks = focus.picks.get_or_insert_with(|| {
            let plan = control::focus_plan(snap, classes, &focus.task, &ctl.journal.keep);
            focus.kept = plan.kept;
            let apps = plan.apps.into_iter().map(|name| Pick {
                name,
                service: false,
                on: true,
            });
            // Background services are listed but left running unless ticked.
            let services = plan.services.into_iter().map(|name| Pick {
                name,
                service: true,
                on: false,
            });
            apps.chain(services).collect()
        });
        let by_name: HashMap<&str, &Group<'_>> = groups.iter().map(|g| (g.name, g)).collect();

        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(format!("Clear the machine for {}", focus.task))
            .open(&mut open)
            .collapsible(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(
                    "Pausing freezes a program so it uses no processor. Nothing is lost, and \
                     Resume all brings everything back.",
                );
                ui.label(
                    RichText::new(
                        concat!(
                        "Paused programs keep their memory until they are ended. ",
                        os!(),
                        " itself is never paused."
                    ),
                    )
                    .color(DIM)
                    .size(11.0),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Left running: {}", focus.kept.join(", ")))
                        .color(DIM)
                        .size(11.0),
                );
                ui.separator();

                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    egui::Grid::new("picks").spacing(vec2(14.0, 3.0)).show(ui, |ui| {
                        for pick in picks.iter_mut() {
                            // A program that has exited since the preview opened.
                            let Some(group) = by_name.get(pick.name.as_str()) else {
                                continue;
                            };
                            ui.checkbox(&mut pick.on, clip(&pick.name, 34));
                            let kind = if pick.service { "service" } else { "app" };
                            ui.label(RichText::new(kind).color(DIM).size(11.0));
                            ui.label(RichText::new(format!("{:.1}%", group.cpu)).size(11.0));
                            ui.label(RichText::new(bytes(group.mem)).size(11.0));
                            ui.end_row();
                        }
                    });
                });

                let chosen: Vec<&&Group<'_>> = picks
                    .iter()
                    .filter(|p| p.on)
                    .filter_map(|p| by_name.get(p.name.as_str()))
                    .collect();
                let cpu: f32 = chosen.iter().map(|g| g.cpu).sum();
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} programs chosen, using {:.1}% of the processor right now",
                        chosen.len(),
                        cpu
                    ))
                    .size(11.0),
                );
                ui.horizontal(|ui| {
                    apply = ui
                        .add_enabled(!chosen.is_empty(), egui::Button::new("Pause chosen"))
                        .clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });

        if apply {
            let list: Vec<Paused> = picks
                .iter()
                .filter(|p| p.on)
                .filter_map(|p| by_name.get(p.name.as_str()))
                .flat_map(|g| g.procs.iter().map(|p| Paused::of(p)))
                .collect();
            let label = format!("programs to clear the machine for {}", focus.task);
            ctl.focus = None;
            ctl.happened.push(Happened::Cleared);
            ctl.pause(&label, list);
        } else if cancel || !open {
            ctl.focus = None;
        }
    }

    fn adapter_window(&mut self, ctx: &egui::Context) {
        let ctl = &mut self.ctl;
        let Some(name) = ctl.adapter.clone() else { return };
        let off = ctl.journal.adapters_off.contains(&name);
        let mut open = true;
        let mut disable = false;
        let mut enable = false;
        egui::Window::new(format!("Adapter: {name}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if off {
                    ui.label("Maestro switched this adapter off.");
                    enable = ui.button("Switch back on").clicked();
                } else {
                    ui.label(
                        "Switching an adapter off cuts every connection that goes through it \
                         until it is switched back on.",
                    );
                    if ctl.adapter_confirm {
                        let confirm = RichText::new("Confirm: switch off").color(RED);
                        disable = ui.button(confirm).clicked();
                    } else if ui.button("Switch off…").clicked() {
                        ctl.adapter_confirm = true;
                    }
                }
                ui.label(
                    RichText::new(concat!(os!(), " will ask for permission first."))
                        .color(DIM)
                        .size(11.0),
                );
            });

        if disable {
            ctl.adapter = None;
            ctl.elevated(
                control::adapter_script(&name, false),
                format!("Switched adapter {name} off"),
                format!("Could not switch adapter {name} off."),
                Effect::AdapterOff(name),
                ctx,
            );
        } else if enable {
            ctl.adapter = None;
            ctl.enable_adapter(name, ctx);
        } else if !open {
            ctl.adapter = None;
        }
    }
}

fn ago(at: Instant) -> String {
    match at.elapsed().as_secs() {
        0..=4 => "just now".to_string(),
        secs @ 5..=89 => format!("{secs}s ago"),
        secs => format!("{}m ago", secs / 60),
    }
}
