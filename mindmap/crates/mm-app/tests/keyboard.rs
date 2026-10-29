//! Keyboard-flow tests: scripted key sequences against the UI-free model.
use mm_app::{Action::*, Dir, Model};
use mm_core::{Document, Subtree};

fn model() -> Model {
    Model::new(Document::new("root"), Box::new(|t, _| (10.0 + t.len() as f32 * 7.0, 24.0)))
}

fn outline(m: &Model) -> String {
    fn walk(t: &Subtree, d: usize, out: &mut String) {
        out.push_str(&format!("{}{}\n", "  ".repeat(d), t.text));
        for c in &t.children {
            walk(c, d + 1, out);
        }
    }
    let mut s = String::new();
    walk(&m.ed.doc.to_subtree(m.ed.doc.root()), 0, &mut s);
    s
}

#[test]
fn rapid_fire_tab_enter_typing() {
    let mut m = model();
    m.dispatch(Tab); // child of root, editing
    m.type_text("a");
    m.dispatch(Tab); // commit, child of a
    m.type_text("a1");
    m.dispatch(Enter); // commit, sibling of a1
    m.type_text("a2");
    m.dispatch(Enter);
    m.type_text("a3");
    // Enter on a fresh empty node climbs a level: sibling of "a" instead.
    m.dispatch(Enter); // commits a3, creates empty sibling of a3
    m.dispatch(Enter); // empty -> outdent: now sibling of a
    m.type_text("b");
    m.dispatch(Escape);
    assert_eq!(outline(&m), "root\n  a\n    a1\n    a2\n    a3\n  b\n");
}

#[test]
fn enter_three_times_climbs_three_levels() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("l1");
    m.dispatch(Tab);
    m.type_text("l2");
    m.dispatch(Tab);
    m.type_text("l3");
    m.dispatch(Enter); // new empty sibling of l3 (depth 3)
    m.dispatch(Enter); // outdent -> depth 2
    m.dispatch(Enter); // outdent -> depth 1
    m.dispatch(Enter); // depth 1 is the top: no further climb
    assert!(m.status.contains("top level"));
    m.type_text("l1b");
    m.dispatch(Escape);
    assert_eq!(outline(&m), "root\n  l1\n    l2\n      l3\n  l1b\n");
}

#[test]
fn tab_on_empty_indents_under_previous_sibling() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Enter);
    m.dispatch(Tab); // fresh empty node indents under a
    m.type_text("child");
    m.dispatch(Escape);
    assert_eq!(outline(&m), "root\n  a\n    child\n");
}

#[test]
fn escape_on_fresh_empty_cancels_cleanly_and_selects_parent() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Escape);
    let before = outline(&m);
    let undo_depth = m.ed.can_undo();
    m.dispatch(Tab);
    m.dispatch(Escape); // cancel the empty node
    assert_eq!(outline(&m), before);
    assert_eq!(m.ed.doc.node(m.primary()).text, "a");
    assert_eq!(m.ed.can_undo(), undo_depth);
    // one undo removes "a" entirely: creation + typing was a single step
    m.dispatch(Undo);
    assert_eq!(outline(&m), "root\n");
    m.dispatch(Redo);
    assert_eq!(outline(&m), "root\n  a\n");
}

#[test]
fn typing_burst_is_one_undo_step() {
    let mut m = model();
    m.dispatch(Tab);
    for ch in ["h", "e", "l", "l", "o"] {
        m.type_text(ch);
    }
    m.dispatch(Escape);
    m.dispatch(Undo);
    assert_eq!(outline(&m), "root\n");
}

#[test]
fn navigation_and_sibling_before() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Enter);
    m.type_text("b");
    m.dispatch(Escape);
    // Layout is a balanced mind map; select via actions instead of geometry.
    let root = m.ed.doc.root();
    let b = m.primary();
    m.dispatch(Select { id: root, extend: false });
    m.dispatch(Navigate { dir: Dir::Right, extend: false });
    assert_ne!(m.primary(), root);
    m.dispatch(Select { id: b, extend: false });
    m.dispatch(SiblingBefore);
    m.type_text("between");
    m.dispatch(Escape);
    assert_eq!(outline(&m), "root\n  a\n  between\n  b\n");
}

#[test]
fn nudge_reorder_indent_outdent() {
    let mut m = model();
    for t in ["a", "b", "c"] {
        m.dispatch(if t == "a" { Tab } else { Enter });
        m.type_text(t);
    }
    m.dispatch(Escape); // c selected
    m.dispatch(Nudge(Dir::Up));
    assert_eq!(outline(&m), "root\n  a\n  c\n  b\n");
    m.dispatch(Nudge(Dir::Right)); // c under a
    assert_eq!(outline(&m), "root\n  a\n    c\n  b\n");
    m.dispatch(Nudge(Dir::Left));
    assert_eq!(outline(&m), "root\n  a\n  c\n  b\n");
}

#[test]
fn grab_mode_moves_between_branches() {
    let mut m = model();
    for (k, t) in ["a", "b", "c"].into_iter().enumerate() {
        m.dispatch(if k == 0 { Tab } else { Enter });
        m.type_text(t);
    }
    m.dispatch(Escape);
    let c = m.primary();
    let a = m.ed.doc.children(m.ed.doc.root())[0];
    m.dispatch(Select { id: c, extend: false });
    m.dispatch(BeginGrab);
    assert!(m.grab.is_some());
    // cycle candidates until the target is `a`
    for _ in 0..10 {
        if m.grab_target() == Some(a) {
            break;
        }
        m.dispatch(GrabCycle(1));
    }
    assert_eq!(m.grab_target(), Some(a));
    m.dispatch(Enter);
    assert!(m.grab.is_none());
    assert_eq!(outline(&m), "root\n  a\n    c\n  b\n");
    m.dispatch(Undo);
    assert_eq!(outline(&m), "root\n  a\n  b\n  c\n");
}

