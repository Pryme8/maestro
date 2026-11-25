//! Phase 4: what happened over time.
//!
//! One line per minute of how loaded the machine was, a list of events, and
//! running counts that the achievements are judged from. Kept on this
//! computer only. It holds no process names except inside event texts that
//! describe an action you took.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// A day of minutes.
pub const MINUTES_KEPT: usize = 24 * 60;
const EVENTS_KEPT: usize = 400;
const DAYS_KEPT: usize = 400;
/// Processor load below which a minute counts as calm.
const CALM_PERCENT: f32 = 15.0;
const FILE: &str = "history.json";

/// The machine over one minute. Loads are averages, 0 to 100.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Minute {
    /// Minutes since 1970.
    pub minute: u64,
    pub cpu: f32,
    pub cpu_peak: f32,
    pub mem: f32,
    /// Bytes per second.
    pub down: f64,
    pub up: f64,
    /// Most AI agents running at once.
    pub agents: u32,
    /// Most processes held paused at once.
    pub paused: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Something Maestro did because you asked.
    Action,
    /// Something that did not work, or a warning.
    Problem,
    /// An agent started or stopped.
    Agent,
    /// An achievement was earned.
    Award,
    /// Maestro itself started.
    Session,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub at_ms: u64,
    pub kind: Kind,
    pub text: String,
}

/// Running counts. Only ever go up.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Stats {
    pub minutes_watched: u64,
    /// Days since 1970 on which Maestro ran.
    pub days: Vec<u64>,
    pub paused: u64,
    pub ended: u64,
    pub blocked: u64,
    pub adapters_off: u64,
    pub cleared: u64,
    pub quests_done: u64,
    pub most_agents: u64,
    /// Longest unbroken run of calm minutes.
    pub calm_run: u64,
}

/// What agents have reported in total, for the achievements that count it.
#[derive(Default, Clone, Copy)]
pub struct Usage {
    pub tokens: f64,
}

pub struct Award {
    pub id: &'static str,
    pub title: &'static str,
    pub how: &'static str,
    pub goal: f64,
    pub progress: fn(&Stats, &Usage) -> f64,
}

pub const AWARDS: &[Award] = &[
    Award { id: "first-watch", title: "First watch", how: "Keep Maestro running for an hour in total.", goal: 60.0, progress: |s, _| s.minutes_watched as f64 },
    Award { id: "long-haul", title: "Long haul", how: "Run Maestro on 7 different days.", goal: 7.0, progress: |s, _| s.days.len() as f64 },
    Award { id: "calm-seas", title: "Calm seas", how: "Keep the processor under 15% for an unbroken hour.", goal: 60.0, progress: |s, _| s.calm_run as f64 },
    Award { id: "steady-hand", title: "Steady hand", how: "Pause 10 processes.", goal: 10.0, progress: |s, _| s.paused as f64 },
    Award { id: "clear-decks", title: "Clear decks", how: "Clear the machine for a task.", goal: 1.0, progress: |s, _| s.cleared as f64 },
    Award { id: "deck-officer", title: "Deck officer", how: "Clear the machine for a task 25 times.", goal: 25.0, progress: |s, _| s.cleared as f64 },
    Award { id: "gatekeeper", title: "Gatekeeper", how: "Block a program from the network.", goal: 1.0, progress: |s, _| s.blocked as f64 },
    Award { id: "radio-silence", title: "Radio silence", how: "Switch a network adapter off.", goal: 1.0, progress: |s, _| s.adapters_off as f64 },
    Award { id: "full-crew", title: "Full crew", how: "Have three AI agents running at once.", goal: 3.0, progress: |s, _| s.most_agents as f64 },
    Award { id: "first-report", title: "First report", how: "Have an agent report its usage to Maestro.", goal: 1.0, progress: |_, u| u.tokens.min(1.0) },
    Award { id: "million", title: "A million tokens", how: "Have agents report a million tokens in total.", goal: 1_000_000.0, progress: |_, u| u.tokens },
    Award { id: "quest-complete", title: "Quest complete", how: "Finish a quest.", goal: 1.0, progress: |s, _| s.quests_done as f64 },
    Award { id: "campaigner", title: "Campaigner", how: "Finish 10 quests.", goal: 10.0, progress: |s, _| s.quests_done as f64 },
];

/// The minute being filled in.
#[derive(Default)]
struct Open {
    minute: u64,
    samples: u32,
    cpu: f64,
    cpu_peak: f32,
    mem: f64,
    down: f64,
    up: f64,
    agents: u32,
    paused: u32,
}

/// One reading of the machine, as the history needs it.
#[derive(Clone, Copy, Default)]
pub struct Reading {
    pub at_ms: u64,
    pub cpu: f32,
    pub mem: f32,
    pub down: f64,
    pub up: f64,
    pub agents: u32,
    pub paused: u32,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct History {
    pub minutes: VecDeque<Minute>,
    pub events: VecDeque<Event>,
    pub stats: Stats,
    /// Achievement id to when it was earned, in milliseconds since 1970.
    pub awards: BTreeMap<String, u64>,
    #[serde(skip)]
    open: Open,
    /// The calm run still going.
    #[serde(skip)]
    calm_now: u64,
    /// Whether to write changes to disk. Off in tests.
    #[serde(skip)]
    on_disk: bool,
}

impl History {
    pub fn load() -> Self {
        let mut history: History = crate::paths::load(FILE)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        history.on_disk = true;
        history
    }

    pub fn save(&self) {
        if !self.on_disk {
            return;
        }
        if let Ok(text) = serde_json::to_string(self) {
            crate::paths::save(FILE, &text);
        }
    }

