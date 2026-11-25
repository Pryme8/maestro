//! The Live page: what each agent session is doing step by step, and the
//! controls for sessions started from here.

use super::*;
use crate::live::{self, Feed, Leave, Run, Step, What};
use crate::services::{self, Service};
use std::sync::atomic::AtomicBool;

pub(super) struct LiveUi {
    feed: Arc<Mutex<Feed>>,
    /// The session whose steps are showing.
    selected: Option<String>,
    /// Runs started from here that are still attached.
    runs: Vec<Attached>,
    folder: String,
    prompt: String,
    leave: Leave,
    /// The model a new session runs on. Empty means the tool's own setting.
    model: String,
    /// The named sign-in a new session runs under. `None` is the tool's own.
    login: Option<String>,
    /// A sign-in being typed in, and the key variable for it, if any.
    new_login: String,
    new_key_var: String,
    say: String,
    /// Why a step is being refused, sent back to the agent.
    why: String,
    problem: Option<String>,
    /// Folders of the works in the managed environment, offered as places to run.
    works: Vec<(String, String)>,
    /// Draw the selected session as a map instead of a list.
    as_map: bool,
    /// Bring the map into view. Used when taking pictures of the pages.
    pub(super) show_map: bool,
    /// Each agent tool and whether it can be used.
    services: Arc<Mutex<Vec<Service>>>,
    checking: Arc<AtomicBool>,
}

struct Attached {
    run: Run,
    paused: bool,
}

