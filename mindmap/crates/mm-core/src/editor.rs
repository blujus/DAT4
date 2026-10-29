use crate::command::{Command, CoreError};
use crate::doc::{Document, Node, NodeId, Shape, Subtree};

type Result<T> = std::result::Result<T, CoreError>;

/// A document plus undo/redo history and transaction grouping.
#[derive(Clone, Debug)]
pub struct Editor {
    pub doc: Document,
    undo: Vec<Vec<Command>>,
    redo: Vec<Vec<Command>>,
    group: Option<Vec<Command>>,
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Editor { doc, undo: Vec::new(), redo: Vec::new(), group: None }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.group.as_ref().is_some_and(|g| !g.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn in_group(&self) -> bool {
        self.group.is_some()
    }

    /// Everything applied until `end_group` becomes a single undo step.
    pub fn begin_group(&mut self) {
        if self.group.is_none() {
            self.group = Some(Vec::new());
        }
    }

    pub fn end_group(&mut self) {
        if let Some(g) = self.group.take() {
            if !g.is_empty() {
                self.undo.push(g);
            }
        }
    }

    /// Roll back and discard the open group without touching redo history.
    pub fn abort_group(&mut self) {
        if let Some(g) = self.group.take() {
            for inv in g.into_iter().rev() {
                let _ = self.doc.apply(inv);
            }
        }
    }

    pub fn apply(&mut self, cmd: Command) -> Result<()> {
        let coalesce_id = match &cmd {
            Command::SetText { id, .. } => Some(*id),
            _ => None,
        };
        let inv = self.doc.apply(cmd)?;
        match &mut self.group {
            Some(g) => {
                // Keystrokes inside one group: keep only the oldest SetText inverse.
                let same = matches!((g.last(), coalesce_id),
                    (Some(Command::SetText { id: last, .. }), Some(id)) if *last == id);
                if !same {
                    g.push(inv);
                }
            }
            None => self.undo.push(vec![inv]),
        }
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        self.end_group();
        let Some(entry) = self.undo.pop() else { return false };
        let mut redo_entry = Vec::with_capacity(entry.len());
        for inv in entry.into_iter().rev() {
            if let Ok(r) = self.doc.apply(inv) {
                redo_entry.push(r);
            }
        }
        self.redo.push(redo_entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.end_group();
        let Some(entry) = self.redo.pop() else { return false };
        let mut undo_entry = Vec::with_capacity(entry.len());
        for cmd in entry.into_iter().rev() {
            if let Ok(r) = self.doc.apply(cmd) {
                undo_entry.push(r);
            }
        }
        self.undo.push(undo_entry);
        true
    }

    // ---- high-level operations -------------------------------------------------

    pub fn add_child(&mut self, parent: NodeId, text: &str) -> Result<NodeId> {
        self.add_child_at(parent, usize::MAX, text)
    }

    pub fn add_child_at(&mut self, parent: NodeId, index: usize, text: &str) -> Result<NodeId> {
        let id = self.doc.next_id();
        self.apply(Command::InsertSubtree {
            parent,
            index,
            nodes: vec![(id, Node::with_text(text))],
        })?;
        Ok(id)
    }

    /// New sibling next to `id`. For the root this adds a child instead.
    pub fn add_sibling(&mut self, id: NodeId, after: bool, text: &str) -> Result<NodeId> {
        match (self.doc.parent(id), self.doc.index_in_parent(id)) {
            (Some(p), Some(i)) => self.add_child_at(p, if after { i + 1 } else { i }, text),
            _ => self.add_child(id, text),
        }
    }

    pub fn delete(&mut self, id: NodeId) -> Result<()> {
        self.apply(Command::RemoveSubtree { id })
    }

    pub fn move_to(&mut self, id: NodeId, parent: NodeId, index: usize) -> Result<()> {
        self.apply(Command::Move { id, parent, index })
    }

    /// Become the last child of the previous sibling. False if there is none.
    pub fn indent(&mut self, id: NodeId) -> bool {
        let Some(prev) = self.doc.prev_sibling(id) else { return false };
        if self.doc.node(prev).collapsed {
            let _ = self.apply(Command::SetCollapsed { id: prev, collapsed: false });
        }
        self.move_to(id, prev, usize::MAX).is_ok()
    }

    /// Become the sibling right after the parent. False if the parent is the root.
    pub fn outdent(&mut self, id: NodeId) -> bool {
        let Some(p) = self.doc.parent(id) else { return false };
        let Some(gp) = self.doc.parent(p) else { return false };
        let Some(pi) = self.doc.index_in_parent(p) else { return false };
        self.move_to(id, gp, pi + 1).is_ok()
    }

    /// Move among siblings by `delta` positions. False at the ends.
    pub fn reorder(&mut self, id: NodeId, delta: isize) -> bool {
        let (Some(p), Some(i)) = (self.doc.parent(id), self.doc.index_in_parent(id)) else {
            return false;
        };
        let len = self.doc.children(p).len() as isize;
        let target = i as isize + delta;
        if target < 0 || target >= len {
            return false;
        }
        self.move_to(id, p, target as usize).is_ok()
    }

    pub fn set_text(&mut self, id: NodeId, text: &str) -> Result<()> {
        if self.doc.get(id).ok_or(CoreError::NoSuchNode)?.text == text {
            return Ok(());
        }
        self.apply(Command::SetText { id, text: text.to_string() })
    }

    pub fn set_note(&mut self, id: NodeId, note: Option<String>) -> Result<()> {
        self.apply(Command::SetNote { id, note })
    }

    pub fn set_color(&mut self, id: NodeId, color: Option<[u8; 3]>) -> Result<()> {
        self.apply(Command::SetColor { id, color })
    }

    pub fn set_shape(&mut self, id: NodeId, shape: Shape) -> Result<()> {
        self.apply(Command::SetShape { id, shape })
    }

    pub fn set_collapsed(&mut self, id: NodeId, collapsed: bool) -> Result<()> {
        if self.doc.get(id).ok_or(CoreError::NoSuchNode)?.collapsed == collapsed {
            return Ok(());
        }
        self.apply(Command::SetCollapsed { id, collapsed })
    }

    pub fn set_mode(&mut self, mode: crate::DiagramMode) -> Result<()> {
        if self.doc.mode == mode {
            return Ok(());
        }
        self.apply(Command::SetMode(mode))
    }

    /// Insert a copy of `tree` under `parent` at `index`; returns the new root id.
    pub fn paste(&mut self, parent: NodeId, index: usize, tree: &Subtree) -> Result<NodeId> {
        let base = self.doc.next_id().0;
        let nodes = tree.flatten(base);
        let id = nodes[0].0;
        self.apply(Command::InsertSubtree { parent, index, nodes })?;
        Ok(id)
    }

    /// Drop selected ids that have a selected ancestor, in document order.
    pub fn top_level(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let set: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
        self.doc
            .visible_ids()
            .into_iter()
            .filter(|&i| {
                set.contains(&i) && {
                    let mut cur = self.doc.parent(i);
                    let mut ok = true;
                    while let Some(c) = cur {
                        if set.contains(&c) {
                            ok = false;
                            break;
                        }
                        cur = self.doc.parent(c);
                    }
                    ok
                }
            })
            .collect()
    }
}
