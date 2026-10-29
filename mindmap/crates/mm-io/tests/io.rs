use mm_core::*;
use mm_io::{csv_io, json, native, Report};
use proptest::prelude::*;

fn sample() -> Document {
    let mut ed = Editor::new(Document::new("Project"));
    let root = ed.doc.root();
    let g = ed.add_child(root, "Goals, \"quoted\"").unwrap();
    ed.add_child(g, "multi\nline").unwrap();
    let r = ed.add_child(root, "=SUM(A1)").unwrap();
    ed.set_note(r, Some("a note".into())).unwrap();
    ed.set_color(r, Some([0xe0, 0x7a, 0x5f])).unwrap();
    ed.set_shape(r, Shape::Pill).unwrap();
    ed.set_collapsed(r, true).unwrap();
    ed.add_child(r, "-5").unwrap();
    ed.doc
}

fn tree(d: &Document) -> Subtree {
    d.to_subtree(d.root())
}

#[test]
fn native_roundtrip() {
    let d = sample();
    let s = native::to_string(&d, None).unwrap();
    let (back, _) = native::from_str(&s, &mut Report::default()).unwrap();
    assert_eq!(tree(&back), tree(&d));
    assert_eq!(back.mode, d.mode);
}

#[test]
fn native_rejects_cycles_and_wrong_format() {
    let bad =
        r#"{"format":"mindmap","version":1,"root":"a","nodes":{"a":{"text":"a","children":["b"]},"b":{"text":"b","children":["a"]}}}"#;
    assert!(native::from_str(bad, &mut Report::default()).is_err());
    assert!(native::from_str(r#"{"format":"other","version":1,"root":"a","nodes":{}}"#, &mut Report::default()).is_err());
}

#[test]
fn json_nested_and_flat_roundtrip() {
    let d = sample();
    for shape in [json::Shape2::Nested, json::Shape2::Flat] {
        let out = json::export(&d, &json::ExportOptions { shape, ..Default::default() }).unwrap();
        let back = json::import(&out, &Default::default(), &mut Report::default()).unwrap();
        assert_eq!(tree(&back), tree(&d), "{shape:?}");
    }
}

#[test]
fn json_flat_errors_and_warnings() {
    let cyc = r#"{"format":"mindmap-export","nodes":[{"id":"a","parent_id":"b","text":"a"},{"id":"b","parent_id":"a","text":"b"},{"id":"r","text":"r"}]}"#;
    assert!(json::import(cyc, &Default::default(), &mut Report::default()).is_err());
    let orphan = r#"{"format":"mindmap-export","nodes":[{"id":"r","text":"r"},{"id":"x","parent_id":"zzz","text":"x"},{"id":"d","parent_id":"r","text":"d"},{"id":"d","parent_id":"r","text":"dup"}]}"#;
    let mut rep = Report::default();
    let d = json::import(orphan, &Default::default(), &mut rep).unwrap();
    assert!(rep.warnings.len() >= 2, "{:?}", rep.warnings);
    d.check_invariants().unwrap();
}

#[test]
fn generic_json_maps_to_tree_with_limits() {
    let v = r#"{"name":"x","tags":["a","b"],"items":[{"name":"one","qty":2},{"qty":3}],"deep":{"k":{"j":1}}}"#;
    let d = json::import(v, &Default::default(), &mut Report::default()).unwrap();
    let t = tree(&d);
    assert_eq!(t.text, "JSON");
    let texts: Vec<&str> = t.children.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["name: x", "tags", "items", "deep"]);
    let items = &t.children[2];
    assert_eq!(items.children[0].text, "one");
    assert_eq!(items.children[0].children[0].text, "qty: 2");
    assert_eq!(items.children[1].text, "[1]");
    let mut rep = Report::default();
    let opts = json::GenericOptions { max_nodes: 5, ..Default::default() };
    let small = json::import(v, &opts, &mut rep).unwrap();
    assert!(small.len() <= 5);
    assert!(!rep.warnings.is_empty());
}

#[test]
fn csv_roundtrip_with_hostile_cells() {
    let d = sample();
    for delimiter in *b",;\t" {
        for bom in [false, true] {
            let out = csv_io::export(&d, &csv_io::ExportOptions { delimiter, bom, ..Default::default() }).unwrap();
            let text = String::from_utf8_lossy(&out);
            assert!(text.contains("'=SUM(A1)"), "formula guarded on export");
            let back = csv_io::import(&out, &Default::default(), &mut Report::default()).unwrap();
            assert_eq!(tree(&back), tree(&d), "delimiter {delimiter} bom {bom}");
        }
    }
}

#[test]
fn csv_path_and_level_layouts() {
    let path = b"L1,L2,L3\nProject,Goals,Q1\nProject,Goals,Q2\nProject,Risks,\n";
    let mut rep = Report::default();
    let d = csv_io::import(path, &Default::default(), &mut rep).unwrap();
    let t = tree(&d);
    assert_eq!(t.text, "Project");
    assert_eq!(t.children[0].text, "Goals");
    assert_eq!(t.children[0].children.len(), 2);
    assert_eq!(t.children[1].text, "Risks");

    let lvl = "level,text\n0,Root\n1,A\n2,A1\n1,B\n";
    let d = csv_io::import(lvl.as_bytes(), &Default::default(), &mut Report::default()).unwrap();
    assert_eq!(tree(&d).children.len(), 2);
    assert_eq!(tree(&d).children[0].children[0].text, "A1");

    // multiple top-level rows are wrapped, with a warning
    let multi = "L1,L2\nA,x\nB,y\n";
    let mut rep = Report::default();
    let d = csv_io::import(multi.as_bytes(), &Default::default(), &mut rep).unwrap();
    assert_eq!(tree(&d).text, "Imported");
    assert!(!rep.warnings.is_empty());
}

#[test]
fn csv_adjacency_problems() {
    let cyc = "id,parent_id,text\nr,,root\na,b,a\nb,a,b\n";
    assert!(csv_io::import(cyc.as_bytes(), &Default::default(), &mut Report::default()).is_err());
    let orphan = "id,parent_id,order,text\nr,,0,root\nx,nope,0,orphan\nc,r,1,second\nb,r,0,first\n";
    let mut rep = Report::default();
    let d = csv_io::import(orphan.as_bytes(), &Default::default(), &mut rep).unwrap();
    assert_eq!(tree(&d).text, "Imported"); // two roots -> wrapped
    assert!(rep.warnings.iter().any(|w| w.contains("not found")));
    let ordered = "id,parent_id,order,text\nr,,0,root\nc,r,1,second\nb,r,0,first\n";
    let d = csv_io::import(ordered.as_bytes(), &Default::default(), &mut Report::default()).unwrap();
    assert_eq!(tree(&d).children[0].text, "first");
}

#[test]
fn csv_utf16_and_ragged_rows() {
    let text = "id,parent_id,text\nr,,héllo\nc,r,wörld\n";
    let mut bytes = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        bytes.extend(u.to_le_bytes());
    }
    let d = csv_io::import(&bytes, &Default::default(), &mut Report::default()).unwrap();
    assert_eq!(tree(&d).text, "héllo");
    let ragged = "id,parent_id,text\nr,,root\nc,r\n";
    let mut rep = Report::default();
    assert!(csv_io::import(ragged.as_bytes(), &Default::default(), &mut rep).is_ok());
    assert!(!rep.warnings.is_empty());
}

