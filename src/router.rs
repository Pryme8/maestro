//! Phase 5: the ledger of AI accounts and the advisor that proposes routes.
//!
//! The ledger holds what you tell it about each account: what it costs or
//! how big its allowance is. The advisor turns that, plus what agents have
//! reported using, into two or three ways to do a task with their cost and
//! their effect on each allowance. It decides nothing by itself and cannot
//! judge answer quality: "best" here means cheapest within your rules.

use crate::receiver::{Store, Totals};
use serde::{Deserialize, Serialize};

const FILE: &str = "ledger.json";
/// Below this size a task is not worth splitting between two tools.
pub const SPLIT_MIN_TOKENS: f64 = 200_000.0;
/// What writing a handoff brief costs, in tokens.
const BRIEF_TOKENS: f64 = 5_000.0;
/// The share of the task the second tool has to read again to pick it up.
const REREAD_SHARE: f64 = 0.10;
/// How a split divides the work.
const DRAFT_SHARE: f64 = 0.8;

/// How demanding the work is, and how capable a model is. You set both.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Tier {
    Light,
    Standard,
    Heavy,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Light, Tier::Standard, Tier::Heavy];

    pub fn label(self) -> &'static str {
        match self {
            Tier::Light => "light",
            Tier::Standard => "standard",
            Tier::Heavy => "heavy",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Tier::ALL.into_iter().find(|t| t.label() == name)
    }

    fn one_below(self) -> Self {
        match self {
            Tier::Heavy => Tier::Standard,
            _ => Tier::Light,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum Plan {
    /// Billed per token. Prices are US dollars per million tokens.
    PayPerUse { in_per_mtok: f64, out_per_mtok: f64 },
    /// A subscription: a number of tokens over a rolling window. Vendors do
    /// not publish this figure, so it is your estimate.
    Allowance { limit_tokens: f64, window_hours: u32 },
}

/// One way of paying for one model through one tool.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Account {
    pub name: String,
    /// Agent id, as in `agents::KINDS`. Usage is matched to accounts by this,
    /// so two accounts on the same agent share one usage figure.
    pub agent: String,
    pub model: String,
    pub tier: Tier,
    pub plan: Plan,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Ledger {
    pub accounts: Vec<Account>,
    /// The share of a task's tokens that is output, used to price pay-per-use work.
    pub output_share: f64,
    /// The share of each allowance the advisor will not plan to use.
    pub reserve: f64,
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            accounts: Vec::new(),
            output_share: 0.10,
            reserve: 0.10,
        }
    }
}

impl Ledger {
    pub fn load() -> Self {
        crate::paths::load(FILE)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            crate::paths::save(FILE, &text);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Task {
    /// Your estimate of the whole task, input and output together.
    pub tokens: f64,
    pub tier: Tier,
    /// Whether you want a second model to review the work.
    pub review: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// Index into the ledger's accounts.
    pub account: usize,
    pub what: &'static str,
    pub tokens: f64,
    /// What the step adds to your bill. Zero inside an allowance.
    pub cost: f64,
    /// For an allowance: the share of it left after this step, 0 to 1.
    pub left_after: Option<f64>,
    /// True when the step would eat into the reserve or run past the allowance.
    pub over: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub title: &'static str,
    pub why: String,
    pub steps: Vec<Step>,
}

impl Route {
    pub fn cost(&self) -> f64 {
        self.steps.iter().map(|s| s.cost).sum()
    }

    pub fn over(&self) -> bool {
        self.steps.iter().any(|s| s.over)
    }

    #[cfg(test)]
    pub fn tokens(&self) -> f64 {
        self.steps.iter().map(|s| s.tokens).sum()
    }
}

#[derive(Default, Debug)]
pub struct Advice {
    pub routes: Vec<Route>,
    /// Things worth saying that are not a route.
    pub notes: Vec<String>,
}

/// Propose routes for a task. `used[i]` is what account `i`'s agent has
/// reported over that account's allowance window; it is ignored for
/// pay-per-use accounts.
pub fn advise(ledger: &Ledger, used: &[Totals], task: Task) -> Advice {
    let mut advice = Advice::default();
    let accounts = &ledger.accounts;
    let step = |account: usize, what: &'static str, tokens: f64| -> Step {
        match accounts[account].plan {
            Plan::PayPerUse { in_per_mtok, out_per_mtok } => {
                let out = tokens * ledger.output_share;
                Step {
                    account,
                    what,
                    tokens,
                    cost: ((tokens - out) * in_per_mtok + out * out_per_mtok) / 1e6,
                    left_after: None,
                    over: false,
                }
            }
            Plan::Allowance { limit_tokens, .. } => {
                let spent = used.get(account).map_or(0.0, Totals::tokens);
                let left = limit_tokens - spent - tokens;
                Step {
                    account,
                    what,
                    tokens,
                    cost: 0.0,
                    left_after: Some(if limit_tokens > 0.0 {
                        (left / limit_tokens).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }),
                    over: left < limit_tokens * ledger.reserve,
                }
            }
        }
    };
    // Something that fits first, then the cheapest, then the allowance with
    // the most left: unused allowance is the part of a subscription that is
    // paid for and lost.
    let best = |candidates: &[usize], what: &'static str, tokens: f64| -> Option<Step> {
        candidates
            .iter()
            .map(|&i| step(i, what, tokens))
            .min_by(|a, b| {
                a.over
                    .cmp(&b.over)
                    .then(a.cost.total_cmp(&b.cost))
                    .then(b.left_after.unwrap_or(0.0).total_cmp(&a.left_after.unwrap_or(0.0)))
            })
    };
    let rated = |tier: Tier| -> Vec<usize> {
        (0..accounts.len()).filter(|&i| accounts[i].tier >= tier).collect()
    };

