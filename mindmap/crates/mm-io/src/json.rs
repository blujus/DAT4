//! JSON interchange: nested / flat export, auto-detecting import, generic JSON -> tree.
use crate::native::{hex, parse_color};
use crate::{IoError, Report, Result};
use mm_core::{DiagramMode, Document, NodeId, Shape, Subtree};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub const EXPORT_FORMAT: &str = "mindmap-export";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape2 {
    Nested,
    Flat,
}

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub shape: Shape2,
    pub pretty: bool,
    pub include_style: bool,
    pub include_notes: bool,
    pub include_layout: bool,
    /// Export only this subtree (it becomes the root).
    pub subtree: Option<NodeId>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { shape: Shape2::Nested, pretty: true, include_style: true, include_notes: true, include_layout: false, subtree: None }
    }
}

fn node_value(doc: &Document, id: NodeId, o: &ExportOptions, nested: bool, order: usize) -> Value {
    let n = doc.node(id);
    let mut m = Map::new();
    m.insert("id".into(), json!(format!("n{}", id.0)));
    if !nested {
        let parent = if Some(id) == o.subtree || n.parent.is_none() { Value::Null } else { json!(format!("n{}", n.parent.unwrap().0)) };
        m.insert("parent_id".into(), parent);
        m.insert("order".into(), json!(order));
    }
    m.insert("text".into(), json!(n.text));
    if o.include_notes {
        m.insert("note".into(), json!(n.note));
    }
    if o.include_style {
        if let Some(c) = n.color {
            m.insert("color".into(), json!(hex(c)));
        }
        m.insert("shape".into(), serde_json::to_value(n.shape).unwrap());
    }
    m.insert("collapsed".into(), json!(n.collapsed));
    if o.include_layout {
        if let Some([x, y]) = n.offset {
            m.insert("x".into(), json!(x));
            m.insert("y".into(), json!(y));
        }
    }
    if nested {
        let kids: Vec<Value> = n.children.iter().enumerate().map(|(i, &c)| node_value(doc, c, o, true, i)).collect();
        m.insert("children".into(), Value::Array(kids));
    }
    Value::Object(m)
}

pub fn export(doc: &Document, o: &ExportOptions) -> Result<String> {
    let root = o.subtree.unwrap_or(doc.root());
    let title = doc.node(root).text.clone();
    let mut top = Map::new();
    top.insert("format".into(), json!(EXPORT_FORMAT));
    top.insert("version".into(), json!(1));
    top.insert("title".into(), json!(title));
    top.insert("mode".into(), serde_json::to_value(doc.mode).unwrap());
    match o.shape {
        Shape2::Nested => {
            top.insert("root".into(), node_value(doc, root, o, true, 0));
        }
        Shape2::Flat => {
            let mut rows = Vec::new();
            for id in doc.subtree_ids(root) {
                rows.push(node_value(doc, id, o, false, doc.index_in_parent(id).unwrap_or(0)));
            }
            top.insert("nodes".into(), Value::Array(rows));
        }
    }
    top.insert("links".into(), json!([]));
    let v = Value::Object(top);
    Ok(if o.pretty { serde_json::to_string_pretty(&v)? + "\n" } else { serde_json::to_string(&v)? })
}

#[derive(Deserialize, Serialize, Default)]
struct Rec {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    order: Option<usize>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    shape: Option<Shape>,
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    children: Vec<Rec>,
}

fn rec_to_subtree(r: Rec, report: &mut Report) -> Subtree {
    Subtree {
        text: r.text,
        note: r.note.filter(|n| !n.is_empty()),
        color: r.color.and_then(|c| {
            let p = parse_color(&c);
            if p.is_none() {
                report.warn(format!("invalid colour {c:?} ignored"));
            }
            p
        }),
        shape: r.shape.unwrap_or_default(),
        collapsed: r.collapsed,
        children: r.children.into_iter().map(|c| rec_to_subtree(c, report)).collect(),
    }
}

/// Build a tree from flat `{id, parent_id, order}` records.
fn flat_to_subtree(rows: Vec<Rec>, report: &mut Report) -> Result<Subtree> {
    use std::collections::{HashMap, HashSet};
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut ids_ok = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let key = r.id.clone().unwrap_or_else(|| format!("#row{i}"));
        if ids.contains_key(&key) {
            report.warn(format!("duplicate id {key:?}; later row skipped"));
            ids_ok.push(false);
        } else {
            ids.insert(key, i);
            ids_ok.push(true);
        }
    }
    let mut kids: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if !ids_ok[i] {
            continue;
        }
        match r.parent_id.as_deref().filter(|p| !p.is_empty()) {
            None => roots.push(i),
            Some(p) => match ids.get(p) {
                Some(&pi) if pi != i => kids.entry(pi).or_default().push(i),
                _ => {
                    report.warn(format!("row {i}: unknown parent {p:?}; attached to the root"));
                    roots.push(i)
                }
            },
        }
    }
    for v in kids.values_mut() {
        v.sort_by_key(|&i| (rows[i].order.unwrap_or(usize::MAX), i));
    }
    // Cycle check: every ok row must be reachable from a root.
    let mut reach = HashSet::new();
    let mut stack = roots.clone();
    while let Some(i) = stack.pop() {
        if reach.insert(i) {
            stack.extend(kids.get(&i).into_iter().flatten());
        }
    }
    if let Some(bad) = (0..rows.len()).find(|i| ids_ok[*i] && !reach.contains(i)) {
        return Err(IoError::Invalid(format!("cycle detected involving node {:?}", rows[bad].id)));
    }
    let mut rows: Vec<Option<Rec>> = rows.into_iter().map(Some).collect();
    fn build(i: usize, rows: &mut Vec<Option<Rec>>, kids: &HashMap<usize, Vec<usize>>, report: &mut Report) -> Subtree {
        let r = rows[i].take().unwrap();
        let mut t = rec_to_subtree(Rec { children: vec![], ..r }, report);
        if let Some(ks) = kids.get(&i) {
            t.children = ks.iter().map(|&k| build(k, rows, kids, report)).collect();
        }
        t
    }
    match roots.len() {
        0 => Err(IoError::Invalid("no root node found".into())),
        1 => Ok(build(roots[0], &mut rows, &kids, report)),
        n => {
            report.warn(format!("{n} root nodes found; wrapped under a new root"));
            let children = roots.into_iter().map(|r| build(r, &mut rows, &kids, report)).collect();
            Ok(Subtree::new("Imported").with_children(children))
        }
    }
}