#[test]
fn io_never_panics_on_garbage() {
    for junk in [&b""[..], b"\x00\x01\x02", b"{", b"[[[[", b"a,b\n", b"id,parent_id,text\n,,\n"] {
        let _ = csv_io::import(junk, &Default::default(), &mut Report::default());
        let _ = json::import(&String::from_utf8_lossy(junk), &Default::default(), &mut Report::default());
        let _ = native::from_str(&String::from_utf8_lossy(junk), &mut Report::default());
    }
}

fn arb_tree() -> impl Strategy<Value = Subtree> {
    let leaf = ("[ -~]{0,12}", proptest::option::of("[ -~]{1,8}")).prop_map(|(t, n)| Subtree { text: t, note: n, ..Default::default() });
    leaf.prop_recursive(4, 40, 4, |inner| {
        ("[ -~]{0,12}", proptest::collection::vec(inner, 0..4), any::<bool>(), proptest::option::of(any::<[u8; 3]>()))
            .prop_map(|(t, c, col, color)| Subtree { text: t, children: c, collapsed: col, color, ..Default::default() })
    })
}

proptest! {
    #[test]
    fn all_formats_roundtrip(t in arb_tree()) {
        let d = Document::from_subtree(&t);
        let n = native::to_string(&d, None).unwrap();
        prop_assert_eq!(tree(&native::from_str(&n, &mut Report::default()).unwrap().0), tree(&d));
        for shape in [json::Shape2::Nested, json::Shape2::Flat] {
            let j = json::export(&d, &json::ExportOptions { shape, ..Default::default() }).unwrap();
            prop_assert_eq!(tree(&json::import(&j, &Default::default(), &mut Report::default()).unwrap()), tree(&d));
        }
        let c = csv_io::export(&d, &Default::default()).unwrap();
        prop_assert_eq!(tree(&csv_io::import(&c, &Default::default(), &mut Report::default()).unwrap()), tree(&d));
    }
}

#[test]
fn csv_guard_is_reversible_for_literal_quotes() {
    let mut ed = Editor::new(Document::new("'=x"));
    ed.add_child(ed.doc.root(), "'-1").unwrap();
    let out = csv_io::export(&ed.doc, &Default::default()).unwrap();
    let back = csv_io::import(&out, &Default::default(), &mut Report::default()).unwrap();
    assert_eq!(tree(&back), tree(&ed.doc));
}