    let able = rated(task.tier);
    let Some(whole) = best(&able, "the whole task", task.tokens) else {
        advice.notes.push(format!(
            "No account in the ledger is rated for {} work. Add one, or lower the task's rating.",
            task.tier.label()
        ));
        return advice;
    };
    let name = |i: usize| accounts[i].name.as_str();
    let single = Route {
        title: "One tool, cheapest that fits",
        why: match (whole.left_after, whole.over) {
            (Some(_), false) => format!(
                "{} has room in its allowance, so the task adds nothing to your bill.",
                name(whole.account)
            ),
            (Some(_), true) => format!(
                "Nothing fits cleanly. {} comes closest, but this would use the part of its \
                 allowance you keep in reserve, or run past it.",
                name(whole.account)
            ),
            (None, _) => format!("{} is the cheapest pay-per-use price you have entered.", name(whole.account)),
        },
        steps: vec![whole.clone()],
    };

    // The same task without touching any allowance.
    let paid: Vec<usize> = able
        .iter()
        .copied()
        .filter(|&i| matches!(accounts[i].plan, Plan::PayPerUse { .. }))
        .collect();
    let spare = best(&paid, "the whole task", task.tokens)
        .filter(|s| s.account != whole.account)
        .map(|s| Route {
            title: "One tool, allowances untouched",
            why: format!(
                "{} is billed per token, so every allowance stays as it is for later work.",
                name(s.account)
            ),
            steps: vec![s],
        });

    // Draft on a cheaper model, review on one rated for the task.
    let mut split = None;
    if task.tokens >= SPLIT_MIN_TOKENS && accounts.len() >= 2 {
        let handoff = BRIEF_TOKENS + task.tokens * REREAD_SHARE;
        let draft_tokens = task.tokens * DRAFT_SHARE;
        let review_tokens = task.tokens * (1.0 - DRAFT_SHARE) + handoff;
        if let Some(draft) = best(&rated(task.tier.one_below()), "draft", draft_tokens) {
            let reviewers: Vec<usize> =
                able.iter().copied().filter(|&i| i != draft.account).collect();
            if let Some(review) = best(&reviewers, "review", review_tokens) {
                let route = Route {
                    title: "Two tools: draft, then review",
                    why: format!(
                        "{} drafts and {} reviews. The handoff costs about {:.0}k extra tokens \
                         for the brief and for reading the work again.",
                        name(draft.account),
                        name(review.account),
                        handoff / 1000.0
                    ),
                    steps: vec![draft, review],
                };
                let saves = route.cost() < single.cost() || (single.over() && !route.over());
                if task.review || saves {
                    split = Some(route);
                } else {
                    advice.notes.push(format!(
                        "A split between two tools was looked at and dropped: the handoff costs \
                         about {:.0}k tokens and saves nothing here.",
                        handoff / 1000.0
                    ));
                }
            }
        }
    } else if task.review {
        advice.notes.push(if accounts.len() < 2 {
            "A review by a second tool needs a second account in the ledger.".to_string()
        } else {
            format!(
                "This task is small. Below about {:.0}k tokens a handoff costs more than it \
                 gives, so the advice is to stay in one tool.",
                SPLIT_MIN_TOKENS / 1000.0
            )
        });
    }

