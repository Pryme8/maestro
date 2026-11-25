//! The Live page: what each agent session is doing step by step, and the
//! controls for sessions started from here.

use super::*;
use crate::live::{self, Feed, Leave, Run, Step, What};

pub(super) struct LiveUi {
    feed: Arc<Mutex<Feed>>,
    /// The session whose steps are showing.
    selected: Option<String>,
    /// Runs started from here that are still attached.
    runs: Vec<Attached>,
    folder: String,
    prompt: String,
    edits: bool,
    say: String,
    problem: Option<String>,
    /// Folders of the works in the managed environment, offered as places to run.
    works: Vec<(String, String)>,
}

struct Attached {
    run: Run,
    paused: bool,
}

impl LiveUi {
    pub(super) fn start(ctx: &egui::Context) -> Self {
        let feed: Arc<Mutex<Feed>> = Arc::default();
        let wake = ctx.clone();
        live::watch(feed.clone(), move || wake.request_repaint());
        let works = crate::environment::Environment::load()
            .works
            .iter()
            .map(|w| (w.name.clone(), w.path.clone()))
            .collect();
        Self {
            feed,
            selected: None,
            runs: Vec::new(),
            folder: String::new(),
            prompt: String::new(),
            edits: false,
            say: String::new(),
            problem: None,
            works,
        }
    }
}

