//! The setup banner and guide: a strip across every page for each agent tool
//! that is on this computer but cannot be used yet, and a window that walks
//! through the steps.

use super::*;
use crate::setup::{self, Guide, Mark};

pub(super) struct SetupUi {
    /// The tool whose guide is open.
    open: Option<&'static str>,
    /// The banner was put away until Maestro next starts.
    hidden: bool,
    /// Tools the user asked not to be reminded about. Saved.
    ignored: Vec<String>,
    problem: Option<String>,
}

impl SetupUi {
    pub(super) fn load() -> Self {
        Self { open: None, hidden: false, ignored: setup::load_ignored(), problem: None }
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
                                .on_hover_text("Shows each step, which are done, and exactly what to do next.")
                                .clicked()
                            {
                                self.setup.open = Some(guide.id);
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
        let Some(id) = self.setup.open else { return };
        let Some(service) = services.iter().find(|s| s.id == id) else { return };
        let guide = setup::guide(service);
        let mut open = true;
        let mut check = false;
        let mut ignore = false;
        let mut sign_in: Option<&'static str> = None;
        egui::Window::new(format!("Set up {}", service.name))
            .id(egui::Id::new("setup-guide"))
            .open(&mut open)
            .collapsible(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                let Some(guide) = &guide else {
                    ui.label(RichText::new(format!("{} is set up. Nothing is left to do.", service.name)).color(TEAL).size(14.0));
                    return;
                };
                ui.label(RichText::new(&guide.headline).size(14.0));
                ui.add_space(6.0);
                // The first step still to do is the one to look at.
                let next = guide.steps.iter().position(|s| s.mark == Mark::ToDo);
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
                            if step.mark != Mark::Done || !step.detail.is_empty() {
                                ui.label(RichText::new(&step.detail).color(DIM).size(12.0));
                            }
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
                            if let Some(command) = step.sign_in {
                                if ui
                                    .button("Sign in")
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
                ui.horizontal(|ui| {
                    if self.lv.checking_now() {
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
                        .small_button(format!("Stop reminding me about {}", guide.name))
                        .on_hover_text("Takes it off the banner for good. It stays under Services on the Live page.")
                        .clicked()
                    {
                        ignore = true;
                    }
                });
                if let Some(problem) = &self.setup.problem {
                    ui.label(RichText::new(problem).color(RED));
                }
            });
        if let Some(command) = sign_in {
            self.setup.problem = crate::services::open_sign_in(command, &[]).err();
        }
        if check {
            self.lv.check_again(ctx);
        }
        if ignore {
            self.setup.ignored.push(id.to_string());
            setup::save_ignored(&self.setup.ignored);
            open = false;
        }
        if !open {
            self.setup.open = None;
            self.setup.problem = None;
        }
    }
}
