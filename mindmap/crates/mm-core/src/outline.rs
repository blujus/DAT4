//! Indented-text outlines: the format used for paste, drag-in and Markdown export.
use crate::doc::{Document, NodeId, Subtree};

/// Render a subtree as a `- ` bullet list, two spaces per level.
pub fn to_outline(doc: &Document, id: NodeId) -> String {
    let mut out = String::new();
    fn walk(doc: &Document, id: NodeId, depth: usize, out: &mut String) {
        let n = doc.node(id);
        out.push_str(&"  ".repeat(depth));
        out.push_str("- ");
        out.push_str(&n.text.replace('\n', " "));
        out.push('\n');
        for &c in &n.children {
            walk(doc, c, depth + 1, out);
        }
    }
    walk(doc, id, 0, &mut out);
    out
}

fn strip_bullet(s: &str) -> &str {
    let t = s.trim_start();
    for b in ["- ", "* ", "+ ", "• "] {
        if let Some(r) = t.strip_prefix(b) {
            return r.trim();
        }
    }
    // "1. foo" / "2) foo"
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r.trim();
        }
    }
    t.trim()
}

fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

/// Parse indented / bulleted text into a forest. Depth comes from relative
/// indentation, so mixed 2/4-space and tab indents all work.
pub fn parse_outline(text: &str) -> Vec<Subtree> {
    let mut roots: Vec<Subtree> = Vec::new();
    // Path of open nodes: (indent, child-index path from the roots).
    let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = indent_width(line);
        let node = Subtree::new(strip_bullet(line));
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let path = match stack.last() {
            None => {
                roots.push(node);
                vec![roots.len() - 1]
            }
            Some((_, parent_path)) => {
                let mut cur = &mut roots[parent_path[0]];
                for &i in &parent_path[1..] {
                    cur = &mut cur.children[i];
                }
                cur.children.push(node);
                let mut p = parent_path.clone();
                p.push(cur.children.len() - 1);
                p
            }
        };
        stack.push((indent, path));
    }
    roots
}

/// Parse text into a single tree: one top-level line becomes the root, several
/// are wrapped under `fallback_root`.
pub fn parse_outline_single(text: &str, fallback_root: &str) -> Subtree {
    let mut forest = parse_outline(text);
    if forest.len() == 1 {
        forest.pop().unwrap()
    } else {
        Subtree::new(fallback_root).with_children(forest)
    }
}
