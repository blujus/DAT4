use serde::{Deserialize, Serialize};

/// Stable node handle. Ids are never reused within a document, so undo can
/// restore a deleted subtree under exactly the same ids.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    Rounded,
    Pill,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagramMode {
    /// Balanced two-sided mind map.
    #[default]
    MindMap,
    /// Left-to-right logic tree.
    Right,
    /// Top-down org chart.
    TopDown,
    /// Ishikawa / fishbone cause-and-effect diagram.
    Fishbone,
}

impl DiagramMode {
    pub const ALL: [DiagramMode; 4] = [
        DiagramMode::MindMap,
        DiagramMode::Right,
        DiagramMode::TopDown,
        DiagramMode::Fishbone,
    ];
    pub fn label(self) -> &'static str {
        match self {
            DiagramMode::MindMap => "Mind map",
            DiagramMode::Right => "Logic chart (left to right)",
            DiagramMode::TopDown => "Org chart (top down)",
            DiagramMode::Fishbone => "Fishbone (Ishikawa)",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub text: String,
    pub note: Option<String>,
    pub color: Option<[u8; 3]>,
    pub shape: Shape,
    pub collapsed: bool,
    /// Manual displacement from the auto-layout position (applies to the subtree).
    pub offset: Option<[f32; 2]>,
}

impl Node {
    pub fn with_text(text: impl Into<String>) -> Self {
        Node { text: text.into(), ..Default::default() }
    }
}

/// Plain recursive value used for clipboard, import, templates and export.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Subtree {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Subtree>,
}

impl Subtree {
    pub fn new(text: impl Into<String>) -> Self {
        Subtree { text: text.into(), ..Default::default() }
    }

    pub fn with_children(mut self, children: Vec<Subtree>) -> Self {
        self.children = children;
        self
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Subtree::count).sum::<usize>()
    }

    /// Flatten in preorder into `(id, node)` pairs with consecutive ids from `base`.
    /// The first node is the subtree root and has `parent == None`.
    pub fn flatten(&self, base: u32) -> Vec<(NodeId, Node)> {
        let mut out = Vec::with_capacity(self.count());
        self.flatten_into(base, None, &mut out);
        out
    }

    fn flatten_into(&self, base: u32, parent: Option<NodeId>, out: &mut Vec<(NodeId, Node)>) -> NodeId {
        let id = NodeId(base + out.len() as u32);
        out.push((
            id,
            Node {
                parent,
                children: Vec::new(),
                text: self.text.clone(),
                note: self.note.clone(),
                color: self.color,
                shape: self.shape,
                collapsed: self.collapsed,
                offset: None,
            },
        ));
        let slot = out.len() - 1;
        let mut kids = Vec::with_capacity(self.children.len());
        for c in &self.children {
            kids.push(c.flatten_into(base, Some(id), out));
        }
        out[slot].1.children = kids;
        id
    }
}

/// A single mind map. Arena of `Option<Node>`; ids are indices and never reused.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) slots: Vec<Option<Node>>,
    pub(crate) root: NodeId,
    pub mode: DiagramMode,
    /// Bumped on every successful mutation; caches key off this.
    pub revision: u64,
}

impl Document {
    pub fn new(root_text: impl Into<String>) -> Self {
        Document {
            slots: vec![Some(Node::with_text(root_text))],
            root: NodeId(0),
            mode: DiagramMode::default(),
            revision: 0,
        }
    }

    pub fn from_subtree(t: &Subtree) -> Self {
        let flat = t.flatten(0);
        let mut slots: Vec<Option<Node>> = Vec::with_capacity(flat.len());
        for (_, n) in flat {
            slots.push(Some(n));
        }
        Document { slots, root: NodeId(0), mode: DiagramMode::default(), revision: 0 }
    }

    pub fn to_subtree(&self, id: NodeId) -> Subtree {
        let n = self.node(id);
        Subtree {
            text: n.text.clone(),
            note: n.note.clone(),
            color: n.color,
            shape: n.shape,
            collapsed: n.collapsed,
            children: n.children.iter().map(|&c| self.to_subtree(c)).collect(),
        }
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots.get(id.index()).and_then(|n| n.as_ref())
    }

    /// Panics if the id is not live; use `get` when the id is untrusted.
    pub fn node(&self, id: NodeId) -> &Node {
        self.get(id).expect("live node id")
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.slots[id.index()].as_mut().expect("live node id")
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    /// Size of the id space (live + deleted slots); handy for id-indexed side tables.
    pub fn slots_len(&self) -> usize {
        self.slots.len()
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    // Low-level builders for importers; they bypass history. Call `check_invariants` after.

    /// Replace the root node's contents (keeps id 0).
    pub fn replace_root(&mut self, node: Node) {
        self.slots[self.root.index()] = Some(node);
    }

    /// Append a detached node, returning its id.
    pub fn push_raw(&mut self, node: Node) -> NodeId {
        self.slots.push(Some(node));
        NodeId(self.slots.len() as u32 - 1)
    }

    /// Append `child` to `parent`'s children and set the back-pointer.
    pub fn link_child(&mut self, parent: NodeId, child: NodeId) {
        self.node_mut(child).parent = Some(parent);
        self.node_mut(parent).children.push(child);
    }

    /// The id the next single insert will get.
    pub fn next_id(&self) -> NodeId {
        NodeId(self.slots.len() as u32)
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.get(id).and_then(|n| n.parent)
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.get(id).map(|n| n.children.as_slice()).unwrap_or(&[])
    }

    pub fn index_in_parent(&self, id: NodeId) -> Option<usize> {
        let p = self.parent(id)?;
        self.children(p).iter().position(|&c| c == id)
    }

    pub fn prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        let i = self.index_in_parent(id)?;
        let p = self.parent(id)?;
        (i > 0).then(|| self.children(p)[i - 1])
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let i = self.index_in_parent(id)?;
        let p = self.parent(id)?;
        self.children(p).get(i + 1).copied()
    }

    pub fn depth(&self, id: NodeId) -> usize {
        let mut d = 0;
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            d += 1;
            cur = p;
        }
        d
    }

    /// True if `anc` is `id` or one of its ancestors.
    pub fn is_ancestor_or_self(&self, anc: NodeId, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == anc {
                return true;
            }
            cur = self.parent(c);
        }
        false
    }

    /// Preorder ids of the subtree rooted at `id` (including it), ignoring collapse.
    pub fn subtree_ids(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children(n).iter().rev());
        }
        out
    }

    /// Preorder ids that are visible (descendants of collapsed nodes are skipped).
    pub fn visible_ids(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(n) = stack.pop() {
            out.push(n);
            let node = self.node(n);
            if !node.collapsed {
                stack.extend(node.children.iter().rev());
            }
        }
        out
    }

    /// Check structural invariants; used by tests and `mm-cli validate`.
    pub fn check_invariants(&self) -> Result<(), String> {
        let root = self.get(self.root).ok_or("root missing")?;
        if root.parent.is_some() {
            return Err("root has a parent".into());
        }
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                return Err(format!("node {id:?} reachable twice (cycle or shared child)"));
            }
            let n = self.get(id).ok_or_else(|| format!("dangling child {id:?}"))?;
            for &c in &n.children {
                let cn = self.get(c).ok_or_else(|| format!("dangling child {c:?}"))?;
                if cn.parent != Some(id) {
                    return Err(format!("child {c:?} does not point back to {id:?}"));
                }
                stack.push(c);
            }
        }
        if seen.len() != self.len() {
            return Err(format!("{} live nodes but {} reachable", self.len(), seen.len()));
        }
        Ok(())
    }
}
