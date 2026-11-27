//! The setup banner and guide: a strip across every page for each agent tool
//! that is on this computer but cannot be used yet, and a window that walks
//! through the steps. A step with a line to run can be run from here, in a
//! terminal the user can see, once they have seen the line and said yes.

use super::*;
use crate::setup::{self, Guide, Mark};

pub(super) struct SetupUi {
    /// The tool whose guide is open.
    open: Option<String>,
    /// The banner was put away until Maestro next starts.
    hidden: bool,
    /// Tools the user asked not to be reminded about. Saved.
    ignored: Vec<String>,
    problem: Option<String>,
    /// A line the user asked Maestro to run, waiting for their yes.
    confirm: Option<&'static str>,
    /// When a step was last run from here. The machine is looked at again by
    /// itself for a while afterwards, so a finished step ticks without a click.
    ran: Option<std::time::Instant>,
    last_check: Option<std::time::Instant>,
}

impl SetupUi {
    pub(super) fn load(open: Option<String>) -> Self {
        Self {
            open,
            hidden: false,
            ignored: setup::load_ignored(),
            problem: None,
            confirm: None,
            ran: None,
            last_check: None,
        }
    }
}

impl SetupUi {
    /// Open one tool's guide.
    pub(super) fn show(&mut self, id: &str) {
        self.open = Some(id.to_string());
    }
}

fn steps_left(guide: &Guide) -> String {
    match guide.left() {
        1 => "1 step left".to_string(),
        n => format!("{n} steps left"),
    }
}

