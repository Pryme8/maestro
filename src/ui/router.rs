//! Phase 5 in the window: the ledger of AI accounts, a task to plan, and the
//! routes the advisor proposes for it. Nothing here starts any tool.

use super::*;
use crate::agents;
use crate::router::{self, Account, Ledger, Plan, Task, Tier};

pub(super) struct RouterUi {
    ledger: Ledger,
    /// The task being planned, in thousands of tokens.
    tokens_k: f64,
    tier: Tier,
    review: bool,
    /// The relay plan on show, if any.
    plan: Option<String>,
}

impl RouterUi {
    pub(super) fn load() -> Self {
        Self {
            ledger: Ledger::load(),
            tokens_k: 200.0,
            tier: Tier::Standard,
            review: false,
            plan: None,
        }
    }
}

impl App {
    pub(super) fn router_ui(&mut self, ui: &mut Ui) {
        let used = self.crew.used(&self.router.ledger);
        let before = self.router.ledger.clone();
        let state = &mut self.router;

        ui.horizontal(|ui| {
            section_title(ui, "ACCOUNTS", "what each AI account costs, as you tell it");
        });
        ui.add_space(4.0);
        let mut remove = None;
        egui::Grid::new("accounts")
            .striped(true)
            .spacing(vec2(12.0, 6.0))
            .show(ui, |ui| {
                for head in ["Name", "Tool", "Model", "Rated", "Plan", "Price or allowance", "Used", ""] {
                    ui.label(RichText::new(head).color(DIM).size(11.0));
                }
                ui.end_row();
                for (i, account) in state.ledger.accounts.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut account.name).desired_width(120.0));
                    let tool = agents::kind_by_id(&account.agent).map_or("?", |k| k.label);
                    egui::ComboBox::from_id_salt(("tool", i))
                        .selected_text(tool)
                        .show_ui(ui, |ui| {
                            for kind in agents::KINDS {
                                ui.selectable_value(&mut account.agent, kind.id.to_string(), kind.label);
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut account.model)
                            .hint_text("model")
                            .desired_width(120.0),
                    );
                    egui::ComboBox::from_id_salt(("tier", i))
                        .selected_text(account.tier.label())
                        .show_ui(ui, |ui| {
                            for tier in Tier::ALL {
                                ui.selectable_value(&mut account.tier, tier, tier.label());
                            }
                        });

                    let mut allowance = matches!(account.plan, Plan::Allowance { .. });
                    let was = allowance;
                    egui::ComboBox::from_id_salt(("plan", i))
                        .selected_text(if allowance { "allowance" } else { "pay per use" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut allowance, false, "pay per use");
                            ui.selectable_value(&mut allowance, true, "allowance");
                        });
                    if allowance != was {
                        account.plan = if allowance {
                            Plan::Allowance { limit_tokens: 1_000_000.0, window_hours: 168 }
                        } else {
                            Plan::PayPerUse { in_per_mtok: 0.0, out_per_mtok: 0.0 }
                        };
                    }

                    ui.horizontal(|ui| match &mut account.plan {
                        Plan::PayPerUse { in_per_mtok, out_per_mtok } => {
                            for (value, word) in [(in_per_mtok, " in"), (out_per_mtok, " out")] {
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(0.0..=10_000.0)
                                        .speed(0.05)
                                        .prefix("$")
                                        .suffix(word),
                                )
                                .on_hover_text("US dollars per million tokens.");
                            }
                        }
                        Plan::Allowance { limit_tokens, window_hours } => {
                            let mut thousands = *limit_tokens / 1000.0;
                            ui.add(
                                egui::DragValue::new(&mut thousands)
                                    .range(0.0..=10_000_000.0)
                                    .speed(10.0)
                                    .suffix("k tokens"),
                            )
                            .on_hover_text(
                                "Your estimate of what the subscription allows over the window. \
                                 Vendors do not publish this figure.",
                            );
                            *limit_tokens = thousands * 1000.0;
                            ui.add(
                                egui::DragValue::new(window_hours)
                                    .range(1..=8760)
                                    .prefix("every ")
                                    .suffix(" h"),
                            )
                            .on_hover_text("The rolling window, in hours. A week is 168.");
                        }
                    });

