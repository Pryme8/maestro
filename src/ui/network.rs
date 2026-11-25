//! The network page: which programs are connected to where, and which ports
//! this computer is listening on. Read-only; acting on a program is done from
//! its details, which a click here opens.

use super::*;
use crate::netmap::{self, Reach};

pub(super) struct NetUi {
    filter: String,
    /// Also list connections that never leave this computer.
    show_own: bool,
    /// Also list ports only this computer can reach.
    show_closed: bool,
}

impl NetUi {
    pub(super) fn new() -> Self {
        Self {
            filter: String::new(),
            show_own: false,
            show_closed: false,
        }
    }
}

/// Rows shown per program before the rest are folded into a count.
const ROWS: usize = 40;

fn reach_colour(reach: Reach) -> Color32 {
    match reach {
        Reach::Public => AMBER,
        Reach::Local => TEAL,
        Reach::Own => DIM,
    }
}

impl App {
    pub(super) fn network_ui(&mut self, ui: &mut Ui, snap: &Snapshot) {
        let map = netmap::map(snap);
        section_title(ui, "COMMS", "who this computer is talking to");
        ui.label(
            RichText::new(format!(
                "{} programs connected  ·  {} destinations on the internet  ·  {} ports other computers can reach",
                map.talkers.len(),
                map.public(),
                map.open()
            ))
            .size(13.0),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.net.filter)
                    .hint_text("filter by program, name or address")
                    .desired_width(260.0),
            );
            if ui
                .checkbox(&mut self.names.on, "Look up names")
                .on_hover_text(
                    "Asks the DNS server this computer already uses for the name registered \
                     to each address. That server sees which addresses were asked about. \
                     Nothing else is sent.",
                )
                .changed()
            {
                self.ctl.journal.no_lookups = !self.names.on;
                self.ctl.journal.save();
            }
            ui.checkbox(&mut self.net.show_own, "Show connections inside this computer");
        });
        ui.label(
            RichText::new(
                "A name is what the address's owner registered for it, which is often the hosting \
                 company and not the site. The owner shown is read from that name. How much each \
                 connection carries is not measured yet.",
            )
            .color(DIM)
            .size(11.0),
        );
        ui.add_space(8.0);

        let needle = self.net.filter.trim().to_lowercase();
        let mut select = None;
        let mut shown = 0;
        for talker in &map.talkers {
            let rows: Vec<(&netmap::Dest, Option<String>)> = talker
                .dests
                .iter()
                .filter(|d| self.net.show_own || d.reach != Reach::Own)
                .map(|d| (d, self.names.get(&d.addr)))
                .filter(|(d, name)| {
                    needle.is_empty()
                        || talker.program.to_lowercase().contains(&needle)
                        || d.addr.contains(&needle)
                        || name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            shown += 1;
            let title = format!(
                "{}   {} on the internet, {} nearer",
                talker.program,
                rows.iter().filter(|(d, _)| d.reach == Reach::Public).count(),
                rows.iter().filter(|(d, _)| d.reach != Reach::Public).count(),
            );
            egui::CollapsingHeader::new(RichText::new(title).size(13.0))
                .id_salt(("talker", &talker.program))
                .default_open(talker.public > 0)
                .show(ui, |ui| {
                    if ui
                        .small_button("Details and actions")
                        .on_hover_text("Pause, end or block this program.")
                        .clicked()
                    {
                        select = Some(talker.program.clone());
                    }
                    egui::Grid::new(("dests", &talker.program))
                        .striped(true)
                        .spacing(vec2(16.0, 4.0))
                        .show(ui, |ui| {
                            for head in ["Where", "Name", "Owner", "Address", "Port", ""] {
                                ui.label(RichText::new(head).color(DIM).size(11.0));
                            }
                            ui.end_row();
                            for (d, name) in rows.iter().take(ROWS) {
                                ui.label(
                                    RichText::new(d.reach.label())
                                        .color(reach_colour(d.reach))
                                        .size(11.0),
                                );
                                ui.label(
                                    RichText::new(clip(name.as_deref().unwrap_or("-"), 46)).size(11.0),
                                )
                                .on_hover_text(name.as_deref().unwrap_or("No name is registered, or it has not come back yet."));
                                let owner = name.as_deref().and_then(netmap::owner).unwrap_or("-");
                                ui.label(RichText::new(owner).size(11.0));
                                ui.label(RichText::new(&d.addr).monospace().size(11.0));
                                let port = match netmap::service(d.port) {
                                    Some(what) => format!("{}  {what}", d.port),
                                    None => d.port.to_string(),
                                };
                                ui.label(RichText::new(port).size(11.0));
                                let count = if d.count > 1 {
                                    format!("x{}", d.count)
                                } else {
                                    String::new()
                                };
                                ui.label(RichText::new(count).color(DIM).size(11.0));
                                ui.end_row();
                            }
                        });
                    if rows.len() > ROWS {
                        ui.label(
                            RichText::new(format!("and {} more", rows.len() - ROWS))
                                .color(DIM)
                                .size(11.0),
                        );
                    }
                });
        }
        if shown == 0 {
            ui.label(RichText::new("Nothing matches.").color(DIM));
        }

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            section_title(ui, "LISTENING", "ports waiting for something to connect");
            ui.add_space(10.0);
            ui.checkbox(&mut self.net.show_closed, "Show ports only this computer can reach");
        });
        ui.label(
            RichText::new(
                "\"Other computers\" means the port is not tied to this computer alone. The \
                 firewall may still turn callers away; firewall rules are not read yet.",
            )
            .color(DIM)
            .size(11.0),
        );
        ui.add_space(4.0);
        egui::Grid::new("listeners")
            .striped(true)
            .spacing(vec2(16.0, 4.0))
            .show(ui, |ui| {
                for head in ["Port", "Kind", "Used for", "Program", "Who can reach it"] {
                    ui.label(RichText::new(head).color(DIM).size(11.0));
                }
                ui.end_row();
                for l in &map.listeners {
                    if !l.open && !self.net.show_closed {
                        continue;
                    }
                    if !needle.is_empty()
                        && !l.program.to_lowercase().contains(&needle)
                        && !l.port.to_string().contains(&needle)
                    {
                        continue;
                    }
                    ui.label(RichText::new(l.port.to_string()).monospace().size(11.0));
                    ui.label(RichText::new(l.protocol).color(DIM).size(11.0));
                    ui.label(RichText::new(netmap::service(l.port).unwrap_or("-")).size(11.0));
                    if ui.link(RichText::new(clip(&l.program, 34)).size(11.0)).clicked() {
                        select = Some(l.program.clone());
                    }
                    let (text, colour) = if l.open {
                        ("other computers", AMBER)
                    } else {
                        ("this computer only", DIM)
                    };
                    ui.label(RichText::new(text).color(colour).size(11.0));
                    ui.end_row();
                }
            });

        if select.is_some() {
            self.selected = select;
        }
    }
}
