//! The different ways the running programs can be drawn. All of them show the
//! same live data and all are read-only.

use super::*;
use std::collections::HashSet;

/// The ways the same running programs can be drawn.
#[derive(PartialEq, Clone, Copy)]
pub enum View {
    /// One tile per program.
    Tiles,
    /// Area is memory, colour is processor load.
    Heat,
    /// Which process started which.
    Tree,
    /// A map of programs joined to the programs they started.
    Mold,
}

impl View {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "tiles" => Some(Self::Tiles),
            "heat" => Some(Self::Heat),
            "tree" => Some(Self::Tree),
            "mold" => Some(Self::Mold),
            _ => None,
        }
    }
}

/// One program's blob in the mold view. Positions are relative to the canvas centre.
pub(super) struct Blob {
    pos: egui::Vec2,
    vel: egui::Vec2,
}

impl App {
    pub(super) fn crew(&mut self, ui: &mut Ui, snap: &Snapshot, groups: &[Group<'_>]) {
        ui.horizontal(|ui| {
            section_title(ui, "PROGRAMS", "everything running");
            ui.add_space(10.0);
            for (view, label) in [
                (View::Tiles, "Tiles"),
                (View::Heat, "Heat map"),
                (View::Tree, "Tree"),
                (View::Mold, "Map"),
            ] {
                ui.selectable_value(&mut self.view, view, label);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("filter by name")
                        .desired_width(160.0),
                );
                ui.selectable_value(&mut self.sort, Sort::Sockets, "Sockets");
                ui.selectable_value(&mut self.sort, Sort::Memory, "Memory");
                ui.selectable_value(&mut self.sort, Sort::Cpu, "CPU");
                ui.label(RichText::new("Sort by").color(DIM));
            });
        });

        // The class chips are both the legend and a filter.
        ui.horizontal(|ui| {
            ui.label(RichText::new("Show").color(DIM).size(11.0));
            ui.selectable_value(&mut self.class_filter, None, "All");
            for class in Class::ALL {
                let count = groups.iter().filter(|g| g.class == class).count();
                if count == 0 {
                    continue;
                }
                let text = RichText::new(format!("{} ({count})", class.label()))
                    .color(class_colour(class))
                    .size(11.0);
                ui.selectable_value(&mut self.class_filter, Some(class), text)
                    .on_hover_text(class.explain());
            }
            let stale = groups.iter().filter(|g| g.stale.is_some()).count();
            if stale > 0 || self.only_stale {
                ui.add_space(8.0);
                ui.toggle_value(
                    &mut self.only_stale,
                    RichText::new(format!("Stale ({stale})")).color(BLUE).size(11.0),
                )
                .on_hover_text(
                    "Programs whose window is not responding, or that have used no processor \
                     or disk for 15 minutes while Maestro has been running. Click one to end it.",
                );
            }
        });
        ui.add_space(6.0);

        let needle = self.filter.trim().to_lowercase();
        let shown: Vec<&Group<'_>> = groups
            .iter()
            .filter(|g| self.class_filter.is_none_or(|class| g.class == class))
            .filter(|g| !self.only_stale || g.stale.is_some())
            .filter(|g| needle.is_empty() || g.name.to_lowercase().contains(&needle))
            .collect();

