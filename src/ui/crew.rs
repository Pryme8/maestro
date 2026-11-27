//! Phase 3 in the window: the AI agents on this machine shown as crew, with
//! what they are using, what they report spending, their budgets and the
//! quests they are working on.

use super::*;
use crate::agents::{self, Agent, Recogniser};
use crate::control::Quest;
use crate::receiver::{self, Store, Totals};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

const CARD_WIDTH: f32 = 300.0;

pub(super) struct Crew {
    pub(super) recogniser: Recogniser,
    store: Arc<Mutex<Store>>,
    /// Why the receiver could not start, if it could not.
    receiver_problem: Option<String>,
    help_open: bool,
    quest_title: String,
    quest_agent: &'static str,
    /// Agents already warned about today's budget, so the log is not flooded.
    warned: HashSet<&'static str>,
}

impl Crew {
    pub(super) fn start(ctx: &egui::Context) -> Self {
        let store = Arc::new(Mutex::new(Store::load()));
        let ctx = ctx.clone();
        let receiver_problem =
            receiver::start(store.clone(), move || ctx.request_repaint()).err();
        Self {
            recogniser: Recogniser::default(),
            store,
            receiver_problem,
            help_open: false,
            quest_title: String::new(),
            quest_agent: agents::KINDS[0].id,
            warned: HashSet::new(),
        }
    }
}

/// What agents have reported, over four spans of time.
pub(super) struct Ledger {
    pub(super) today: HashMap<String, Totals>,
    pub(super) week: HashMap<String, Totals>,
    pub(super) month: HashMap<String, Totals>,
    pub(super) lifetime: HashMap<String, Totals>,
}

impl Crew {
    /// What each account's agent reported over that account's window.
    pub(super) fn used(&self, ledger: &crate::router::Ledger) -> Vec<Totals> {
        match self.store.lock() {
            Ok(store) => crate::router::used(ledger, &store),
            Err(_) => Vec::new(),
        }
    }

    pub(super) fn ledger(&self) -> Ledger {
        match self.store.lock() {
            Ok(mut store) => {
                store.roll_day();
                Ledger {
                    today: store.today.clone(),
                    week: store.over(7),
                    month: store.over(30),
                    lifetime: store.lifetime.clone(),
                }
            }
            Err(_) => Ledger {
                today: HashMap::new(),
                week: HashMap::new(),
                month: HashMap::new(),
                lifetime: HashMap::new(),
            },
        }
    }
}

impl App {
    pub(super) fn crew_ui(&mut self, ui: &mut Ui, found: &[Agent<'_>]) {
        // Copy the totals out so the lock is not held while drawing.
        let (today, lifetime, last_report_ms) = match self.crew.store.lock() {
            Ok(mut store) => {
                store.roll_day();
                (store.today.clone(), store.lifetime.clone(), store.last_report_ms)
            }
            Err(_) => Default::default(),
        };

        ui.horizontal(|ui| {
            section_title(ui, "CREW", "AI agents on this machine");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.link("How to connect an agent").clicked() {
                    self.crew.help_open = true;
                }
                let status = match &self.crew.receiver_problem {
                    Some(_) => RichText::new("usage reports: not listening").color(RED),
                    None if last_report_ms > 0 => {
                        RichText::new("usage reports: receiving").color(TEAL)
                    }
                    None => RichText::new(format!(
                        "usage reports: listening on port {}",
                        receiver::PORT
                    ))
                    .color(DIM),
                };
                let label = ui.label(status.size(11.0));
                if let Some(problem) = &self.crew.receiver_problem {
                    label.on_hover_text(problem);
                }
            });
        });
        ui.add_space(6.0);

