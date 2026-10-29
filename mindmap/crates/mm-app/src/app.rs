//! egui front end: canvas, menus, inspector, dialogs. All editing semantics live in `Model`.
use crate::model::{Action, Dir, Model};
use crate::theme::{branch_rgb, c32, on_color, PALETTE};
use eframe::egui::{
    self, epaint::CubicBezierShape, vec2, Align2, Color32, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind,
    Vec2,
};
use mm_core::{outline, templates, DiagramMode, Document, NodeId, Shape, Subtree};
use mm_io::{csv_io, json, native, Report};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

type SizeCache = Rc<RefCell<HashMap<(String, bool), (f32, f32)>>>;

const BASE_FONT: f32 = 15.0;
const ROOT_FONT: f32 = 19.0;
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 8.0;

struct Camera {
    center: Vec2,
    zoom: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FindMode {
    Jump,
    MoveTo,
}

struct Find {
    query: String,
    mode: FindMode,
    selected: usize,
    just_opened: bool,
}

struct PendingImport {
    tree: Subtree,
    mode: DiagramMode,
    warnings: Vec<String>,
    name: String,
}

#[derive(Clone)]
enum After {
    NewTemplate(usize),
    Open,
    Quit,
}

enum NodeDrag {
    Pan,
    Marquee {
        start: Pos2,
    },
    /// Dragging nodes: they follow the cursor (manual position) unless dropped onto another node (re-parent).
    Nodes {
        ids: Vec<NodeId>,
        orig: Vec<(NodeId, [f32; 2])>,
        acc: Vec2,
    },
}

pub struct MindmapApp {
    model: Model,
    cam: Camera,
    cam_target: Option<Vec2>,
    path: Option<PathBuf>,
    find: Option<Find>,
    drag: Option<NodeDrag>,
    note_buf: Option<(NodeId, String)>,
    show_inspector: bool,
    show_help: bool,
    pending_fit: bool,
    pending_import: Option<PendingImport>,
    confirm: Option<After>,
    allow_close: bool,
    last_autosave: Instant,
    last_title: String,
    last_edit_id: Option<NodeId>,
    toast: Option<(String, Instant)>,
    fonts_ready: Rc<Cell<bool>>,
    /// The edit was started by typing a character: caret goes to the end instead of select-all.
    caret_at_end: bool,
}

fn autosave_path(path: &Option<PathBuf>) -> PathBuf {
    match path {
        Some(p) => {
            let mut s = p.as_os_str().to_owned();
            s.push(".autosave");
            PathBuf::from(s)
        }
        None => std::env::temp_dir().join("mindmap-untitled.autosave"),
    }
}

impl MindmapApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let cache: SizeCache = Rc::default();
        let measure_ctx = ctx.clone();
        // Fonts only exist after egui's first pass; until then estimate sizes, and relayout once ready.
        let fonts_ready = Rc::new(Cell::new(false));
        let ready = fonts_ready.clone();
        let measure: crate::model::Measure = Box::new(move |text, is_root| {
            let size = if is_root { ROOT_FONT } else { BASE_FONT };
            if !ready.get() {
                let longest = text.lines().map(|l| l.chars().count()).max().unwrap_or(1).max(1) as f32;
                let lines = text.lines().count().max(1) as f32;
                return ((longest * size * 0.55 + 2.0 * PAD_X).max(48.0), lines * size * 1.25 + 2.0 * PAD_Y);
            }
            let key = (text.to_string(), is_root);
            if let Some(&v) = cache.borrow().get(&key) {
                return v;
            }
            let shown = if text.is_empty() { " " } else { text };
            let g = measure_ctx.fonts_mut(|f| f.layout(shown.to_string(), FontId::proportional(size), Color32::WHITE, f32::INFINITY));
            let v = ((g.size().x + 2.0 * PAD_X).max(48.0), g.size().y + 2.0 * PAD_Y);
            let mut c = cache.borrow_mut();
            if c.len() > 20_000 {
                c.clear();
            }
            c.insert(key, v);
            v
        });