        let clicked = match self.view {
            View::Tiles => self.tiles(ui, &shown),
            View::Heat => self.heat_map(ui, &shown),
            View::Tree => self.tree(ui, snap, &needle),
            View::Mold => self.mold_graph(ui, snap, &shown),
        };
        if let Some(name) = clicked {
            self.selected = if self.selected.as_deref() == Some(name.as_str()) {
                None
            } else {
                Some(name)
            };
        }
    }

    fn is_selected(&self, name: &str) -> bool {
        self.selected.as_deref() == Some(name)
    }

    /// One tile per program.
    fn tiles(&self, ui: &mut Ui, shown: &[&Group<'_>]) -> Option<String> {
        let mut clicked = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for group in shown {
                if tile(ui, group, self.is_selected(group.name)).clicked() {
                    clicked = Some(group.name.to_string());
                }
            }
        });
        clicked
    }

    /// A treemap: each program's area is its memory, its colour is its processor load.
    fn heat_map(&self, ui: &mut Ui, shown: &[&Group<'_>]) -> Option<String> {
        let size = vec2(ui.available_width(), ui.available_height().max(480.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let mut items: Vec<(usize, f32)> = shown
            .iter()
            .enumerate()
            .map(|(i, g)| (i, g.mem as f32))
            .filter(|(_, mem)| *mem > 0.0)
            .collect();
        items.sort_by(|a, b| b.1.total_cmp(&a.1));

        let painter = ui.painter().with_clip_rect(rect);
        let mut clicked = None;
        for (i, cell) in squarify(&items, rect) {
            let group = shown[i];
            let cell = cell.shrink(1.0);
            if cell.width() < 1.0 || cell.height() < 1.0 {
                continue;
            }
            let load = (group.cpu * 4.0 / 100.0).clamp(0.0, 1.0);
            let fill = lerp(
                Color32::from_rgb(22, 48, 52),
                heat(group.cpu * 4.0),
                load.max(0.12),
            );
            painter.rect_filled(cell, CornerRadius::same(3), fill);
            let response = ui.interact(cell, ui.id().with(("heat", group.name)), Sense::click());
            if response.hovered() || self.is_selected(group.name) {
                painter.rect_filled(cell, CornerRadius::same(3), Color32::from_white_alpha(30));
            }
            if cell.width() > 64.0 && cell.height() > 22.0 {
                let ink = if load > 0.45 { BG } else { TEXT };
                let chars = (cell.width() / 7.5) as usize;
                let text = painter.with_clip_rect(cell);
                text.text(
                    cell.min + vec2(6.0, 5.0),
                    Align2::LEFT_TOP,
                    clip(group.name, chars),
                    FontId::proportional(12.0),
                    ink,
                );
                if cell.height() > 40.0 {
                    text.text(
                        cell.min + vec2(6.0, 21.0),
                        Align2::LEFT_TOP,
                        format!("{}  {:.1}%", bytes(group.mem), group.cpu),
                        FontId::proportional(11.0),
                        ink.gamma_multiply(0.75),
                    );
                }
            }
            if response.clicked() {
                clicked = Some(group.name.to_string());
            }
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(summary(group));
        }
        clicked
    }

    /// Which process started which, with totals for everything beneath each one.
    fn tree(&self, ui: &mut Ui, snap: &Snapshot, needle: &str) -> Option<String> {
        let by_pid: HashMap<u32, &Proc> = snap.processes.iter().map(|p| (p.pid, p)).collect();
        let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
        let mut roots: Vec<&Proc> = Vec::new();
        for p in &snap.processes {
            // The system reuses process numbers, so a "parent" that started later
            // than its child is a different, unrelated program.
            let parent = p
                .parent_pid
                .and_then(|pid| by_pid.get(&pid))
                .filter(|parent| parent.pid != p.pid && parent.start_secs <= p.start_secs);
            match parent {
                Some(parent) => children.entry(parent.pid).or_default().push(p),
                None => roots.push(p),
            }
        }
        let mut totals = HashMap::new();
        for root in &roots {
            subtree_totals(root, &children, &mut totals, 0);
        }
        let weight = |p: &Proc| totals.get(&p.pid).map(|t: &(f32, u64)| t.1).unwrap_or(0);
        roots.sort_by_key(|p| std::cmp::Reverse(weight(p)));
        for kids in children.values_mut() {
            kids.sort_by_key(|p| std::cmp::Reverse(weight(p)));
        }

        let tree = Tree {
            children: &children,
            totals: &totals,
            selected: self.selected.as_deref(),
            needle,
        };
        let mut clicked = None;
        for root in roots {
            tree.node(ui, root, &mut clicked, 0);
        }
        clicked
    }

    /// A map of programs joined to the programs they started. Size is memory,
    /// fill and pulse are processor load, and the ring shows the program's class.
    fn mold_graph(
        &mut self,
        ui: &mut Ui,
        snap: &Snapshot,
        shown: &[&Group<'_>],
    ) -> Option<String> {
        const MAX_BLOBS: usize = 80;
        let size = vec2(ui.available_width(), ui.available_height().max(560.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(7, 11, 15));

        let mut nodes: Vec<&Group<'_>> = shown.to_vec();
        nodes.sort_by(|a, b| b.mem.cmp(&a.mem));
        let hidden = nodes.len().saturating_sub(MAX_BLOBS);
        nodes.truncate(MAX_BLOBS);
        let index: HashMap<&str, usize> =
            nodes.iter().enumerate().map(|(i, g)| (g.name, i)).collect();
        let radius: Vec<f32> = nodes
            .iter()
            .map(|g| (4.0 + (g.mem as f32 / 1_048_576.0).sqrt() * 0.6).clamp(5.0, 60.0))
            .collect();

        // A tendril runs from a program to each different program it started.
        let name_of: HashMap<u32, &str> = snap
            .processes
            .iter()
            .map(|p| (p.pid, p.name.as_str()))
            .collect();
        let mut edges: HashSet<(usize, usize)> = HashSet::new();
        for (child, group) in nodes.iter().enumerate() {
            for p in &group.procs {
                let parent = p
                    .parent_pid
                    .and_then(|pid| name_of.get(&pid))
                    .and_then(|name| index.get(name));
                if let Some(&parent) = parent {
                    if parent != child {
                        edges.insert((parent, child));
                    }
                }
            }
        }

        // Keep each blob's place between frames; new programs sprout on a spiral.
        self.mold.retain(|name, _| index.contains_key(name.as_str()));
        for (i, group) in nodes.iter().enumerate() {
            self.mold.entry(group.name.to_string()).or_insert_with(|| {
                let angle = i as f32 * 2.399_963;
                let reach = 40.0 + 16.0 * (i as f32).sqrt();
                Blob {
                    pos: vec2(angle.cos(), angle.sin()) * reach,
                    vel: egui::Vec2::ZERO,
                }
            });
        }
        let mut pos: Vec<egui::Vec2> = nodes.iter().map(|g| self.mold[g.name].pos).collect();
        let mut vel: Vec<egui::Vec2> = nodes.iter().map(|g| self.mold[g.name].vel).collect();

        // A small physical model: blobs push apart, tendrils pull together and
        // everything drifts toward the centre.
        let squash = (rect.width() / rect.height()).max(1.0);
        for _ in 0..2 {
            let mut push = vec![egui::Vec2::ZERO; nodes.len()];
            for i in 0..nodes.len() {
                for j in (i + 1)..nodes.len() {
                    let gap = pos[i] - pos[j];
                    let dist = gap.length().max(1.0);
                    let dir = gap / dist;
                    let clear = radius[i] + radius[j] + 14.0;
                    let force = if dist < clear {
                        (clear - dist) * 0.12
                    } else {
                        (300.0 / (dist * dist)).min(0.5)
                    };
                    push[i] += dir * force;
                    push[j] -= dir * force;
                }
            }
            for &(a, b) in &edges {
                let gap = pos[b] - pos[a];
                let dist = gap.length().max(1.0);
                let rest = radius[a] + radius[b] + 46.0;
                let pull = gap / dist * ((dist - rest) * 0.015);
                push[a] += pull;
                push[b] -= pull;
            }
            for i in 0..nodes.len() {
                push[i] -= vec2(pos[i].x, pos[i].y * squash) * 0.003;
                vel[i] = (vel[i] + push[i]) * 0.8;
                let speed = vel[i].length();
                if speed > 12.0 {
                    vel[i] *= 12.0 / speed;
                }
                pos[i] += vel[i];
            }
        }
        for (i, group) in nodes.iter().enumerate() {
            if let Some(blob) = self.mold.get_mut(group.name) {
                blob.pos = pos[i];
                blob.vel = vel[i];
            }
        }

        let centre = rect.center();
        let time = ui.input(|i| i.time) as f32;

        for &(a, b) in &edges {
            let (from, to) = (centre + pos[a], centre + pos[b]);
            let across = (to - from).rot90().normalized();
            let sway = (time * 0.8 + a as f32 * 1.7 + b as f32).sin() * 12.0;
            let bend_a = from + (to - from) * 0.33 + across * sway;
            let bend_b = from + (to - from) * 0.66 - across * sway;
            let load = nodes[b].cpu;
            let stroke = Stroke::new(
                1.5 + load.min(10.0) * 0.5,
                lerp(LINE, heat(load * 4.0), 0.55),
            );
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [from, bend_a, bend_b, to],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }

        let mut clicked = None;
        for (i, group) in nodes.iter().enumerate() {
            let at = centre + pos[i];
            let load = (group.cpu * 4.0 / 100.0).clamp(0.0, 1.0);
            let colour = heat(group.cpu * 4.0);
            let pulse =
                1.0 + 0.05 * load.max(0.15) * (time * (1.0 + group.cpu * 0.3) + i as f32).sin();
            let r = radius[i] * pulse;
            painter.circle_filled(at, r + 5.0, colour.gamma_multiply(0.15));
            painter.circle_filled(
                at,
                r,
                lerp(Color32::from_rgb(22, 48, 52), colour, load.max(0.2)),
            );
            painter.circle_stroke(at, r, Stroke::new(2.0, class_colour(group.class)));
            if self.is_selected(group.name) {
                painter.circle_stroke(at, r + 4.0, Stroke::new(2.0, TEXT));
            }
            if r > 17.0 {
                painter.text(
                    at,
                    Align2::CENTER_CENTER,
                    clip(group.name, (r / 3.4) as usize),
                    FontId::proportional(11.0),
                    if load > 0.45 { BG } else { TEXT },
                );
            }
            let hit = Rect::from_center_size(at, vec2(r * 2.0, r * 2.0)).intersect(rect);
            let response = ui.interact(hit, ui.id().with(("mold", group.name)), Sense::click());
            if response.clicked() {
                clicked = Some(group.name.to_string());
            }
            response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(summary(group));
        }

        let mut note = "lines join a program to the programs it started".to_string();
        if hidden > 0 {
            note.push_str(&format!(
                "  ·  showing the {MAX_BLOBS} largest, {hidden} smaller ones not drawn"
            ));
        }
        painter.text(
            rect.left_bottom() + vec2(10.0, -8.0),
            Align2::LEFT_BOTTOM,
            note,
            FontId::proportional(11.0),
            DIM,
        );

        // The blobs move, so keep drawing at about 30 frames a second while this view is open.
        ui.ctx().request_repaint_after(Duration::from_millis(33));
        clicked
    }
}

/// Processor share and memory for a process and everything it started.
fn subtree_totals(
    p: &Proc,
    children: &HashMap<u32, Vec<&Proc>>,
    totals: &mut HashMap<u32, (f32, u64)>,
    depth: usize,
) -> (f32, u64) {
    let (mut cpu, mut mem) = (p.cpu_percent, p.memory_bytes);
    if depth < 64 {
        for kid in children.get(&p.pid).into_iter().flatten() {
            let (c, m) = subtree_totals(kid, children, totals, depth + 1);
            cpu += c;
            mem += m;
        }
    }
    totals.insert(p.pid, (cpu, mem));
    (cpu, mem)
}

struct Tree<'a> {
    children: &'a HashMap<u32, Vec<&'a Proc>>,
    totals: &'a HashMap<u32, (f32, u64)>,
    selected: Option<&'a str>,
    needle: &'a str,
}

impl Tree<'_> {
    /// True when this process or anything beneath it matches the filter.
    fn matches(&self, p: &Proc, depth: usize) -> bool {
        self.needle.is_empty()
            || p.name.to_lowercase().contains(self.needle)
            || (depth < 64
                && self
                    .children
                    .get(&p.pid)
                    .into_iter()
                    .flatten()
                    .any(|kid| self.matches(kid, depth + 1)))
    }

    fn node(&self, ui: &mut Ui, p: &Proc, clicked: &mut Option<String>, depth: usize) {
        if depth >= 64 || !self.matches(p, depth) {
            return;
        }
        let (cpu, mem) = self.totals.get(&p.pid).copied().unwrap_or_default();
        let label = format!("{}   #{}   {:.1}%   {}", p.name, p.pid, cpu, bytes(mem));
        let selected = self.selected == Some(p.name.as_str());
        let kids = self.children.get(&p.pid).filter(|kids| !kids.is_empty());
        let Some(kids) = kids else {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                if ui.selectable_label(selected, label).clicked() {
                    *clicked = Some(p.name.clone());
                }
            });
            return;
        };
        let id = ui.make_persistent_id(("process", p.pid, p.start_secs));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, depth == 0)
            .show_header(ui, |ui| {
                if ui.selectable_label(selected, label).clicked() {
                    *clicked = Some(p.name.clone());
                }
            })
            .body(|ui| {
                for kid in kids {
                    self.node(ui, kid, clicked, depth + 1);
                }
            });
    }
}

