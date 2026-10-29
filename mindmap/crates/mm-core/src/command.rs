use crate::doc::{DiagramMode, Document, Node, NodeId, Shape};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("no such node")]
    NoSuchNode,
    #[error("the root node cannot be removed or moved")]
    RootImmutable,
    #[error("a node cannot be moved into its own subtree")]
    WouldCycle,
    #[error("node id already in use")]
    IdInUse,
}

/// Every document mutation. `Document::apply` returns the inverse command.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `nodes[0]` is the subtree root; links between the nodes are already set.
    InsertSubtree { parent: NodeId, index: usize, nodes: Vec<(NodeId, Node)> },
    RemoveSubtree { id: NodeId },
    /// `index` is the position among the new parent's children *after* removal.
    Move { id: NodeId, parent: NodeId, index: usize },
    SetText { id: NodeId, text: String },
    SetNote { id: NodeId, note: Option<String> },
    SetColor { id: NodeId, color: Option<[u8; 3]> },
    SetShape { id: NodeId, shape: Shape },
    SetCollapsed { id: NodeId, collapsed: bool },
    SetOffset { id: NodeId, offset: Option<[f32; 2]> },
    SetMode(DiagramMode),
}

impl Document {
    pub fn apply(&mut self, cmd: Command) -> Result<Command, CoreError> {
        let inverse = self.apply_inner(cmd)?;
        self.revision += 1;
        Ok(inverse)
    }

    fn require(&self, id: NodeId) -> Result<(), CoreError> {
        self.contains(id).then_some(()).ok_or(CoreError::NoSuchNode)
    }

    fn apply_inner(&mut self, cmd: Command) -> Result<Command, CoreError> {
        match cmd {
            Command::InsertSubtree { parent, index, nodes } => {
                self.require(parent)?;
                let top = nodes.first().ok_or(CoreError::NoSuchNode)?.0;
                for (id, _) in &nodes {
                    if self.contains(*id) {
                        return Err(CoreError::IdInUse);
                    }
                }
                let need = nodes.iter().map(|(id, _)| id.index() + 1).max().unwrap_or(0);
                if self.slots.len() < need {
                    self.slots.resize(need, None);
                }
                for (id, mut n) in nodes {
                    if id == top {
                        n.parent = Some(parent);
                    }
                    self.slots[id.index()] = Some(n);
                }
                let kids = &mut self.node_mut(parent).children;
                let at = index.min(kids.len());
                kids.insert(at, top);
                Ok(Command::RemoveSubtree { id: top })
            }
            Command::RemoveSubtree { id } => {
                self.require(id)?;
                if id == self.root {
                    return Err(CoreError::RootImmutable);
                }
                let parent = self.node(id).parent.ok_or(CoreError::RootImmutable)?;
                let index = self.index_in_parent(id).ok_or(CoreError::NoSuchNode)?;
                let ids = self.subtree_ids(id);
                let nodes: Vec<(NodeId, Node)> =
                    ids.iter().map(|&i| (i, self.node(i).clone())).collect();
                self.node_mut(parent).children.remove(index);
                for i in ids {
                    self.slots[i.index()] = None;
                }
                Ok(Command::InsertSubtree { parent, index, nodes })
            }
            Command::Move { id, parent, index } => {
                self.require(id)?;
                self.require(parent)?;
                if id == self.root {
                    return Err(CoreError::RootImmutable);
                }
                if self.is_ancestor_or_self(id, parent) {
                    return Err(CoreError::WouldCycle);
                }
                let old_parent = self.node(id).parent.ok_or(CoreError::RootImmutable)?;
                let old_index = self.index_in_parent(id).ok_or(CoreError::NoSuchNode)?;
                self.node_mut(old_parent).children.remove(old_index);
                let kids = &mut self.node_mut(parent).children;
                let at = index.min(kids.len());
                kids.insert(at, id);
                self.node_mut(id).parent = Some(parent);
                Ok(Command::Move { id, parent: old_parent, index: old_index })
            }
            Command::SetText { id, text } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).text, text);
                Ok(Command::SetText { id, text: old })
            }
            Command::SetNote { id, note } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).note, note);
                Ok(Command::SetNote { id, note: old })
            }
            Command::SetColor { id, color } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).color, color);
                Ok(Command::SetColor { id, color: old })
            }
            Command::SetShape { id, shape } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).shape, shape);
                Ok(Command::SetShape { id, shape: old })
            }
            Command::SetCollapsed { id, collapsed } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).collapsed, collapsed);
                Ok(Command::SetCollapsed { id, collapsed: old })
            }
            Command::SetOffset { id, offset } => {
                self.require(id)?;
                let old = std::mem::replace(&mut self.node_mut(id).offset, offset);
                Ok(Command::SetOffset { id, offset: old })
            }
            Command::SetMode(mode) => {
                let old = std::mem::replace(&mut self.mode, mode);
                Ok(Command::SetMode(old))
            }
        }
    }
}