#[test]
fn grab_cannot_drop_into_own_subtree_and_escape_cancels() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Tab);
    m.type_text("a1");
    m.dispatch(Escape);
    let a = m.ed.doc.children(m.ed.doc.root())[0];
    m.dispatch(Select { id: a, extend: false });
    m.dispatch(BeginGrab);
    for _ in 0..8 {
        m.dispatch(GrabCycle(1));
        let t = m.grab_target().unwrap();
        assert!(!m.ed.doc.is_ancestor_or_self(a, t), "candidates exclude the grabbed subtree");
    }
    m.dispatch(Escape);
    assert!(m.grab.is_none());
    assert_eq!(outline(&m), "root\n  a\n    a1\n");
}

#[test]
fn copy_paste_cut_and_outline_paste() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Tab);
    m.type_text("a1");
    m.dispatch(Escape);
    let a = m.ed.doc.children(m.ed.doc.root())[0];
    m.dispatch(Select { id: a, extend: false });
    m.dispatch(Copy);
    let text = m.clip_out.take().unwrap();
    assert_eq!(text, "- a\n  - a1\n");
    m.dispatch(Paste(Some(text)));
    assert_eq!(outline(&m), "root\n  a\n    a1\n    a\n      a1\n");
    // external multi-line text: one node per line, indentation = depth
    let root = m.ed.doc.root();
    m.dispatch(Select { id: root, extend: false });
    m.dispatch(Paste(Some("x\n\ty\n\tz\nw".into())));
    assert!(outline(&m).ends_with("  x\n    y\n    z\n  w\n"));
    m.dispatch(Undo);
    assert!(!outline(&m).contains("  w\n"));
    // cut removes and keeps the clipboard
    let a = m.ed.doc.children(m.ed.doc.root())[0];
    m.dispatch(Select { id: a, extend: false });
    m.dispatch(Cut);
    assert!(m.clip_out.is_some());
    assert_eq!(m.ed.doc.children(m.ed.doc.root()).len(), 0);
}

#[test]
fn delete_collapse_levels_and_root_protection() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Tab);
    m.type_text("a1");
    m.dispatch(Escape);
    let root = m.ed.doc.root();
    m.dispatch(Select { id: root, extend: false });
    m.dispatch(Delete);
    assert!(m.status.contains("root"));
    m.dispatch(ShowLevel(1));
    assert!(m.ed.doc.node(root).collapsed);
    m.dispatch(ShowLevel(0));
    assert!(!m.ed.doc.node(root).collapsed);
    let a = m.ed.doc.children(root)[0];
    m.dispatch(Select { id: a, extend: false });
    m.dispatch(ToggleCollapse);
    assert!(m.ed.doc.node(a).collapsed);
    m.dispatch(Delete);
    assert_eq!(outline(&m), "root\n");
    assert_eq!(m.primary(), root);
    m.dispatch(Undo);
    assert!(outline(&m).contains("a1"));
}

#[test]
fn edit_existing_node_is_one_undo_step() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("a");
    m.dispatch(Escape);
    m.dispatch(BeginEdit);
    m.set_buffer("renamed");
    m.dispatch(Escape);
    assert_eq!(outline(&m), "root\n  renamed\n");
    m.dispatch(Undo);
    assert_eq!(outline(&m), "root\n  a\n");
}

#[test]
fn find_and_jump_reveal_collapsed() {
    let mut m = model();
    m.dispatch(Tab);
    m.type_text("alpha");
    m.dispatch(Tab);
    m.type_text("Needle");
    m.dispatch(Escape);
    let alpha = m.ed.doc.children(m.ed.doc.root())[0];
    m.dispatch(Select { id: alpha, extend: false });
    m.dispatch(ToggleCollapse);
    let hits = m.find("needle");
    assert_eq!(hits.len(), 1);
    m.jump_to(hits[0]);
    assert!(!m.ed.doc.node(alpha).collapsed);
    assert_eq!(m.primary(), hits[0]);
}

#[test]
fn model_stays_consistent_under_random_key_mashing() {
    // deterministic pseudo-random action stream
    let mut seed = 0x2545F4914F6CDD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut m = model();
    for _ in 0..4000 {
        let a = match next() % 17 {
            0 => Tab,
            1 => Enter,
            2 => SiblingBefore,
            3 => Escape,
            4 => BeginEdit,
            5 => Delete,
            6 => Navigate { dir: [Dir::Up, Dir::Down, Dir::Left, Dir::Right][(next() % 4) as usize], extend: next() % 2 == 0 },
            7 => Nudge([Dir::Up, Dir::Down, Dir::Left, Dir::Right][(next() % 4) as usize]),
            8 => ToggleCollapse,
            9 => Undo,
            10 => Redo,
            11 => Copy,
            12 if m.ed.doc.len() < 200 => Paste(None), // paste-into-self doubles, so cap growth
            12 => Escape,
            13 => BeginGrab,
            14 => GrabCycle(1),
            15 => ShowLevel((next() % 4) as usize),
            _ => GrabDrop,
        };
        if std::env::var("MM_TRACE").is_ok() {
            eprintln!("{a:?} len={}", m.ed.doc.len());
        }
        m.dispatch(a);
        if m.edit.is_some() && next() % 3 == 0 {
            m.type_text("x");
        }
        m.ed.doc.check_invariants().unwrap();
        assert!(m.ed.doc.contains(m.primary()));
    }
}
