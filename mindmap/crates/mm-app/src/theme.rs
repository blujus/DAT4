//! Colours and node styling. Branch colours follow the first-level ancestor.
use egui::Color32;
use mm_core::{Document, NodeId};

pub const PALETTE: [[u8; 3]; 8] = [
    [0x4e, 0x79, 0xa7], // blue
    [0xf2, 0x8e, 0x2b], // orange
    [0x59, 0xa1, 0x4f], // green
    [0xb0, 0x7a, 0xa1], // purple
    [0xe1, 0x57, 0x59], // red
    [0x4e, 0xa8, 0xa3], // teal
    [0xed, 0xc9, 0x48], // yellow
    [0x9c, 0x75, 0x5f], // brown
];

pub fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The colour a node inherits: its own, else its branch's palette colour.
pub fn branch_rgb(doc: &Document, id: NodeId) -> [u8; 3] {
    let mut cur = id;
    let mut top = id;
    while let Some(p) = doc.parent(cur) {
        top = cur;
        cur = p;
    }
    // Walk again so explicit colours on any ancestor win.
    let mut n = Some(id);
    while let Some(c) = n {
        if let Some(col) = doc.node(c).color {
            return col;
        }
        n = doc.parent(c);
    }
    if top == doc.root() {
        return [0x5b, 0x6b, 0x8c];
    }
    let idx = doc.index_in_parent(top).unwrap_or(0);
    PALETTE[idx % PALETTE.len()]
}

pub fn luminance(c: Color32) -> f32 {
    (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0
}

pub fn on_color(fill: Color32) -> Color32 {
    if luminance(fill) > 0.6 {
        Color32::from_rgb(0x20, 0x20, 0x28)
    } else {
        Color32::WHITE
    }
}
