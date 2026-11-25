//! Phase 4 in the window: the last day of the machine drawn along a line,
//! what happened on it, what agents reported over time, and the achievements.

use super::controls::Happened;
use super::*;
use crate::agents::{self, Agent};
use crate::history::{self, History, Kind, Minute, Usage, AWARDS};
use crate::receiver::Totals;
use std::collections::HashSet;

#[derive(PartialEq, Clone, Copy)]
pub enum Page {
    Base,
    Timeline,
    Network,
    Router,
}

impl Page {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "base" => Some(Page::Base),
            "timeline" => Some(Page::Timeline),
            "network" => Some(Page::Network),
            "router" => Some(Page::Router),
            _ => None,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Span {
    Hour,
    SixHours,
    Day,
}

impl Span {
    fn minutes(self) -> u64 {
        match self {
            Span::Hour => 60,
            Span::SixHours => 360,
            Span::Day => 1440,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Span::Hour => "1 hour",
            Span::SixHours => "6 hours",
            Span::Day => "24 hours",
        }
    }
}

pub(super) struct Timeline {
    span: Span,
    all_events: bool,
    /// Agents running at the last look. `None` before the first look.
    agents_seen: Option<HashSet<&'static str>>,
}

impl Timeline {
    pub(super) fn new() -> Self {
        Self {
            span: Span::Hour,
            all_events: false,
            agents_seen: None,
        }
    }
}

const CARD_WIDTH: f32 = 250.0;

impl App {
    /// Move what has just happened into the history, and hand out any
    /// achievements it earned. Runs every frame; cheap when nothing happened.
    pub(super) fn keep_history(&mut self, found: &[Agent<'_>]) {
        let now = history::now_ms();
        let tokens = self.crew.ledger().lifetime.values().map(Totals::tokens).sum();
        let history = self.history.clone();
        let Ok(mut h) = history.lock() else { return };
        let mut changed = false;

        let here: HashSet<&'static str> = found.iter().map(|a| a.kind.id).collect();
        match &self.tl.agents_seen {
            None => {
                h.event(now, Kind::Session, "Maestro started".to_string());
                changed = true;
            }
            Some(before) => {
                let label = |id: &str| agents::kind_by_id(id).map_or("An agent", |k| k.label);
                for id in here.difference(before) {
                    h.event(now, Kind::Agent, format!("{} started", label(id)));
                    changed = true;
                }
                for id in before.difference(&here) {
                    h.event(now, Kind::Agent, format!("{} stopped", label(id)));
                    changed = true;
                }
            }
        }
        self.tl.agents_seen = Some(here);
        self.live.set(found.len(), self.ctl.journal.paused.len());

        for happened in self.ctl.happened.drain(..) {
            changed = true;
            match happened {
                Happened::Note { text, ok } => {
                    h.event(now, if ok { Kind::Action } else { Kind::Problem }, text);
                }
                Happened::Paused(n) => h.stats.paused += n as u64,
                Happened::Ended(n) => h.stats.ended += n as u64,
                Happened::Blocked => h.stats.blocked += 1,
                Happened::AdapterOff => h.stats.adapters_off += 1,
                Happened::Cleared => h.stats.cleared += 1,
                Happened::QuestDone => h.stats.quests_done += 1,
            }
        }

        for award in h.earn(now, &Usage { tokens }) {
            changed = true;
            self.ctl.announce(format!("Achievement earned: {}. {}", award.title, award.how));
        }
        if changed {
            h.save();
        }
    }

    pub(super) fn timeline_ui(&mut self, ui: &mut Ui) {
        let history = self.history.clone();
        let Ok(h) = history.lock() else { return };
        let now = history::now_ms();

        ui.horizontal(|ui| {
            section_title(ui, "TIMELINE", "the machine, minute by minute");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for span in [Span::Day, Span::SixHours, Span::Hour] {
                    ui.selectable_value(&mut self.tl.span, span, span.label());
                }
            });
        });
        ui.add_space(4.0);
        chart(ui, &h, now, self.tl.span.minutes());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            for (colour, text) in [
                (TEAL, "processor, coloured by load"),
                (VIOLET, "memory in use"),
                (BLUE, "download"),
                (AMBER, "upload"),
            ] {
                legend(ui, colour, text);
            }
            ui.label(
                RichText::new("Gaps are times Maestro was not running.")
                    .color(DIM)
                    .size(10.0),
            );
        });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            section_title(ui, "EVENTS", "what happened, newest first");
        });
        ui.add_space(4.0);
        if h.events.is_empty() {
            ui.label(RichText::new("Nothing yet.").color(DIM).size(11.0));
        }
        let shown = if self.tl.all_events { h.events.len() } else { 8 };
        for event in h.events.iter().rev().take(shown) {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(8.0, 12.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, kind_colour(event.kind));
                ui.label(
                    RichText::new(format!("{:>10}", ago(now, event.at_ms)))
                        .color(DIM)
                        .monospace()
                        .size(11.0),
                );
                ui.label(RichText::new(&event.text).size(11.0));
            });
        }
        if h.events.len() > 8 {
            let text = if self.tl.all_events {
                "Show fewer".to_string()
            } else {
                format!("Show all {}", h.events.len())
            };
            if ui.link(RichText::new(text).size(11.0)).clicked() {
                self.tl.all_events = !self.tl.all_events;
            }
        }

        ui.add_space(14.0);
        self.ledger_ui(ui);
        ui.add_space(14.0);
        let tokens = self.crew.ledger().lifetime.values().map(Totals::tokens).sum();
        awards_ui(ui, &h, now, &Usage { tokens });
    }

    /// What each agent has reported, over four spans of time.
    fn ledger_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            section_title(ui, "LEDGER", "what agents have reported spending");
        });
        ui.add_space(4.0);
        let ledger = self.crew.ledger();
        let spans = [&ledger.today, &ledger.week, &ledger.month, &ledger.lifetime];
        let mut ids: Vec<&String> = ledger.lifetime.keys().collect();
        ids.sort_by(|a, b| {
            ledger.lifetime[*b]
                .cost_usd
                .total_cmp(&ledger.lifetime[*a].cost_usd)
                .then(a.cmp(b))
        });
        if ids.is_empty() {
            ui.label(
                RichText::new(
                    "No agent has reported usage yet. \"How to connect an agent\" on the \
                     base page shows the settings.",
                )
                .color(DIM)
                .size(11.0),
            );
            return;
        }
        let cell = |ui: &mut Ui, t: Option<&Totals>, strong: bool| {
            let text = match t {
                Some(t) if t.tokens() > 0.0 || t.cost_usd > 0.0 => {
                    format!("${:.2}  ·  {}", t.cost_usd, crew::count(t.tokens()))
                }
                _ => "-".to_string(),
            };
            let text = RichText::new(text).size(11.0);
            ui.label(if strong { text.strong() } else { text });
        };
        egui::Grid::new("ledger")
            .striped(true)
            .spacing(vec2(22.0, 5.0))
            .show(ui, |ui| {
                for head in ["Agent", "Vendor", "Today", "7 days", "30 days", "All time"] {
                    ui.label(RichText::new(head).color(DIM).size(11.0));
                }
                ui.end_row();
                for id in &ids {
                    let kind = agents::kind_by_id(id);
                    ui.label(RichText::new(kind.map_or(id.as_str(), |k| k.label)).size(11.0));
                    ui.label(
                        RichText::new(kind.map_or("unknown", |k| k.vendor))
                            .color(DIM)
                            .size(11.0),
                    );
                    for span in spans {
                        cell(ui, span.get(*id), false);
                    }
                    ui.end_row();
                }
                if ids.len() > 1 {
                    ui.label(RichText::new("All agents").strong().size(11.0));
                    ui.label("");
                    for span in spans {
                        let mut sum = Totals::default();
                        span.values().for_each(|t| sum.add(t));
                        cell(ui, Some(&sum), true);
                    }
                    ui.end_row();
                }
            });
        ui.label(
            RichText::new(
                "Cost and tokens as each agent reported them. On a subscription the cost is \
                 the vendor's own estimate, not a bill. Days run on UTC.",
            )
            .color(DIM)
            .size(10.0),
        );
    }
}

