use crate::{Config, Edge, EdgeKind, Layout, Rect};
use mm_core::{DiagramMode, Document, NodeId};

pub(crate) struct Ctx<'a> {
    pub doc: &'a Document,
    pub size: Vec<(f32, f32)>,
    pub memo: Vec<f32>,
    pub rects: Vec<Option<Rect>>,
    pub gap_x: f32,
    pub gap_y: f32,
}

impl<'a> Ctx<'a> {
    pub fn new(doc: &'a Document, measure: &dyn Fn(NodeId) -> (f32, f32), cfg: &Config) -> Self {
        let n = doc.slots_len();
        let mut size = vec![(0.0, 0.0); n];
        for id in doc.visible_ids() {
            size[id.index()] = measure(id);
        }
        Ctx { doc, size, memo: vec![f32::NAN; n], rects: vec![None; n], gap_x: cfg.gap_x, gap_y: cfg.gap_y }
    }

    pub fn kids(&self, id: NodeId) -> &'a [NodeId] {
        let n = self.doc.node(id);
        if n.collapsed {
            &[]
        } else {
            &n.children
        }
    }

    /// Vertical extent of a subtree in a left/right layout.
    pub fn height(&mut self, id: NodeId) -> f32 {
        let m = self.memo[id.index()];
        if !m.is_nan() {
            return m;
        }
        let kids = self.kids(id);
        let mut total = 0.0;
        for (i, &c) in kids.iter().enumerate() {
            if i > 0 {
                total += self.gap_y;
            }
            total += self.height(c);
        }
        let h = total.max(self.size[id.index()].1);
        self.memo[id.index()] = h;
        h
    }

    /// Horizontal extent of a subtree (widest root-to-leaf chain) in a left/right layout.
    pub fn extent(&self, id: NodeId) -> f32 {
        let w = self.size[id.index()].0;
        let kids = self.kids(id);
        if kids.is_empty() {
            return w;
        }
        w + self.gap_x + kids.iter().map(|&c| self.extent(c)).fold(0.0, f32::max)
    }

    /// Place `id` so its vertical centre is at `y_top + height/2`.
    /// `dir = 1`: node's left edge at `anchor_x`; `dir = -1`: node's right edge at `anchor_x`.
    pub fn place_h(&mut self, id: NodeId, anchor_x: f32, y_top: f32, dir: f32, side: &mut [i8]) {
        let h = self.height(id);
        let (w, nh) = self.size[id.index()];
        let cy = y_top + h / 2.0;
        let x = if dir > 0.0 { anchor_x } else { anchor_x - w };
        self.rects[id.index()] = Some(Rect::new(x, cy - nh / 2.0, w, nh));
        side[id.index()] = dir as i8;
        let kids = self.kids(id);
        let total = self.stack_height(kids);
        let child_anchor = if dir > 0.0 { x + w + self.gap_x } else { x - self.gap_x };
        let mut y = y_top + (h - total) / 2.0;
        for &c in kids {
            let ch = self.height(c);
            self.place_h(c, child_anchor, y, dir, side);
            y += ch + self.gap_y;
        }
    }

    pub fn stack_height(&mut self, kids: &[NodeId]) -> f32 {
        let mut total = 0.0;
        for (i, &c) in kids.iter().enumerate() {
            if i > 0 {
                total += self.gap_y;
            }
            total += self.height(c);
        }
        total
    }

    fn width(&mut self, id: NodeId) -> f32 {
        let m = self.memo[id.index()];
        if !m.is_nan() {
            return m;
        }
        let kids = self.kids(id);
        let mut total = 0.0;
        for (i, &c) in kids.iter().enumerate() {
            if i > 0 {
                total += self.gap_x * 0.35;
            }
            total += self.width(c);
        }
        let w = total.max(self.size[id.index()].0);
        self.memo[id.index()] = w;
        w
    }

    fn place_v(&mut self, id: NodeId, x_left: f32, y_top: f32) {
        let w = self.width(id);
        let (nw, nh) = self.size[id.index()];
        self.rects[id.index()] = Some(Rect::new(x_left + (w - nw) / 2.0, y_top, nw, nh));
        let kids = self.kids(id);
        let mut total = 0.0;
        for (i, &c) in kids.iter().enumerate() {
            if i > 0 {
                total += self.gap_x * 0.35;
            }
            total += self.width(c);
        }
        let mut x = x_left + (w - total) / 2.0;
        let y = y_top + nh + self.gap_y * 3.0;
        for &c in kids {
            let cw = self.width(c);
            self.place_v(c, x, y);
            x += cw + self.gap_x * 0.35;
        }
    }
}

