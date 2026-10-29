use mm_core::*;
use proptest::prelude::*;

fn sample() -> Editor {
    let mut ed = Editor::new(Document::new("root"));
    let a = ed.add_child(ed.doc.root(), "a").unwrap();
    ed.add_child(a, "a1").unwrap();
    ed.add_child(ed.doc.root(), "b").unwrap();
    ed
}

#[test]
fn undo_redo_roundtrip() {
    let mut ed = sample();
    let before = ed.doc.to_subtree(ed.doc.root());
    let a = ed.doc.children(ed.doc.root())[0];
    ed.delete(a).unwrap();
    assert_eq!(ed.doc.len(), 2);
    assert!(ed.undo());
    assert_eq!(ed.doc.to_subtree(ed.doc.root()), before);
    assert!(ed.redo());
    assert_eq!(ed.doc.len(), 2);
    ed.doc.check_invariants().unwrap();
}

#[test]
fn group_is_one_undo_step_and_coalesces_text() {
    let mut ed = sample();
    let n = ed.doc.len();
    ed.begin_group();
    let id = ed.add_child(ed.doc.root(), "").unwrap();
    for s in ["h", "he", "hel", "hell", "hello"] {
        ed.set_text(id, s).unwrap();
    }
    ed.end_group();
    assert_eq!(ed.doc.node(id).text, "hello");
    assert!(ed.undo());
    assert_eq!(ed.doc.len(), n);
    assert!(ed.redo());
    assert_eq!(ed.doc.node(id).text, "hello");
}

#[test]
fn abort_group_leaves_no_trace() {
    let mut ed = sample();
    let rev_nodes = ed.doc.len();
    let had_redo = ed.can_redo();
    ed.begin_group();
    let id = ed.add_child(ed.doc.root(), "").unwrap();
    ed.set_text(id, "x").unwrap();
    ed.abort_group();
    assert_eq!(ed.doc.len(), rev_nodes);
    assert_eq!(ed.can_redo(), had_redo);
    ed.doc.check_invariants().unwrap();
}

#[test]
fn move_rejects_cycles_and_root() {
    let mut ed = sample();
    let root = ed.doc.root();
    let a = ed.doc.children(root)[0];
    let a1 = ed.doc.children(a)[0];
    assert_eq!(ed.move_to(a, a1, 0), Err(CoreError::WouldCycle));
    assert_eq!(ed.move_to(root, a, 0), Err(CoreError::RootImmutable));
    assert_eq!(ed.delete(root), Err(CoreError::RootImmutable));
}

#[test]
fn indent_outdent_reorder() {
    let mut ed = sample();
    let root = ed.doc.root();
    let (a, b) = (ed.doc.children(root)[0], ed.doc.children(root)[1]);
    assert!(ed.indent(b));
    assert_eq!(ed.doc.parent(b), Some(a));
    assert_eq!(ed.doc.children(a).last(), Some(&b));
    assert!(ed.outdent(b));
    assert_eq!(ed.doc.parent(b), Some(root));
    assert_eq!(ed.doc.index_in_parent(b), Some(1));
    assert!(ed.reorder(b, -1));
    assert_eq!(ed.doc.index_in_parent(b), Some(0));
    assert!(!ed.reorder(b, -1));
    assert!(!ed.outdent(a)); // parent is root
    ed.doc.check_invariants().unwrap();
}

#[test]
fn outline_roundtrip_and_formats() {
    let ed = sample();
    let text = outline::to_outline(&ed.doc, ed.doc.root());
    let back = Document::from_subtree(&outline::parse_outline_single(&text, "x"));
    assert_eq!(back.to_subtree(back.root()), ed.doc.to_subtree(ed.doc.root()));
    let forest = outline::parse_outline("1. one\n\t* two\n\t\t- three\n   - odd\nfour");
    assert_eq!(forest.len(), 2);
    assert_eq!(forest[0].children[0].children[0].text, "three");
    assert_eq!(forest[0].children.len(), 2);
}

#[test]
fn templates_build() {
    for t in templates::TEMPLATES {
        let d = t.build();
        d.check_invariants().unwrap();
    }
}

#[derive(Debug, Clone)]
enum Op {
    Add(usize),
    Sibling(usize),
    Delete(usize),
    Move(usize, usize, usize),
    Indent(usize),
    Outdent(usize),
    Reorder(usize, bool),
    Text(usize),
    Collapse(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any::<usize>().prop_map(Op::Add),
        any::<usize>().prop_map(Op::Sibling),
        any::<usize>().prop_map(Op::Delete),
        (any::<usize>(), any::<usize>(), 0usize..6).prop_map(|(a, b, c)| Op::Move(a, b, c)),
        any::<usize>().prop_map(Op::Indent),
        any::<usize>().prop_map(Op::Outdent),
        (any::<usize>(), any::<bool>()).prop_map(|(a, b)| Op::Reorder(a, b)),
        any::<usize>().prop_map(Op::Text),
        any::<usize>().prop_map(Op::Collapse),
    ]
}

proptest! {
    #[test]
    fn random_ops_keep_invariants_and_undo_restores(ops in proptest::collection::vec(op(), 1..60)) {
        let mut ed = Editor::new(sample().doc); // fresh history: undo-all returns to the sample
        let original = ed.doc.to_subtree(ed.doc.root());
        let pick = |ed: &Editor, i: usize| { let v = ed.doc.subtree_ids(ed.doc.root()); v[i % v.len()] };
        let mut steps = 0;
        for o in ops {
            let before = ed.can_undo();
            let _ = match o {
                Op::Add(i) => { let p = pick(&ed, i); ed.add_child(p, "n").map(|_| ()) }
                Op::Sibling(i) => { let p = pick(&ed, i); ed.add_sibling(p, true, "s").map(|_| ()) }
                Op::Delete(i) => { let p = pick(&ed, i); ed.delete(p) }
                Op::Move(i, j, k) => { let (a, b) = (pick(&ed, i), pick(&ed, j)); ed.move_to(a, b, k) }
                Op::Indent(i) => { let p = pick(&ed, i); ed.indent(p); Ok(()) }
                Op::Outdent(i) => { let p = pick(&ed, i); ed.outdent(p); Ok(()) }
                Op::Reorder(i, up) => { let p = pick(&ed, i); ed.reorder(p, if up { -1 } else { 1 }); Ok(()) }
                Op::Text(i) => { let p = pick(&ed, i); ed.set_text(p, "t") }
                Op::Collapse(i) => { let p = pick(&ed, i); ed.set_collapsed(p, true) }
            };
            let _ = before;
            prop_assert!(ed.doc.check_invariants().is_ok(), "{:?}", ed.doc.check_invariants());
            steps += 1;
        }
        let after = ed.doc.to_subtree(ed.doc.root());
        while ed.undo() {}
        prop_assert_eq!(ed.doc.to_subtree(ed.doc.root()), original);
        prop_assert!(ed.doc.check_invariants().is_ok());
        while ed.redo() {}
        prop_assert_eq!(ed.doc.to_subtree(ed.doc.root()), after);
        let _ = steps;
    }
}