        let mut doc = templates::TEMPLATES.last().map(|t| t.build()).unwrap_or_else(|| Document::new("Central idea"));
        let mut path = None;
        let mut toast = None;
        if let Some(p) = open {
            match native::load_path(&p) {
                Ok((d, _, rep)) => {
                    doc = d;
                    path = Some(p);
                    if !rep.warnings.is_empty() {
                        toast = Some((rep.warnings.join("; "), Instant::now()));
                    }
                }
                Err(e) => toast = Some((format!("Could not open {}: {e}", p.display()), Instant::now())),
            }
        } else if autosave_path(&None).exists() {
            toast = Some(("An autosave from a previous session exists: File > Recover autosave".into(), Instant::now()));
        }
        MindmapApp {
            model: Model::new(doc, measure),
            cam: Camera { center: Vec2::ZERO, zoom: 1.0 },
            cam_target: None,
            path,
            find: None,
            drag: None,
            note_buf: None,
            show_inspector: true,
            show_help: false,
            pending_fit: true,
            pending_import: None,
            confirm: None,
            allow_close: false,
            last_autosave: Instant::now(),
            last_title: String::new(),
            last_edit_id: None,
            toast,
            fonts_ready,
            caret_at_end: false,
        }
    }

    fn say(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    // ---- file operations ------------------------------------------------------

    fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.model.set_document(doc);
        self.path = path;
        self.pending_fit = true;
        self.note_buf = None;
    }

    fn new_from_template(&mut self, idx: usize) {
        if let Some(t) = templates::TEMPLATES.get(idx) {
            self.replace_document(t.build(), None);
        }
    }

    fn guarded(&mut self, after: After) {
        if self.model.is_dirty() {
            self.confirm = Some(after);
        } else {
            self.run_after(after);
        }
    }

    fn run_after(&mut self, after: After) {
        match after {
            After::NewTemplate(i) => self.new_from_template(i),
            After::Open => self.open_dialog(),
            After::Quit => self.allow_close = true,
        }
    }

    fn open_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Mindmap", &["json"]).pick_file() else { return };
        match native::load_path(&p) {
            Ok((doc, _, rep)) => {
                self.replace_document(doc, Some(p));
                if !rep.warnings.is_empty() {
                    self.say(rep.warnings.join("; "));
                }
            }
            Err(e) => self.say(format!("Open failed: {e}")),
        }
    }

    fn save(&mut self, as_new: bool) {
        let target = if as_new || self.path.is_none() {
            rfd::FileDialog::new().add_filter("Mindmap", &["json"]).set_file_name("map.mm.json").save_file()
        } else {
            self.path.clone()
        };
        let Some(p) = target else { return };
        let view = native::View { center: [self.cam.center.x, self.cam.center.y], zoom: self.cam.zoom };
        match native::save_path(&p, &self.model.ed.doc, Some(view)) {
            Ok(()) => {
                let _ = std::fs::remove_file(autosave_path(&self.path));
                self.path = Some(p.clone());
                self.model.mark_saved();
                self.say(format!("Saved {}", p.display()));
            }
            Err(e) => self.say(format!("Save failed: {e}")),
        }
    }

    fn recover(&mut self) {
        let p = autosave_path(&self.path);
        match native::load_path(&p) {
            Ok((doc, _, _)) => {
                let keep = self.path.clone();
                self.replace_document(doc, keep);
                self.say("Recovered autosave (unsaved)");
                self.model.saved_rev = u64::MAX;
            }
            Err(e) => self.say(format!("No autosave to recover: {e}")),
        }
    }

    fn autosave(&mut self) {
        if self.model.is_dirty() && self.last_autosave.elapsed() > Duration::from_secs(5) {
            if let Ok(s) = native::to_string(&self.model.ed.doc, None) {
                let _ = mm_io::write_atomic(&autosave_path(&self.path), s.as_bytes());
            }
            self.last_autosave = Instant::now();
        }
    }

    fn import_dialog(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("CSV / JSON / outline", &["csv", "tsv", "json", "md", "txt"]).pick_file() else {
            return;
        };
        let mut rep = Report::default();
        let result = std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            match ext.as_str() {
                "csv" | "tsv" => {
                    let o = csv_io::ImportOptions { trim: true, delimiter: (ext == "tsv").then_some(b'\t'), ..Default::default() };
                    csv_io::import_subtree(&bytes, &o, &mut rep).map(|t| (t, DiagramMode::MindMap)).map_err(|e| e.to_string())
                }
                "md" | "txt" => {
                    let text = mm_io::decode_text(&bytes, &mut rep);
                    Ok((outline::parse_outline_single(&text, "Imported"), DiagramMode::MindMap))
                }
                _ => {
                    let text = mm_io::decode_text(&bytes, &mut rep);
                    json::import_subtree(&text, &Default::default(), &mut rep).map_err(|e| e.to_string())
                }
            }
        });
        match result {
            Ok((tree, mode)) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.pending_import = Some(PendingImport { tree, mode, warnings: rep.warnings, name });
            }
            Err(e) => self.say(format!("Import failed: {e}")),
        }
    }

    fn export_dialog(&mut self, kind: &str) {
        let doc = &self.model.ed.doc;
        let sub = Some(self.model.primary()).filter(|&p| p != doc.root());
        let (ext, label) = match kind {
            "csv" => ("csv", "CSV"),
            "md" => ("md", "Markdown outline"),
            _ => ("json", "JSON"),
        };
        let Some(p) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(format!("map.{ext}")).save_file() else { return };
        let bytes: Result<Vec<u8>, String> = match kind {
            "csv" => csv_io::export(doc, &csv_io::ExportOptions { bom: true, ..Default::default() }).map_err(|e| e.to_string()),
            "csv-sel" => csv_io::export(doc, &csv_io::ExportOptions { subtree: sub, ..Default::default() }).map_err(|e| e.to_string()),
            "md" => Ok(outline::to_outline(doc, doc.root()).into_bytes()),
            "json-flat" => json::export(doc, &json::ExportOptions { shape: json::Shape2::Flat, ..Default::default() })
                .map(String::into_bytes)
                .map_err(|e| e.to_string()),
            _ => json::export(doc, &json::ExportOptions::default()).map(String::into_bytes).map_err(|e| e.to_string()),
        };
        match bytes.and_then(|b| mm_io::write_atomic(&p, &b).map_err(|e| e.to_string())) {
            Ok(()) => self.say(format!("Exported {}", p.display())),
            Err(e) => self.say(format!("Export failed: {e}")),
        }
    }

    // ---- camera ---------------------------------------------------------------

    fn world_to_screen(&self, rect: Rect, p: Pos2) -> Pos2 {
        rect.center() + (p.to_vec2() - self.cam.center) * self.cam.zoom
    }

    fn screen_to_world(&self, rect: Rect, p: Pos2) -> Pos2 {
        ((p - rect.center()) / self.cam.zoom + self.cam.center).to_pos2()
    }

    fn fit(&mut self, rect: Rect, bounds: mm_layout::Rect, max_zoom: f32) {
        let pad = 60.0;
        let zx = (rect.width() - 2.0 * pad).max(50.0) / bounds.w.max(1.0);
        let zy = (rect.height() - 2.0 * pad).max(50.0) / bounds.h.max(1.0);
        self.cam.zoom = zx.min(zy).clamp(0.05, max_zoom);
        let (cx, cy) = bounds.center();
        self.cam.center = vec2(cx, cy);
        self.cam_target = None;
    }

    // ---- keyboard -------------------------------------------------------------

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.find.is_some() || self.pending_import.is_some() || self.confirm.is_some() {
            return;
        }
        let editing = self.model.edit.is_some();
        let typing_elsewhere = !editing && ctx.egui_wants_keyboard_input();
        let mut actions: Vec<Action> = Vec::new();
        let mut cmds: Vec<&'static str> = Vec::new();
        let mut typed: Option<String> = None;
        ctx.input_mut(|i| {
            let none = Modifiers::NONE;
            // Keys shared by editing and navigating.
            if editing || !typing_elsewhere {
                if i.consume_key(none, Key::Tab) {
                    actions.push(Action::Tab);
                }
                if i.consume_key(none, Key::Enter) {
                    actions.push(Action::Enter);
                }
                if i.consume_key(none, Key::Escape) {
                    actions.push(Action::Escape);
                }
            }
            if editing || typing_elsewhere {
                return;
            }
            if i.consume_key(Modifiers::SHIFT, Key::Enter) || i.consume_key(Modifiers::ALT, Key::Enter) {
                actions.push(Action::SiblingBefore);
            }
            if i.consume_key(none, Key::F2) || i.consume_key(none, Key::Space) {
                actions.push(Action::BeginEdit);
            }
            if i.consume_key(none, Key::Delete) || i.consume_key(none, Key::Backspace) {
                actions.push(Action::Delete);
            }
            for (k, d) in [(Key::ArrowUp, Dir::Up), (Key::ArrowDown, Dir::Down), (Key::ArrowLeft, Dir::Left), (Key::ArrowRight, Dir::Right)]
            {
                if i.consume_key(Modifiers::ALT, k) {
                    actions.push(Action::Nudge(d));
                } else if i.consume_key(Modifiers::SHIFT, k) {
                    actions.push(Action::Navigate { dir: d, extend: true });
                } else if i.consume_key(none, k) {
                    actions.push(Action::Navigate { dir: d, extend: false });
                }
            }
            if i.consume_key(Modifiers::COMMAND, Key::Period) {
                actions.push(Action::ToggleCollapse);
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y) {
                actions.push(Action::Redo);
            } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                actions.push(Action::Undo);
            }
            if i.consume_key(Modifiers::COMMAND, Key::A) {
                actions.push(Action::SelectAll);
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::L) {
                actions.push(Action::ResetLayout { selection_only: true });
            } else if i.consume_key(Modifiers::COMMAND, Key::L) {
                actions.push(Action::ResetLayout { selection_only: false });
            }
            if i.consume_key(Modifiers::ALT, Key::M) {
                actions.push(Action::BeginGrab);
            }
            for (n, k) in [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9]
                .into_iter()
                .enumerate()
            {
                if i.consume_key(Modifiers::COMMAND, k) {
                    actions.push(Action::ShowLevel(n));
                }
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Num1) {
                cmds.push("fit");
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Num2) {
                cmds.push("fit-sel");
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::M) {
                cmds.push("move-to");
            }
            if i.consume_key(Modifiers::COMMAND, Key::F) || i.consume_key(Modifiers::COMMAND, Key::P) {
                cmds.push("find");
            }
            if i.consume_key(Modifiers::COMMAND, Key::S) {
                cmds.push("save");
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
                cmds.push("save-as");
            }
            if i.consume_key(Modifiers::COMMAND, Key::O) {
                cmds.push("open");
            }
            if i.consume_key(Modifiers::COMMAND, Key::N) {
                cmds.push("new");
            }
            if i.consume_key(Modifiers::COMMAND, Key::I) {
                cmds.push("inspector");
            }
            if i.consume_key(Modifiers::COMMAND, Key::Equals) {
                cmds.push("zoom-in");
            }
            if i.consume_key(Modifiers::COMMAND, Key::Minus) {
                cmds.push("zoom-out");
            }
            // Clipboard events.
            let mut events = std::mem::take(&mut i.events);
            events.retain(|e| match e {
                egui::Event::Copy => {
                    actions.push(Action::Copy);
                    false
                }
                egui::Event::Cut => {
                    actions.push(Action::Cut);
                    false
                }
                egui::Event::Paste(t) => {
                    if i.modifiers.shift {
                        actions.push(Action::PasteSibling(Some(t.clone())));
                    } else {
                        actions.push(Action::Paste(Some(t.clone())));
                    }
                    false
                }
                egui::Event::Text(t) => {
                    // "Just start typing" replaces the selected node's text.
                    let plain = !i.modifiers.command && !i.modifiers.alt && !i.modifiers.ctrl;
                    if plain && t != " " && !t.chars().any(char::is_control) && typed.is_none() {
                        typed = Some(t.clone());
                    }
                    false
                }
                _ => true,
            });
            i.events = events;
        });
        for a in actions {
            self.model.dispatch(a);
        }
        if let Some(t) = typed {
            self.model.dispatch(Action::BeginEdit);
            self.model.set_buffer(&t);
            self.caret_at_end = true;
        }
        for c in cmds {
            self.run_command(c);
        }
        if let Some(t) = self.model.clip_out.take() {
            ctx.copy_text(t);
        }
    }

    fn run_command(&mut self, c: &str) {
        match c {
            "fit" => self.pending_fit = true,
            "fit-sel" => {
                let ids = self.model.sel.clone();
                let mut b: Option<mm_layout::Rect> = None;
                for id in ids {
                    if let Some(r) = self.model.layout.rect(id) {
                        b = Some(b.map_or(r, |x| x.union(&r)));
                    }
                }
                if let Some(b) = b {
                    self.cam_target = None;
                    self.cam.center = vec2(b.center().0, b.center().1);
                    self.cam.zoom = 1.2;
                }
            }
            "find" => self.find = Some(Find { query: String::new(), mode: FindMode::Jump, selected: 0, just_opened: true }),
            "move-to" => self.find = Some(Find { query: String::new(), mode: FindMode::MoveTo, selected: 0, just_opened: true }),
            "save" => self.save(false),
            "save-as" => self.save(true),
            "open" => self.guarded(After::Open),
            "new" => self.guarded(After::NewTemplate(templates::TEMPLATES.len() - 1)),
            "inspector" => self.show_inspector = !self.show_inspector,
            "zoom-in" => self.cam.zoom = (self.cam.zoom * 1.2).min(8.0),
            "zoom-out" => self.cam.zoom = (self.cam.zoom / 1.2).max(0.05),
            _ => {}
        }
    }

    // ---- UI pieces ------------------------------------------------------------

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                ui.menu_button("New from template", |ui| {
                    for (i, t) in templates::TEMPLATES.iter().enumerate() {
                        if ui.button(t.name).clicked() {
                            self.guarded(After::NewTemplate(i));
                            ui.close();
                        }
                    }
                });
                if ui.button("Open…  Cmd+O").clicked() {
                    self.guarded(After::Open);
                    ui.close();
                }
                if ui.button("Save  Cmd+S").clicked() {
                    self.save(false);
                    ui.close();
                }
                if ui.button("Save As…  Shift+Cmd+S").clicked() {
                    self.save(true);
                    ui.close();
                }
                if ui.add_enabled(autosave_path(&self.path).exists(), egui::Button::new("Recover autosave")).clicked() {
                    self.recover();
                    ui.close();
                }
                ui.separator();
                if ui.button("Import CSV / JSON / outline…").clicked() {
                    self.import_dialog();
                    ui.close();
                }
                ui.menu_button("Export", |ui| {
                    for (label, kind) in [
                        ("CSV (whole map)", "csv"),
                        ("CSV (selected branch)", "csv-sel"),
                        ("JSON (nested)", "json"),
                        ("JSON (flat records)", "json-flat"),
                        ("Markdown outline", "md"),
                    ] {
                        if ui.button(label).clicked() {
                            self.export_dialog(kind);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Quit").clicked() {
                    self.guarded(After::Quit);
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(self.model.ed.can_undo(), egui::Button::new("Undo  Cmd+Z")).clicked() {
                    self.model.dispatch(Action::Undo);
                    ui.close();
                }
                if ui.add_enabled(self.model.ed.can_redo(), egui::Button::new("Redo  Shift+Cmd+Z")).clicked() {
                    self.model.dispatch(Action::Redo);
                    ui.close();
                }
                ui.separator();
                for (label, a) in [
                    ("Cut  Cmd+X", Action::Cut),
                    ("Copy  Cmd+C", Action::Copy),
                    ("Paste as child  Cmd+V", Action::Paste(None)),
                    ("Delete", Action::Delete),
                ] {
                    if ui.button(label).clicked() {
                        self.model.dispatch(a);
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Find / jump…  Cmd+F").clicked() {
                    self.run_command("find");
                    ui.close();
                }
                if ui.button("Move selection to…  Shift+Cmd+M").clicked() {
                    self.run_command("move-to");
                    ui.close();
                }
                if ui.button("Grab and move  Alt+M").clicked() {
                    self.model.dispatch(Action::BeginGrab);
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                for m in DiagramMode::ALL {
                    if ui.radio(self.model.ed.doc.mode == m, m.label()).clicked() {
                        let _ = self.model.ed.set_mode(m);
                        self.pending_fit = true;
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Auto-layout all  Cmd+L").clicked() {
                    self.model.dispatch(Action::ResetLayout { selection_only: false });
                    ui.close();
                }
                if ui.button("Auto-layout selected branch  Shift+Cmd+L").clicked() {
                    self.model.dispatch(Action::ResetLayout { selection_only: true });
                    ui.close();
                }
                ui.separator();
                if ui.button("Fit all  Shift+Cmd+1").clicked() {
                    self.pending_fit = true;
                    ui.close();
                }
                if ui.button("Fit selection  Shift+Cmd+2").clicked() {
                    self.run_command("fit-sel");
                    ui.close();
                }
                ui.menu_button("Show levels", |ui| {
                    for n in 1..=5 {
                        if ui.button(format!("Level {n}  Cmd+{n}")).clicked() {
                            self.model.dispatch(Action::ShowLevel(n));
                            ui.close();
                        }
                    }
                    if ui.button("All  Cmd+0").clicked() {
                        self.model.dispatch(Action::ShowLevel(0));
                        ui.close();
                    }
                });
                ui.checkbox(&mut self.show_inspector, "Inspector  Cmd+I");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard shortcuts").clicked() {
                    self.show_help = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{} nodes", self.model.ed.doc.len())).weak());
            });
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let id = self.model.primary();
        ui.heading("Node");
        let doc = &self.model.ed.doc;
        ui.label(egui::RichText::new(doc.node(id).text.lines().next().unwrap_or("")).strong());
        ui.separator();

        // Note editor: one undo step per focus session.
        ui.label("Note");
        if self.note_buf.as_ref().map(|(n, _)| *n) != Some(id) {
            self.note_buf = Some((id, self.model.ed.doc.node(id).note.clone().unwrap_or_default()));
        }
        let buf = &mut self.note_buf.as_mut().unwrap().1;
        let r = ui.add(egui::TextEdit::multiline(buf).desired_rows(6).desired_width(f32::INFINITY).hint_text("Longer thoughts…"));
        if r.gained_focus() {
            self.model.ed.begin_group();
        }
        if r.changed() {
            let v = Some(buf.clone()).filter(|s| !s.is_empty());
            let _ = self.model.ed.set_note(id, v);
        }
        if r.lost_focus() {
            self.model.ed.end_group();
        }

        ui.add_space(8.0);
        ui.label("Colour");
        ui.horizontal_wrapped(|ui| {
            for c in PALETTE {
                let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                ui.painter().rect_filled(rect, 4.0, c32(c));
                if resp.clicked() {
                    let _ = self.model.ed.set_color(id, Some(c));
                }
            }
            if ui.small_button("auto").clicked() {
                let _ = self.model.ed.set_color(id, None);
            }
        });
        ui.add_space(8.0);
        ui.label("Shape");
        let cur = self.model.ed.doc.node(id).shape;
        ui.horizontal(|ui| {
            for (s, l) in [(Shape::Rounded, "Rounded"), (Shape::Pill, "Pill"), (Shape::None, "Plain")] {
                if ui.selectable_label(cur == s, l).clicked() {
                    let _ = self.model.ed.set_shape(id, s);
                }
            }
        });
        ui.add_space(8.0);
        if self.model.ed.doc.node(id).offset.is_some() && ui.button("Reset manual position").clicked() {
            let _ = self.model.ed.apply(mm_core::Command::SetOffset { id, offset: None });
        }
    }

    fn find_popup(&mut self, ctx: &egui::Context) {
        let Some(find) = &mut self.find else { return };
        let mut close = false;
        let mut choose: Option<NodeId> = None;
        let results = self.model.find(&find.query);
        egui::Window::new(if find.mode == FindMode::Jump { "Jump to node" } else { "Move selection under…" })
            .anchor(Align2::CENTER_TOP, vec2(0.0, 80.0))
            .collapsible(false)
            .resizable(false)
            .fixed_size(vec2(420.0, 0.0))
            .show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut find.query).desired_width(f32::INFINITY).hint_text("type to search…"));
                if find.just_opened {
                    r.request_focus();
                    find.just_opened = false;
                }
                if r.changed() {
                    find.selected = 0;
                }
                ctx.input_mut(|i| {
                    if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                        find.selected = (find.selected + 1).min(results.len().saturating_sub(1));
                    }
                    if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                        find.selected = find.selected.saturating_sub(1);
                    }
                    if i.consume_key(Modifiers::NONE, Key::Escape) {
                        close = true;
                    }
                    if i.consume_key(Modifiers::NONE, Key::Enter) {
                        choose = results.get(find.selected).copied();
                        close = true;
                    }
                });
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for (n, &id) in results.iter().take(50).enumerate() {
                        let text = self.model.ed.doc.node(id).text.replace('\n', " ");
                        let depth = self.model.ed.doc.depth(id);
                        let label = format!("{}{}", "  ".repeat(depth.min(6)), if text.is_empty() { "(empty)" } else { &text });
                        if ui.selectable_label(n == find.selected, label).clicked() {
                            choose = Some(id);
                            close = true;
                        }
                    }
                    if results.is_empty() && !find.query.is_empty() {
                        ui.weak("No matches");
                    }
                });
            });
        let mode = find.mode;
        if close {
            self.find = None;
        }
        if let Some(id) = choose {
            match mode {
                FindMode::Jump => self.model.jump_to(id),
                FindMode::MoveTo => self.model.dispatch(Action::MoveSelectionTo(id)),
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(p) = &self.pending_import {
            let mut decision: Option<u8> = None;
            egui::Window::new(format!("Import {}", p.name))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("{} nodes found.", p.tree.count()));
                    if !p.warnings.is_empty() {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!("{} warning(s):", p.warnings.len())).strong());
                        egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                            for w in &p.warnings {
                                ui.label(format!("• {w}"));
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Open as new map").clicked() {
                            decision = Some(1);
                        }
                        if ui.button("Insert under selected node").clicked() {
                            decision = Some(2);
                        }
                        if ui.button("Cancel").clicked() {
                            decision = Some(0);
                        }
                    });
                });
            if let Some(d) = decision {
                let p = self.pending_import.take().unwrap();
                match d {
                    1 => {
                        let mut doc = Document::from_subtree(&p.tree);
                        doc.mode = p.mode;
                        self.replace_document(doc, None);
                        self.model.saved_rev = u64::MAX;
                    }
                    2 => self.model.import_branch(&p.tree),
                    _ => {}
                }
            }
        }

        if let Some(after) = self.confirm.clone() {
            let mut decision: Option<u8> = None;
            egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(
                ctx,
                |ui| {
                    ui.label("This map has changes that have not been saved.");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            decision = Some(1);
                        }
                        if ui.button("Discard").clicked() {
                            decision = Some(2);
                        }
                        if ui.button("Cancel").clicked() {
                            decision = Some(0);
                        }
                    });
                },
            );
            if let Some(d) = decision {
                self.confirm = None;
                match d {
                    1 => {
                        self.save(false);
                        if !self.model.is_dirty() {
                            self.run_after(after);
                        }
                    }
                    2 => {
                        self.model.mark_saved();
                        self.run_after(after);
                    }
                    _ => {}
                }
            }
        }

        if self.show_help {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts").open(&mut open).default_width(460.0).show(ctx, |ui| {
                egui::Grid::new("help").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, d) in HELP {
                        ui.monospace(*k);
                        ui.label(*d);
                        ui.end_row();
                    }
                });
            });
            self.show_help = open;
        }
    }

    // ---- canvas ---------------------------------------------------------------

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let ctx = ui.ctx().clone();
        self.model.ensure_layout();

        if self.pending_fit {
            let b = self.model.layout.bounds;
            self.fit(rect, b, 1.0);
            self.pending_fit = false;
        }

        // Zoom and scroll.
        if resp.hovered() {
            let (zoom, scroll, pointer) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
            if (zoom - 1.0).abs() > f32::EPSILON {
                if let Some(p) = pointer {
                    let before = self.screen_to_world(rect, p);
                    self.cam.zoom = (self.cam.zoom * zoom).clamp(0.03, 8.0);
                    let after = self.screen_to_world(rect, p);
                    self.cam.center += before.to_vec2() - after.to_vec2();
                }
            } else if scroll != Vec2::ZERO {
                self.cam.center -= scroll / self.cam.zoom;
                self.cam_target = None;
            }
        }

        // Keep the cursor node in view after keyboard actions.
        if self.model.focus_request {
            self.model.focus_request = false;
            let focus = self.model.grab_target().unwrap_or(self.model.primary());
            if let Some(r) = self.model.layout.rect(focus) {
                let a = self.world_to_screen(rect, Pos2::new(r.x, r.y));
                let b = self.world_to_screen(rect, Pos2::new(r.right(), r.bottom()));
                let inner = rect.shrink(70.0);
                if !(inner.contains(a) && inner.contains(b)) {
                    self.cam_target = Some(vec2(r.center().0, r.center().1));
                }
            }
        }
        if let Some(t) = self.cam_target {
            let d = t - self.cam.center;
            if d.length() * self.cam.zoom < 1.0 {
                self.cam.center = t;
                self.cam_target = None;
            } else {
                self.cam.center += d * 0.3;
                ctx.request_repaint();
            }
        }

        // Pointer interaction.
        let pointer_world = resp.interact_pointer_pos().map(|p| self.screen_to_world(rect, p));
        let hit = |this: &Self, p: Pos2| mm_layout::hit_test(&this.model.layout, p.x, p.y);
        if resp.drag_started() {
            let origin = ctx.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            let w = self.screen_to_world(rect, origin);
            let shift = ctx.input(|i| i.modifiers.shift);
            let middle = ctx.input(|i| i.pointer.middle_down());
            self.drag = Some(match hit(self, w) {
                Some(id) if !middle && self.model.edit.is_none() && self.model.ed.doc.parent(id).is_some() => {
                    if !self.model.is_selected(id) {
                        self.model.dispatch(Action::Select { id, extend: false });
                    }
                    let ids: Vec<NodeId> =
                        self.model.selected_roots().into_iter().filter(|&i| self.model.ed.doc.parent(i).is_some()).collect();
                    let orig = ids.iter().map(|&i| (i, self.model.ed.doc.node(i).offset.unwrap_or([0.0, 0.0]))).collect();
                    // One undo step for the whole drag; aborted if it ends as a re-parent.
                    if self.model.ed.doc.mode != DiagramMode::Fishbone {
                        self.model.ed.begin_group();
                    }
                    NodeDrag::Nodes { ids, orig, acc: Vec2::ZERO }
                }
                None if shift && !middle => NodeDrag::Marquee { start: origin },
                _ => NodeDrag::Pan,
            });
        }
        if resp.dragged() {
            match &mut self.drag {
                Some(NodeDrag::Pan) => {
                    self.cam.center -= resp.drag_delta() / self.cam.zoom;
                    self.cam_target = None;
                }
                Some(NodeDrag::Nodes { orig, acc, .. }) if self.model.ed.doc.mode != DiagramMode::Fishbone => {
                    *acc += resp.drag_delta() / self.cam.zoom;
                    for &(id, o) in orig.iter() {
                        let _ = self.model.ed.apply(mm_core::Command::SetOffset { id, offset: Some([o[0] + acc.x, o[1] + acc.y]) });
                    }
                    self.model.ensure_layout();
                }
                _ => {}
            }
        }
        // Dropping the pointer onto another node re-parents; Alt forces a plain move instead.
        let alt = ctx.input(|i| i.modifiers.alt);
        let mut drop_target: Option<NodeId> = None;
        if let (Some(NodeDrag::Nodes { ids, .. }), Some(p)) = (&self.drag, pointer_world) {
            let doc = &self.model.ed.doc;
            drop_target = self
                .model
                .layout
                .order
                .iter()
                .rev()
                .copied()
                .filter(|&t| !ids.iter().any(|&g| doc.is_ancestor_or_self(g, t)))
                .find(|&t| self.model.layout.rect(t).is_some_and(|r| r.contains(p.x, p.y)))
                .filter(|_| !alt);
        }
        if resp.drag_stopped() {
            match self.drag.take() {
                Some(NodeDrag::Nodes { .. }) => {
                    if let Some(t) = drop_target {
                        self.model.ed.abort_group(); // undo the live move, then re-parent
                        self.model.dispatch(Action::MoveSelectionTo(t));
                    } else {
                        self.model.ed.end_group();
                        self.model.status = if self.model.ed.doc.mode == DiagramMode::Fishbone {
                            "Fishbone positions are automatic; drop onto a node to re-parent".into()
                        } else {
                            "Moved. View > Auto-layout (Cmd+L) restores automatic positions".into()
                        };
                    }
                }
                Some(NodeDrag::Marquee { start }) => {
                    if let Some(end) = resp.interact_pointer_pos() {
                        let sel = Rect::from_two_pos(start, end);
                        let hits: Vec<NodeId> = self
                            .model
                            .layout
                            .order
                            .iter()
                            .copied()
                            .filter(|&id| {
                                self.model.layout.rect(id).is_some_and(|r| {
                                    Rect::from_min_max(
                                        self.world_to_screen(rect, Pos2::new(r.x, r.y)),
                                        self.world_to_screen(rect, Pos2::new(r.right(), r.bottom())),
                                    )
                                    .intersects(sel)
                                })
                            })
                            .collect();
                        if !hits.is_empty() {
                            self.model.sel = hits;
                        }
                    }
                }
                _ => {}
            }
        }
        if resp.double_clicked() {
            if let Some(id) = pointer_world.and_then(|p| hit(self, p)) {
                self.model.dispatch(Action::Select { id, extend: false });
                self.model.dispatch(Action::BeginEdit);
            }
        } else if resp.clicked() {
            let extend = ctx.input(|i| i.modifiers.shift || i.modifiers.command);
            match pointer_world.and_then(|p| hit(self, p)) {
                Some(id) => self.model.dispatch(Action::Select { id, extend }),
                None => {
                    if self.model.edit.is_some() {
                        self.model.dispatch(Action::Escape);
                    } else if !extend {
                        let p = self.model.primary();
                        self.model.sel = vec![p];
                    }
                }
            }
        }

        self.paint(ui, rect, drop_target);
        self.edit_overlay(ui, rect);

        // Marquee rectangle.
        if let (Some(NodeDrag::Marquee { start }), Some(cur)) = (&self.drag, ctx.pointer_latest_pos()) {
            let r = Rect::from_two_pos(*start, cur);
            ui.painter().rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(80, 140, 255, 40));
            ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_rgb(80, 140, 255)), StrokeKind::Inside);
        }
    }

    fn paint(&self, ui: &egui::Ui, rect: Rect, drop_target: Option<NodeId>) {
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let z = self.cam.zoom;
        let doc = &self.model.ed.doc;
        let layout = &self.model.layout;
        let dark = visuals.dark_mode;
        painter.rect_filled(rect, 0.0, visuals.extreme_bg_color);
        let view = rect.expand(80.0);

        // Edges first.
        for e in &layout.edges {
            let a = self.world_to_screen(rect, Pos2::new(e.a.0, e.a.1));
            let b = self.world_to_screen(rect, Pos2::new(e.b.0, e.b.1));
            if !view.intersects(Rect::from_two_pos(a, b)) {
                continue;
            }
            let col = c32(branch_rgb(doc, e.node)).gamma_multiply(if dark { 0.9 } else { 1.0 });
            let w = (if e.heavy { 3.5 } else { 2.0 } * z).clamp(1.0, 8.0);
            match e.kind {
                mm_layout::EdgeKind::Line => {
                    painter.line_segment([a, b], Stroke::new(w, col));
                }
                kind => {
                    let (c1, c2) = if kind == mm_layout::EdgeKind::CurveV {
                        let dy = (b.y - a.y) * 0.5;
                        (a + vec2(0.0, dy), b - vec2(0.0, dy))
                    } else {
                        let dx = (b.x - a.x) * 0.5;
                        (a + vec2(dx, 0.0), b - vec2(dx, 0.0))
                    };
                    painter.add(CubicBezierShape::from_points_stroke([a, c1, c2, b], false, Color32::TRANSPARENT, Stroke::new(w, col)));
                }
            }
        }

        let editing = self.model.edit.as_ref().map(|e| e.id);
        let grab_ids: Vec<NodeId> = self.model.grab.as_ref().map(|g| g.ids.clone()).unwrap_or_default();
        let grab_target = self.model.grab_target();
        let root = doc.root();
        for &id in &layout.order {
            let Some(r) = layout.rect(id) else { continue };
            let min = self.world_to_screen(rect, Pos2::new(r.x, r.y));
            let max = self.world_to_screen(rect, Pos2::new(r.right(), r.bottom()));
            let sr = Rect::from_min_max(min, max);
            if !view.intersects(sr) {
                continue;
            }
            let node = doc.node(id);
            let depth_one = doc.parent(id) == Some(root) || id == root;
            let base = c32(branch_rgb(doc, id));
            let (fill, text_col, border) =
                if id == root || (depth_one && doc.mode == DiagramMode::Fishbone) || (depth_one && node.shape != Shape::None) {
                    (base, on_color(base), Color32::TRANSPARENT)
                } else {
                    (visuals.panel_fill, visuals.strong_text_color(), base)
                };
            let radius: CornerRadius = match node.shape {
                Shape::Pill => ((sr.height() / 2.0).min(255.0) as u8).into(),
                _ => ((8.0 * z).clamp(1.0, 20.0) as u8).into(),
            };
            let faded = grab_ids.contains(&id);
            let alpha = |c: Color32| if faded { c.gamma_multiply(0.4) } else { c };
            if node.shape != Shape::None {
                painter.rect_filled(sr, radius, alpha(fill));
                if border != Color32::TRANSPARENT {
                    painter.rect_stroke(sr, radius, Stroke::new((1.6 * z).max(1.0), alpha(border)), StrokeKind::Inside);
                }
            }
            if editing != Some(id) && z > 0.3 {
                let font = FontId::proportional(if id == root { ROOT_FONT } else { BASE_FONT } * z);
                let color = if node.shape == Shape::None { visuals.strong_text_color() } else { text_col };
                let shown = if node.text.is_empty() { " ".to_string() } else { node.text.clone() };
                let g = painter.layout_no_wrap(shown, font, alpha(color));
                let pos = Pos2::new(sr.min.x + PAD_X * z, sr.center().y - g.size().y / 2.0);
                painter.galley(pos, g, color);
            }
            if node.collapsed && !node.children.is_empty() && z > 0.25 {
                let right = (r.center().0) >= layout.rect(root).map_or(0.0, |rr| rr.center().0);
                let pos = if doc.mode == DiagramMode::TopDown {
                    Pos2::new(sr.center().x, sr.max.y + 10.0 * z)
                } else if right && doc.mode != DiagramMode::Fishbone {
                    Pos2::new(sr.max.x + 12.0 * z, sr.center().y)
                } else {
                    Pos2::new(sr.min.x - 12.0 * z, sr.center().y)
                };
                painter.circle_filled(pos, 9.0 * z.max(0.7), base);
                painter.text(
                    pos,
                    Align2::CENTER_CENTER,
                    format!("{}", doc.subtree_ids(id).len() - 1),
                    FontId::proportional(10.0 * z.max(0.8)),
                    on_color(base),
                );
            }
            if !node.note.as_deref().unwrap_or("").is_empty() && z > 0.3 {
                painter.circle_filled(
                    Pos2::new(sr.max.x - 5.0 * z, sr.min.y + 5.0 * z),
                    3.5 * z.max(0.7),
                    Color32::from_rgb(0xf2, 0xc9, 0x4c),
                );
            }
            if self.model.is_selected(id) {
                let primary = self.model.primary() == id;
                painter.rect_stroke(
                    sr.expand(3.0),
                    radius,
                    Stroke::new(if primary { 2.5 } else { 1.8 }, visuals.selection.stroke.color),
                    StrokeKind::Outside,
                );
            }
            if drop_target == Some(id) || grab_target == Some(id) {
                painter.rect_stroke(sr.expand(5.0), radius, Stroke::new(3.0, Color32::from_rgb(0x59, 0xa1, 0x4f)), StrokeKind::Outside);
                painter.text(
                    sr.center_top() - vec2(0.0, 6.0),
                    Align2::CENTER_BOTTOM,
                    "drop here",
                    FontId::proportional(11.0),
                    Color32::from_rgb(0x59, 0xa1, 0x4f),
                );
            }
        }

        // Ghost for node drag.
        if let (Some(NodeDrag::Nodes { ids, .. }), Some(p), DiagramMode::Fishbone) = (&self.drag, ui.ctx().pointer_latest_pos(), doc.mode) {
            if let Some(&first) = ids.first() {
                let text = doc.node(first).text.clone();
                let label = if ids.len() > 1 { format!("{text} (+{})", ids.len() - 1) } else { text };
                let g = painter.layout_no_wrap(label, FontId::proportional(BASE_FONT), visuals.strong_text_color());
                let r = Rect::from_min_size(p + vec2(12.0, 12.0), g.size() + vec2(16.0, 10.0));
                painter.rect_filled(r, 6.0, visuals.window_fill.gamma_multiply(0.92));
                painter.rect_stroke(r, 6.0, Stroke::new(1.0, visuals.selection.stroke.color), StrokeKind::Inside);
                painter.galley(r.min + vec2(8.0, 5.0), g, visuals.strong_text_color());
            }
        }
    }

    fn edit_overlay(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(edit) = self.model.edit.clone() else {
            self.last_edit_id = None;
            return;
        };
        let Some(r) = self.model.layout.rect(edit.id) else { return };
        let z = self.cam.zoom;
        let min = self.world_to_screen(rect, Pos2::new(r.x, r.y));
        let is_root = edit.id == self.model.ed.doc.root();
        let font = FontId::proportional(if is_root { ROOT_FONT } else { BASE_FONT } * z);
        let width = (r.w * z).max(80.0 * z);
        let area = Rect::from_min_size(min + vec2(PAD_X * z - 2.0, PAD_Y * z - 3.0), vec2(width, r.h * z));
        let mut buf = edit.buffer.clone();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        let te = egui::TextEdit::multiline(&mut buf)
            .id_salt("node-editor")
            .frame(egui::Frame::NONE)
            .font(font)
            .desired_rows(1)
            .desired_width(width)
            .lock_focus(true)
            .return_key(Some(egui::KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)));
        let out = child.add(te);
        if self.last_edit_id != Some(edit.id) {
            out.request_focus();
            // Select all existing text so typing replaces, Esc keeps it.
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), out.id) {
                let end = buf.chars().count();
                let start = if self.caret_at_end { end } else { 0 };
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(start), egui::text::CCursor::new(end))));
                self.caret_at_end = false;
                state.store(ui.ctx(), out.id);
            }
            self.last_edit_id = Some(edit.id);
        }
        if out.changed() {
            self.model.set_buffer(&buf);
        }
    }
}

