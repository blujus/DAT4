use crate::{IoError, Report, Result};
use mm_core::{DiagramMode, Document, Node, NodeId, Shape};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FORMAT: &str = "mindmap";
pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    root: String,
    #[serde(default)]
    mode: DiagramMode,
    nodes: indexmap_like::Map<NodeRec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<View>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub center: [f32; 2],
    pub zoom: f32,
}

#[derive(Serialize, Deserialize)]
struct NodeRec {
    text: String,
    #[serde(default)]
    children: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_shape")]
    shape: Shape,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    collapsed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offset: Option<[f32; 2]>,
}

fn is_default_shape(s: &Shape) -> bool {
    *s == Shape::Rounded
}

/// serde_json's `Map<String, Value>` keeps order, but we want typed values.
mod indexmap_like {
    use serde::de::{MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Insertion-ordered string-keyed map (a `Vec` of pairs).
    pub struct Map<T>(pub Vec<(String, T)>);

    impl<T: Serialize> Serialize for Map<T> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut m = s.serialize_map(Some(self.0.len()))?;
            for (k, v) in &self.0 {
                m.serialize_entry(k, v)?;
            }
            m.end()
        }
    }

    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Map<T> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V<T>(std::marker::PhantomData<T>);
            impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
                type Value = Map<T>;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a map")
                }
                fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Map<T>, A::Error> {
                    let mut v = Vec::new();
                    while let Some((k, val)) = a.next_entry()? {
                        v.push((k, val));
                    }
                    Ok(Map(v))
                }
            }
            d.deserialize_map(V(std::marker::PhantomData))
        }
    }
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let p = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
        return match h.len() {
            6 => Some([p(0, 2)?, p(2, 2)?, p(4, 2)?]),
            3 => Some([p(0, 1)? * 17, p(1, 1)? * 17, p(2, 1)? * 17]),
            _ => None,
        };
    }
    Some(match s.to_ascii_lowercase().as_str() {
        "red" => [0xe0, 0x5a, 0x4f],
        "orange" => [0xf2, 0x8e, 0x2b],
        "yellow" => [0xf2, 0xc9, 0x4c],
        "green" => [0x59, 0xa1, 0x4f],
        "teal" => [0x4e, 0xa8, 0xa3],
        "blue" => [0x4e, 0x79, 0xa7],
        "purple" => [0xb0, 0x7a, 0xa1],
        "pink" => [0xff, 0x9d, 0xa7],
        "gray" | "grey" => [0x9a, 0x9a, 0x9a],
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        _ => return None,
    })
}

pub fn to_string(doc: &Document, view: Option<View>) -> Result<String> {
    let ids = doc.subtree_ids(doc.root());
    let name = |id: NodeId| format!("n{}", id.0);
    let nodes = ids
        .iter()
        .map(|&id| {
            let n = doc.node(id);
            (
                name(id),
                NodeRec {
                    text: n.text.clone(),
                    children: n.children.iter().map(|&c| name(c)).collect(),
                    note: n.note.clone(),
                    color: n.color.map(hex),
                    shape: n.shape,
                    collapsed: n.collapsed,
                    offset: n.offset,
                },
            )
        })
        .collect();
    let f =
        File { format: FORMAT.into(), version: VERSION, root: name(doc.root()), mode: doc.mode, nodes: indexmap_like::Map(nodes), view };
    Ok(serde_json::to_string_pretty(&f)? + "\n")
}

/// Parse the native format. Ids in the file may be arbitrary strings.
pub fn from_str(s: &str, report: &mut Report) -> Result<(Document, Option<View>)> {
    let f: File = serde_json::from_str(s)?;
    if f.format != FORMAT {
        return Err(IoError::Invalid(format!("not a mindmap file (format = {:?})", f.format)));
    }
    if f.version > VERSION {
        report.warn(format!("file version {} is newer than supported {VERSION}; some data may be ignored", f.version));
    }
    let recs: HashMap<&str, &NodeRec> = f.nodes.0.iter().map(|(k, v)| (k.as_str(), v)).collect();
    if !recs.contains_key(f.root.as_str()) {
        return Err(IoError::Invalid(format!("root {:?} is not in nodes", f.root)));
    }
    // Rebuild via preorder from the root so ids are compact and the tree is validated.
    let mut doc = Document::new("");
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![(f.root.as_str(), None::<NodeId>)];
    let mut first = true;
    while let Some((key, parent)) = stack.pop() {
        if !visited.insert(key) {
            return Err(IoError::Invalid(format!("node {key:?} is referenced more than once (cycle or shared child)")));
        }
        let Some(rec) = recs.get(key) else {
            report.warn(format!("child {key:?} is missing from nodes; skipped"));
            continue;
        };
        let node = Node {
            parent,
            children: Vec::new(),
            text: rec.text.clone(),
            note: rec.note.clone(),
            color: rec.color.as_deref().and_then(|c| {
                let p = parse_color(c);
                if p.is_none() {
                    report.warn(format!("invalid colour {c:?} ignored"));
                }
                p
            }),
            shape: rec.shape,
            collapsed: rec.collapsed,
            offset: rec.offset,
        };
        let id = if first {
            first = false;
            doc.replace_root(node);
            doc.root()
        } else {
            doc.push_raw(node)
        };
        if let Some(p) = parent {
            doc.link_child(p, id);
        }
        for child in rec.children.iter().rev() {
            stack.push((child.as_str(), Some(id)));
        }
    }
    // Children were pushed reversed onto the stack, so linking order is original order.
    doc.mode = f.mode;
    doc.check_invariants().map_err(IoError::Invalid)?;
    Ok((doc, f.view))
}

pub fn save_path(path: &std::path::Path, doc: &Document, view: Option<View>) -> Result<()> {
    crate::write_atomic(path, to_string(doc, view)?.as_bytes())
}

pub fn load_path(path: &std::path::Path) -> Result<(Document, Option<View>, Report)> {
    let mut report = Report::default();
    let bytes = std::fs::read(path)?;
    let text = crate::decode_text(&bytes, &mut report);
    let (doc, view) = from_str(&text, &mut report)?;
    Ok((doc, view, report))
}