    advice.routes.push(single);
    advice.routes.extend(spare);
    advice.routes.extend(split);
    advice
}

/// What each account's agent reported over that account's allowance window.
/// Usage is known per agent only, so accounts on the same agent share it.
pub fn used(ledger: &Ledger, store: &Store) -> Vec<Totals> {
    ledger
        .accounts
        .iter()
        .map(|account| match account.plan {
            Plan::Allowance { window_hours, .. } => store
                .window(window_hours)
                .get(&account.agent)
                .copied()
                .unwrap_or_default(),
            Plan::PayPerUse { .. } => Totals::default(),
        })
        .collect()
}

/// A count in short form: 950, 34k, 1.2M.
pub fn short(n: f64) -> String {
    if n <= 0.0 {
        "0".to_string()
    } else if n >= 1_000_000.0 {
        format!("{:.1}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.0}k", n / 1_000.0)
    } else {
        format!("{n:.0}")
    }
}

/// An allowance window in words.
pub fn window_words(hours: u32) -> String {
    if hours % 24 == 0 && hours >= 48 {
        format!("{} days", hours / 24)
    } else {
        format!("{hours} hours")
    }
}

/// One step of a route in a line.
pub fn describe(ledger: &Ledger, step: &Step) -> String {
    let account = &ledger.accounts[step.account];
    let mut line = format!(
        "{} on {} ({}): about {} tokens, ",
        step.what,
        account.name,
        account.model,
        short(step.tokens)
    );
    match step.left_after {
        Some(left) => {
            line.push_str(&format!(
                "inside the allowance, {:.0}% of it left after",
                left * 100.0
            ));
            if step.over {
                line.push_str(" (uses the reserve or runs past it)");
            }
        }
        None => line.push_str(&format!("${:.2}", step.cost)),
    }
    line
}

/// What one tool hands the next. Filled in by the first tool as its last act.
pub const BRIEF: &str = "\
# Handoff brief
## Goal
What the whole task is for, in two or three sentences.
## Done so far
What now exists and where.
## Decisions
What was decided and why, so it is not decided again.
## Open questions
What is unsettled or unverified.
## Files touched
Paths, one per line.
## Next step
The one thing the next tool should do first.";

/// A route written out as the steps a relay would take. Nothing is started:
/// this build only shows the plan.
pub fn relay_plan(ledger: &Ledger, route: &Route) -> String {
    let mut out = String::from(
        "DRY RUN. Nothing is started. This is what a relay would do, in a managed \
         environment and a git worktree, after you approve the route.\n\
         No flag that widens a tool's permissions is added. Left like this, most of \
         these tools can read the worktree but cannot change it or run commands; \
         granting more is a choice you make per route.\n",
    );
    for (i, step) in route.steps.iter().enumerate() {
        let account = &ledger.accounts[step.account];
        out.push_str(&format!("\nStep {}: {}\n", i + 1, describe(ledger, step)));
        let login = matches!(account.plan, Plan::Allowance { .. });
        match relay::command(&account.agent, login) {
            Ok(command) => out.push_str(&format!("  would run: {command}\n")),
            Err(why) => out.push_str(&format!("  no command: {why}\n")),
        }
        if i + 1 < route.steps.len() {
            out.push_str(
                "  then: this tool writes the handoff brief, and the next step starts from it.\n",
            );
        }
    }
    if route.steps.len() > 1 {
        out.push_str(&format!("\nThe handoff brief:\n{BRIEF}\n"));
    }
    out
}

