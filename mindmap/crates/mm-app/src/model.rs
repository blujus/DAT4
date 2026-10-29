//! The editing state machine: selection, inline edit, grab mode, clipboard.
//! No UI types in here, so keyboard flows are unit-testable without a window.
use mm_core::{outline, Editor, NodeId, Subtree};
use mm_layout::{compute, spatial_neighbor, Config, Direction, Layout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl From<Dir> for Direction {
    fn from(d: Dir) -> Self {
        match d {
            Dir::Up => Direction::Up,
            Dir::Down => Direction::Down,
            Dir::Left => Direction::Left,
            Dir::Right => Direction::Right,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    /// Tab: child (or indent when editing a fresh empty node).
    Tab,
    /// Enter: sibling (or outdent when editing a fresh empty node).
    Enter,
    /// Shift+Enter / Alt+Enter while not editing: sibling above.
    SiblingBefore,
    Escape,
    /// F2 / Space.
    BeginEdit,
    Delete,
    Navigate {
        dir: Dir,
        extend: bool,
    },
    /// Alt+arrows: Up/Down reorder, Left outdent, Right indent.
    Nudge(Dir),
    ToggleCollapse,
    Undo,
    Redo,
    Copy,
    Cut,
    /// Paste clipboard text (system clipboard) as children of the selection.
    Paste(Option<String>),
    /// Paste as a sibling after the selection.
    PasteSibling(Option<String>),
    BeginGrab,
    GrabCycle(i32),
    GrabDrop,
    /// 0 = expand all, n = show n levels.
    ShowLevel(usize),
    Select {
        id: NodeId,
        extend: bool,
    },
    SelectAll,
    /// Clear manual node positions and let auto-layout place them again.
    ResetLayout {
        selection_only: bool,
    },
    /// Mouse drag-drop and "move to…" finder: move the selection under `target`.
    MoveSelectionTo(NodeId),
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub id: NodeId,
    pub is_new: bool,
    pub buffer: String,
}

#[derive(Clone, Debug)]
pub struct Grab {
    pub ids: Vec<NodeId>,
    pub target: NodeId,
}

pub type Measure = Box<dyn Fn(&str, bool) -> (f32, f32)>;

pub struct Model {
    pub ed: Editor,
    /// Selected ids; the last one is the primary (cursor) node.
    pub sel: Vec<NodeId>,
    pub edit: Option<Edit>,
    pub grab: Option<Grab>,
    pub layout: Layout,
    layout_rev: u64,
    layout_mode: Option<mm_core::DiagramMode>,
    pub measure: Measure,
    pub config: Config,
    internal_clip: Option<(String, Subtree)>,
    /// Text the UI should put on the system clipboard.
    pub clip_out: Option<String>,
    pub status: String,
    /// Set whenever the camera should bring the primary node into view.
    pub focus_request: bool,
    pub saved_rev: u64,
}

impl Model {
    pub fn new(doc: mm_core::Document, measure: Measure) -> Self {
        let root = doc.root();
        let saved_rev = doc.revision;
        let mut m = Model {
            ed: Editor::new(doc),
            sel: vec![root],
            edit: None,
            grab: None,
            layout: Layout::default(),
            layout_rev: u64::MAX,
            layout_mode: None,
            measure,
            config: Config::default(),
            internal_clip: None,
            clip_out: None,
            status: String::new(),
            focus_request: true,
            saved_rev,
        };
        m.ensure_layout();
        m
    }

    pub fn set_document(&mut self, doc: mm_core::Document) {
        let root = doc.root();
        self.saved_rev = doc.revision;
        self.ed = Editor::new(doc);
        self.sel = vec![root];
        self.edit = None;
        self.grab = None;
        self.layout_rev = u64::MAX;
        self.focus_request = true;
        self.ensure_layout();
    }

    /// Force a relayout on the next `ensure_layout` (e.g. after fonts load).
    pub fn invalidate_layout(&mut self) {
        self.layout_rev = u64::MAX;
    }

    pub fn is_dirty(&self) -> bool {
        self.ed.doc.revision != self.saved_rev
    }

    pub fn mark_saved(&mut self) {
        self.saved_rev = self.ed.doc.revision;
    }

    pub fn primary(&self) -> NodeId {
        self.sel.last().copied().filter(|&i| self.ed.doc.contains(i)).unwrap_or(self.ed.doc.root())
    }

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.sel.contains(&id)
    }

    pub fn ensure_layout(&mut self) {
        let rev = self.ed.doc.revision;
        if self.layout_rev == rev && self.layout_mode == Some(self.ed.doc.mode) {
            return;
        }
        let doc = &self.ed.doc;
        let root = doc.root();
        let measure = &self.measure;
        self.layout = compute(doc, &|id| measure(&doc.node(id).text, id == root), &self.config);
        self.layout_rev = rev;
        self.layout_mode = Some(doc.mode);
    }

    // ---- selection ------------------------------------------------------------

    fn select_only(&mut self, id: NodeId) {
        self.reveal(id);
        self.sel = vec![id];
        self.focus_request = true;
    }

    /// Expand collapsed ancestors so `id` is visible.
    pub fn reveal(&mut self, id: NodeId) {
        let mut cur = self.ed.doc.parent(id);
        let mut to_open = Vec::new();
        while let Some(p) = cur {
            if self.ed.doc.node(p).collapsed {
                to_open.push(p);
            }
            cur = self.ed.doc.parent(p);
        }
        for p in to_open {
            let _ = self.ed.set_collapsed(p, false);
        }
    }

    /// Selected nodes without selected ancestors, in document order.
    pub fn selected_roots(&self) -> Vec<NodeId> {
        self.ed.top_level(&self.sel)
    }

    // ---- editing --------------------------------------------------------------

    fn begin_edit(&mut self, id: NodeId, is_new: bool) {
        let buffer = self.ed.doc.node(id).text.clone();
        self.edit = Some(Edit { id, is_new, buffer });
        self.sel = vec![id];
        self.focus_request = true;
    }

    /// Called by the UI whenever the text box content changes.
    pub fn set_buffer(&mut self, text: &str) {
        if let Some(e) = &mut self.edit {
            e.buffer = text.to_string();
            let id = e.id;
            let _ = self.ed.set_text(id, text);
        }
    }

    pub fn type_text(&mut self, s: &str) {
        if let Some(e) = &self.edit {
            let t = format!("{}{}", e.buffer, s);
            self.set_buffer(&t);
        }
    }

    fn commit_edit(&mut self) {
        if self.edit.take().is_some() {
            self.ed.end_group();
        }
    }

    fn start_new_child(&mut self, parent: NodeId) {
        self.ed.begin_group();
        if self.ed.doc.node(parent).collapsed {
            let _ = self.ed.set_collapsed(parent, false);
        }
        if let Ok(id) = self.ed.add_child(parent, "") {
            self.begin_edit(id, true);
        }
    }

    fn start_new_sibling(&mut self, of: NodeId, after: bool) {
        self.ed.begin_group();
        if let Ok(id) = self.ed.add_sibling(of, after, "") {
            self.begin_edit(id, true);
        }
    }

    fn editing_fresh_empty(&self) -> bool {
        self.edit.as_ref().is_some_and(|e| e.is_new && e.buffer.is_empty())
    }

    // ---- dispatch -------------------------------------------------------------

    pub fn dispatch(&mut self, action: Action) {
        self.ensure_layout();
        self.status.clear();
        if self.grab.is_some() {
            return self.dispatch_grab(action);
        }
        if let Some(e) = self.edit.clone() {
            return self.dispatch_editing(action, e);
        }
        let cur = self.primary();
        match action {
            Action::Tab => self.start_new_child(cur),
            Action::Enter => {
                if self.ed.doc.parent(cur).is_none() {
                    self.start_new_child(cur)
                } else {
                    self.start_new_sibling(cur, true)
                }
            }
            Action::SiblingBefore => {
                if self.ed.doc.parent(cur).is_some() {
                    self.start_new_sibling(cur, false)
                }
            }
            Action::Escape => self.select_only(cur),
            Action::BeginEdit => {
                self.ed.begin_group();
                self.begin_edit(cur, false)
            }
            Action::Delete => self.delete_selection(),
            Action::Navigate { dir, extend } => {
                if let Some(n) = spatial_neighbor(&self.layout, cur, dir.into()) {
                    if extend {
                        if !self.sel.contains(&n) {
                            self.sel.push(n);
                        }
                        self.focus_request = true;
                    } else {
                        self.select_only(n);
                    }
                }
            }
            Action::Nudge(dir) => self.nudge(dir),
            Action::ToggleCollapse => {
                let n = self.ed.doc.node(cur);
                if !n.children.is_empty() {
                    let c = !n.collapsed;
                    let _ = self.ed.set_collapsed(cur, c);
                    if c {
                        self.sel = vec![cur];
                    }
                }
            }
            Action::Undo => {
                if self.ed.undo() {
                    self.after_history();
                }
            }
            Action::Redo => {
                if self.ed.redo() {
                    self.after_history();
                }
            }
            Action::Copy => self.copy(),
            Action::Cut => {
                self.copy();
                self.delete_selection();
            }
            Action::Paste(t) => self.paste(t, false),
            Action::PasteSibling(t) => self.paste(t, true),
            Action::BeginGrab => {
                let ids = self.selected_roots().into_iter().filter(|&i| self.ed.doc.parent(i).is_some()).collect::<Vec<_>>();
                if let Some(&first) = ids.first() {
                    let target = self.ed.doc.parent(first).unwrap();
                    self.grab = Some(Grab { ids, target });
                    self.status = "Grab: ↑/↓ choose new parent, Enter drop, Esc cancel".into();
                }
            }
            Action::GrabCycle(_) | Action::GrabDrop => {}
            Action::ShowLevel(n) => self.show_level(n),
            Action::Select { id, extend } => {
                if !self.ed.doc.contains(id) {
                    return;
                }
                if extend {
                    if let Some(p) = self.sel.iter().position(|&s| s == id) {
                        if self.sel.len() > 1 {
                            self.sel.remove(p);
                        }
                    } else {
                        self.sel.push(id);
                    }
                } else {
                    self.select_only(id);
                }
            }
            Action::SelectAll => {
                self.sel = self.layout.order.clone();
            }
            Action::MoveSelectionTo(target) => self.move_selection_to(target),
            Action::ResetLayout { selection_only } => self.reset_layout(selection_only),
        }
        self.ensure_layout();
    }

    fn reset_layout(&mut self, selection_only: bool) {
        let roots = if selection_only { self.selected_roots() } else { vec![self.ed.doc.root()] };
        self.ed.begin_group();
        let n = self.ed.clear_offsets(&roots);
        self.ed.end_group();
        self.focus_request = true;
        self.status = if n == 0 {
            "Nothing to reset: everything is already auto-laid-out".into()
        } else {
            format!("Auto-layout restored ({n} position(s) reset)")
        };
    }

    fn after_history(&mut self) {
        self.sel.retain(|&i| self.ed.doc.contains(i));
        if self.sel.is_empty() {
            self.sel = vec![self.ed.doc.root()];
        }
        self.ensure_layout();
        self.focus_request = true;
    }

    fn dispatch_editing(&mut self, action: Action, e: Edit) {
        let fresh_empty = self.editing_fresh_empty();
        match action {
            Action::Tab => {
                if fresh_empty {
                    // Indent under the previous sibling.
                    if !self.ed.indent(e.id) {
                        self.status = "No previous sibling to indent under".into();
                    }
                } else {
                    self.commit_edit();
                    self.start_new_child(e.id);
                }
            }
            Action::Enter => {
                if fresh_empty {
                    if !self.ed.outdent(e.id) {
                        self.status = "Already at the top level".into();
                    }
                } else {
                    self.commit_edit();
                    if self.ed.doc.parent(e.id).is_none() {
                        self.start_new_child(e.id);
                    } else {
                        self.start_new_sibling(e.id, true);
                    }
                }
            }
            Action::Escape => {
                if fresh_empty {
                    let parent = self.ed.doc.parent(e.id);
                    self.edit = None;
                    self.ed.abort_group();
                    let target = parent.filter(|&p| self.ed.doc.contains(p)).unwrap_or(self.ed.doc.root());
                    self.select_only(target);
                } else {
                    self.commit_edit();
                }
            }
            // Everything else commits first so shortcuts act on the committed text.
            other => {
                self.commit_edit();
                self.dispatch(other);
                return;
            }
        }
        self.ensure_layout();
    }

    fn dispatch_grab(&mut self, action: Action) {
        let Some(grab) = self.grab.clone() else { return };
        // Candidate parents: visible nodes outside the grabbed subtrees.
        let cands: Vec<NodeId> =
            self.layout.order.iter().copied().filter(|&n| !grab.ids.iter().any(|&g| self.ed.doc.is_ancestor_or_self(g, n))).collect();
        match action {
            Action::Escape => self.grab = None,
            Action::Enter | Action::GrabDrop => {
                self.grab = None;
                self.move_selection_to_ids(&grab.ids, grab.target);
            }
            Action::Navigate { dir: Dir::Up, .. } | Action::GrabCycle(-1) => self.cycle_grab(&cands, -1),
            Action::Navigate { dir: Dir::Down, .. } | Action::GrabCycle(1) => self.cycle_grab(&cands, 1),
            Action::Navigate { dir, .. } => {
                if let Some(n) = spatial_neighbor(&self.layout, grab.target, dir.into()) {
                    if cands.contains(&n) {
                        self.grab.as_mut().unwrap().target = n;
                        self.focus_request = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn cycle_grab(&mut self, cands: &[NodeId], delta: i32) {
        if cands.is_empty() {
            return;
        }
        let g = self.grab.as_mut().unwrap();
        let i = cands.iter().position(|&c| c == g.target).unwrap_or(0) as i32;
        let n = cands.len() as i32;
        g.target = cands[((i + delta).rem_euclid(n)) as usize];
        self.focus_request = true;
    }

    /// The node the grab would drop under (for highlighting).
    pub fn grab_target(&self) -> Option<NodeId> {
        self.grab.as_ref().map(|g| g.target)
    }

    fn move_selection_to(&mut self, target: NodeId) {
        let ids = self.selected_roots();
        self.move_selection_to_ids(&ids, target);
    }

    fn move_selection_to_ids(&mut self, ids: &[NodeId], target: NodeId) {
        self.ed.begin_group();
        let mut moved = 0;
        for &id in ids {
            if self.ed.doc.parent(id).is_none() {
                continue;
            }
            if self.ed.move_to(id, target, usize::MAX).is_ok() {
                moved += 1;
            }
        }
        self.ed.end_group();
        if moved > 0 {
            if self.ed.doc.node(target).collapsed {
                let _ = self.ed.set_collapsed(target, false);
            }
            self.sel = ids.to_vec();
            self.focus_request = true;
            self.status = format!("Moved {moved} node(s)");
        } else {
            self.status = "Nothing moved (cannot move a node into itself)".into();
        }
        self.ensure_layout();
    }

    fn delete_selection(&mut self) {
        let ids: Vec<NodeId> = self.selected_roots().into_iter().filter(|&i| self.ed.doc.parent(i).is_some()).collect();
        if ids.is_empty() {
            self.status = "The root node cannot be deleted".into();
            return;
        }
        let first = ids[0];
        let fallback = self
            .ed
            .doc
            .prev_sibling(first)
            .or_else(|| self.ed.doc.next_sibling(first))
            .or_else(|| self.ed.doc.parent(first))
            .unwrap_or(self.ed.doc.root());
        self.ed.begin_group();
        for id in &ids {
            let _ = self.ed.delete(*id);
        }
        self.ed.end_group();
        let fb = if ids.contains(&fallback) { self.ed.doc.root() } else { fallback };
        self.select_only(if self.ed.doc.contains(fb) { fb } else { self.ed.doc.root() });
    }

    fn nudge(&mut self, dir: Dir) {
        let mut ids = self.selected_roots();
        ids.retain(|&i| self.ed.doc.parent(i).is_some());
        if dir == Dir::Down || dir == Dir::Right {
            ids.reverse();
        }
        self.ed.begin_group();
        let mut any = false;
        for id in ids {
            any |= match dir {
                Dir::Up => self.ed.reorder(id, -1),
                Dir::Down => self.ed.reorder(id, 1),
                Dir::Left => self.ed.outdent(id),
                Dir::Right => self.ed.indent(id),
            };
        }
        self.ed.end_group();
        if any {
            self.focus_request = true;
        }
    }

    fn show_level(&mut self, n: usize) {
        self.ed.begin_group();
        for id in self.ed.doc.subtree_ids(self.ed.doc.root()) {
            let depth = self.ed.doc.depth(id);
            let has_kids = !self.ed.doc.children(id).is_empty();
            let collapsed = n != 0 && has_kids && depth + 1 >= n;
            let _ = self.ed.set_collapsed(id, collapsed);
        }
        self.ed.end_group();
        let root = self.ed.doc.root();
        let vis = self.ed.doc.visible_ids();
        self.sel.retain(|i| vis.contains(i));
        if self.sel.is_empty() {
            self.sel = vec![root];
        }
    }

    fn copy(&mut self) {
        let roots = self.selected_roots();
        let Some(&first) = roots.first() else { return };
        let tree = self.ed.doc.to_subtree(first);
        let text = outline::to_outline(&self.ed.doc, first);
        self.internal_clip = Some((text.trim().to_string(), tree));
        self.clip_out = Some(text);
    }

    fn paste(&mut self, text: Option<String>, as_sibling: bool) {
        let forest: Vec<Subtree> = match (&text, &self.internal_clip) {
            (Some(t), Some((clip_text, tree))) if t.trim() == clip_text => vec![tree.clone()],
            (None, Some((_, tree))) => vec![tree.clone()],
            (Some(t), _) => outline::parse_outline(t),
            (None, None) => Vec::new(),
        };
        if forest.is_empty() {
            return;
        }
        let cur = self.primary();
        let (parent, mut index) = match (as_sibling, self.ed.doc.parent(cur), self.ed.doc.index_in_parent(cur)) {
            (true, Some(p), Some(i)) => (p, i + 1),
            _ => (cur, usize::MAX),
        };
        self.ed.begin_group();
        if self.ed.doc.node(parent).collapsed {
            let _ = self.ed.set_collapsed(parent, false);
        }
        let mut new_ids = Vec::new();
        for t in &forest {
            if let Ok(id) = self.ed.paste(parent, index, t) {
                new_ids.push(id);
                if index != usize::MAX {
                    index += 1;
                }
            }
        }
        self.ed.end_group();
        if !new_ids.is_empty() {
            self.status = format!("Pasted {} node(s)", forest.iter().map(Subtree::count).sum::<usize>());
            self.sel = new_ids;
            self.focus_request = true;
        }
        self.ensure_layout();
    }

    /// Insert an imported tree as a new branch under the selection (one undo step).
    pub fn import_branch(&mut self, tree: &Subtree) {
        let parent = self.primary();
        self.ed.begin_group();
        let id = self.ed.paste(parent, usize::MAX, tree);
        self.ed.end_group();
        if let Ok(id) = id {
            self.select_only(id);
        }
        self.ensure_layout();
    }

    /// Case-insensitive substring search over node text, in document order.
    pub fn find(&self, query: &str) -> Vec<NodeId> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.ed
            .doc
            .subtree_ids(self.ed.doc.root())
            .into_iter()
            .filter(|&i| {
                let n = self.ed.doc.node(i);
                n.text.to_lowercase().contains(&q) || n.note.as_deref().is_some_and(|x| x.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn jump_to(&mut self, id: NodeId) {
        if self.ed.doc.contains(id) {
            self.select_only(id);
            self.ensure_layout();
        }
    }
}
