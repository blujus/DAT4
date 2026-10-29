//! Pure layout: document + measured node sizes -> rectangles and edges.
#![forbid(unsafe_code)]

mod fishbone;
mod nav;
mod tree;

pub use nav::{hit_test, spatial_neighbor, Direction};

use mm_core::{DiagramMode, Document, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn translated(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// Bezier with horizontal tangents (left/right trees).
    CurveH,
    /// Bezier with vertical tangents (top-down).
    CurveV,
    /// Straight segment (fishbone spine, bones, ribs).
    Line,
}

#[derive(Clone, Copy, Debug)]
pub struct Edge {
    /// The node this edge belongs to (the child end); used for colouring.
    pub node: NodeId,
    pub a: (f32, f32),
    pub b: (f32, f32),
    pub kind: EdgeKind,
    /// Fishbone spine / bones are drawn heavier.
    pub heavy: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Indexed by `NodeId`; `None` for deleted or hidden (collapsed-away) nodes.
    pub rects: Vec<Option<Rect>>,
    pub edges: Vec<Edge>,
    /// Visible nodes in preorder.
    pub order: Vec<NodeId>,
    pub bounds: Rect,
}

impl Layout {
    pub fn rect(&self, id: NodeId) -> Option<Rect> {
        self.rects.get(id.index()).copied().flatten()
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub gap_x: f32,
    pub gap_y: f32,
    pub fishbone_slope: f32,
    pub fishbone_rib_gap: f32,
    pub fishbone_min_bone: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config { gap_x: 44.0, gap_y: 10.0, fishbone_slope: 0.45, fishbone_rib_gap: 22.0, fishbone_min_bone: 70.0 }
    }
}

/// `measure(id)` returns the node's (width, height) in layout units.
pub fn compute(doc: &Document, measure: &dyn Fn(NodeId) -> (f32, f32), cfg: &Config) -> Layout {
    let order = doc.visible_ids();
    let mut layout = match doc.mode {
        DiagramMode::Fishbone => fishbone::layout(doc, measure, cfg),
        mode => tree::layout(doc, mode, measure, cfg),
    };
    let mut bounds: Option<Rect> = None;
    for r in layout.rects.iter().flatten() {
        bounds = Some(bounds.map_or(*r, |b| b.union(r)));
    }
    for e in &layout.edges {
        for p in [e.a, e.b] {
            let pr = Rect::new(p.0, p.1, 0.0, 0.0);
            bounds = Some(bounds.map_or(pr, |b| b.union(&pr)));
        }
    }
    layout.bounds = bounds.unwrap_or_default();
    layout.order = order;
    layout
}