pub(crate) fn layout(doc: &Document, mode: DiagramMode, measure: &dyn Fn(NodeId) -> (f32, f32), cfg: &Config) -> Layout {
    let mut cx = Ctx::new(doc, measure, cfg);
    let mut side = vec![0i8; cx.rects.len()];
    let root = doc.root();
    match mode {
        DiagramMode::Right => {
            let h = cx.height(root);
            cx.place_h(root, 0.0, -h / 2.0, 1.0, &mut side);
        }
        DiagramMode::TopDown => {
            let w = cx.width(root);
            cx.place_v(root, -w / 2.0, 0.0);
        }
        _ => {
            // Balanced mind map: root centred, children split into right/left halves.
            let (rw, rh) = cx.size[root.index()];
            cx.rects[root.index()] = Some(Rect::new(-rw / 2.0, -rh / 2.0, rw, rh));
            let kids = cx.kids(root);
            let heights: Vec<f32> = kids.iter().map(|&c| cx.height(c)).collect();
            let total: f32 = heights.iter().sum::<f32>() + cfg.gap_y * kids.len().saturating_sub(1) as f32;
            let mut acc = 0.0;
            let (mut right, mut left) = (Vec::new(), Vec::new());
            for (&c, &h) in kids.iter().zip(&heights) {
                if acc + h / 2.0 <= total / 2.0 || right.is_empty() {
                    right.push(c);
                } else {
                    left.push(c);
                }
                acc += h + cfg.gap_y;
            }
            for (group, dir) in [(right, 1.0f32), (left, -1.0f32)] {
                if group.is_empty() {
                    continue;
                }
                let gh = cx.stack_height(&group);
                let anchor = if dir > 0.0 { rw / 2.0 + cfg.gap_x } else { -rw / 2.0 - cfg.gap_x };
                let mut y = -gh / 2.0;
                for c in group {
                    let ch = cx.height(c);
                    cx.place_h(c, anchor, y, dir, &mut side);
                    y += ch + cfg.gap_y;
                }
            }
        }
    }

    // Manual offsets move a node together with its whole subtree.
    let mut stack = vec![(root, 0.0f32, 0.0f32)];
    while let Some((id, ax, ay)) = stack.pop() {
        let n = doc.node(id);
        let (mut dx, mut dy) = (ax, ay);
        if let Some([ox, oy]) = n.offset {
            dx += ox;
            dy += oy;
        }
        if let Some(r) = cx.rects[id.index()].as_mut() {
            *r = r.translated(dx, dy);
        }
        if !n.collapsed {
            for &c in &n.children {
                stack.push((c, dx, dy));
            }
        }
    }

    // Edges from final rectangles.
    let mut edges = Vec::new();
    for id in doc.visible_ids() {
        let Some(p) = doc.parent(id) else { continue };
        let (Some(pr), Some(cr)) = (cx.rects[p.index()], cx.rects[id.index()]) else { continue };
        let (a, b, kind) = match mode {
            DiagramMode::TopDown => ((pr.center().0, pr.bottom()), (cr.center().0, cr.y), EdgeKind::CurveV),
            _ if side[id.index()] >= 0 => ((pr.right(), pr.center().1), (cr.x, cr.center().1), EdgeKind::CurveH),
            _ => ((pr.x, pr.center().1), (cr.right(), cr.center().1), EdgeKind::CurveH),
        };
        edges.push(Edge { node: id, a, b, kind, heavy: false });
    }
    Layout { rects: cx.rects, edges, order: Vec::new(), bounds: Rect::default() }
}