/// The command-line flags for the ledger and the advisor. Returns the exit
/// code when one of them was given.
pub fn cli(args: &[String]) -> Option<i32> {
    let after = |flag: &str| args.iter().position(|a| a == flag).map(|i| &args[i + 1..]);
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if !["--account-add", "--account-remove", "--ledger", "--advise"]
        .iter()
        .any(|flag| has(flag))
    {
        return None;
    }
    let mut ledger = Ledger::load();

    if let Some(rest) = after("--account-add") {
        return Some(match parse_account(rest) {
            Ok(account) if ledger.accounts.iter().any(|a| a.name == account.name) => {
                eprintln!("There is already an account called {}.", account.name);
                2
            }
            Ok(account) => {
                println!("added {}", account.name);
                ledger.accounts.push(account);
                ledger.save();
                0
            }
            Err(problem) => {
                eprintln!("{problem}");
                eprintln!(
                    "Use: --account-add <name> <agent> <model> <light|standard|heavy> \
                     payperuse <$ in per million> <$ out per million>\n\
                     or:  --account-add <name> <agent> <model> <light|standard|heavy> \
                     allowance <tokens> <window in hours>"
                );
                2
            }
        });
    }
    if let Some(rest) = after("--account-remove") {
        let before = ledger.accounts.len();
        ledger.accounts.retain(|a| Some(&a.name) != rest.first());
        if ledger.accounts.len() == before {
            eprintln!("No account has that name.");
            return Some(2);
        }
        ledger.save();
        println!("removed {}", rest[0]);
        return Some(0);
    }

    let used = used(&ledger, &Store::load());
    if has("--ledger") {
        if ledger.accounts.is_empty() {
            println!("The ledger is empty. Add an account with --account-add.");
        }
        for (account, used) in ledger.accounts.iter().zip(&used) {
            let plan = match account.plan {
                Plan::PayPerUse { in_per_mtok, out_per_mtok } => format!(
                    "pay per use, ${in_per_mtok} in and ${out_per_mtok} out per million tokens"
                ),
                Plan::Allowance { limit_tokens, window_hours } => format!(
                    "allowance of {} tokens over {}, {} reported used, {} left (estimate)",
                    short(limit_tokens),
                    window_words(window_hours),
                    short(used.tokens()),
                    short((limit_tokens - used.tokens()).max(0.0))
                ),
            };
            println!(
                "{}: {} {} ({}), {plan}",
                account.name,
                account.agent,
                account.model,
                account.tier.label()
            );
        }
        return Some(0);
    }

    let rest = after("--advise")?;
    let Some(tokens) = rest.first().and_then(|t| t.parse::<f64>().ok()).filter(|t| *t > 0.0) else {
        eprintln!("Use: --advise <tokens> [--tier light|standard|heavy] [--review] [--relay-plan]");
        return Some(2);
    };
    let tier = after("--tier")
        .and_then(|r| r.first())
        .and_then(|t| Tier::parse(t))
        .unwrap_or(Tier::Standard);
    let task = Task { tokens, tier, review: has("--review") };
    let advice = advise(&ledger, &used, task);
    for (i, route) in advice.routes.iter().enumerate() {
        println!("Route {}: {}", i + 1, route.title);
        println!("  {}", route.why);
        for step in &route.steps {
            println!("  - {}", describe(&ledger, step));
        }
        println!("  adds ${:.2} to your bill", route.cost());
        if has("--relay-plan") {
            for line in relay_plan(&ledger, route).lines() {
                println!("  | {line}");
            }
        }
    }
    for note in &advice.notes {
        println!("Note: {note}");
    }
    Some(0)
}

fn parse_account(words: &[String]) -> Result<Account, String> {
    let [name, agent, model, tier, kind, first, second, ..] = words else {
        return Err("Not enough details for an account.".to_string());
    };
    if crate::agents::kind_by_id(agent).is_none() {
        let known: Vec<&str> = crate::agents::KINDS.iter().map(|k| k.id).collect();
        return Err(format!(
            "{agent} is not an agent Maestro knows. Known: {}.",
            known.join(", ")
        ));
    }
    let tier = Tier::parse(tier).ok_or("The rating must be light, standard or heavy.")?;
    let number = |text: &String| {
        text.parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or(format!("{text} is not a number of zero or more."))
    };
    let plan = match kind.as_str() {
        "payperuse" => Plan::PayPerUse {
            in_per_mtok: number(first)?,
            out_per_mtok: number(second)?,
        },
        "allowance" => Plan::Allowance {
            limit_tokens: number(first)?,
            window_hours: second
                .parse::<u32>()
                .ok()
                .filter(|h| (1..=8760).contains(h))
                .ok_or("The window must be a whole number of hours, from 1 to 8760.")?,
        },
        _ => return Err("The plan must be payperuse or allowance.".to_string()),
    };
    Ok(Account {
        name: name.clone(),
        agent: agent.clone(),
        model: model.clone(),
        tier,
        plan,
    })
}