const HELP: &[(&str, &str)] = &[
    ("Tab", "New child (while typing: commit and add child)"),
    ("Enter", "New sibling (while typing: commit and add sibling)"),
    ("Enter on empty", "Climb out one level (outdent); repeat to go up more"),
    ("Tab on empty", "Indent under previous sibling"),
    ("Esc", "Finish editing / cancel an empty new node"),
    ("Shift+Enter", "New line in node (typing) / sibling above (navigating)"),
    ("Arrow keys", "Move selection spatially (Shift extends)"),
    ("Alt+Up/Down", "Reorder among siblings"),
    ("Alt+Left/Right", "Outdent / indent (move between branches)"),
    ("Alt+M", "Grab: Up/Down choose parent, Enter drop, Esc cancel"),
    ("Shift+Cmd+M", "Move selection under a node found by name"),
    ("Cmd+F / Cmd+P", "Find and jump"),
    ("F2 / Space", "Edit selected node (or just start typing)"),
    ("Cmd+.", "Collapse / expand branch"),
    ("Cmd+1..9, Cmd+0", "Show N levels / all"),
    ("Cmd+C Cmd+X Cmd+V", "Copy, cut, paste branch (Shift+Cmd+V sibling); pastes indented text as nodes"),
    ("Delete", "Delete selection"),
    ("Cmd+Z / Shift+Cmd+Z", "Undo / redo"),
    ("Shift+Cmd+1 / Shift+Cmd+2", "Fit all / fit selection"),
    ("Cmd+= / Cmd+-", "Zoom in / out (also pinch or Cmd++scroll)"),
    ("Drag node", "Move it (and its branch) anywhere; drop onto a node to re-parent, hold Alt to force a plain move"),
    ("Cmd+L / Shift+Cmd+L", "Auto-layout: clear manual positions (all / selected branch)"),
    ("Shift+drag", "Marquee select"),
    ("Cmd+S Shift+Cmd+S Cmd+O Cmd+N", "Save, save as, open, new"),
    ("Cmd+I", "Toggle inspector"),
];