        // One card for each agent that is running, then one for each tool
        // that is installed with nothing running, all drawn the same way.
        let mut cards: Vec<(Face, Press)> = found
            .iter()
            .map(|agent| {
                let face = Face {
                    id: agent.kind.id,
                    label: agent.kind.label,
                    about: format!("{} · {}", agent.kind.vendor, agent.kind.what),
                    state: if agent.cpu >= 1.0 { ("working", AMBER) } else { ("idle", DIM) },
                    line: format!(
                        "{} running, {} processes, {} helpers  ·  {:.1}% processor  ·  {}",
                        agent.sessions,
                        agent.procs.len(),
                        agent.helpers.len(),
                        agent.cpu,
                        bytes(agent.mem)
                    ),
                    hover: "Helpers are the shells, tools and scripts the agent has started. \
                            Their load is counted with the agent's."
                        .to_string(),
                    button: Some((
                        "Show processes".to_string(),
                        "Opens this agent's program with its files, network and actions.",
                    )),
                    attention: false,
                };
                (face, Press::Processes(agent.procs.first().map(|p| p.name.clone())))
            })
            .collect();
        {
            use crate::services::{Reach, State};
            // The services list and the recogniser name one tool differently.
            let kind_id = |id: &str| if id == "gemini" { "gemini-cli" } else { id }.to_string();
            for service in self.lv.services_now() {
                if service.state == State::Missing || found.iter().any(|a| a.kind.id == kind_id(service.id)) {
                    continue;
                }
                let Some(kind) = crate::agents::KINDS.iter().find(|k| k.id == kind_id(service.id)) else { continue };
                let steps = crate::setup::guide(&service).map_or(0, |g| g.left());
                let tool = match service.id {
                    "claude-code" => Some(crate::live::Tool::Claude),
                    "cursor" => Some(crate::live::Tool::Cursor),
                    _ => None,
                };
                let (state, line, button, press) = if steps > 0 {
                    (
                        ("not set up", AMBER),
                        "Installed, but it cannot be used yet.".to_string(),
                        Some((format!("Set up  ·  {steps} to do"), "Shows each step and does most of them for you.")),
                        Press::SetUp(service.id),
                    )
                } else if service.state == State::Ready && service.reach == Reach::Direct && tool.is_some() {
                    (
                        ("ready", TEAL),
                        "Nothing running. Set up and ready to start.".to_string(),
                        Some(("Start a session".to_string(), "Opens the start form on the Live page with this tool chosen.")),
                        Press::Start(tool.unwrap_or_default()),
                    )
                } else {
                    (
                        (service.state.label(), DIM),
                        "Nothing running. Maestro can see it, but cannot give it work yet.".to_string(),
                        None,
                        Press::Nothing,
                    )
                };
                let face = Face {
                    id: kind.id,
                    label: kind.label,
                    about: format!("{} · {}", kind.vendor, kind.what),
                    state,
                    line,
                    hover: service.found.clone(),
                    button,
                    attention: steps > 0,
                };
                cards.push((face, press));
            }
        }
        if cards.is_empty() {
            ui.label(RichText::new("No AI agents are running or installed.").color(DIM));
        }
        let mut pressed = None;
        // Wrap by hand: framed cards do not wrap on their own. 26 is the
        // frame's margin and border on both sides.
        let per_row = (((ui.available_width() + 10.0) / (CARD_WIDTH + 36.0)).floor() as usize).max(1);
        for row in cards.chunks(per_row) {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                for (face, press) in row {
                    let spent = today.get(face.id).copied();
                    if self.card(ui, face, spent) {
                        pressed = Some(press.clone());
                    }
                }
            });
            ui.add_space(10.0);
        }
        match pressed {
            Some(Press::Processes(Some(name))) => self.selected = Some(name),
            Some(Press::Start(tool)) => {
                self.lv.open_start(tool);
                self.page = Page::Live;
            }
            Some(Press::SetUp(id)) => self.setup.show(id),
            _ => {}
        }

        self.warn_over_budget(&today);
        self.quests_ui(ui, &lifetime);
        self.help_window(ui.ctx());
    }

    /// One card. Returns true when its button was pressed.
    fn card(&mut self, ui: &mut Ui, face: &Face, spent: Option<Totals>) -> bool {
        let id = face.id;
        let mut show = false;
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(1.0, if face.attention { AMBER } else { LINE }))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(CARD_WIDTH);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(face.label).strong().size(15.0));
                        ui.label(RichText::new(&face.about).color(DIM).size(11.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (text, colour) = face.state;
                            ui.label(RichText::new(text).color(colour).size(11.0));
                        });
                    });
                    ui.label(RichText::new(&face.line).color(DIM).size(11.0)).on_hover_text(&face.hover);

                    ui.add_space(4.0);
                    let budget = self.ctl.journal.budgets.get(id).copied().unwrap_or(0.0);
                    match spent {
                        Some(t) => {
                            ui.label(
                                RichText::new(format!(
                                    "${:.2} today  ·  {} in, {} out, {} cached",
                                    t.cost_usd,
                                    count(t.tokens_in),
                                    count(t.tokens_out),
                                    count(t.cache_read + t.cache_write)
                                ))
                                .size(11.0),
                            );
                        }
                        None => {
                            ui.label(
                                RichText::new("No usage reported today.").color(DIM).size(11.0),
                            );
                        }
                    }

                    // Energy: how much of today's budget is left.
                    if budget > 0.0 {
                        let used = spent.map(|t| t.cost_usd).unwrap_or(0.0);
                        let left = (1.0 - used / budget).clamp(0.0, 1.0) as f32;
                        // Full is calm, empty is red.
                        bar(ui, left, heat((1.0 - left) * 100.0));
                        ui.label(
                            RichText::new(format!(
                                "energy: ${:.2} of ${:.2} left today",
                                (budget - used).max(0.0),
                                budget
                            ))
                            .color(DIM)
                            .size(11.0),
                        );
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Budget").color(DIM).size(11.0));
                        let mut value = budget;
                        let edit = egui::DragValue::new(&mut value)
                            .range(0.0..=10_000.0)
                            .speed(0.25)
                            .prefix("$")
                            .suffix(" a day");
                        if ui
                            .add(edit)
                            .on_hover_text("Daily spending limit, in US dollars. 0 means none. The day runs on UTC.")
                            .changed()
                        {
                            if value > 0.0 {
                                self.ctl.journal.budgets.insert(id.to_string(), value);
                            } else {
                                self.ctl.journal.budgets.remove(id);
                            }
                            self.ctl.journal.save();
                            self.crew.warned.remove(id);
                        }
                        if let Some((text, hover)) = &face.button {
                            show = ui.button(text.as_str()).on_hover_text(*hover).clicked();
                        }
                    });
                });
            });
        show
    }

    /// Put one line in the action log the first time an agent passes its budget.
    fn warn_over_budget(&mut self, today: &HashMap<String, Totals>) {
        for kind in agents::KINDS {
            let Some(budget) = self.ctl.journal.budgets.get(kind.id).copied() else {
                continue;
            };
            let spent = today.get(kind.id).map(|t| t.cost_usd).unwrap_or(0.0);
            if spent >= budget && self.crew.warned.insert(kind.id) {
                self.ctl.note(
                    format!(
                        "{} has spent ${spent:.2}, past its ${budget:.2} budget for today",
                        kind.label
                    ),
                    false,
                    None,
                );
            } else if spent < budget {
                // A new day, or a raised budget.
                self.crew.warned.remove(kind.id);
            }
        }
    }

    /// Quests: named pieces of work, with what the agent spent on each.
    fn quests_ui(&mut self, ui: &mut Ui, lifetime: &HashMap<String, Totals>) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            section_title(ui, "QUESTS", "work you are tracking, and what it cost");
        });
        ui.add_space(4.0);

        // Keep open quests up to date with what their agent has reported.
        for quest in self.ctl.journal.quests.iter_mut().filter(|q| !q.done) {
            let now = lifetime.get(&quest.agent).copied().unwrap_or_default();
            quest.cost = (now.cost_usd - quest.base_cost).max(0.0);
            quest.tokens = (now.tokens() - quest.base_tokens).max(0.0);
        }

        let mut finish = None;
        let mut remove = None;
        for (i, quest) in self.ctl.journal.quests.iter().enumerate() {
            let agent = agents::kind_by_id(&quest.agent)
                .map(|k| k.label)
                .unwrap_or(quest.agent.as_str());
            ui.horizontal(|ui| {
                let title = RichText::new(&quest.title).size(12.0);
                ui.label(if quest.done { title.color(DIM) } else { title });
                ui.label(
                    RichText::new(format!(
                        "{agent}  ·  ${:.2}  ·  {} tokens",
                        quest.cost,
                        count(quest.tokens)
                    ))
                    .color(DIM)
                    .size(11.0),
                );
                if quest.done {
                    ui.label(RichText::new("done").color(TEAL).size(11.0));
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                } else if ui.small_button("Done").clicked() {
                    finish = Some(i);
                }
            });
        }
        if let Some(i) = finish {
            self.ctl.journal.quests[i].done = true;
            let title = self.ctl.journal.quests[i].title.clone();
            self.ctl.happened.push(super::controls::Happened::QuestDone);
            self.ctl.note(format!("Finished quest: {title}"), true, None);
            self.ctl.journal.save();
        }
        if let Some(i) = remove {
            self.ctl.journal.quests.remove(i);
            self.ctl.journal.save();
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.crew.quest_title)
                    .hint_text("name a piece of work")
                    .desired_width(240.0),
            );
            let current = agents::kind_by_id(self.crew.quest_agent)
                .map(|k| k.label)
                .unwrap_or("agent");
            egui::ComboBox::from_id_salt("quest agent")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for kind in agents::KINDS {
                        ui.selectable_value(&mut self.crew.quest_agent, kind.id, kind.label);
                    }
                });
            let title = self.crew.quest_title.trim().to_string();
            if ui
                .add_enabled(!title.is_empty(), egui::Button::new("Start quest"))
                .clicked()
            {
                let base = lifetime
                    .get(self.crew.quest_agent)
                    .copied()
                    .unwrap_or_default();
                self.ctl.journal.quests.push(Quest {
                    title,
                    agent: self.crew.quest_agent.to_string(),
                    base_cost: base.cost_usd,
                    base_tokens: base.tokens(),
                    cost: 0.0,
                    tokens: 0.0,
                    done: false,
                });
                self.ctl.journal.save();
                self.crew.quest_title.clear();
            }
        });
        ui.label(
            RichText::new(
                "A quest counts what its agent reports from the moment it starts. \
                 An agent that reports nothing shows $0.00.",
            )
            .color(DIM)
            .size(10.0),
        );
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.crew.help_open {
            return;
        }
        let mut open = true;
        egui::Window::new("How to connect an agent")
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(
                    "Maestro finds running agents by itself. To also see what an agent \
                     spends, the agent has to report its usage to Maestro. Agents that \
                     speak OpenTelemetry can do this.",
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Claude Code").strong());
                ui.label("Set these before starting it, or put them in its settings file under \"env\":");
                let mut text = format!(
                    "CLAUDE_CODE_ENABLE_TELEMETRY=1\n\
                     OTEL_METRICS_EXPORTER=otlp\n\
                     OTEL_EXPORTER_OTLP_PROTOCOL=http/json\n\
                     OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:{}\n\
                     OTEL_METRIC_EXPORT_INTERVAL=10000",
                    receiver::PORT
                );
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(5),
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Other agents").strong());
                ui.label(format!(
                    "Point any OpenTelemetry metrics exporter at http://localhost:{} using the \
                     http/json protocol. Maestro reads metrics whose names end in \
                     \"token.usage\" and \"cost.usage\".",
                    receiver::PORT
                ));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Maestro listens on this computer only. It keeps the token and cost \
                         figures and discards everything else in a report, including account \
                         and email fields. Any program on this computer can send it figures, \
                         so treat them as the agent's own claim.",
                    )
                    .color(DIM)
                    .size(11.0),
                );
            });
        self.crew.help_open = open;
    }
}

/// A count in short form: 950, 34k, 1.2M.
pub(super) fn count(n: f64) -> String {
    // An empty sum is minus zero, which would print as "-0".
    if n <= 0.0 {
        "0".to_string()
    } else if n >= 1_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else if n >= 10_000.0 {
        format!("{:.0}k", n / 1_000.0)
    } else if n >= 1_000.0 {
        format!("{:.1}k", n / 1_000.0)
    } else {
        format!("{n:.0}")
    }
}

/// What one Crew card shows, whether the agent is running or only installed.
struct Face {
    /// The agent kind's id, which usage and budgets are filed under.
    id: &'static str,
    label: &'static str,
    /// Who makes it and what it is.
    about: String,
    state: (&'static str, Color32),
    line: String,
    hover: String,
    /// The button's words and what hovering over it says.
    button: Option<(String, &'static str)>,
    /// Outlined, because something is left for the user to do.
    attention: bool,
}

/// What pressing a card's button does.
#[derive(Clone)]
enum Press {
    Processes(Option<String>),
    Start(crate::live::Tool),
    SetUp(&'static str),
    Nothing,
}