/// How each vendor's own tool is started for one prompt without its chat
/// window, and whether its terms let another program do that. The sources
/// behind every line are in docs/research/relay.md.
pub mod relay {
    /// The commands for one tool: on a pay-per-use key, and on a subscription
    /// login. `Err` gives the reason there is no command.
    struct Relay {
        agent: &'static str,
        with_key: Result<&'static str, &'static str>,
        with_login: Result<&'static str, &'static str>,
    }

    const RELAYS: &[Relay] = &[
        Relay {
            agent: "claude-code",
            with_key: Ok("claude -p \"<task and brief>\" --output-format json"),
            // Anthropic's page on this allows a person's own login in the
            // unmodified tool. Its consumer terms also limit automated use,
            // and do not say how the two fit together.
            with_login: Ok("claude -p \"<task and brief>\" --output-format json"),
        },
        Relay {
            agent: "codex",
            with_key: Ok("codex exec \"<task and brief>\" --json --cd <worktree>"),
            with_login: Err(
                "OpenAI's terms do not say whether another program may start Codex on a \
                 ChatGPT login, and its documents recommend an API key for automation.",
            ),
        },
        Relay {
            agent: "cursor",
            with_key: Ok("agent -p \"<task and brief>\" --output-format json --workspace <worktree>"),
            with_login: Err(
                "Cursor's terms do not say whether another program may start its agent on a \
                 subscription login, its acceptable use policy bars automated access, and its \
                 documents say to use an API key for scripts.",
            ),
        },
        Relay {
            agent: "gemini-cli",
            with_key: Ok("gemini -p \"<task and brief>\" --output-format json"),
            with_login: Err(
                "Google moved its consumer plans off Gemini CLI on 2026-06-18, and the tool \
                 that replaced it does not support being wrapped by another program on a \
                 consumer plan.",
            ),
        },
    ];

    /// The command for one step, or the reason there is none. `login` is
    /// true for an account on a subscription allowance.
    pub fn command(agent: &str, login: bool) -> Result<&'static str, &'static str> {
        match RELAYS.iter().find(|relay| relay.agent == agent) {
            Some(relay) if login => relay.with_login,
            Some(relay) => relay.with_key,
            None => Err(
                "no documented way to run this tool for one prompt without its window has \
                 been checked yet.",
            ),
        }
    }