fn ago(now: u64, at: u64) -> String {
    match now.saturating_sub(at) / 1000 {
        0..=2 => "now".to_string(),
        s @ 3..=89 => format!("{s}s ago"),
        s @ 90..=5399 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}

/// One step as a row: when, what kind, and the words.
fn step_row(ui: &mut Ui, now: u64, step: &Step) {
    let (kind, colour, text, hover): (&str, Color32, String, Option<&str>) = match &step.what {
        What::Start { model } => ("started", DIM, format!("on {model}"), None),
        What::Asked(text) => ("asked", AMBER, text.clone(), None),
        What::Said(text) => ("said", TEXT, text.clone(), None),
        What::Thinking => ("thinking", DIM, String::new(), None),
        What::Tool { name, detail, .. } => ("tool", TEAL, format!("{name}  {}", clip(detail, 150)), Some(detail)),
        What::ToolDone { name, ok: true, size, preview } => (
            "result",
            DIM,
            format!("{name} sent back {}", bytes(*size as u64)),
            Some(preview),
        ),
        What::ToolDone { name, ok: false, preview, .. } => {
            ("failed", RED, format!("{name}: {}", clip(preview, 150)), Some(preview))
        }
        What::Finished { ok: true, cost, text } => (
            "finished",
            BLUE,
            format!("{}{}", cost.map(|c| format!("${c:.4}  ")).unwrap_or_default(), clip(text, 300)),
            Some(text),
        ),
        What::Finished { ok: false, text, .. } => ("error", RED, text.clone(), None),
        What::Problem(text) => ("problem", RED, text.clone(), None),
        What::Ended => ("ended", DIM, "The process has exited.".to_string(), None),
    };
    ui.horizontal_top(|ui| {
        ui.add_sized(
            vec2(52.0, 14.0),
            egui::Label::new(RichText::new(ago(now, step.at_ms)).color(DIM).size(10.0)),
        );
        ui.add_sized(
            vec2(58.0, 14.0),
            egui::Label::new(RichText::new(kind).color(colour).size(11.0)),
        );
        let mono = matches!(step.what, What::Tool { .. });
        let mut body = RichText::new(text).size(11.5);
        if mono {
            body = body.monospace();
        }
        if !matches!(step.what, What::Said(_) | What::Asked(_)) {
            body = body.color(colour);
        }
        let label = ui.add(egui::Label::new(body).wrap());
        if let Some(hover) = hover.filter(|h| !h.is_empty()) {
            label.on_hover_text(hover);
        }
    });
}

impl App {
    pub(super) fn live_ui(&mut self, ui: &mut Ui) {
        let lv = &mut self.lv;
        let now = live::now_ms();
        section_title(ui, "LIVE", "what your agents are doing, step by step");
        ui.label(
            RichText::new(
                "Sessions started anywhere are followed by reading the session files Claude Code \
                 keeps on this computer; they can be watched here, and paused or ended from the \
                 Base page. Sessions started here can also be told things, interrupted and \
                 stopped. Steps are held in memory only. Other tools are not followed yet.",
            )
            .color(DIM)
            .size(11.0),
        );
        ui.add_space(6.0);

        // Start a run.
        egui::CollapsingHeader::new(RichText::new("Start a session from here").size(13.0))
            .id_salt("live-start")
            .default_open(lv.runs.is_empty())
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder").color(DIM).size(11.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut lv.folder)
                            .hint_text("the folder it works in")
                            .desired_width(420.0),
                    );
                    for (name, path) in &lv.works {
                        if ui
                            .small_button(format!("work: {name}"))
                            .on_hover_text("A separate copy of a repository that can be rolled back.")
                            .clicked()
                        {
                            lv.folder = path.clone();
                        }
                    }
                });
                ui.add(
                    egui::TextEdit::multiline(&mut lv.prompt)
                        .hint_text("What should it do?")
                        .desired_rows(3)
                        .desired_width(620.0),
                );
                ui.checkbox(
                    &mut lv.edits,
                    "Let it create and change files in that folder without asking",
                )
                .on_hover_text(
                    "Off: it can read and search, and anything needing approval is refused. \
                     On: it may also edit files. Either way it cannot be approved step by step \
                     from here yet. A work from the managed environment can be rolled back; an \
                     ordinary folder cannot.",
                );
                ui.horizontal(|ui| {
                    let ready = !lv.folder.trim().is_empty() && !lv.prompt.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Start Claude Code"))
                        .on_hover_text(
                            "Starts the `claude` command on your own login, with no window. \
                             It uses your plan like any other session.",
                        )
                        .clicked()
                    {
                        let leave = if lv.edits { Leave::EditFiles } else { Leave::ReadOnly };
                        let wake = ui.ctx().clone();
                        let folder = std::path::PathBuf::from(lv.folder.trim());
                        match Run::start(lv.feed.clone(), &folder, lv.prompt.trim(), leave, move || {
                            wake.request_repaint()
                        }) {
                            Ok(run) => {
                                self.ctl.note(
                                    format!("Started Claude Code in {}", folder.display()),
                                    true,
                                    None,
                                );
                                lv.selected = Some(run.id.clone());
                                lv.runs.push(Attached { run, paused: false });
                                lv.prompt.clear();
                                lv.problem = None;
                            }
                            Err(problem) => lv.problem = Some(problem),
                        }
                    }
                    if let Some(problem) = &lv.problem {
                        ui.label(RichText::new(problem).color(RED).size(11.0));
                    }
                });
            });
        ui.add_space(8.0);

        let feed = lv.feed.clone();
        let Ok(feed) = feed.lock() else { return };
        if feed.sessions.is_empty() {
            ui.label(
                RichText::new("No agent session has been active in the last hour.").color(DIM),
            );
            return;
        }

        // The sessions, most recently active first.
        let mut order: Vec<&live::Session> = feed.sessions.iter().collect();
        order.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
        if lv.selected.as_ref().is_none_or(|id| !order.iter().any(|s| &s.id == id)) {
            lv.selected = order.first().map(|s| s.id.clone());
        }
        ui.horizontal_wrapped(|ui| {
            for session in &order {
                let active = now.saturating_sub(session.last_ms) < 15_000;
                let mut text = format!("{}  ·  {}", clip(&session.name(), 34), ago(now, session.last_ms));
                if session.ours {
                    text.insert_str(0, "▶ ");
                }
                let text = RichText::new(text).color(if active { TEAL } else { TEXT }).size(12.0);
                let picked = lv.selected.as_deref() == Some(session.id.as_str());
                if ui
                    .selectable_label(picked, text)
                    .on_hover_text(format!("{}\n{}", session.cwd, session.id))
                    .clicked()
                {
                    lv.selected = Some(session.id.clone());
                }
            }
        });
        ui.add_space(6.0);

        let Some(session) = order.iter().find(|s| Some(&s.id) == lv.selected.as_ref()) else {
            return;
        };
        ui.label(RichText::new(session.name()).strong().size(15.0));
        ui.label(RichText::new(&session.cwd).monospace().color(DIM).size(11.0));

        // Which tools it has used, most used first.
        let mut counts: Vec<(&String, &u32)> = session.tool_counts.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if !counts.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Tools used").color(DIM).size(11.0));
                for (name, count) in counts.iter().take(14) {
                    ui.label(RichText::new(format!("{} {count}", clip(name, 28))).color(TEAL).size(11.0));
                }
            });
        }
        let waiting = session.waiting_on();
        if !waiting.is_empty() {
            ui.label(
                RichText::new(format!("Waiting on: {}", waiting.join(", ")))
                    .color(AMBER)
                    .size(11.0),
            );
        }

        // Controls, for a session started here.
        let mut drop_run = false;
        if let Some(at) = lv.runs.iter().position(|a| a.run.id == session.id) {
            let attached = &mut lv.runs[at];
            let running = attached.run.running();
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if !running {
                    ui.label(RichText::new("This session has ended.").color(DIM).size(11.0));
                    drop_run = ui.small_button("Dismiss controls").clicked();
                    return;
                }
                let sent = ui.add_enabled(
                    attached.run.open(),
                    egui::TextEdit::singleline(&mut lv.say)
                        .hint_text("tell it something")
                        .desired_width(380.0),
                );
                let enter = sent.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let send = ui
                    .add_enabled(attached.run.open() && !lv.say.trim().is_empty(), egui::Button::new("Send"))
                    .on_hover_text("It reads this when it next looks for a message, usually after its current step.")
                    .clicked();
                if (send || enter) && !lv.say.trim().is_empty() {
                    match attached.run.say(lv.say.trim()) {
                        Ok(()) => lv.say.clear(),
                        Err(problem) => lv.problem = Some(problem),
                    }
                }
                if ui
                    .add_enabled(attached.run.open(), egui::Button::new("Interrupt"))
                    .on_hover_text("Asks it to drop what it is doing and wait for your next message.")
                    .clicked()
                {
                    if let Err(problem) = attached.run.interrupt() {
                        lv.problem = Some(problem);
                    }
                }
                let label = if attached.paused { "Resume" } else { "Pause" };
                if ui
                    .button(label)
                    .on_hover_text("Freezes the agent's own process. Programs it started keep running.")
                    .clicked()
                {
                    let pid = attached.run.pid;
                    let result = if attached.paused {
                        crate::control::resume(pid)
                    } else {
                        crate::control::suspend(pid)
                    };
                    match result {
                        Ok(()) => attached.paused = !attached.paused,
                        Err(problem) => lv.problem = Some(problem),
                    }
                }
                if ui
                    .add_enabled(attached.run.open(), egui::Button::new("No more messages"))
                    .on_hover_text("It finishes what it has been asked and then exits.")
                    .clicked()
                {
                    attached.run.finish();
                }
                if ui
                    .button(RichText::new("Stop now").color(RED))
                    .on_hover_text("Ends the agent's process at once.")
                    .clicked()
                {
                    attached.run.stop();
                }
            });
        } else {
            ui.label(
                RichText::new("Started outside Maestro: it can be watched here, not directed.")
                    .color(DIM)
                    .size(11.0),
            );
        }
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt(("live-steps", &session.id))
            .max_height((ui.available_height() - 8.0).max(320.0))
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let skip = session.steps.len().saturating_sub(250);
                for step in session.steps.iter().skip(skip) {
                    step_row(ui, now, step);
                }
            });

        if drop_run {
            lv.runs.retain(|a| a.run.id != session.id);
        }
        // The "ago" times move on even when nothing new arrives.
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
}