/// Something the user asked a run to do.
enum Act {
    Say(String),
    Interrupt,
    Pause,
    Finish,
    Stop,
    Answer { request: String, tool: String, allow: bool },
    Dismiss,
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
        let mut ui = Self {
            feed,
            selected: None,
            runs: Vec::new(),
            folder: String::new(),
            prompt: String::new(),
            leave: Leave::AskFirst,
            model: String::new(),
            login: None,
            new_login: String::new(),
            new_key_var: String::new(),
            say: String::new(),
            why: String::new(),
            problem: None,
            works,
            as_map: true,
            show_map: false,
            services: Arc::default(),
            checking: Arc::default(),
        };
        ui.check_services(ctx);
        ui
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
        What::NeedsYou { tool, detail } => ("asks", AMBER, format!("May it use {tool}?  {}", clip(detail, 150)), Some(detail)),
        What::Decided { tool, allowed: true } => ("allowed", TEAL, format!("You allowed {tool}."), None),
        What::Decided { tool, allowed: false } => ("refused", RED, format!("You refused {tool}."), None),
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
                 stopped, and they ask you before each step that needs permission. Steps are held in memory only. Other tools are not followed yet.",
            )
            .color(DIM)
            .size(11.0),
        );
        ui.add_space(6.0);
        lv.services_ui(ui);
        let blocked = lv.claude_block();

        // Start a run.
        egui::CollapsingHeader::new(RichText::new("Start a session from here").size(13.0))
            .id_salt("live-start")
            .default_open(false)
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
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sign-in").color(DIM).size(11.0));
                    let names: Vec<String> = lv
                        .services
                        .lock()
                        .map(|list| {
                            list.iter()
                                .filter(|s| s.id == "claude-code")
                                .flat_map(|s| s.logins.iter().map(|l| l.login.name.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    egui::ComboBox::from_id_salt("live-login")
                        .selected_text(lv.login.clone().unwrap_or_else(|| "Claude Code's own".to_string()))
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut lv.login, None, "Claude Code's own");
                            for name in names {
                                ui.selectable_value(&mut lv.login, Some(name.clone()), name);
                            }
                        });
                    ui.label(
                        RichText::new("The session stays under this sign-in until it ends. Add more under Services.")
                            .color(DIM)
                            .size(11.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Model").color(DIM).size(11.0));
                    let shown = if lv.model.trim().is_empty() {
                        "Claude Code's own setting".to_string()
                    } else {
                        lv.model.trim().to_string()
                    };
                    egui::ComboBox::from_id_salt("live-model")
                        .selected_text(shown)
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut lv.model, String::new(), "Claude Code's own setting");
                            for (name, what) in live::MODELS {
                                ui.selectable_value(&mut lv.model, name.to_string(), format!("{name}: {what}"));
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut lv.model)
                            .hint_text("or a model's full name")
                            .desired_width(220.0),
                    );
                    ui.label(
                        RichText::new("A cheaper model for a small job uses less of your plan.")
                            .color(DIM)
                            .size(11.0),
                    );
                });
                ui.label(RichText::new("Before a step that needs permission, such as running a command or changing a file:").color(DIM).size(11.0));
                ui.radio_value(&mut lv.leave, Leave::AskFirst, "Ask me each time")
                    .on_hover_text("Each such step waits here until you allow or refuse it.");
                ui.radio_value(&mut lv.leave, Leave::EditFiles, "Let it change files in that folder, and ask me about everything else")
                    .on_hover_text(
                        "File edits go ahead without asking. A work from the managed environment \
                         can be rolled back; an ordinary folder cannot.",
                    );
                ui.radio_value(&mut lv.leave, Leave::ReadOnly, "Refuse them all: it may only read and search")
                    .on_hover_text("Nothing is asked. Anything that would need permission is refused by the tool itself.");
                ui.horizontal(|ui| {
                    let ready = blocked.is_none() && !lv.folder.trim().is_empty() && !lv.prompt.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Start Claude Code"))
                        .on_hover_text(
                            "Starts the `claude` command on your own login, with no window. \
                             It uses your plan like any other session.",
                        )
                        .clicked()
                    {
                        let leave = lv.leave;
                        let wake = ui.ctx().clone();
                        let folder = std::path::PathBuf::from(lv.folder.trim());
                        let logins = crate::logins::Logins::load();
                        let who = lv.login.as_deref().and_then(|name| logins.find(name));
                        match Run::start(lv.feed.clone(), &folder, lv.prompt.trim(), leave, &lv.model, who, move || {
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
                    if let Some(blocked) = &blocked {
                        ui.label(RichText::new(blocked).color(AMBER).size(11.0));
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
                let waiting = !session.asks.is_empty();
                if waiting {
                    text.insert_str(0, "needs you  ·  ");
                }
                let colour = if waiting { AMBER } else if active { TEAL } else { TEXT };
                let text = RichText::new(text).color(colour).size(12.0);
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
        ui.horizontal(|ui| {
            ui.label(RichText::new(session.name()).strong().size(15.0));
            ui.add_space(12.0);
            ui.selectable_value(&mut lv.as_map, true, "Map");
            ui.selectable_value(&mut lv.as_map, false, "List");
        });
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

        // What the user asked for this frame. Carried out once the list of
        // sessions is let go of, because acting on a run writes to that list.
        let mut act: Option<Act> = None;
        let attached = lv.runs.iter_mut().find(|a| a.run.id == session.id);

        // Steps it is waiting to be allowed to take.
        if attached.is_some() {
            for ask in &session.asks {
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(PANEL_HI)
                    .stroke(Stroke::new(1.0, AMBER))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "It is waiting for you: may it use {}?",
                                live::short_tool(&ask.tool)
                            ))
                            .color(AMBER)
                            .strong()
                            .size(12.5),
                        );
                        if !ask.detail.is_empty() {
                            let whole = serde_json::to_string_pretty(&ask.input).unwrap_or_default();
                            ui.add(egui::Label::new(RichText::new(&ask.detail).monospace().size(11.5)).wrap())
                                .on_hover_text(clip(&whole, 2000));
                        }
                        for (label, text) in [("It says", &ask.description), ("Why it asks", &ask.reason)] {
                            if !text.is_empty() {
                                ui.label(RichText::new(format!("{label}: {text}")).color(DIM).size(11.0));
                            }
                        }
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .button("Allow this once")
                                .on_hover_text("This one step goes ahead exactly as shown. The next one is asked again.")
                                .clicked()
                            {
                                act = Some(Act::Answer {
                                    request: ask.request_id.clone(),
                                    tool: ask.tool.clone(),
                                    allow: true,
                                });
                            }
                            if ui
                                .button(RichText::new("Refuse").color(RED))
                                .on_hover_text("The step is not taken, and the agent is told why if you say.")
                                .clicked()
                            {
                                act = Some(Act::Answer {
                                    request: ask.request_id.clone(),
                                    tool: ask.tool.clone(),
                                    allow: false,
                                });
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut lv.why)
                                    .hint_text("when refusing, say why so it can try another way")
                                    .desired_width(330.0),
                            );
                            ui.label(RichText::new(ago(now, ask.at_ms)).color(DIM).size(10.0));
                        });
                    });
            }
        }

        // Controls, for a session started here.
        if let Some(attached) = attached {
            let running = attached.run.running();
            let open = attached.run.open();
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if !running {
                    ui.label(RichText::new("This session has ended.").color(DIM).size(11.0));
                    if ui.small_button("Dismiss controls").clicked() {
                        act = Some(Act::Dismiss);
                    }
                    return;
                }
                let box_ = ui.add_enabled(
                    open,
                    egui::TextEdit::singleline(&mut lv.say)
                        .hint_text("tell it something")
                        .desired_width(380.0),
                );
                let enter = box_.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let send = ui
                    .add_enabled(open && !lv.say.trim().is_empty(), egui::Button::new("Send"))
                    .on_hover_text("It reads this when it next looks for a message, usually after its current step.")
                    .clicked();
                if (send || enter) && !lv.say.trim().is_empty() {
                    act = Some(Act::Say(lv.say.trim().to_string()));
                }
                if ui
                    .add_enabled(open, egui::Button::new("Interrupt"))
                    .on_hover_text("Asks it to drop what it is doing and wait for your next message.")
                    .clicked()
                {
                    act = Some(Act::Interrupt);
                }
                let label = if attached.paused { "Resume" } else { "Pause" };
                if ui
                    .button(label)
                    .on_hover_text("Freezes the agent's own process. Programs it started keep running.")
                    .clicked()
                {
                    act = Some(Act::Pause);
                }
                if ui
                    .add_enabled(open, egui::Button::new("No more messages"))
                    .on_hover_text("It finishes what it has been asked and then exits.")
                    .clicked()
                {
                    act = Some(Act::Finish);
                }
                if ui
                    .button(RichText::new("Stop now").color(RED))
                    .on_hover_text("Ends the agent's process at once.")
                    .clicked()
                {
                    act = Some(Act::Stop);
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

        // A run that could not sign in means the services need another look.
        let sign_in_failed = session.ours
            && session.steps.iter().rev().take(4).any(|s| {
                matches!(&s.what, What::Finished { ok: false, text, .. } if text.contains("authenticate") || text.contains("logged in"))
            });
        let busy = now.saturating_sub(session.last_ms) < 15_000;
        let list_height = if lv.as_map {
            session_map(ui, now, &session.name(), busy, &live::map(session), lv.show_map);
            ui.add_space(4.0);
            150.0
        } else {
            (ui.available_height() - 8.0).max(320.0)
        };

        egui::ScrollArea::vertical()
            .id_salt(("live-steps", &session.id))
            .max_height(list_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let skip = session.steps.len().saturating_sub(250);
                for step in session.steps.iter().skip(skip) {
                    step_row(ui, now, step);
                }
            });

        // Carry out what was asked, now that the session list is free.
        let session_id = session.id.clone();
        let check_again = sign_in_failed;
        drop(feed);
        if check_again && lv.claude_block().is_none() {
            lv.check_services(ui.ctx());
        }
        match act {
            None => {}
            Some(Act::Dismiss) => lv.runs.retain(|a| a.run.id != session_id),
            Some(act) => {
                if let Some(attached) = lv.runs.iter_mut().find(|a| a.run.id == session_id) {
                    let result = match act {
                        Act::Say(text) => attached.run.say(&text).map(|()| lv.say.clear()),
                        Act::Interrupt => attached.run.interrupt(),
                        Act::Pause => {
                            let pid = attached.run.pid;
                            let result = if attached.paused {
                                crate::control::resume(pid)
                            } else {
                                crate::control::suspend(pid)
                            };
                            result.map(|()| attached.paused = !attached.paused)
                        }
                        Act::Finish => {
                            attached.run.finish();
                            Ok(())
                        }
                        Act::Stop => {
                            attached.run.stop();
                            Ok(())
                        }
                        Act::Answer { request, tool, allow } => {
                            let result = attached.run.answer(&request, allow, &lv.why);
                            if result.is_ok() {
                                let verb = if allow { "Allowed" } else { "Refused" };
                                self.ctl.note(
                                    format!("{verb} a step by the agent: {}", live::short_tool(&tool)),
                                    true,
                                    None,
                                );
                                lv.why.clear();
                            }
                            result
                        }
                        Act::Dismiss => Ok(()),
                    };
                    lv.problem = result.err();
                }
            }
        }
        // The "ago" times move on even when nothing new arrives.
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }
}

