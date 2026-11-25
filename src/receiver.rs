//! Phase 3: receive usage numbers that agents report about themselves.
//!
//! Agents such as Claude Code can export OpenTelemetry metrics. This listens
//! on this computer only, takes the token and cost figures, and discards
//! everything else in the message, including account and email fields.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// The standard port for OpenTelemetry over HTTP.
pub const PORT: u16 = 4318;
const MAX_BODY: u64 = 8 * 1024 * 1024;
const FILE: &str = "usage.json";
const DAYS_KEPT: usize = 400;
/// Eight days of hours, enough for a weekly allowance window.
const HOURS_KEPT: usize = 8 * 24;

#[derive(Serialize, Deserialize, Default, Clone, Copy, Debug, PartialEq)]
pub struct Totals {
    pub cost_usd: f64,
    pub tokens_in: f64,
    pub tokens_out: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl Totals {
    pub fn tokens(&self) -> f64 {
        self.tokens_in + self.tokens_out + self.cache_read + self.cache_write
    }

    pub fn add(&mut self, other: &Totals) {
        self.cost_usd += other.cost_usd;
        self.tokens_in += other.tokens_in;
        self.tokens_out += other.tokens_out;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

/// Everything agents have reported, by agent id.
#[derive(Serialize, Deserialize, Default)]
pub struct Store {
    /// Days since 1970 in UTC that `today` covers.
    day: u64,
    pub today: HashMap<String, Totals>,
    /// Never reset, so work tracked across midnight keeps adding up.
    pub lifetime: HashMap<String, Totals>,
    /// Finished days, by days since 1970 in UTC. The last `DAYS_KEPT` are kept.
    #[serde(default)]
    pub days: BTreeMap<u64, HashMap<String, Totals>>,
    /// The last `HOURS_KEPT` hours, by hours since 1970. Allowances are
    /// counted over rolling windows of hours.
    #[serde(default)]
    pub hours: BTreeMap<u64, HashMap<String, Totals>>,
    /// The hour new figures are filed under.
    #[serde(skip)]
    now_hour: u64,
    /// Last value of each cumulative series, to turn it into increases.
    #[serde(skip)]
    last_seen: HashMap<String, f64>,
    /// When any agent last reported, in milliseconds since 1970.
    #[serde(skip)]
    pub last_report_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Store {
    pub fn load() -> Self {
        let mut store: Store = crate::paths::load(FILE)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if store.roll_day() {
            store.save();
        }
        store
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            crate::paths::save(FILE, &text);
        }
    }

    /// Start a fresh "today" when the UTC day changes. Returns true if it did.
    pub fn roll_day(&mut self) -> bool {
        self.roll_to(now_ms() / 86_400_000)
    }

    fn roll_to(&mut self, day: u64) -> bool {
        if self.day == day {
            return false;
        }
        let finished = std::mem::take(&mut self.today);
        if !finished.is_empty() {
            self.days.insert(self.day, finished);
        }
        while self.days.len() > DAYS_KEPT {
            self.days.pop_first();
        }
        self.day = day;
        true
    }

    /// What each agent reported over the last `span` days, today included.
    pub fn over(&self, span: u64) -> HashMap<String, Totals> {
        let from = (self.day + 1).saturating_sub(span);
        let mut sum = self.today.clone();
        for day in self.days.range(from..self.day).map(|(_, day)| day) {
            for (agent, totals) in day {
                sum.entry(agent.clone()).or_default().add(totals);
            }
        }
        sum
    }

    /// Take the usage figures out of one OpenTelemetry metrics message.
    /// Returns how many figures were used.
    pub fn ingest(&mut self, body: &Value) -> usize {
        self.roll_day();
        self.now_hour = now_ms() / 3_600_000;
        let used = self.apply(body);
        if used > 0 {
            self.last_report_ms = now_ms();
            self.save();
        }
        used
    }

    /// Add one message's figures to the totals, without touching the disk.
    fn apply(&mut self, body: &Value) -> usize {
        let mut used = 0;
        for resource in list(body, "resourceMetrics") {
            let service = attribute(resource.get("resource"), "service.name");
            for scope in list(resource, "scopeMetrics") {
                for metric in list(scope, "metrics") {
                    let Some(name) = metric.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(field) = Field::of(name) else { continue };
                    let Some(sum) = metric.get("sum") else { continue };
                    let agent = agent_id(service.as_deref(), name);
                    let cumulative = is_cumulative(sum);
                    for point in list(sum, "dataPoints") {
                        let Some(value) = number(point) else { continue };
                        let kind = attribute(Some(point), "type").unwrap_or_default();
                        let increase = if cumulative {
                            let key = format!("{agent}|{name}|{}", series_key(point));
                            let last = self.last_seen.insert(key, value).unwrap_or(0.0);
                            // A restart begins the count again from zero.
                            if value >= last { value - last } else { value }
                        } else {
                            value
                        };
                        if increase <= 0.0 || !increase.is_finite() {
                            continue;
                        }
                        for totals in [
                            self.today.entry(agent.clone()).or_default(),
                            self.lifetime.entry(agent.clone()).or_default(),
                            self.hours
                                .entry(self.now_hour)
                                .or_default()
                                .entry(agent.clone())
                                .or_default(),
                        ] {
                            field.add(totals, &kind, increase);
                        }
                        used += 1;
                    }
                }
            }
        }
        while self.hours.len() > HOURS_KEPT {
            self.hours.pop_first();
        }
        used
    }

    /// What each agent reported over the last `span` hours, counting the
    /// hour `now_hour` as the last one.
    pub fn over_hours(&self, span: u64, now_hour: u64) -> HashMap<String, Totals> {
        let from = (now_hour + 1).saturating_sub(span);
        let mut sum: HashMap<String, Totals> = HashMap::new();
        for hour in self.hours.range(from..=now_hour).map(|(_, hour)| hour) {
            for (agent, totals) in hour {
                sum.entry(agent.clone()).or_default().add(totals);
            }
        }
        sum
    }

    /// What each agent reported over a rolling window ending now. Windows
    /// of up to a week are counted by the hour, longer ones by the day.
    pub fn window(&self, hours: u32) -> HashMap<String, Totals> {
        if hours as usize <= HOURS_KEPT - 24 {
            self.over_hours(hours as u64, now_ms() / 3_600_000)
        } else {
            self.over((hours as u64).div_ceil(24))
        }
    }
}

/// Which total a metric feeds.
enum Field {
    Cost,
    Tokens,
}

impl Field {
    fn of(metric: &str) -> Option<Self> {
        if metric.ends_with("cost.usage") {
            Some(Field::Cost)
        } else if metric.ends_with("token.usage") {
            Some(Field::Tokens)
        } else {
            None
        }
    }

    fn add(&self, totals: &mut Totals, kind: &str, amount: f64) {
        match self {
            Field::Cost => totals.cost_usd += amount,
            Field::Tokens => match kind {
                "output" => totals.tokens_out += amount,
                "cacheRead" => totals.cache_read += amount,
                "cacheCreation" => totals.cache_write += amount,
                _ => totals.tokens_in += amount,
            },
        }
    }
}

/// The agent a metric belongs to: its service name when it gives one,
/// otherwise the first part of the metric name.
fn agent_id(service: Option<&str>, metric: &str) -> String {
    let raw = match service {
        Some(name) if !name.is_empty() && !name.starts_with("unknown_service") => name,
        _ => metric.split('.').next().unwrap_or(metric),
    };
    raw.to_lowercase().replace('_', "-")
}

fn list<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// A string attribute from an object that has an `attributes` list.
fn attribute(holder: Option<&Value>, key: &str) -> Option<String> {
    list(holder?, "attributes")
        .find(|a| a.get("key").and_then(Value::as_str) == Some(key))
        .and_then(|a| a.get("value")?.get("stringValue")?.as_str())
        .map(str::to_string)
}

/// A data point's value. Whole numbers may arrive as text.
fn number(point: &Value) -> Option<f64> {
    if let Some(v) = point.get("asDouble") {
        return v.as_f64();
    }
    let v = point.get("asInt")?;
    v.as_f64().or_else(|| v.as_str()?.parse().ok())
}

fn is_cumulative(sum: &Value) -> bool {
    match sum.get("aggregationTemporality") {
        Some(Value::Number(n)) => n.as_u64() == Some(2),
        Some(Value::String(s)) => s.ends_with("CUMULATIVE"),
        _ => false,
    }
}

/// Identifies one series: its attributes and when it started counting.
fn series_key(point: &Value) -> String {
    let mut parts: Vec<String> = list(point, "attributes").map(Value::to_string).collect();
    parts.sort_unstable();
    let start = point
        .get("startTimeUnixNano")
        .map(Value::to_string)
        .unwrap_or_default();
    format!("{}|{start}", parts.join(","))
}

/// Listen for reports on this computer only. `on_report` runs after each one
/// that carried usage figures. Returns an explanation if the port is taken.
pub fn start(
    store: Arc<Mutex<Store>>,
    on_report: impl Fn() + Send + 'static,
) -> Result<(), String> {
    let server = tiny_http::Server::http(("127.0.0.1", PORT)).map_err(|e| {
        format!("Could not listen on port {PORT}: {e}. Another telemetry collector may be using it.")
    })?;
    std::thread::spawn(move || {
        for mut request in server.incoming_requests() {
            let (code, body) = handle(&mut request, &store);
            if code == 200 && body == OK_USED {
                on_report();
            }
            let response = tiny_http::Response::from_string("{}")
                .with_status_code(code)
                .with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json")
                        .expect("static header is valid"),
                );
            let _ = request.respond(response);
        }
    });
    Ok(())
}

const OK_USED: &str = "used";
const OK_IGNORED: &str = "ignored";

fn handle(request: &mut tiny_http::Request, store: &Arc<Mutex<Store>>) -> (u16, &'static str) {
    if *request.method() != tiny_http::Method::Post {
        return (405, "");
    }
    let path = request.url().split('?').next().unwrap_or("").to_string();
    // Logs and traces are accepted and dropped so agents do not report errors.
    if path == "/v1/logs" || path == "/v1/traces" {
        return (200, OK_IGNORED);
    }
    if path != "/v1/metrics" {
        return (404, "");
    }
    let is_json = request.headers().iter().any(|h| {
        h.field.equiv("Content-Type") && h.value.as_str().to_lowercase().contains("json")
    });
    if !is_json {
        // Only the JSON form is read. The agent must be set to `http/json`.
        return (415, "");
    }
    let mut text = String::new();
    if request
        .as_reader()
        .take(MAX_BODY)
        .read_to_string(&mut text)
        .is_err()
    {
        return (400, "");
    }
    let Ok(body) = serde_json::from_str::<Value>(&text) else {
        return (400, "");
    };
    let Ok(mut store) = store.lock() else {
        return (500, "");
    };
    if store.ingest(&body) > 0 {
        (200, OK_USED)
    } else {
        (200, OK_IGNORED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(temporality: u64, cost: f64, input: &str, output: &str) -> Value {
        json!({ "resourceMetrics": [{
            "resource": { "attributes": [
                { "key": "service.name", "value": { "stringValue": "claude-code" } },
                { "key": "user.email", "value": { "stringValue": "someone@example.com" } }
            ]},
            "scopeMetrics": [{ "metrics": [
                { "name": "claude_code.cost.usage", "unit": "USD", "sum": {
                    "aggregationTemporality": temporality,
                    "dataPoints": [{ "asDouble": cost, "startTimeUnixNano": "1",
                        "attributes": [{ "key": "model", "value": { "stringValue": "m" } }] }] } },
                { "name": "claude_code.token.usage", "unit": "tokens", "sum": {
                    "aggregationTemporality": temporality,
                    "dataPoints": [
                        { "asInt": input, "startTimeUnixNano": "1",
                          "attributes": [{ "key": "type", "value": { "stringValue": "input" } }] },
                        { "asInt": output, "startTimeUnixNano": "1",
                          "attributes": [{ "key": "type", "value": { "stringValue": "output" } }] }
                    ] } },
                { "name": "claude_code.session.count", "sum": { "dataPoints": [{ "asInt": "1" }] } }
            ]}]
        }]})
    }

    fn store() -> Store {
        let mut store = Store::default();
        store.roll_day();
        store
    }

    #[test]
    fn delta_reports_add_up() {
        let mut store = store();
        store.apply(&message(1, 0.25, "1000", "200"));
        store.apply(&message(1, 0.5, "500", "100"));
        let totals = store.today["claude-code"];
        assert_eq!(totals.cost_usd, 0.75);
        assert_eq!(totals.tokens_in, 1500.0);
        assert_eq!(totals.tokens_out, 300.0);
        assert_eq!(store.lifetime["claude-code"], totals);
    }

    #[test]
    fn cumulative_reports_count_only_the_increase() {
        let mut store = store();
        store.apply(&message(2, 1.0, "1000", "100"));
        store.apply(&message(2, 1.5, "1800", "100"));
        let totals = store.today["claude-code"];
        assert_eq!(totals.cost_usd, 1.5);
        assert_eq!(totals.tokens_in, 1800.0);
        assert_eq!(totals.tokens_out, 100.0);
    }

    #[test]
    fn usage_is_filed_by_the_hour_for_rolling_windows() {
        let mut store = store();
        for (hour, cost) in [(1000, 1.0), (1003, 2.0), (1005, 4.0)] {
            store.now_hour = hour;
            store.apply(&message(1, cost, "100", "0"));
        }
        // A five-hour window ending in hour 1005 covers hours 1001 to 1005.
        assert_eq!(store.over_hours(5, 1005)["claude-code"].cost_usd, 6.0);
        assert_eq!(store.over_hours(6, 1005)["claude-code"].cost_usd, 7.0);
        assert_eq!(store.over_hours(1, 1005)["claude-code"].cost_usd, 4.0);
        assert!(store.over_hours(2, 1010).is_empty());
        // Old hours are dropped once more than eight days are held.
        for hour in 2000..2000 + HOURS_KEPT as u64 + 10 {
            store.now_hour = hour;
            store.apply(&message(1, 1.0, "1", "0"));
        }
        assert_eq!(store.hours.len(), HOURS_KEPT);
    }

    #[test]
    fn finished_days_are_kept_and_summed_over_a_span() {
        let mut store = Store::default();
        store.roll_to(100);
        store.apply(&message(1, 1.0, "100", "0"));
        store.roll_to(101);
        store.apply(&message(1, 2.0, "200", "0"));
        // A day with nothing reported leaves no entry.
        store.roll_to(102);
        store.roll_to(110);
        store.apply(&message(1, 4.0, "400", "0"));
        assert_eq!(store.days.keys().copied().collect::<Vec<_>>(), [100, 101]);
        assert_eq!(store.over(1)["claude-code"].cost_usd, 4.0);
        // Ten days back from day 110 reaches day 101 but not day 100.
        assert_eq!(store.over(10)["claude-code"].cost_usd, 6.0);
        assert_eq!(store.over(30)["claude-code"].cost_usd, 7.0);
        assert_eq!(store.lifetime["claude-code"].tokens_in, 700.0);
    }

    #[test]
    fn the_agent_is_named_from_the_metric_when_no_service_is_given() {
        assert_eq!(agent_id(None, "claude_code.cost.usage"), "claude-code");
        assert_eq!(agent_id(Some("unknown_service:node"), "codex.token.usage"), "codex");
        assert_eq!(agent_id(Some("Claude_Code"), "x.cost.usage"), "claude-code");
    }
}