fn legend(ui: &mut Ui, colour: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(2), colour);
        ui.label(RichText::new(text).color(DIM).size(10.0));
    });
}

fn kind_colour(kind: Kind) -> Color32 {
    match kind {
        Kind::Action => TEAL,
        Kind::Problem => RED,
        Kind::Agent => VIOLET,
        Kind::Award => AMBER,
        Kind::Session => SLATE,
    }
}

/// The chart: processor and memory on top, network below, events as marks.
fn chart(ui: &mut Ui, h: &History, now_ms: u64, span: u64) {
    const LOAD: f32 = 96.0;
    const NET: f32 = 40.0;
    const GAP: f32 = 10.0;
    let size = vec2(ui.available_width(), LOAD + NET + GAP + 36.0);
    let (outer, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter().with_clip_rect(outer);
    painter.rect_filled(outer, CornerRadius::same(6), PANEL);
    painter.rect_stroke(
        outer,
        CornerRadius::same(6),
        Stroke::new(1.0, LINE),
        egui::StrokeKind::Inside,
    );

    let left = outer.left() + 12.0;
    let width = outer.width() - 24.0;
    let load = Rect::from_min_size(pos2(left, outer.top() + 10.0), vec2(width, LOAD));
    let net = Rect::from_min_size(pos2(left, load.bottom() + GAP), vec2(width, NET));

    let now_minute = now_ms / 60_000;
    let start = now_minute.saturating_sub(span);
    let x_of = |minute: f64| left + ((minute - start as f64) / span as f64) as f32 * width;
    let bar = (width / span as f32).max(1.0);
    let minutes: Vec<&Minute> = h.minutes.iter().filter(|m| m.minute >= start).collect();

    // Guides at half and full load, and the network lane's middle.
    let guide = Stroke::new(1.0, Color32::from_rgb(26, 37, 49));
    for y in [load.top(), load.center().y, load.bottom(), net.center().y] {
        painter.line_segment([pos2(left, y), pos2(left + width, y)], guide);
    }

    if minutes.is_empty() {
        painter.text(
            load.center(),
            Align2::CENTER_CENTER,
            "The timeline fills in as Maestro runs, one line a minute.",
            FontId::proportional(12.0),
            DIM,
        );
    }

    let net_top = minutes
        .iter()
        .map(|m| m.down.max(m.up))
        .fold(16.0 * 1024.0, f64::max);
    let mut memory: Vec<egui::Pos2> = Vec::new();
    let mut last_minute = 0;
    let memory_line = Stroke::new(1.5, VIOLET);
    for m in &minutes {
        let x = x_of(m.minute as f64);
        // Processor: a bar to the average, with a faint mark at the peak.
        let top = load.bottom() - LOAD * (m.cpu / 100.0).clamp(0.0, 1.0);
        painter.rect_filled(
            Rect::from_min_max(pos2(x, top), pos2(x + bar, load.bottom())),
            CornerRadius::ZERO,
            heat(m.cpu),
        );
        let peak = load.bottom() - LOAD * (m.cpu_peak / 100.0).clamp(0.0, 1.0);
        painter.rect_filled(
            Rect::from_min_size(pos2(x, peak), vec2(bar, 1.0)),
            CornerRadius::ZERO,
            Color32::from_rgb(70, 92, 112),
        );
        // Network: download above the middle, upload below it.
        let half = NET / 2.0;
        let down = half * (m.down / net_top).clamp(0.0, 1.0) as f32;
        let up = half * (m.up / net_top).clamp(0.0, 1.0) as f32;
        let mid = net.center().y;
        painter.rect_filled(
            Rect::from_min_max(pos2(x, mid - down), pos2(x + bar, mid)),
            CornerRadius::ZERO,
            BLUE,
        );
        painter.rect_filled(
            Rect::from_min_max(pos2(x, mid), pos2(x + bar, mid + up)),
            CornerRadius::ZERO,
            AMBER,
        );
        // Memory: a line, broken where Maestro was not running.
        if m.minute != last_minute + 1 && memory.len() > 1 {
            painter.add(egui::Shape::line(std::mem::take(&mut memory), memory_line));
        } else if m.minute != last_minute + 1 {
            memory.clear();
        }
        last_minute = m.minute;
        memory.push(pos2(
            x + bar / 2.0,
            load.bottom() - LOAD * (m.mem / 100.0).clamp(0.0, 1.0),
        ));
    }
    if memory.len() > 1 {
        painter.add(egui::Shape::line(memory, memory_line));
    }

    // Events: a mark on the top edge of the chart.
    for event in h.events.iter().filter(|e| e.at_ms / 60_000 >= start) {
        let x = x_of(event.at_ms as f64 / 60_000.0);
        painter.line_segment(
            [pos2(x, load.top()), pos2(x, load.top() + 9.0)],
            Stroke::new(2.0, kind_colour(event.kind)),
        );
    }

    // The scale underneath.
    let y = net.bottom() + 6.0;
    let font = FontId::proportional(10.0);
    let (back, half_back) = match span {
        60 => ("1 hour ago", "30 min ago"),
        360 => ("6 hours ago", "3 hours ago"),
        _ => ("24 hours ago", "12 hours ago"),
    };
    painter.text(pos2(left, y), Align2::LEFT_TOP, back, font.clone(), DIM);
    painter.text(pos2(left + width / 2.0, y), Align2::CENTER_TOP, half_back, font.clone(), DIM);
    painter.text(pos2(left + width, y), Align2::RIGHT_TOP, "now", font.clone(), DIM);
    painter.text(
        pos2(left + width / 4.0, y),
        Align2::CENTER_TOP,
        format!("network lane is {} tall", rate(net_top)),
        font,
        DIM,
    );

    // Under the pointer: that minute's figures and anything that happened then.
    let Some(pointer) = response.hover_pos() else { return };
    if pointer.x < left || pointer.x > left + width {
        return;
    }
    let at = start + ((pointer.x - left) / width * span as f32) as u64;
    // On the 24-hour view one pixel covers more than a minute.
    let reach = ((span as f32 / width).ceil() as u64).max(1);
    painter.line_segment(
        [pos2(pointer.x, load.top()), pos2(pointer.x, net.bottom())],
        Stroke::new(1.0, DIM),
    );
    let near = |minute: u64| minute.abs_diff(at) <= reach;
    let minute = minutes
        .iter()
        .filter(|m| near(m.minute))
        .min_by_key(|m| m.minute.abs_diff(at));
    let events: Vec<_> = h.events.iter().filter(|e| near(e.at_ms / 60_000)).collect();
    response.on_hover_ui_at_pointer(|ui| {
        ui.label(RichText::new(ago(now_ms, at * 60_000)).strong().size(11.0));
        match minute {
            Some(m) => {
                ui.label(
                    RichText::new(format!(
                        "processor {:.0}% (peak {:.0}%)  ·  memory {:.0}%\n\
                         down {}  ·  up {}\n\
                         {} agents running  ·  {} processes paused",
                        m.cpu,
                        m.cpu_peak,
                        m.mem,
                        rate(m.down),
                        rate(m.up),
                        m.agents,
                        m.paused
                    ))
                    .size(11.0),
                );
            }
            None => {
                ui.label(RichText::new("Maestro was not running.").color(DIM).size(11.0));
            }
        }
        for event in events {
            ui.label(
                RichText::new(&event.text)
                    .color(kind_colour(event.kind))
                    .size(11.0),
            );
        }
    });
}

fn awards_ui(ui: &mut Ui, h: &History, now: u64, usage: &Usage) {
    ui.horizontal(|ui| {
        section_title(
            ui,
            "LOG BOOK",
            &format!("{} of {} achievements earned", h.awards.len(), AWARDS.len()),
        );
    });
    ui.add_space(6.0);
    // Wrap by hand: framed cards do not wrap on their own.
    let per_row = (((ui.available_width() + 10.0) / (CARD_WIDTH + 36.0)).floor() as usize).max(1);
    for row in AWARDS.chunks(per_row) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            for award in row {
                let earned = h.awards.get(award.id).copied();
                let have = (award.progress)(&h.stats, usage).min(award.goal);
                egui::Frame::new()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, if earned.is_some() { AMBER } else { LINE }))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_width(CARD_WIDTH);
                            ui.set_min_height(78.0);
                            let title = RichText::new(award.title).strong().size(13.0);
                            ui.label(if earned.is_some() { title.color(AMBER) } else { title });
                            ui.label(RichText::new(award.how).color(DIM).size(11.0));
                            ui.add_space(2.0);
                            match earned {
                                Some(at) => {
                                    ui.label(
                                        RichText::new(format!("earned {}", ago(now, at)))
                                            .color(AMBER)
                                            .size(11.0),
                                    );
                                }
                                None => {
                                    bar(ui, (have / award.goal) as f32, TEAL);
                                    if award.goal > 1.0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} of {}",
                                                crew::count(have),
                                                crew::count(award.goal)
                                            ))
                                            .color(DIM)
                                            .size(10.0),
                                        );
                                    }
                                }
                            }
                        });
                    });
            }
        });
        ui.add_space(10.0);
    }
}

/// How long ago, in words.
fn ago(now_ms: u64, at_ms: u64) -> String {
    let secs = now_ms.saturating_sub(at_ms) / 1000;
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=172_799 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}
