//! CSV import/export. Export is the adjacency-list schema; import also accepts
//! path columns and indent/level outlines.
use crate::native::{hex, parse_color};
use crate::{IoError, Report, Result};
use mm_core::{Document, NodeId, Shape, Subtree};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub delimiter: u8,
    pub bom: bool,
    pub include_style: bool,
    pub include_layout: bool,
    /// Prefix cells starting with `= + - @` so spreadsheets don't evaluate them.
    pub injection_guard: bool,
    pub subtree: Option<NodeId>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { delimiter: b',', bom: false, include_style: true, include_layout: false, injection_guard: true, subtree: None }
    }
}

const HOT: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// True if the text, ignoring any leading quotes, starts with a formula character.
/// Such cells get one extra leading quote on export (even when they already start
/// with quotes) so that stripping exactly one on import is always lossless.
fn is_hot(s: &str) -> bool {
    s.trim_start_matches('\'').starts_with(HOT)
}

fn guard(s: &str, on: bool) -> String {
    if on && is_hot(s) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

fn unguard(s: &str) -> String {
    match s.strip_prefix('\'') {
        Some(rest) if is_hot(rest) => rest.to_string(),
        _ => s.to_string(),
    }
}

pub fn export(doc: &Document, o: &ExportOptions) -> Result<Vec<u8>> {
    let root = o.subtree.unwrap_or(doc.root());
    let mut w = csv::WriterBuilder::new().delimiter(o.delimiter).from_writer(Vec::new());
    let mut head = vec!["id", "parent_id", "order", "level", "text", "note"];
    if o.include_style {
        head.extend(["color", "shape", "collapsed"]);
    }
    if o.include_layout {
        head.extend(["x", "y"]);
    }
    w.write_record(&head)?;
    let base_depth = doc.depth(root);
    for id in doc.subtree_ids(root) {
        let n = doc.node(id);
        let parent = if id == root { String::new() } else { format!("n{}", n.parent.unwrap().0) };
        let mut row = vec![
            format!("n{}", id.0),
            parent,
            doc.index_in_parent(id).filter(|_| id != root).unwrap_or(0).to_string(),
            (doc.depth(id) - base_depth).to_string(),
            guard(&n.text, o.injection_guard),
            guard(n.note.as_deref().unwrap_or(""), o.injection_guard),
        ];
        if o.include_style {
            row.push(n.color.map(hex).unwrap_or_default());
            row.push(
                match n.shape {
                    Shape::Rounded => "rounded",
                    Shape::Pill => "pill",
                    Shape::None => "none",
                }
                .into(),
            );
            row.push(n.collapsed.to_string());
        }
        if o.include_layout {
            let [x, y] = n.offset.unwrap_or([0.0, 0.0]);
            row.push(n.offset.map(|_| x.to_string()).unwrap_or_default());
            row.push(n.offset.map(|_| y.to_string()).unwrap_or_default());
        }
        w.write_record(&row)?;
    }
    let mut out = w.into_inner().map_err(|e| IoError::Invalid(e.to_string()))?;
    if o.bom {
        let mut with = vec![0xEF, 0xBB, 0xBF];
        with.append(&mut out);
        out = with;
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// `id` + `parent_id` columns.
    Adjacency,
    /// One column per level; each row spells out the path.
    Path,
    /// `level` + `text` columns.
    Level,
}

#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    pub layout: Option<Layout>,
    pub delimiter: Option<u8>,
    pub trim: bool,
    /// Column header overrides.
    pub text_col: Option<String>,
    pub parent_col: Option<String>,
    pub id_col: Option<String>,
}

fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    b",;\t"
        .iter()
        .copied()
        .max_by_key(|&d| first.bytes().filter(|&b| b == d).count())
        .filter(|&d| first.bytes().any(|b| b == d))
        .unwrap_or(b',')
}

struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    fn col(&self, names: &[&str], over: &Option<String>) -> Option<usize> {
        if let Some(o) = over {
            return self.headers.iter().position(|h| h.eq_ignore_ascii_case(o));
        }
        names.iter().find_map(|n| self.headers.iter().position(|h| h.eq_ignore_ascii_case(n)))
    }
}

