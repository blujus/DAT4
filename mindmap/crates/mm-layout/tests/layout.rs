use mm_core::*;
use mm_layout::*;

fn big_tree() -> Document {
    let mut ed = Editor::new(Document::new("root"));
    let root = ed.doc.root();
    for i in 0..6 {
        let a = ed.add_child(root, &format!("cat{i}")).unwrap();
        for j in 0..4 {
            let b = ed.add_child(a, &format!("cause{i}.{j}")).unwrap();
            for k in 0..2 {
                ed.add_child(b, &format!("sub{k}")).unwrap();
            }
        }
    }
    ed.doc
}

fn measure(id: NodeId) -> (f32, f32) {
    (60.0 + (id.0 % 5) as f32 * 8.0, 24.0)
}

fn overlaps(l: &Layout, ids: &[NodeId]) -> Option<(NodeId, NodeId)> {
    for (i, &a) in ids.iter().enumerate() {
        for &b in &ids[i + 1..] {
            let (ra, rb) = (l.rect(a).unwrap(), l.rect(b).unwrap());
            // shrink slightly: touching edges are fine
            let ra = Rect::new(ra.x + 0.5, ra.y + 0.5, ra.w - 1.0, ra.h - 1.0);
            if ra.intersects(&rb) {
                return Some((a, b));
            }
        }
    }
    None
}

#[test]
fn no_overlaps_in_any_mode() {
    for mode in DiagramMode::ALL {
        let mut doc = big_tree();
        doc.mode = mode;
        let l = compute(&doc, &measure, &Config::default());
        let ids = doc.visible_ids();
        assert!(ids.iter().all(|&i| l.rect(i).is_some()), "{mode:?}: every visible node placed");
        assert_eq!(overlaps(&l, &ids), None, "{mode:?}");
        assert!(!l.edges.is_empty());
    }
}

#[test]
fn collapse_hides_descendants() {
    let doc = big_tree();
    let a = doc.children(doc.root())[0];
    let hidden = doc.children(a)[0];
    let mut ed = Editor::new(doc);
    ed.set_collapsed(a, true).unwrap();
    let l = compute(&ed.doc, &measure, &Config::default());
    assert!(l.rect(a).is_some());
    assert!(l.rect(hidden).is_none());
}

#[test]
fn fishbone_alternates_sides_and_head_is_rightmost() {
    let mut doc = big_tree();
    doc.mode = DiagramMode::Fishbone;
    let l = compute(&doc, &measure, &Config::default());
    let root = doc.root();
    let head = l.rect(root).unwrap();
    let kids = doc.children(root);
    for (i, &k) in kids.iter().enumerate() {
        let r = l.rect(k).unwrap();
        if i % 2 == 0 {
            assert!(r.bottom() <= 0.0, "even bones above spine");
        } else {
            assert!(r.y >= 0.0, "odd bones below spine");
        }
        assert!(r.right() <= head.x + 1.0, "categories left of the head");
    }
}

#[test]
fn offsets_move_whole_subtree() {
    let doc = big_tree();
    let a = doc.children(doc.root())[0];
    let child = doc.children(a)[0];
    let base = compute(&doc, &measure, &Config::default());
    let mut ed = Editor::new(doc);
    ed.apply(Command::SetOffset { id: a, offset: Some([100.0, 50.0]) }).unwrap();
    let moved = compute(&ed.doc, &measure, &Config::default());
    let (b0, b1) = (base.rect(child).unwrap(), moved.rect(child).unwrap());
    assert_eq!((b1.x - b0.x, b1.y - b0.y), (100.0, 50.0));
}

#[test]
fn spatial_navigation_finds_neighbours() {
    let mut doc = Document::new("root");
    doc.mode = DiagramMode::Right;
    let mut ed = Editor::new(doc);
    let root = ed.doc.root();
    let a = ed.add_child(root, "a").unwrap();
    let b = ed.add_child(root, "b").unwrap();
    let l = compute(&ed.doc, &|_| (50.0, 20.0), &Config::default());
    assert_eq!(spatial_neighbor(&l, root, Direction::Right), Some(a));
    assert_eq!(spatial_neighbor(&l, a, Direction::Down), Some(b));
    assert_eq!(spatial_neighbor(&l, b, Direction::Up), Some(a));
    assert_eq!(spatial_neighbor(&l, a, Direction::Left), Some(root));
    assert_eq!(spatial_neighbor(&l, root, Direction::Left), None);
    let (cx, cy) = l.rect(b).unwrap().center();
    assert_eq!(hit_test(&l, cx, cy), Some(b));
}

#[test]
fn large_tree_layout_is_fast_enough() {
    let mut ed = Editor::new(Document::new("root"));
    let root = ed.doc.root();
    let mut parents = vec![root];
    for i in 0..10_000 {
        let p = parents[i / 4 % parents.len()];
        let n = ed.add_child(p, "node").unwrap();
        parents.push(n);
    }
    let t = std::time::Instant::now();
    let l = compute(&ed.doc, &|_| (80.0, 24.0), &Config::default());
    let el = t.elapsed();
    assert_eq!(l.order.len(), 10_001);
    eprintln!("layout of 10k nodes: {el:?}");
}

#[test]
fn fishbone_wide_labels_do_not_overlap() {
    // Wide category boxes with few or no ribs used to collide with their neighbours.
    let mut ed = Editor::new(Document::new("Late deliveries"));
    let root = ed.doc.root();
    for (i, name) in ["Method", "Machine", "Manpower", "Material", "Measurement", "Mother Nature", "Management", "Money"].iter().enumerate()
    {
        let c = ed.add_child(root, name).unwrap();
        for j in 0..(i % 3) {
            ed.add_child(c, &format!("cause {j}")).unwrap();
        }
    }
    ed.doc.mode = DiagramMode::Fishbone;
    for width in [60.0f32, 120.0, 190.0] {
        let l = compute(&ed.doc, &|id| (width + (id.0 % 3) as f32 * 30.0, 32.0), &Config::default());
        let ids = ed.doc.visible_ids();
        assert_eq!(overlaps(&l, &ids), None, "width {width}");
    }
}