impl eframe::App for MindmapApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.model.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(After::Quit);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.fonts_ready.get() {
            self.fonts_ready.set(true);
            self.model.invalidate_layout();
            self.pending_fit = true;
        }
        self.handle_keys(&ctx);

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let status = if !self.model.status.is_empty() {
                    self.model.status.clone()
                } else if let Some((m, t)) = &self.toast {
                    if t.elapsed() < Duration::from_secs(6) {
                        m.clone()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                if status.is_empty() {
                    ui.weak(match (&self.model.edit, &self.model.grab) {
                        (Some(_), _) => "Typing: Tab = child · Enter = sibling · Enter on empty = climb a level · Esc = done",
                        (_, Some(_)) => "Grab: Up/Down choose new parent · Enter drop · Esc cancel",
                        _ => "Tab child · Enter sibling · arrows navigate · Alt+arrows restructure · Alt+M grab · Cmd+F find · Help menu for shortcuts",
                    });
                } else {
                    ui.label(status);
                }
            });
        });
        if self.show_inspector {
            egui::Panel::right("inspector").default_size(230.0).show(ui, |ui| self.inspector(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.canvas(ui));

        self.find_popup(&ctx);
        self.dialogs(&ctx);

        // Title and autosave.
        let name = self
            .path
            .as_ref()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let title = format!("{}{} — Mindmap", name, if self.model.is_dirty() { " •" } else { "" });
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        self.autosave();
        ctx.request_repaint_after(Duration::from_secs(5));
    }
}