    /// Take one reading. Returns true when a minute was completed, which is
    /// when the caller should save.
    pub fn record(&mut self, r: Reading) -> bool {
        let minute = r.at_ms / 60_000;
        let mut closed = false;
        if self.open.samples > 0 && self.open.minute != minute {
            self.close();
            closed = true;
        }
        let open = &mut self.open;
        open.minute = minute;
        open.samples += 1;
        open.cpu += r.cpu as f64;
        open.cpu_peak = open.cpu_peak.max(r.cpu);
        open.mem += r.mem as f64;
        open.down += r.down;
        open.up += r.up;
        open.agents = open.agents.max(r.agents);
        open.paused = open.paused.max(r.paused);
        self.stats.most_agents = self.stats.most_agents.max(r.agents as u64);
        closed
    }

    fn close(&mut self) {
        let open = std::mem::take(&mut self.open);
        let n = open.samples.max(1) as f64;
        let done = Minute {
            minute: open.minute,
            cpu: (open.cpu / n) as f32,
            cpu_peak: open.cpu_peak,
            mem: (open.mem / n) as f32,
            down: open.down / n,
            up: open.up / n,
            agents: open.agents,
            paused: open.paused,
        };
        // A gap, such as Maestro being closed, ends a calm run.
        let follows = self.minutes.back().is_some_and(|m| m.minute + 1 == done.minute);
        if done.cpu < CALM_PERCENT {
            self.calm_now = if follows { self.calm_now + 1 } else { 1 };
            self.stats.calm_run = self.stats.calm_run.max(self.calm_now);
        } else {
            self.calm_now = 0;
        }
        self.stats.minutes_watched += 1;
        let day = done.minute / (24 * 60);
        if self.stats.days.last() != Some(&day) {
            self.stats.days.push(day);
            let extra = self.stats.days.len().saturating_sub(DAYS_KEPT);
            self.stats.days.drain(..extra);
        }
        self.minutes.push_back(done);
        while self.minutes.len() > MINUTES_KEPT {
            self.minutes.pop_front();
        }
    }

    pub fn event(&mut self, at_ms: u64, kind: Kind, text: String) {
        self.events.push_back(Event { at_ms, kind, text });
        while self.events.len() > EVENTS_KEPT {
            self.events.pop_front();
        }
    }

    /// Mark every achievement whose goal is now met. Returns the new ones.
    pub fn earn(&mut self, at_ms: u64, usage: &Usage) -> Vec<&'static Award> {
        let mut new = Vec::new();
        for award in AWARDS {
            if !self.awards.contains_key(award.id)
                && (award.progress)(&self.stats, usage) >= award.goal
            {
                self.awards.insert(award.id.to_string(), at_ms);
                self.event(at_ms, Kind::Award, format!("Earned: {}", award.title));
                new.push(award);
            }
        }
        new
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(minute: u64, second: u64, cpu: f32) -> Reading {
        Reading {
            at_ms: (minute * 60 + second) * 1000,
            cpu,
            mem: 50.0,
            down: 100.0,
            up: 10.0,
            agents: 1,
            paused: 0,
        }
    }

    #[test]
    fn readings_are_averaged_into_minutes() {
        let mut h = History::default();
        assert!(!h.record(reading(10, 0, 10.0)));
        assert!(!h.record(reading(10, 30, 30.0)));
        // The first reading of the next minute completes the one before.
        assert!(h.record(reading(11, 0, 80.0)));
        assert_eq!(h.minutes.len(), 1);
        let m = h.minutes[0];
        assert_eq!((m.minute, m.cpu, m.cpu_peak, m.agents), (10, 20.0, 30.0, 1));
        assert_eq!(h.stats.minutes_watched, 1);
        assert_eq!(h.stats.days, [0]);
    }

    #[test]
    fn only_a_day_of_minutes_is_kept() {
        let mut h = History::default();
        for minute in 0..(MINUTES_KEPT as u64 + 50) {
            h.record(reading(minute, 0, 5.0));
        }
        assert_eq!(h.minutes.len(), MINUTES_KEPT);
        assert_eq!(h.minutes.back().map(|m| m.minute), Some(MINUTES_KEPT as u64 + 48));
    }

    #[test]
    fn a_calm_run_is_broken_by_load_and_by_gaps() {
        let mut h = History::default();
        for minute in 0..=30 {
            h.record(reading(minute, 0, 5.0));
        }
        assert_eq!(h.stats.calm_run, 30);
        // One busy minute, then calm again: the run starts over.
        h.record(reading(31, 0, 90.0));
        for minute in 32..=40 {
            h.record(reading(minute, 0, 5.0));
        }
        assert_eq!(h.stats.calm_run, 31);
        // Maestro was closed for a while: minutes either side do not join up.
        let mut h = History::default();
        for minute in (0..=20).chain(500..=540) {
            h.record(reading(minute, 0, 5.0));
        }
        assert_eq!(h.stats.calm_run, 40);
    }

    #[test]
    fn achievements_are_earned_once() {
        let mut h = History::default();
        let usage = Usage::default();
        assert!(h.earn(1, &usage).is_empty());
        h.stats.cleared = 1;
        h.stats.paused = 12;
        let ids: Vec<_> = h.earn(2, &usage).iter().map(|a| a.id).collect();
        assert_eq!(ids, ["steady-hand", "clear-decks"]);
        assert!(h.earn(3, &usage).is_empty());
        assert_eq!(h.awards["clear-decks"], 2);
        assert_eq!(h.events.len(), 2);
        // Reported tokens count towards the usage achievements.
        let ids: Vec<_> = h.earn(4, &Usage { tokens: 2_000_000.0 }).iter().map(|a| a.id).collect();
        assert_eq!(ids, ["first-report", "million"]);
    }

    #[test]
    fn award_ids_are_unique() {
        let mut ids: Vec<_> = AWARDS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), AWARDS.len());
    }
}