    #[cfg(test)]
    pub fn agents() -> Vec<&'static str> {
        RELAYS.iter().map(|relay| relay.agent).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, tier: Tier, plan: Plan) -> Account {
        Account {
            name: name.to_string(),
            agent: name.to_string(),
            model: "m".to_string(),
            tier,
            plan,
        }
    }

    fn used(tokens: f64) -> Totals {
        Totals { tokens_in: tokens, ..Totals::default() }
    }

    fn ledger() -> Ledger {
        Ledger {
            accounts: vec![
                account("sub", Tier::Heavy, Plan::Allowance { limit_tokens: 1_000_000.0, window_hours: 168 }),
                account("api", Tier::Heavy, Plan::PayPerUse { in_per_mtok: 2.0, out_per_mtok: 10.0 }),
                account("cheap", Tier::Standard, Plan::PayPerUse { in_per_mtok: 0.2, out_per_mtok: 1.0 }),
            ],
            ..Ledger::default()
        }
    }

    const NONE: [Totals; 3] = [Totals { cost_usd: 0.0, tokens_in: 0.0, tokens_out: 0.0, cache_read: 0.0, cache_write: 0.0 }; 3];

    #[test]
    fn an_allowance_with_room_wins_and_pay_per_use_is_offered_beside_it() {
        let task = Task { tokens: 100_000.0, tier: Tier::Heavy, review: false };
        let advice = advise(&ledger(), &NONE, task);
        assert_eq!(advice.routes.len(), 2);
        let first = &advice.routes[0];
        assert_eq!((first.steps[0].account, first.cost(), first.over()), (0, 0.0, false));
        assert_eq!(first.steps[0].left_after, Some(0.9));
        // 90k input at $2 and 10k output at $10, per million.
        let second = &advice.routes[1];
        assert_eq!(second.steps[0].account, 1);
        assert!((second.cost() - 0.28).abs() < 1e-9, "{}", second.cost());
    }

    #[test]
    fn a_nearly_spent_allowance_gives_way_to_pay_per_use() {
        let task = Task { tokens: 100_000.0, tier: Tier::Heavy, review: false };
        // 850k of a million used: the task would cut into the 10% reserve.
        let advice = advise(&ledger(), &[used(850_000.0), used(0.0), used(0.0)], task);
        assert_eq!(advice.routes[0].steps[0].account, 1);
        assert_eq!(advice.routes.len(), 1);
    }

    #[test]
    fn nothing_is_proposed_above_what_the_accounts_are_rated_for() {
        let mut ledger = ledger();
        ledger.accounts.truncate(0);
        ledger.accounts.push(account("cheap", Tier::Light, Plan::PayPerUse { in_per_mtok: 0.2, out_per_mtok: 1.0 }));
        let advice = advise(&ledger, &NONE, Task { tokens: 1000.0, tier: Tier::Heavy, review: false });
        assert!(advice.routes.is_empty());
        assert_eq!(advice.notes.len(), 1);
    }

    #[test]
    fn a_split_is_offered_only_when_it_saves_or_is_asked_for() {
        let mut ledger = ledger();
        ledger.accounts.remove(0);
        // Pay-per-use only. Drafting 80% on the cheap model saves money.
        let task = Task { tokens: 1_000_000.0, tier: Tier::Heavy, review: false };
        let advice = advise(&ledger, &NONE, task);
        let split = advice.routes.last().unwrap();
        assert_eq!(split.steps.len(), 2);
        assert_eq!((split.steps[0].account, split.steps[1].account), (1, 0));
        assert!(split.cost() < advice.routes[0].cost());
        // The handoff is counted: more tokens in total than the task alone.
        assert!((split.tokens() - 1_105_000.0).abs() < 1.0, "{}", split.tokens());

        // With a roomy allowance the single route is free, so the split is dropped...
        let advice = advise(&super::tests::ledger(), &NONE, Task { tokens: 300_000.0, tier: Tier::Heavy, review: false });
        assert!(advice.routes.iter().all(|r| r.steps.len() == 1));
        assert!(advice.notes.iter().any(|n| n.contains("dropped")));
        // ...unless a review was asked for.
        let advice = advise(&super::tests::ledger(), &NONE, Task { tokens: 300_000.0, tier: Tier::Heavy, review: true });
        assert!(advice.routes.iter().any(|r| r.steps.len() == 2));
    }

    #[test]
    fn a_relay_plan_shows_commands_only_where_the_terms_allow() {
        for agent in relay::agents() {
            assert!(crate::agents::kind_by_id(agent).is_some(), "{agent}");
        }
        // Pay per use: every checked tool has a documented command.
        assert!(relay::command("codex", false).unwrap().starts_with("codex exec"));
        // A subscription login: only where the vendor's terms speak to it.
        assert!(relay::command("claude-code", true).is_ok());
        assert!(relay::command("codex", true).is_err());
        assert!(relay::command("cursor", true).is_err());
        assert!(relay::command("ollama", false).is_err());

        let mut ledger = ledger();
        ledger.accounts[0].agent = "codex".to_string();
        ledger.accounts[1].agent = "claude-code".to_string();
        let route = Route {
            title: "t",
            why: String::new(),
            steps: vec![
                Step { account: 1, what: "draft", tokens: 1000.0, cost: 0.0, left_after: None, over: false },
                Step { account: 0, what: "review", tokens: 1000.0, cost: 0.0, left_after: Some(0.5), over: false },
            ],
        };
        let plan = relay_plan(&ledger, &route);
        assert!(plan.starts_with("DRY RUN"));
        assert!(plan.contains("would run: claude -p"));
        assert!(plan.contains("no command: OpenAI's terms"));
        assert!(plan.contains("# Handoff brief"));
    }

    #[test]
    fn a_small_task_stays_in_one_tool() {
        let task = Task { tokens: 20_000.0, tier: Tier::Standard, review: true };
        let advice = advise(&ledger(), &NONE, task);
        assert!(advice.routes.iter().all(|r| r.steps.len() == 1));
        assert!(advice.notes.iter().any(|n| n.contains("stay in one tool")));
    }
}
