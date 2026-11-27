//! The Optimize page: a report on this machine, read by an agent that
//! advises and changes nothing.

use super::*;
use crate::live::{self, Run};
use crate::optimize::{self, Finding, Program};

#[derive(Default)]
pub(super) struct OptimizeUi {
    /// The part of the report the system has to be asked for, on its way.
    asking: Option<Receiver<String>>,
    machine: Option<String>,
    /// The report as it will be sent, or as it was.
    report: String,
    /// The tool that does the reading.
    tool: live::Tool,
    /// The model the analysis runs on. Empty means the tool's own setting.
    model: String,
    run: Option<Run>,
    started_ms: u64,
    outcome: Option<Outcome>,
    problem: Option<String>,
    /// Asked for on the command line: run on this model once the report is ready.
    auto: Option<String>,
    /// A run was asked for on the command line, so its answer is printed too.
    wanted: bool,
}

impl OptimizeUi {
    pub(super) fn start(auto: Option<String>) -> Self {
        Self { wanted: auto.is_some(), auto, ..Default::default() }
    }

    /// Nothing asked for on the command line is still on its way.
    pub(super) fn settled(&self) -> bool {
        !self.wanted || self.outcome.is_some() || self.problem.is_some()
    }
}

enum Outcome {
    Findings(Vec<Finding>, Option<f64>),
    /// The agent answered in words instead of the list asked for.
    Words(String, Option<f64>),
}

fn level_colour(level: &str) -> Color32 {
    match level {
        "high" => RED,
        "medium" => AMBER,
        "low" => TEXT,
        _ => DIM,
    }
}

impl App {
    pub(super) fn optimize_ui(&mut self, ui: &mut Ui, snap: &Snapshot, groups: &[Group<'_>]) {
        let (op, lv) = (&mut self.op, &self.lv);
        let now = live::now_ms();
        section_title(ui, "OPTIMIZE", "an agent reads a report on this machine and advises");
        ui.label(
            RichText::new(
                "Maestro gathers a report: the firmware, the drivers, the settings that matter, and which \
                 programs are stale or heavy. An agent you are signed in to reads it by Maestro's rules and \
                 lists what is worth changing. It only advises: it is given the report and nothing else, it \
                 cannot run commands or touch files, and Maestro changes nothing by itself. The report goes \
                 to the agent's maker under your sign-in, so it is shown below first. It holds no user name, \
                 computer name, serial number or file path.",
            )
            .color(DIM)
            .size(11.0),
        );
        ui.add_space(6.0);

        // The slow half of the report is read once, off this thread.
        if op.machine.is_none() && op.asking.is_none() {
            let (send, receive) = sync_channel(1);
            let wake = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = send.send(optimize::machine());
                wake.request_repaint();
            });
            op.asking = Some(receive);
        }
        if let Some(receive) = &op.asking {
            if let Ok(text) = receive.try_recv() {
                op.machine = Some(text);
                op.asking = None;
            }
        }
        let programs = || -> Vec<Program> {
            groups
                .iter()
                .map(|g| Program {
                    name: g.name.to_string(),
                    processes: g.procs.len(),
                    cpu: g.cpu,
                    mem: g.mem,
                    gpu_mem: g.gpu_mem,
                    class: g.class.label(),
                    stale: g.stale.map(|s| s.short()),
                    paused: g.paused,
                })
                .collect()
        };
        // Until it is sent, the report follows the machine as it is now.
        if op.run.is_none() && op.outcome.is_none() {
            if let Some(machine) = &op.machine {
                op.report = format!("{}{machine}", optimize::measured(snap, &programs()));
            }
        }

        // A run in progress: take its answer when it comes.
        if let Some(run) = op.run.as_mut() {
            if let Some((ok, cost, text)) = lv.answer_of(&run.id) {
                if op.wanted {
                    println!("{text}");
                }
                if ok {
                    op.outcome = Some(match optimize::read_findings(&text) {
                        Ok(found) => Outcome::Findings(found, cost),
                        Err(words) => Outcome::Words(words, cost),
                    });
                } else {
                    op.problem = Some(format!("The agent could not finish: {}", clip(&text, 300)));
                }
                run.stop();
                op.run = None;
            } else if !run.running() {
                op.problem = Some("The agent stopped without answering. Its steps are on the Live page.".to_string());
                op.run = None;
            }
        }