/// Squarified treemap: split `rect` into one cell per item, with areas in
/// proportion to the values and cells kept as close to square as possible.
/// `items` must be sorted largest first.
fn squarify(items: &[(usize, f32)], mut rect: Rect) -> Vec<(usize, Rect)> {
    let total: f32 = items.iter().map(|(_, v)| v).sum();
    if total <= 0.0 || rect.area() <= 0.0 {
        return Vec::new();
    }
    let scale = rect.area() / total;
    let areas: Vec<f32> = items.iter().map(|(_, v)| v * scale).collect();
    let mut cells = Vec::with_capacity(items.len());
    let mut start = 0;
    while start < areas.len() {
        let short = rect.width().min(rect.height()).max(1.0);
        // Add items to the strip while its worst cell keeps getting squarer.
        let (mut end, mut sum, mut best) = (start, 0.0f32, f32::INFINITY);
        while end < areas.len() {
            let next = sum + areas[end];
            let thick = next / short;
            let worst = areas[start..=end]
                .iter()
                .map(|a| {
                    let ratio = thick * thick / a.max(1e-6);
                    ratio.max(1.0 / ratio.max(1e-6))
                })
                .fold(0.0f32, f32::max);
            if worst > best {
                break;
            }
            best = worst;
            sum = next;
            end += 1;
        }
        let thick = (sum / short).max(1e-6);
        if rect.width() >= rect.height() {
            let mut y = rect.min.y;
            for k in start..end {
                let h = areas[k] / thick;
                cells.push((
                    items[k].0,
                    Rect::from_min_size(pos2(rect.min.x, y), vec2(thick, h)),
                ));
                y += h;
            }
            rect.min.x += thick;
        } else {
            let mut x = rect.min.x;
            for k in start..end {
                let w = areas[k] / thick;
                cells.push((
                    items[k].0,
                    Rect::from_min_size(pos2(x, rect.min.y), vec2(w, thick)),
                ));
                x += w;
            }
            rect.min.y += thick;
        }
        start = end;
    }
    cells
}