/// Limits for generic JSON import.
#[derive(Clone, Debug)]
pub struct GenericOptions {
    pub max_depth: usize,
    pub max_nodes: usize,
    /// For arrays of objects, prefer these keys as the element label.
    pub label_fields: Vec<String>,
    pub title: String,
}

impl Default for GenericOptions {
    fn default() -> Self {
        GenericOptions {
            max_depth: 64,
            max_nodes: 50_000,
            label_fields: vec!["name".into(), "title".into(), "label".into(), "id".into()],
            title: "JSON".into(),
        }
    }
}

struct Gen<'a> {
    o: &'a GenericOptions,
    count: usize,
    truncated: bool,
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some("null".into()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

impl Gen<'_> {
    fn leaf(&mut self, text: String) -> Option<Subtree> {
        if self.count >= self.o.max_nodes {
            self.truncated = true;
            return None;
        }
        self.count += 1;
        Some(Subtree::new(text))
    }

    fn children(&mut self, v: &Value, depth: usize) -> Vec<Subtree> {
        let mut out = Vec::new();
        if depth >= self.o.max_depth {
            if !matches!(v, Value::Null) && scalar(v).is_none() {
                self.truncated = true;
            }
            return out;
        }
        match v {
            Value::Object(m) => {
                for (k, val) in m {
                    if let Some(t) = self.entry(k, val, depth) {
                        out.push(t);
                    }
                }
            }
            Value::Array(a) => {
                for (i, val) in a.iter().enumerate() {
                    if let Some(t) = self.element(i, val, depth) {
                        out.push(t);
                    }
                }
            }
            _ => {}
        }
        out
    }

    fn entry(&mut self, key: &str, val: &Value, depth: usize) -> Option<Subtree> {
        if let Some(s) = scalar(val) {
            return self.leaf(format!("{key}: {s}"));
        }
        let mut t = self.leaf(key.to_string())?;
        t.children = self.children(val, depth + 1);
        Some(t)
    }

    fn element(&mut self, i: usize, val: &Value, depth: usize) -> Option<Subtree> {
        if let Some(s) = scalar(val) {
            return self.leaf(s);
        }
        if let Value::Object(m) = val {
            let label = self
                .o
                .label_fields
                .iter()
                .find_map(|f| m.get(f).and_then(|v| scalar(v)).map(|s| (f.clone(), s)));
            if let Some((field, text)) = label {
                let mut t = self.leaf(text)?;
                let mut rest = m.clone();
                rest.remove(&field);
                t.children = self.children(&Value::Object(rest), depth + 1);
                return Some(t);
            }
        }
        let mut t = self.leaf(format!("[{i}]"))?;
        t.children = self.children(val, depth + 1);
        Some(t)
    }
}

pub fn generic_to_subtree(v: &Value, o: &GenericOptions, report: &mut Report) -> Subtree {
    let mut g = Gen { o, count: 1, truncated: false };
    let mut root = Subtree::new(o.title.clone());
    if let Some(s) = scalar(v) {
        root.children.push(Subtree::new(s));
    } else {
        root.children = g.children(v, 0);
    }
    if g.truncated {
        report.warn(format!("input truncated at {} nodes / depth {}", o.max_nodes, o.max_depth));
    }
    root
}

/// What an incoming JSON document turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detected {
    Native,
    Nested,
    Flat,
    Generic,
}

pub fn detect(v: &Value) -> Detected {
    match v.get("format").and_then(Value::as_str) {
        Some("mindmap") => Detected::Native,
        Some(EXPORT_FORMAT) if v.get("root").is_some() => Detected::Nested,
        Some(EXPORT_FORMAT) if v.get("nodes").is_some_and(Value::is_array) => Detected::Flat,
        _ => Detected::Generic,
    }
}

/// Import any supported JSON into a tree (not yet a document, so it can also be pasted as a branch).
pub fn import_subtree(text: &str, o: &GenericOptions, report: &mut Report) -> Result<(Subtree, DiagramMode)> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    let mode = v.get("mode").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    let t = match detect(&v) {
        Detected::Native => {
            let (doc, _) = crate::native::from_str(text, report)?;
            doc.to_subtree(doc.root())
        }
        Detected::Nested => {
            let rec: Rec = serde_json::from_value(v["root"].clone())?;
            rec_to_subtree(rec, report)
        }
        Detected::Flat => {
            let rows: Vec<Rec> = serde_json::from_value(v["nodes"].clone())?;
            flat_to_subtree(rows, report)?
        }
        Detected::Generic => generic_to_subtree(&v, o, report),
    };
    Ok((t, mode))
}

pub fn import(text: &str, o: &GenericOptions, report: &mut Report) -> Result<Document> {
    let (t, mode) = import_subtree(text, o, report)?;
    let mut d = Document::from_subtree(&t);
    d.mode = mode;
    Ok(d)
}