        // If the chosen tool cannot be used and the other can, the other is chosen.
        if op.run.is_none() && lv.block_for(op.tool).is_some() {
            for other in [live::Tool::Claude, live::Tool::Cursor] {
                if lv.block_for(other).is_none() {
                    op.tool = other;
                }
            }
        }
        let block = lv.block_for(op.tool);
        ui.horizontal(|ui| {
            if let Some(run) = op.run.as_mut() {
                ui.spinner();
                ui.label(format!("The agent is reading the report. {} seconds so far.", now.saturating_sub(op.started_ms) / 1000));
                if ui.button("Stop").clicked() {
                    run.stop();
                    op.run = None;
                }
                ui.ctx().request_repaint_after(Duration::from_millis(500));
                return;
            }
            let ready = block.is_none() && op.machine.is_some();
            let label = if op.outcome.is_some() { "Run the analysis again" } else { "Run the analysis" };
            let button = ui
                .add_enabled(ready, egui::Button::new(label))
                .on_hover_text(
                    "Sends the report below to the chosen tool under your sign-in. It uses your plan like \
                     any other session, and its steps show on the Live page.",
                )
                .on_disabled_hover_text(block.clone().unwrap_or_else(|| "The machine is still being read.".to_string()));
            let auto = if ready { op.auto.take() } else { None };
            if let Some(model) = &auto {
                op.model = model.clone();
            }
            if button.clicked() || auto.is_some() {
                op.outcome = None;
                op.problem = None;
                if let Some(machine) = &op.machine {
                    op.report = format!("{}{machine}", optimize::measured(snap, &programs()));
                }
                let folder = crate::paths::data_dir().join("optimize");
                let _ = std::fs::create_dir_all(&folder);
                let wake = ui.ctx().clone();
                // The model names offered are Claude Code's; Cursor uses its own setting.
                let model = if op.tool == live::Tool::Claude { op.model.as_str() } else { "" };
                match lv.start_advice(op.tool, &folder, &optimize::prompt(&op.report), model, move || wake.request_repaint()) {
                    Ok(run) => {
                        op.run = Some(run);
                        op.started_ms = now;
                    }
                    Err(err) => op.problem = Some(err),
                }
            }
            ui.label(RichText::new("Read by").color(DIM).size(11.0));
            for tool in [live::Tool::Claude, live::Tool::Cursor] {
                let why = lv.block_for(tool);
                ui.add_enabled_ui(why.is_none(), |ui| ui.selectable_value(&mut op.tool, tool, tool.label()))
                    .response
                    .on_disabled_hover_text(why.unwrap_or_default());
            }
            if op.tool == live::Tool::Cursor {
                return;
            }
            ui.label(RichText::new("Model").color(DIM).size(11.0));
            let shown = if op.model.is_empty() { "Claude Code's own setting".to_string() } else { op.model.clone() };
            egui::ComboBox::from_id_salt("optimize-model").selected_text(shown).width(190.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut op.model, String::new(), "Claude Code's own setting");
                for (name, what) in live::MODELS {
                    ui.selectable_value(&mut op.model, name.to_string(), format!("{name}: {what}"));
                }
            });
            if op.asking.is_some() {
                ui.spinner();
                ui.label(RichText::new("Reading firmware, drivers and settings…").color(DIM).size(11.0));
            } else if ui
                .small_button("Read the machine again")
                .on_hover_text("Asks the system for its firmware, drivers and settings afresh.")
                .clicked()
            {
                op.machine = None;
                op.outcome = None;
            }
        });
        if let Some(problem) = &op.problem {
            ui.label(RichText::new(problem).color(RED));
        }
        ui.add_space(6.0);

        let mut show: Option<String> = None;
        match &op.outcome {
            Some(Outcome::Findings(found, cost)) => {
                let spent = cost.map(|c| format!("  ·  cost ${c:.4} by the tool's own estimate")).unwrap_or_default();
                if found.is_empty() {
                    ui.label(RichText::new(format!("The agent found nothing worth changing.{spent}")).color(TEAL));
                } else {
                    ui.label(
                        RichText::new(format!("{} findings, largest gain first.{spent}", found.len())).color(DIM).size(11.0),
                    );
                }
                for finding in found {
                    egui::Frame::new()
                        .fill(PANEL)
                        .stroke(Stroke::new(1.0, LINE))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width().min(860.0));
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&finding.title).strong().size(14.0));
                                ui.label(RichText::new(&finding.area).color(TEAL).size(11.0));
                                let gain = if finding.gain == "high" { TEAL } else { level_colour(&finding.gain) };
                                ui.label(RichText::new(format!("gain: {}", finding.gain)).color(gain).size(11.0));
                                ui.label(
                                    RichText::new(format!("risk: {}", finding.risk))
                                        .color(level_colour(&finding.risk))
                                        .size(11.0),
                                );
                            });
                            ui.label(RichText::new(&finding.why).size(12.0));
                            ui.label(RichText::new(&finding.how).color(DIM).size(12.0));
                            if !finding.program.is_empty() && groups.iter().any(|g| g.name == finding.program) {
                                if ui
                                    .small_button(format!("Show {} on Base", clip(&finding.program, 40)))
                                    .on_hover_text("Opens the program's details, where it can be paused or ended.")
                                    .clicked()
                                {
                                    show = Some(finding.program.clone());
                                }
                            }
                        });
                    ui.add_space(4.0);
                }
                ui.label(
                    RichText::new(
                        "This is advice from an AI model reading a report. Check a firmware or driver step \
                         against the maker's own page before you take it.",
                    )
                    .color(DIM)
                    .size(11.0),
                );
            }
            Some(Outcome::Words(words, cost)) => {
                let spent = cost.map(|c| format!("  ·  cost ${c:.4}")).unwrap_or_default();
                ui.label(
                    RichText::new(format!("The agent answered in words instead of a list.{spent}")).color(AMBER).size(11.0),
                );
                ui.label(RichText::new(words).size(12.0));
            }
            None => {}
        }

        ui.add_space(6.0);
        let title = if op.outcome.is_some() || op.run.is_some() { "The report that was sent" } else { "The report that will be sent" };
        egui::CollapsingHeader::new(RichText::new(title).size(13.0))
            .id_salt("optimize-report")
            .default_open(op.outcome.is_none())
            .show(ui, |ui| {
                if op.report.is_empty() {
                    ui.label(RichText::new("Being gathered…").color(DIM));
                } else {
                    egui::ScrollArea::vertical().id_salt("optimize-report-text").max_height(360.0).show(ui, |ui| {
                        ui.label(RichText::new(&op.report).monospace().size(11.0));
                    });
                }
            });

        if let Some(name) = show {
            self.selected = Some(name);
            self.page = Page::Base;
        }
    }
}