/// How lately something happened, from 1 (now) fading to 0 over a few minutes.
fn glow(now: u64, at: u64) -> f32 {
    let age = now.saturating_sub(at) as f32 / 1000.0;
    (-age / 45.0).exp()
}

/// Tools drawn before the rest are folded into a count.
const MAP_TOOLS: usize = 12;
/// Targets drawn per tool.
const MAP_LEAVES: usize = 6;

/// The session as a map: the agent in the middle, each tool it has used
/// around it, and what each tool was pointed at on the outside. Whatever was
/// used most recently is brightest.
fn session_map(ui: &mut Ui, now: u64, title: &str, busy: bool, branches: &[live::Branch], scroll_to: bool) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let size = vec2(ui.available_width(), 470.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if scroll_to {
        ui.scroll_to_rect(rect, Some(Align::TOP));
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::same(6), PANEL);
    let centre = rect.center();
    let quiet = Color32::from_rgb(52, 70, 88);
    let time = ui.input(|i| i.time) as f32;

    // Most recently used tools, kept in the order they were first called.
    let mut shown: Vec<&live::Branch> = branches.iter().collect();
    if shown.len() > MAP_TOOLS {
        let mut by_recency = shown.clone();
        by_recency.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
        let cut = by_recency[MAP_TOOLS - 1].last_ms;
        shown.retain(|b| b.last_ms >= cut);
        shown.truncate(MAP_TOOLS);
    }
    let hidden = branches.len() - shown.len();

    let (rx1, ry1) = (rect.width() * 0.19, rect.height() * 0.25);
    let (rx2, ry2) = (rect.width() * 0.36, rect.height() * 0.42);
    let wedge = TAU / shown.len().max(1) as f32;
    for (i, branch) in shown.iter().enumerate() {
        let angle = -FRAC_PI_2 + wedge * i as f32;
        let at = centre + vec2(angle.cos() * rx1, angle.sin() * ry1);
        let heat = glow(now, branch.last_ms);
        let failed = branch.failed_ms.map_or(0.0, |ms| glow(now, ms));
        let mut colour = lerp(quiet, TEAL, heat);
        if failed > 0.25 {
            colour = lerp(colour, RED, failed);
        }
        let weight = 1.0 + (branch.count as f32).ln().min(3.0);
        painter.line_segment([centre, at], Stroke::new(weight, colour));

        // What the tool was pointed at, fanned out beyond it.
        let leaves = &branch.leaves[..branch.leaves.len().min(MAP_LEAVES)];
        let spread = (wedge * 0.82).min(1.5);
        for (j, leaf) in leaves.iter().enumerate() {
            let offset = (j as f32 + 0.5) / leaves.len() as f32 - 0.5;
            let a = angle + spread * offset;
            let leaf_at = centre + vec2(a.cos() * rx2, a.sin() * ry2);
            let leaf_heat = glow(now, leaf.last_ms);
            let leaf_colour = lerp(quiet, AMBER, leaf_heat);
            painter.line_segment([at, leaf_at], Stroke::new(1.0, lerp(LINE, leaf_colour, 0.6)));
            let radius = 3.0 + (leaf.count.min(8) as f32) * 0.5;
            painter.circle_filled(leaf_at, radius, leaf_colour);
            let right = a.cos() >= 0.0;
            let mut label = leaf.label.clone();
            if leaf.count > 1 {
                label.push_str(&format!("  x{}", leaf.count));
            }
            let text_at = leaf_at + vec2(if right { radius + 5.0 } else { -radius - 5.0 }, 0.0);
            let text_rect = painter.text(
                text_at,
                if right { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER },
                label,
                FontId::proportional(10.5),
                lerp(DIM, TEXT, leaf_heat),
            );
            let hit = text_rect.union(Rect::from_center_size(leaf_at, vec2(radius * 2.0, radius * 2.0)));
            ui.interact(hit, ui.id().with(("leaf", i, j)), Sense::hover())
                .on_hover_text(format!(
                    "{}\n{} times, last {}",
                    leaf.full,
                    leaf.count,
                    ago(now, leaf.last_ms)
                ));
        }

        let radius = 7.0 + (branch.count as f32).ln().min(4.0) * 2.5;
        if branch.waiting {
            // A ring that breathes while a call is out.
            let pulse = 3.0 + 2.0 * (time * 4.0).sin().abs();
            painter.circle_stroke(at, radius + pulse, Stroke::new(1.5, AMBER));
        }
        painter.circle_filled(at, radius, colour);
        painter.text(
            at,
            Align2::CENTER_CENTER,
            branch.count.to_string(),
            FontId::proportional(10.0),
            BG,
        );
        let below = angle.sin() >= 0.0;
        let mut name = clip(live::short_tool(&branch.tool), 22);
        if branch.leaves.len() > leaves.len() {
            name.push_str(&format!("  +{}", branch.leaves.len() - leaves.len()));
        }
        // The name sits on the line to the agent, so it gets a backing to stay readable.
        let name_colour = lerp(DIM, TEXT, heat.max(0.35));
        let galley = painter.layout_no_wrap(name, FontId::proportional(11.5), name_colour);
        let name_at = at + vec2(0.0, if below { -radius - 10.0 } else { radius + 10.0 });
        let name_rect = Rect::from_center_size(name_at, galley.size());
        painter.rect_filled(name_rect.expand2(vec2(4.0, 1.0)), CornerRadius::same(3), PANEL);
        painter.galley(name_rect.min, galley, name_colour);
        let mut about = format!("{}\n{} calls, last {}", branch.tool, branch.count, ago(now, branch.last_ms));
        if branch.waiting {
            about.push_str("\nWaiting for a call to come back.");
        }
        if let Some(ms) = branch.failed_ms {
            about.push_str(&format!("\nA call failed {}.", ago(now, ms)));
        }
        ui.interact(
            Rect::from_center_size(at, vec2(radius * 2.0 + 8.0, radius * 2.0 + 8.0)),
            ui.id().with(("tool", i)),
            Sense::hover(),
        )
        .on_hover_text(about);
    }

    // The agent itself.
    let fill = if busy { TEAL } else { PANEL_HI };
    painter.circle_filled(centre, 20.0, fill);
    painter.circle_stroke(centre, 20.0, Stroke::new(1.5, TEAL));
    painter.text(
        centre + vec2(0.0, 31.0),
        Align2::CENTER_CENTER,
        clip(title, 26),
        FontId::proportional(11.5),
        TEXT,
    );
    if branches.is_empty() {
        painter.text(
            centre + vec2(0.0, 52.0),
            Align2::CENTER_CENTER,
            "No tool has been called in the steps seen.",
            FontId::proportional(11.0),
            DIM,
        );
    }
    let mut legend = "Teal: tools, brighter when used lately. Amber: what they were used on. Ring: waiting. Red: a call failed.".to_string();
    if hidden > 0 {
        legend.push_str(&format!("  {hidden} less recent tools are not drawn."));
    }
    painter.text(
        rect.left_bottom() + vec2(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        legend,
        FontId::proportional(10.0),
        DIM,
    );
    if branches.iter().any(|b| b.waiting) {
        ui.ctx().request_repaint_after(Duration::from_millis(60));
    }
}

fn state_colour(state: services::State) -> Color32 {
    match state {
        services::State::Ready => TEAL,
        services::State::SignIn | services::State::Stopped | services::State::AgentMissing => AMBER,
        services::State::Missing => DIM,
        services::State::Unknown => RED,
    }
}

impl LiveUi {
    /// Check every tool again, off the window's thread.
    fn check_services(&mut self, ctx: &egui::Context) {
        if self.checking.swap(true, Ordering::Relaxed) {
            return;
        }
        let (list, checking, ctx) = (self.services.clone(), self.checking.clone(), ctx.clone());
        std::thread::spawn(move || {
            let found = services::check();
            if let Ok(mut list) = list.lock() {
                *list = found;
            }
            checking.store(false, Ordering::Relaxed);
            ctx.request_repaint();
        });
    }

    /// Whether Claude Code can be started, and if not, why.
    fn claude_block(&self) -> Option<String> {
        let list = self.services.lock().ok()?;
        let claude = list.iter().find(|s| s.id == "claude-code")?;
        if claude.state == services::State::Missing {
            return Some("Claude Code is not installed. See Services above.".to_string());
        }
        match &self.login {
            // A named sign-in stands or falls by itself, whatever the tool's own is.
            Some(name) => match claude.logins.iter().find(|l| &l.login.name == name) {
                Some(state) if state.ready => None,
                Some(state) => Some(format!("The sign-in {name}: {}. See Services above.", state.text)),
                None => Some(format!("The sign-in {name} is not on the list any more.")),
            },
            None => (claude.state != services::State::Ready)
                .then(|| format!("Claude Code: {}. See Services above.", claude.state.label())),
        }
    }

    /// Each tool, whether it can be used, how that was checked and what to do.
    fn services_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut sign_in: Option<(&'static str, Vec<(String, std::ffi::OsString)>)> = None;
        let mut forget: Option<String> = None;
        let mut add = false;
        let needs: usize = self
            .services
            .lock()
            .map(|l| l.iter().filter(|s| s.reach == services::Reach::Direct && s.state != services::State::Ready).count())
            .unwrap_or(0);
        egui::CollapsingHeader::new(RichText::new("Services").size(13.0))
            .id_salt("live-services")
            .default_open(needs > 0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let busy = self.checking.load(Ordering::Relaxed);
                    if ui.add_enabled(!busy, egui::Button::new("Check again")).clicked() {
                        self.check_services(&ctx);
                    }
                    let note = if busy {
                        "Checking…"
                    } else {
                        "Maestro checks whether a key is set, never what it holds. Keys set after Maestro started are seen after a restart."
                    };
                    ui.label(RichText::new(note).color(DIM).size(11.0));
                });
                let Ok(list) = self.services.lock() else { return };
                for s in list.iter() {
                    // One line each; the detail opens for what needs you.
                    let needs_you = s.reach == services::Reach::Direct && s.state != services::State::Ready;
                    let reach = match s.reach {
                        services::Reach::Direct => "Maestro can follow and direct it",
                        services::Reach::SeenOnly => "Maestro can only see it running",
                    };
                    let mut title = egui::text::LayoutJob::default();
                    let part = |text: &str, colour: Color32, size: f32| {
                        (text.to_string(), egui::TextFormat { color: colour, font_id: FontId::proportional(size), ..Default::default() })
                    };
                    for (text, format) in [
                        part(s.name, TEXT, 12.5),
                        part(&format!("   {}", s.state.label()), state_colour(s.state), 12.0),
                        part(&format!("   {reach}"), DIM, 11.0),
                    ] {
                        title.append(&text, 0.0, format);
                    }
                    egui::CollapsingHeader::new(title)
                        .id_salt(("service", s.id))
                        .default_open(needs_you)
                        .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(command) = s.sign_in {
                            if ui
                                .button("Sign in…")
                                .on_hover_text(format!(
                                    "Opens a terminal running `{command}`. You finish signing in there and in your browser; Maestro never sees the password."
                                ))
                                .clicked()
                            {
                                sign_in = Some((command, Vec::new()));
                            }
                        }
                    });
                    ui.label(RichText::new(&s.found).size(11.0));
                    for step in &s.fix {
                        ui.label(RichText::new(format!("- {step}")).color(AMBER).size(11.0));
                    }
                    ui.label(RichText::new(format!("How this was checked: {}", s.how)).color(DIM).size(10.0));
                    if s.id == "claude-code" {
                        ui.add_space(4.0);
                        ui.label(RichText::new("More sign-ins, kept apart").color(TEAL).size(11.0));
                        ui.label(
                            RichText::new(
                                "Each one has its own folder for Claude Code's sign-in, settings and history, \
                                 so a work account and a personal one never share anything. A session is \
                                 started under one and stays under it. Maestro does not open the files in \
                                 those folders.",
                            )
                            .color(DIM)
                            .size(11.0),
                        );
                        for l in &s.logins {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&l.login.name).strong().size(12.0));
                                let colour = if l.ready { TEAL } else { AMBER };
                                ui.label(RichText::new(&l.text).color(colour).size(11.0));
                                if l.login.key_var.is_none() && !l.ready {
                                    if ui
                                        .button("Sign in…")
                                        .on_hover_text(
                                            "Opens a terminal running `claude auth login` for this sign-in only. \
                                             Sign in there with the account you want it to be.",
                                        )
                                        .clicked()
                                    {
                                        match l.login.env() {
                                            Ok(env) => sign_in = Some(("claude auth login", env)),
                                            Err(problem) => self.problem = Some(problem),
                                        }
                                    }
                                }
                                if ui
                                    .small_button("forget")
                                    .on_hover_text(format!(
                                        "Takes it off this list. Its folder stays, with its sign-in and history:\n{}",
                                        l.login.folder().display()
                                    ))
                                    .clicked()
                                {
                                    forget = Some(l.login.name.clone());
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_login)
                                    .hint_text("name, such as Work")
                                    .desired_width(150.0),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.new_key_var)
                                    .hint_text("key variable, or leave empty")
                                    .desired_width(190.0),
                            )
                            .on_hover_text(
                                "Leave empty to sign in with an account. Or give the name of an environment \
                                 variable that holds an API key for this sign-in. Maestro keeps the name, \
                                 never the key.",
                            );
                            add = ui
                                .add_enabled(!self.new_login.trim().is_empty(), egui::Button::new("Add sign-in"))
                                .clicked();
                        });
                    }
                        });
                }
            });
        if let Some((command, env)) = sign_in {
            if let Err(problem) = services::open_sign_in(command, &env) {
                self.problem = Some(problem);
            }
        }
        if add || forget.is_some() {
            let mut logins = crate::logins::Logins::load();
            let result = match &forget {
                Some(name) => logins.forget(name).map(|_| {
                    if self.login.as_ref() == Some(name) {
                        self.login = None;
                    }
                }),
                None => logins.add(&self.new_login, Some(&self.new_key_var)).map(|_| {
                    self.new_login.clear();
                    self.new_key_var.clear();
                }),
            };
            match result {
                Ok(()) => {
                    logins.save();
                    self.problem = None;
                    self.check_services(&ctx);
                }
                Err(problem) => self.problem = Some(problem),
            }
        }
    }
}