pub fn import_subtree(bytes: &[u8], o: &ImportOptions, report: &mut Report) -> Result<Subtree> {
    let text = crate::decode_text(bytes, report);
    let delim = o.delimiter.unwrap_or_else(|| sniff_delimiter(&text));
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).flexible(true).has_headers(true).from_reader(text.as_bytes());
    let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.trim().to_string()).collect();
    let mut rows = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        let rec = rec?;
        if rec.len() != headers.len() {
            report.warn(format!("row {}: expected {} columns, found {}", i + 2, headers.len(), rec.len()));
        }
        let mut r: Vec<String> = rec.iter().map(|c| if o.trim { c.trim().to_string() } else { c.to_string() }).collect();
        r.resize(headers.len().max(r.len()), String::new());
        rows.push(r);
    }
    if rows.is_empty() {
        return Err(IoError::Invalid("CSV has no data rows".into()));
    }
    let table = Table { headers, rows };
    let layout = o.layout.unwrap_or_else(|| {
        let has_id = table.col(&["id"], &o.id_col).is_some();
        let has_parent = table.col(&["parent_id", "parent"], &o.parent_col).is_some();
        let has_level = table.col(&["level", "depth"], &None).is_some();
        let has_text = table.col(&["text", "title", "name", "label"], &o.text_col).is_some();
        if has_id && has_parent {
            Layout::Adjacency
        } else if has_level && has_text {
            Layout::Level
        } else {
            Layout::Path
        }
    });
    match layout {
        Layout::Adjacency => adjacency(&table, o, report),
        Layout::Level => level(&table, o, report),
        Layout::Path => Ok(path(&table, report)),
    }
}

pub fn import(bytes: &[u8], o: &ImportOptions, report: &mut Report) -> Result<Document> {
    Ok(Document::from_subtree(&import_subtree(bytes, o, report)?))
}

fn get(row: &[String], i: Option<usize>) -> &str {
    i.and_then(|i| row.get(i)).map(String::as_str).unwrap_or("")
}

fn adjacency(t: &Table, o: &ImportOptions, report: &mut Report) -> Result<Subtree> {
    let id_c = t.col(&["id"], &o.id_col);
    let parent_c = t.col(&["parent_id", "parent"], &o.parent_col);
    let text_c = t.col(&["text", "title", "name", "label"], &o.text_col);
    let order_c = t.col(&["order", "position"], &None);
    let note_c = t.col(&["note", "notes"], &None);
    let color_c = t.col(&["color", "colour"], &None);
    let shape_c = t.col(&["shape"], &None);
    let coll_c = t.col(&["collapsed"], &None);
    if text_c.is_none() {
        return Err(IoError::Invalid("no text column found (expected text/title/name/label)".into()));
    }

    let mut index: HashMap<String, usize> = HashMap::new();
    let mut ok = vec![true; t.rows.len()];
    for (i, r) in t.rows.iter().enumerate() {
        let id = get(r, id_c).trim().to_string();
        let key = if id.is_empty() { format!("#row{}", i + 2) } else { id };
        match index.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => {
                report.warn(format!("row {}: duplicate id {:?}; skipped", i + 2, e.key()));
                ok[i] = false;
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(i);
            }
        }
    }
    let mut kids: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, r) in t.rows.iter().enumerate() {
        if !ok[i] {
            continue;
        }
        let p = get(r, parent_c).trim();
        if p.is_empty() {
            roots.push(i);
        } else if let Some(&pi) = index.get(p).filter(|&&pi| pi != i) {
            kids.entry(pi).or_default().push(i);
        } else {
            report.warn(format!("row {}: parent {p:?} not found; attached to the root", i + 2));
            roots.push(i);
        }
    }
    for v in kids.values_mut() {
        v.sort_by_key(|&i| (get(&t.rows[i], order_c).trim().parse::<i64>().unwrap_or(i64::MAX), i));
    }
    let mut reach = HashSet::new();
    let mut stack = roots.clone();
    while let Some(i) = stack.pop() {
        if reach.insert(i) {
            stack.extend(kids.get(&i).into_iter().flatten());
        }
    }
    if let Some(bad) = (0..t.rows.len()).find(|i| ok[*i] && !reach.contains(i)) {
        return Err(IoError::Invalid(format!("cycle detected involving row {}", bad + 2)));
    }

    let make = |i: usize, report: &mut Report| -> Subtree {
        let r = &t.rows[i];
        let mut s = Subtree::new(unguard(get(r, text_c)));
        let note = unguard(get(r, note_c));
        s.note = (!note.is_empty()).then_some(note);
        let c = get(r, color_c).trim();
        if !c.is_empty() {
            s.color = parse_color(c);
            if s.color.is_none() {
                report.warn(format!("row {}: invalid colour {c:?} ignored", i + 2));
            }
        }
        s.shape = match get(r, shape_c).trim().to_ascii_lowercase().as_str() {
            "pill" => Shape::Pill,
            "none" => Shape::None,
            _ => Shape::Rounded,
        };
        s.collapsed = matches!(get(r, coll_c).trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes");
        s
    };
    fn build(i: usize, kids: &HashMap<usize, Vec<usize>>, make: &dyn Fn(usize, &mut Report) -> Subtree, report: &mut Report) -> Subtree {
        let mut s = make(i, report);
        if let Some(ks) = kids.get(&i) {
            s.children = ks.iter().map(|&k| build(k, kids, make, report)).collect();
        }
        s
    }
    Ok(match roots.len() {
        0 => return Err(IoError::Invalid("no root row found".into())),
        1 => build(roots[0], &kids, &make, report),
        n => {
            report.warn(format!("{n} root rows found; wrapped under a new root"));
            let children = roots.iter().map(|&r| build(r, &kids, &make, report)).collect();
            Subtree::new("Imported").with_children(children)
        }
    })
}

