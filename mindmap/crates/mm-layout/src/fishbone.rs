use crate::tree::Ctx;
use crate::{Config, Edge, EdgeKind, Layout, Rect};
use mm_core::{Document, NodeId};

/// Ishikawa layout. Root = effect (head, right). Level 1 = cause categories on
/// diagonal bones alternating above/below the spine. Level 2+ = ribs/sub-causes
/// extending to the left of the bone.
pub(crate) fn layout(doc: &Document, measure: &dyn Fn(NodeId) -> (f32, f32), cfg: &Config) -> Layout {
    let mut cx = Ctx::new(doc, measure, cfg);
    let root = doc.root();
    let mut edges: Vec<Edge> = Vec::new();
    let mut side = vec![-1i8; cx.rects.len()];

    let (rw, rh) = cx.size[root.index()];
    cx.rects[root.index()] = Some(Rect::new(0.0, -rh / 2.0, rw, rh));

    let bones: Vec<NodeId> = cx.kids(root).to_vec();
    // Next free spine attach point (x) for each side; bones are paired top/bottom.
    let mut cursor = [-30.0f32, -30.0f32];
    let rib_gap = cfg.fishbone_rib_gap;
    let pad = 12.0;
    let mut tail_x = -120.0f32;

    for (bi, &bone) in bones.iter().enumerate() {
        let s: f32 = if bi % 2 == 0 { -1.0 } else { 1.0 }; // -1 = above the spine
        let ci = if bi % 2 == 0 { 0 } else { 1 };
        let (cw, ch) = cx.size[bone.index()];
        let ribs: Vec<NodeId> = cx.kids(bone).to_vec();
        let rib_heights: Vec<f32> = ribs.iter().map(|&r| cx.height(r)).collect();
        let ribs_total: f32 = rib_heights.iter().sum::<f32>() + cfg.gap_y * ribs.len().saturating_sub(1) as f32;
        let max_rib_extent = ribs.iter().map(|&r| cx.extent(r)).fold(0.0, f32::max);

        let bone_h = (ribs_total + 2.0 * pad).max(cfg.fishbone_min_bone);
        let dx = bone_h * cfg.fishbone_slope;
        let px = cursor[ci];
        let qx = px - dx;
        // Category box sits just beyond the bone end.
        let (qy, box_y) = if s < 0.0 { (-bone_h, -bone_h - ch) } else { (bone_h, bone_h) };
        cx.rects[bone.index()] = Some(Rect::new(qx - cw / 2.0, box_y, cw, ch));
        edges.push(Edge { node: bone, a: (qx, qy), b: (px, 0.0), kind: EdgeKind::Line, heavy: true });

        // Ribs stacked top-to-bottom within the bone's vertical span.
        let (span_top, _) = if s < 0.0 { (-bone_h, 0.0) } else { (0.0, bone_h) };
        let mut y = span_top + (bone_h - ribs_total) / 2.0;
        for (&rib, &rh_) in ribs.iter().zip(&rib_heights) {
            let cy = y + rh_ / 2.0;
            let t = (cy.abs() / bone_h).clamp(0.0, 1.0);
            let attach_x = px - dx * t;
            cx.place_h(rib, attach_x - rib_gap, y, -1.0, &mut side);
            let rr = cx.rects[rib.index()].expect("rib placed");
            edges.push(Edge { node: rib, a: (rr.right(), cy), b: (attach_x, cy), kind: EdgeKind::Line, heavy: false });
            // Deeper levels use ordinary curved tree edges (growing leftwards).
            let mut stack: Vec<NodeId> = cx.kids(rib).to_vec();
            while let Some(d) = stack.pop() {
                let p = doc.parent(d).expect("has parent");
                let (pr, cr) = (cx.rects[p.index()].expect("placed"), cx.rects[d.index()].expect("placed"));
                edges.push(Edge {
                    node: d,
                    a: (pr.x, pr.center().1),
                    b: (cr.right(), cr.center().1),
                    kind: EdgeKind::CurveH,
                    heavy: false,
                });
                stack.extend(cx.kids(d));
            }
            y += rh_ + cfg.gap_y;
        }

        let footprint = (dx + rib_gap + max_rib_extent).max(dx + cw / 2.0) + cfg.gap_x * 0.6;
        cursor[ci] = px - footprint;
        tail_x = tail_x.min(cursor[ci]);
    }

    // Spine runs from the tail to the head.
    edges.push(Edge { node: root, a: (tail_x.min(-120.0) + 40.0, 0.0), b: (0.0, 0.0), kind: EdgeKind::Line, heavy: true });
    Layout { rects: cx.rects, edges, order: Vec::new(), bounds: Rect::default() }
}
