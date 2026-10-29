use crate::Layout;
use mm_core::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Node whose rectangle is topmost under the point (later nodes win).
pub fn hit_test(layout: &Layout, x: f32, y: f32) -> Option<NodeId> {
    layout
        .order
        .iter()
        .rev()
        .copied()
        .find(|&id| layout.rect(id).is_some_and(|r| r.contains(x, y)))
}

/// Nearest visible node in `dir` from `from`, weighting off-axis distance double.
pub fn spatial_neighbor(layout: &Layout, from: NodeId, dir: Direction) -> Option<NodeId> {
    let fr = layout.rect(from)?;
    let (fx, fy) = fr.center();
    let mut best: Option<(f32, NodeId)> = None;
    for &id in &layout.order {
        if id == from {
            continue;
        }
        let Some(r) = layout.rect(id) else { continue };
        let (cx, cy) = r.center();
        let (primary, cross) = match dir {
            Direction::Right => (r.x - fr.right(), (cy - fy).abs()),
            Direction::Left => (fr.x - r.right(), (cy - fy).abs()),
            Direction::Down => (r.y - fr.bottom(), (cx - fx).abs()),
            Direction::Up => (fr.y - r.bottom(), (cx - fx).abs()),
        };
        // Must lie (at least partly) in the requested half-plane.
        let along = match dir {
            Direction::Right => cx - fx,
            Direction::Left => fx - cx,
            Direction::Down => cy - fy,
            Direction::Up => fy - cy,
        };
        if along <= 0.5 || primary < -fr.w.max(fr.h) {
            continue;
        }
        let score = primary.max(0.0) + 2.0 * cross + along * 0.01;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}