impl App {
    /// Draw the banner under the page tabs, and the guide if one is open.
    pub(super) fn setup_ui(&mut self, ctx: &egui::Context) {
        let services = self.lv.services_now();
        let guides = setup::guides(&services, &self.setup.ignored);

        if !guides.is_empty() && !self.setup.hidden {
            egui::TopBottomPanel::top("setup-banner")
                .frame(egui::Frame::new().fill(Color32::from_rgb(58, 44, 16)).inner_margin(Margin::symmetric(14, 6)))
                .show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("NOT SET UP").color(AMBER).strong().size(12.0));
                        for guide in &guides {
                            ui.label(RichText::new(&guide.headline).color(TEXT).size(12.0));
                            if ui
                                .button(RichText::new(format!("Set up {}  ·  {}", guide.name, steps_left(guide))).color(AMBER))
                                .on_hover_text("Shows each step, which are done, and what to do next. Maestro can do most of them for you.")
                                .clicked()
                            {
                                self.setup.open = Some(guide.id.to_string());
                            }
                            ui.add_space(10.0);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .small_button("Not now")
                                .on_hover_text("Puts this away until Maestro next starts.")
                                .clicked()
                            {
                                self.setup.hidden = true;
                            }
                        });
                    });
                });
        }

        // The guide stays open after its last step is done, to say so.
        let Some(id) = self.setup.open.clone() else { return };
        let Some(service) = services.iter().find(|s| s.id == id) else { return };
        let guide = setup::guide(service);
        let no_agent = self.lv.agent_block();
        let checking = self.lv.checking_now();
        let mut open = true;
        let mut check = false;
        let mut ignore = false;
        let mut sign_in: Option<&'static str> = None;
        let mut run_now: Option<&'static str> = None;
        let mut helper = false;
        let setup_state = &mut self.setup;
        egui::Window::new(format!("Set up {}", service.name))
            .id(egui::Id::new("setup-guide"))
            .open(&mut open)
            .collapsible(false)
            .default_width(580.0)
            .show(ctx, |ui| {
                let Some(guide) = &guide else {
                    ui.label(RichText::new(format!("{} is set up. Nothing is left to do.", service.name)).color(TEAL).size(14.0));
                    return;
                };
                ui.label(RichText::new(&guide.headline).size(14.0));
                ui.add_space(6.0);
                // The first step still open is the one to look at.
                let next = guide.steps.iter().position(|s| matches!(s.mark, Mark::ToDo | Mark::NotChecked));
                for (at, step) in guide.steps.iter().enumerate() {
                    let (sign, colour) = match step.mark {
                        Mark::Done => ("✔".to_string(), TEAL),
                        Mark::ToDo => (format!("{}", at + 1), AMBER),
                        Mark::NotChecked => ("?".to_string(), DIM),
                        Mark::OnMaestro => ("…".to_string(), DIM),
                    };
                    let is_next = next == Some(at);
                    egui::Frame::new()
                        .fill(if is_next { PANEL_HI } else { PANEL })
                        .stroke(Stroke::new(1.0, if is_next { AMBER } else { LINE }))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(sign).color(colour).strong().size(15.0));
                                ui.label(RichText::new(&step.title).strong().size(13.0));
                                let note = match step.mark {
                                    Mark::Done => "done",
                                    Mark::ToDo if is_next => "do this next",
                                    Mark::ToDo => "to do",
                                    Mark::NotChecked => "Maestro cannot check this",
                                    Mark::OnMaestro => "waiting on Maestro, not on you",
                                };
                                ui.label(RichText::new(note).color(colour).size(11.0));
                            });
                            ui.label(RichText::new(&step.detail).color(DIM).size(12.0));
                            if step.mark == Mark::Done {
                                return;
                            }
                            for (text, what) in [(step.command.as_deref(), "Copy the line"), (step.link, "Copy the address")] {
                                let Some(text) = text else { continue };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(text).monospace().color(TEXT).size(12.0));
                                    if ui.small_button(what).clicked() {
                                        ui.ctx().copy_text(text.to_string());
                                    }
                                });
                            }
                            if let Some(line) = step.run {
                                if setup_state.confirm == Some(line) {
                                    ui.label(
                                        RichText::new("Maestro will open a terminal you can see and run exactly this line in it:")
                                            .color(AMBER)
                                            .size(12.0),
                                    );
                                    ui.label(RichText::new(line).monospace().size(12.0));
                                    ui.horizontal(|ui| {
                                        if ui.button("Yes, run it").clicked() {
                                            run_now = Some(line);
                                        }
                                        if ui.button("Cancel").clicked() {
                                            setup_state.confirm = None;
                                        }
                                    });
                                } else if ui
                                    .button(RichText::new("Do it for me").color(TEAL))
                                    .on_hover_text("Shows the exact line first. Nothing runs until you say yes.")
                                    .clicked()
                                {
                                    setup_state.confirm = Some(line);
                                }
                            }
                            if let Some(command) = step.sign_in {
                                if ui
                                    .button(RichText::new("Sign in").color(TEAL))
                                    .on_hover_text(format!("Opens a terminal running `{command}`."))
                                    .clicked()
                                {
                                    sign_in = Some(command);
                                }
                            }
                        });
                    ui.add_space(4.0);
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if checking {
                        ui.spinner();
                        ui.label(RichText::new("Checking…").color(DIM));
                    } else if ui
                        .button("Check again")
                        .on_hover_text("Looks at this computer again and ticks off what is now done.")
                        .clicked()
                    {
                        check = true;
                    }
                    if ui
                        .add_enabled(no_agent.is_none(), egui::Button::new("Have an agent walk me through it"))
                        .on_hover_text(
                            "Starts a Claude Code session with these steps. It explains each one and asks before \
                             every command, on the Live page, where you can also answer it. It uses your plan.",
                        )
                        .on_disabled_hover_text(no_agent.clone().unwrap_or_default())
                        .clicked()
                    {
                        helper = true;
                    }
                    if ui
                        .small_button(format!("Stop reminding me about {}", guide.name))
                        .on_hover_text("Takes it off the banner for good. It stays under Services on the Live page.")
                        .clicked()
                    {
                        ignore = true;
                    }
                });
                if setup_state.ran.is_some() {
                    ui.label(
                        RichText::new("Finish in the terminal that opened. Maestro keeps looking and ticks the step when it is done.")
                            .color(DIM)
                            .size(11.0),
                    );
                }
                if let Some(problem) = &setup_state.problem {
                    ui.label(RichText::new(problem).color(RED));
                }
            });

        let now = std::time::Instant::now();
        if let Some(command) = sign_in {
            self.setup.problem = crate::services::open_sign_in(command, &[]).err();
            self.setup.ran = Some(now);
        }
        if let Some(line) = run_now {
            self.setup.confirm = None;
            self.setup.problem = crate::services::run_setup_line(line).err();
            if self.setup.problem.is_none() {
                self.setup.ran = Some(now);
                self.ctl.note(format!("Setup: ran `{line}` in a terminal, on your click"), true, None);
            }
        }
        if helper {
            if let Some(guide) = &guide {
                let folder = crate::paths::data_dir().join("setup");
                let _ = std::fs::create_dir_all(&folder);
                let wake = ctx.clone();
                match self.lv.start_helper(&folder, &setup::helper_brief(guide), move || wake.request_repaint()) {
                    Ok(()) => {
                        self.page = Page::Live;
                        self.setup.ran = Some(now);
                    }
                    Err(err) => self.setup.problem = Some(err),
                }
            }
        }
        // For five minutes after a step was started from here, look again every few seconds.
        if let Some(ran) = self.setup.ran {
            if ran.elapsed() > Duration::from_secs(300) {
                self.setup.ran = None;
            } else {
                if !checking && self.setup.last_check.is_none_or(|at| at.elapsed() > Duration::from_secs(4)) {
                    self.setup.last_check = Some(now);
                    check = true;
                }
                ctx.request_repaint_after(Duration::from_secs(1));
            }
        }
        if check {
            self.lv.check_again(ctx);
        }
        if ignore {
            self.setup.ignored.push(id);
            setup::save_ignored(&self.setup.ignored);
            open = false;
        }
        if !open {
            self.setup.open = None;
            self.setup.problem = None;
            self.setup.confirm = None;
            self.setup.ran = None;
        }
    }
}