fn level(t: &Table, o: &ImportOptions, report: &mut Report) -> Result<Subtree> {
    let level_c = t.col(&["level", "depth"], &None).ok_or_else(|| IoError::Invalid("no level column".into()))?;
    let text_c = t.col(&["text", "title", "name", "label"], &o.text_col).ok_or_else(|| IoError::Invalid("no text column".into()))?;
    let mut forest: Vec<Subtree> = Vec::new();
    let mut stack: Vec<usize> = Vec::new(); // child indices path, one per level
    for (i, r) in t.rows.iter().enumerate() {
        let lvl: usize = match r[level_c].trim().parse() {
            Ok(l) => l,
            Err(_) => {
                report.warn(format!("row {}: bad level {:?}; treated as 0", i + 2, r[level_c]));
                0
            }
        };
        let lvl = lvl.min(stack.len());
        stack.truncate(lvl);
        let node = Subtree::new(unguard(&r[text_c]));
        if lvl == 0 {
            forest.push(node);
            stack.push(forest.len() - 1);
        } else {
            let mut cur = &mut forest[stack[0]];
            for &ix in &stack[1..] {
                cur = &mut cur.children[ix];
            }
            cur.children.push(node);
            stack.push(cur.children.len() - 1);
        }
    }
    Ok(wrap(forest, report))
}

fn path(t: &Table, report: &mut Report) -> Subtree {
    // Trie of (parent index, text) -> node index into a flat arena.
    struct N {
        text: String,
        kids: Vec<usize>,
    }
    let mut arena = vec![N { text: String::new(), kids: vec![] }]; // 0 = virtual root
    let mut lookup: HashMap<(usize, String), usize> = HashMap::new();
    for r in &t.rows {
        let mut cur = 0usize;
        for cell in r {
            let c = cell.trim();
            if c.is_empty() {
                break;
            }
            let key = (cur, c.to_string());
            cur = match lookup.get(&key) {
                Some(&i) => i,
                None => {
                    arena.push(N { text: unguard(c), kids: vec![] });
                    let i = arena.len() - 1;
                    arena[cur].kids.push(i);
                    lookup.insert(key, i);
                    i
                }
            };
        }
    }
    fn build(arena: &[N], i: usize) -> Subtree {
        Subtree { text: arena[i].text.clone(), children: arena[i].kids.iter().map(|&k| build(arena, k)).collect(), ..Default::default() }
    }
    let forest: Vec<Subtree> = arena[0].kids.iter().map(|&k| build(&arena, k)).collect();
    wrap(forest, report)
}

fn wrap(mut forest: Vec<Subtree>, report: &mut Report) -> Subtree {
    if forest.len() == 1 {
        forest.pop().unwrap()
    } else {
        report.warn(format!("{} top-level rows; wrapped under a new root", forest.len()));
        Subtree::new("Imported").with_children(forest)
    }
}