                    match account.plan {
                        Plan::Allowance { limit_tokens, .. } => {
                            let spent = used.get(i).map_or(0.0, |t| t.tokens());
                            let share = if limit_tokens > 0.0 { spent / limit_tokens } else { 0.0 };
                            ui.label(
                                RichText::new(format!(
                                    "{} of {} ({:.0}%)",
                                    router::short(spent),
                                    router::short(limit_tokens),
                                    share * 100.0
                                ))
                                .color(heat(share as f32 * 100.0))
                                .size(11.0),
                            )
                            .on_hover_text(
                                "What this tool reported over the window. It is counted per tool, \
                                 so two accounts on the same tool show the same figure, and use \
                                 from other devices is not seen.",
                            );
                        }
                        Plan::PayPerUse { .. } => {
                            ui.label(RichText::new("-").color(DIM).size(11.0));
                        }
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        if let Some(i) = remove {
            state.ledger.accounts.remove(i);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("Add account").clicked() {
                state.ledger.accounts.push(Account {
                    name: format!("account {}", state.ledger.accounts.len() + 1),
                    agent: agents::KINDS[0].id.to_string(),
                    model: String::new(),
                    tier: Tier::Standard,
                    plan: Plan::PayPerUse { in_per_mtok: 0.0, out_per_mtok: 0.0 },
                });
            }
            ui.add_space(12.0);
            for (value, label, hint) in [
                (
                    &mut state.ledger.output_share,
                    "output share",
                    "The share of a task's tokens that is output. Used to price pay-per-use work.",
                ),
                (
                    &mut state.ledger.reserve,
                    "reserve",
                    "The share of each allowance the advisor will not plan to use.",
                ),
            ] {
                ui.label(RichText::new(label).color(DIM).size(11.0));
                let mut percent = *value * 100.0;
                ui.add(egui::DragValue::new(&mut percent).range(0.0..=90.0).speed(0.5).suffix("%"))
                    .on_hover_text(hint);
                *value = percent / 100.0;
            }
        });
        ui.label(
            RichText::new(
                "Maestro does not know any vendor's prices or limits. Every figure here is \
                 yours, and allowance figures are estimates.",
            )
            .color(DIM)
            .size(10.0),
        );
        if !same(&before, &state.ledger) {
            state.ledger.save();
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            section_title(ui, "TASK", "what you are about to hand to an agent");
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut state.tokens_k)
                    .range(1.0..=100_000.0)
                    .speed(5.0)
                    .suffix("k tokens"),
            )
            .on_hover_text("Your estimate of the whole task, input and output together.");
            ui.label(RichText::new("needs a model rated").color(DIM).size(11.0));
            egui::ComboBox::from_id_salt("task tier")
                .selected_text(state.tier.label())
                .show_ui(ui, |ui| {
                    for tier in Tier::ALL {
                        ui.selectable_value(&mut state.tier, tier, tier.label());
                    }
                });
            ui.checkbox(&mut state.review, "have a second tool review it");
        });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            section_title(ui, "ROUTES", "ways to do it, cheapest within your rules");
        });
        ui.add_space(4.0);
        let task = Task {
            tokens: state.tokens_k * 1000.0,
            tier: state.tier,
            review: state.review,
        };
        let advice = router::advise(&state.ledger, &used, task);
        for route in &advice.routes {
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(1.0, if route.over() { AMBER } else { LINE }))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width((ui.available_width() - 2.0).min(760.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(route.title).strong().size(13.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("adds ${:.2} to your bill", route.cost()))
                                    .color(TEAL)
                                    .size(12.0),
                            );
                        });
                    });
                    ui.label(RichText::new(&route.why).color(DIM).size(11.0));
                    for step in &route.steps {
                        ui.label(
                            RichText::new(router::describe(&state.ledger, step)).size(11.0),
                        );
                    }
                    if ui
                        .small_button("Show relay plan")
                        .on_hover_text("What a relay would run for this route. Nothing is started.")
                        .clicked()
                    {
                        state.plan = Some(router::relay_plan(&state.ledger, route));
                    }
                });
            ui.add_space(8.0);
        }
        for note in &advice.notes {
            ui.label(RichText::new(note).color(AMBER).size(11.0));
        }
        ui.label(
            RichText::new(
                "The advisor cannot judge how good an answer will be. It ranks by cost and by \
                 what is left of each allowance, using the ratings you gave.",
            )
            .color(DIM)
            .size(10.0),
        );

        if let Some(text) = &state.plan {
            let mut open = true;
            let mut text = text.clone();
            egui::Window::new("Relay plan")
                .open(&mut open)
                .collapsible(false)
                .default_width(620.0)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            if !open {
                state.plan = None;
            }
        }
    }
}

/// Whether two ledgers would save as the same file.
fn same(a: &Ledger, b: &Ledger) -> bool {
    a.accounts == b.accounts && a.output_share == b.output_share && a.reserve == b.reserve
}
