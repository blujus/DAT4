# Mindmap — Product & Technical Spec

A fast, keyboard-first, local-first mind-mapping tool written in Rust.

Working name: **Mindmap** (rename freely). Status: draft v0.1.

---

## 1. Goals and non-goals

### Goals
1. **Fast**: 60 fps pan/zoom with 10,000 nodes visible-or-not; cold start < 300 ms; edit latency (keypress → pixel) < 16 ms.
2. **Keyboard-first**: every common action reachable without the mouse; mouse/touch fully supported too.
3. **Local-first**: plain-file documents, no account, no network required. Sync is optional and later.
4. **Native + Web**: one codebase targeting Linux/macOS/Windows and WASM (browser).
5. **Small and hackable**: UI-independent core library with a thin shell on top, so the engine is testable headless.

### Non-goals (v1)
- Real-time multi-user collaboration (design must not preclude it; see §10).
- Rich-text/WYSIWYG documents inside nodes (nodes hold short text + optional note).
- Mobile apps (touch input works on the web build, but no native mobile shell).
- Plugin/scripting runtime.

---

## 2. Target users & core use cases
- Brainstorming and outlining (fast capture, rearrange later).
- Note structuring / study maps (hundreds of nodes).
- Large knowledge maps (thousands of nodes) that must stay responsive.

---

## 3. Feature scope

### 3.1 MVP (v0.1)
| Area | Features |
|---|---|
| Nodes | Create child / sibling, edit text inline, delete, collapse/expand, multi-line text, optional note field |
| Structure | Tree with one root; drag-and-drop re-parenting; reorder siblings |
| Layout | Auto-layout (radial and left-to-right tree), manual node offset override |
| Canvas | Infinite pan/zoom, zoom-to-fit, zoom-to-selection, minimap |
| Selection | Single, multi (shift/ctrl-click, marquee), select subtree |
| Edit | Unlimited undo/redo, cut/copy/paste of subtrees (also as indented text) |
| Style | Per-node color, shape (rounded rect / pill / none), font size; global themes (light/dark) |
| Files | Save/open `.mm.json` (see §6), autosave + crash recovery, export PNG/SVG/Markdown outline |
| Import / Export | **CSV and JSON import and export** (see §6.2–6.3), via File menu, command palette, drag-and-drop onto the window, and `mm-cli` |
| Search | Fuzzy find across nodes, jump & highlight |
| Input | Command palette (`Ctrl/Cmd+K`), configurable keymap |

### 3.2 v0.2+
- Cross-links (non-tree edges) between arbitrary nodes, with labels.
- Floating nodes / multiple roots (graph mode).
- Image & link attachments, URL preview chip.
- Markdown outline import (indented lists, OPML, FreeMind `.mm`, XMind best-effort).
- Import/export of cross-links in CSV (separate edges file) and column-mapping presets saved per user.
- Tags/filters, node focus mode ("hide everything but this branch").
- Presentation mode (step through branches).
- Optional sync via CRDT over a user-chosen backend (§10).

---

## 4. UX specification

### 4.1 Layout
- Full-window canvas. Minimal chrome: slim top bar (file name, undo/redo, search, theme), collapsible right inspector (style + note for selection), optional minimap bottom-right.
- Everything also accessible through the command palette.

### 4.2 Default keymap (Xmind/Freeplane-style, remappable)
| Action | Key |
|---|---|
| New child | `Tab` |
| New sibling (after) | `Enter` |
| New sibling (before) | `Shift+Enter` |
| Edit selected node | `F2` / `Space` / just start typing |
| Commit / cancel edit | `Enter` or `Esc` (commit on blur) |
| Navigate | Arrow keys (spatial: nearest node in that direction) |
| Collapse / expand | `/` or `Ctrl+.` |
| Delete | `Delete` / `Backspace` |
| Move node among siblings | `Alt+↑/↓` |
| Re-parent (indent/outdent) | `Alt+→/←` |
| Undo / Redo | `Ctrl/Cmd+Z` / `Shift+Ctrl/Cmd+Z` |
| Copy / Cut / Paste | standard |
| Zoom | `Ctrl/Cmd +/-/0`, pinch, scroll with modifier |
| Fit to view / selection | `Shift+1` / `Shift+2` |
| Find | `Ctrl/Cmd+F` |
| Command palette | `Ctrl/Cmd+K` |

### 4.3 Mouse / touch
- Drag empty space: pan. Wheel: scroll-pan; `Ctrl/Cmd`+wheel or pinch: zoom around cursor.
- Click: select. Double-click: edit. Drag node: move/re-parent (drop target highlight shows prospective parent and sibling index).
- Drag on empty space with `Shift`: marquee selection.
- Hover node: reveals "+" handles for adding child/sibling and a collapse chevron.

### 4.4 Visual design
- Curved (cubic Bézier) branch connectors, tapered stroke optional; branch color inherited from first-level ancestor by default.
- Level-of-detail rendering: below ~30% zoom, draw nodes as plain rects (no text); below ~10%, cluster to blobs.
- Animations (150 ms ease-out) for layout changes, collapse/expand; disabled via reduced-motion setting.
- Accessibility: high-contrast theme, scalable UI font, full keyboard operation, AccessKit tree for screen readers (see §7.4).

---

## 5. Architecture

### 5.1 Workspace layout
```
mindmap/
├─ Cargo.toml                (workspace)
├─ crates/
│  ├─ mm-core/       document model, commands, undo/redo, serialization. No UI, no I/O. #![forbid(unsafe_code)]
│  ├─ mm-layout/     layout algorithms (pure functions: tree + sizes -> positions), incremental updates
│  ├─ mm-render/     scene building, culling, LOD, text shaping cache; backend-agnostic draw list
│  ├─ mm-io/         file formats, import/export, autosave, recovery
│  ├─ mm-app/        UI shell (windowing, input mapping, inspector, palette)
│  └─ mm-cli/        headless tools: convert, export, validate (also powers integration tests)
└─ assets/ (fonts, themes)
```
`mm-core`, `mm-layout`, `mm-io` compile to `wasm32-unknown-unknown` and have zero platform deps.

### 5.2 Technology choices

| Concern | Choice | Rationale / alternative |
|---|---|---|
| Windowing + GPU | `winit` + `wgpu` | Portable (Vulkan/Metal/DX12/WebGPU, WebGL2 fallback) |
| UI shell | **`egui` via `eframe`** (v1) | Fastest path to inspector/palette/dialogs; canvas is a custom painted widget so we're not limited by egui layout. Alternatives: `iced`, `slint`, `xilem` (revisit if egui text-editing/IME proves insufficient) |
| Vector rendering | v1: egui painter meshes (tessellated paths); v2: `vello` for GPU path rendering at huge scale | Keep drawing behind a `Renderer` trait so the swap is local to `mm-render` |
| Text | `cosmic-text` (shaping, bidi, fallback) with a glyph atlas cache; egui's text initially | Correct i18n/IME/emoji matter for a notes tool |
| Data structures | Arena (`slotmap`/`generational-arena`) keyed by `NodeId` | Cheap handles, cache-friendly, no `Rc<RefCell>` graphs |
| Spatial index | `rstar` R-tree (or uniform grid) over node AABBs | O(log n) hit-testing and viewport culling |
| Serialization | `serde` + `serde_json` (format v1); `postcard`/`rkyv` for in-memory snapshots | Human-diffable on disk |
| Errors / logging | `thiserror`, `tracing` | |
| Async/background | `rayon` for layout/export; no async runtime in core | Keep core sync and deterministic |
| Testing | `proptest`, `insta` snapshots, `criterion` benches | See §9 |
| Web build | `trunk` + `wasm-bindgen`; File System Access API with download fallback | |

### 5.3 Core model (`mm-core`)
```rust
pub struct NodeId(slotmap::DefaultKey);

pub struct Node {
    pub parent: Option<NodeId>,
    pub children: SmallVec<[NodeId; 4]>, // ordered
    pub text: CompactString,
    pub note: Option<Box<str>>,
    pub style: StyleOverride,             // sparse; falls back to theme
    pub collapsed: bool,
    pub manual_offset: Option<Vec2>,      // user displacement from auto-layout
}

pub struct Document {
    nodes: SlotMap<NodeId, Node>,
    root: NodeId,
    links: Vec<CrossLink>,                // v0.2
    meta: DocMeta,
}
```
- **All mutations go through `Command`s** (`AddChild`, `SetText`, `Move`, `Delete`, `SetStyle`, …). Each command returns its inverse, giving undo/redo for free, a single place for invariants (no cycles, root can't be deleted), and a natural operation log for future sync.
- **Transactions**: group commands (e.g. paste of a subtree, typing burst coalesced by time/cursor) into one undo step.
- **Events**: `Document::apply` returns a `Change` set (dirty nodes, structural vs. content vs. style) consumed by layout and render for incremental updates.

### 5.4 Layout (`mm-layout`)
- Input: tree, per-node measured sizes, layout mode. Output: `HashMap/Vec<NodeId → Rect>`.
- Algorithms: (a) **Reingold–Tilford / Buchheim** for left-to-right and top-down tidy trees, O(n); (b) **radial** (angular wedge proportional to subtree leaf weight).
- **Incremental**: only recompute the ancestors-and-siblings path of dirty nodes when subtree sizes change; full relayout is the fallback and must still be < 10 ms for 5k nodes.
- Collapsed subtrees contribute only their root's size.
- Manual offsets are applied after auto-layout and survive re-layout; "Reset layout" clears them.
- Animation: render interpolates between previous and target rects; layout itself is instantaneous.

### 5.5 Rendering (`mm-render`)
Pipeline per frame:
1. **Camera**: `{ center: Vec2, zoom: f32 }` → world↔screen transforms.
2. **Cull**: query R-tree with the viewport AABB → visible node set (+ edges whose endpoints or bounding box intersect).
3. **LOD**: choose draw style by `zoom` (full / simplified / blobs).
4. **Build draw list**: backend-agnostic `Vec<DrawCmd>` (rounded rect, bezier stroke, glyph run, image). Retained per-node geometry is cached and invalidated by the change set; only camera transform changes while panning.
5. **Submit** to the backend (egui painter → wgpu).
- Text layout results cached per `(text, font_size, max_width)`; measuring is what layout needs, so one cache serves both.
- Target allocation-free steady state while panning (reuse buffers).

### 5.6 App shell (`mm-app`)
- **State**: `AppState { doc, view (camera, selection, editing), history, settings }`.
- **Unidirectional flow**: Input → `Action` enum → `update()` (pure-ish, returns `Command`s + view changes) → doc apply → layout/render invalidation → paint.
- Input mapping layer translates raw events + keymap config into `Action`s; same path for palette items, so every feature is automatically bindable and testable.
- Inline text editing uses a lightweight editor widget (IME-aware) overlaid at the node's screen rect; commits via `SetText` command.

---

## 6. File format

`.mm.json` (UTF-8, pretty-printed, stable key order, designed to diff well in git):
```json
{
  "format": "mindmap",
  "version": 1,
  "id": "6f1c…",
  "root": "n1",
  "nodes": {
    "n1": { "text": "Project", "children": ["n2", "n3"] },
    "n2": { "text": "Goals", "children": [], "collapsed": true,
            "style": { "color": "#e07a5f" }, "note": "…" },
    "n3": { "text": "Risks", "children": [], "offset": [24.0, -8.0] }
  },
  "links": [],
  "view": { "center": [0, 0], "zoom": 1.0 },
  "settings": { "layout": "right" }
}
```
- IDs are stable short strings (so diffs and future CRDT ops are meaningful).
- Unknown fields are preserved on round-trip (forward compatibility).
- Versioned with explicit migrations in `mm-io`.
- Writes are atomic (temp file + rename). Autosave journal (`.mm.json.journal` of commands) enables crash recovery.
- Exports: PNG (offscreen wgpu render), SVG (from the draw list), Markdown outline, OPML, plus CSV and JSON interchange (below).

### 6.1 Import/export overview
| Format | Import | Export | Round-trips losslessly? |
|---|---|---|---|
| `.mm.json` (native) | ✅ | ✅ | Yes (it is the native format) |
| JSON interchange (`mindmap-export`, §6.2) | ✅ | ✅ | Yes for content/structure/style; view state optional |
| JSON generic (arbitrary JSON, §6.2.3) | ✅ | n/a | One-way |
| CSV (§6.3) | ✅ | ✅ | Yes for text/structure/note/color/collapsed; layout offsets optional |

All importers and exporters live in `mm-io`, are pure functions over `Read`/`Write` (so they work in WASM and the CLI), and share one error type with line/column or JSON-pointer locations. Import produces a *new* `Document` (never mutates the open one) plus a `Report` of warnings; the UI shows a preview with the report before committing. Import into an existing map is also supported (`Import as branch under selected node`) and is a single undoable transaction.

### 6.2 JSON import/export

**6.2.1 Export — two shapes** (user chooses; default is *nested*):

*Nested* (human-friendly, mirrors the visual tree):
```json
{
  "format": "mindmap-export", "version": 1,
  "title": "Project",
  "root": {
    "id": "n1", "text": "Project", "note": null,
    "style": { "color": "#e07a5f", "shape": "pill" },
    "collapsed": false,
    "children": [
      { "id": "n2", "text": "Goals", "children": [] }
    ]
  },
  "links": []
}
```
*Flat* (machine-friendly, easy to load into tools/DBs): an array of `{ id, parent_id, order, text, note, color, shape, collapsed, x, y }` records plus `links`. `order` is the zero-based sibling index; `parent_id` is `null` for the root.

Options: pretty/compact, include style, include notes, include layout offsets, include view state, selected-subtree-only.

**6.2.2 Import — auto-detected.** The importer sniffs the input and accepts, in order: (1) native `.mm.json`; (2) `mindmap-export` nested; (3) `mindmap-export` flat; (4) generic JSON (6.2.3). Unknown fields are ignored with a warning (or preserved in a `extra` bag when importing native/interchange files).

**6.2.3 Generic JSON.** Any JSON value maps to a tree so users can visualize API responses and config files:
- Object → node per key; the key becomes the node text, value is its child (scalars are shown as `key: value` on one node by default).
- Array → children in order, labelled `[0]`, `[1]`, … (or by a configurable "label field" such as `name`/`title` when elements are objects).
- A configurable **max depth** and **max nodes** (default 50,000) protect against huge inputs; truncated branches are marked and reported.

Validation: duplicate IDs → regenerated with a warning; dangling `parent_id` → attached to root with a warning; cycles → rejected with the offending IDs; missing/multiple roots → import wizard asks (wrap under a synthetic root, or pick one). Parser is streaming-tolerant for large files (`serde_json` with depth limit; fuzzed).

### 6.3 CSV import/export

**6.3.1 Export schema.** One row per node, depth-first (parent always precedes children), UTF-8 with optional BOM (for Excel), RFC 4180 quoting, header row always written:

| Column | Type | Description |
|---|---|---|
| `id` | string | Stable node ID |
| `parent_id` | string | Empty for the root |
| `order` | int | Sibling index under the parent |
| `level` | int | Depth (root = 0); informational |
| `text` | string | Node text (embedded newlines quoted) |
| `note` | string | Optional note |
| `color` | `#rrggbb` | Optional |
| `shape` | enum | `rounded`, `pill`, `none` (optional) |
| `collapsed` | bool | `true`/`false` |
| `x`, `y` | float | Layout offset, only if "include layout" is on |

Options: delimiter (`,` `;` tab), BOM on/off, include optional columns, selected-subtree-only, **spreadsheet-injection guard** (default on): cells starting with `=`, `+`, `-`, `@` are prefixed with `'` on export; the importer strips that prefix.

**6.3.2 Import — three layouts, auto-detected with manual override:**
1. **Adjacency list** (the export schema above): requires `id` + `parent_id` (+ `text`), other columns optional.
2. **Path / indented columns**: columns `L1, L2, L3, …` (or any header names) where each row names its ancestry, e.g. `Project, Goals, Q1 target`. Rows sharing the same prefix are merged; empty trailing cells end the path. Typical for data coming from spreadsheets and pivot tables.
3. **Single column with leading indentation or `level` column**: for quick outline pastes.

**Column-mapping dialog** (shown whenever headers aren't recognized exactly): maps columns → `text` / `note` / `color` / `parent` / `order` / ignore, shows a live preview of the first 100 rows as a tree, and lets the user save the mapping as a preset.

Parsing details: delimiter and encoding sniffing (UTF-8, UTF-8 BOM, UTF-16, with Windows-1252 fallback prompt), tolerant of ragged rows (warning), trimmed whitespace (option), CRLF/LF, quoted multi-line fields. Built on the `csv` crate. Colors accept `#rgb`, `#rrggbb`, and CSS names; invalid values are dropped with a warning.

### 6.4 Entry points
- **GUI:** `File ▸ Import…` / `Export…` (format picker + options + preview), drag-and-drop a `.csv`/`.json` onto the window, clipboard paste of tab/comma-separated text or JSON (`Ctrl/Cmd+Shift+V` = "Paste as structure"), and "Copy as CSV/JSON" on the selection.
- **CLI:** `mm-cli import in.csv -o out.mm.json [--layout adjacency|path|indent] [--map text=Name,parent=Parent] [--delimiter ';']`, `mm-cli export map.mm.json --format csv|json [--shape nested|flat] [--subtree <id>]`. Same code path as the GUI, so scripting and CI use the identical logic. Reads stdin / writes stdout when paths are `-`.
- **Web:** same dialogs; files via File System Access API or `<input type=file>` / download fallback.

### 6.5 Limits & guarantees
- Import of 100k rows ≤ 1 s native (parse + build + layout deferred); streamed, with a progress bar and cancel in the GUI.
- Export → import round-trip of the same format yields a structurally identical document (IDs, order, text, notes, supported styles); covered by property tests (§9).
- Imports never execute or fetch anything; URLs in cells are stored as plain text.

---

## 7. Cross-cutting requirements

### 7.1 Performance budgets
| Scenario | Budget |
|---|---|
| Cold start to interactive (native) | < 300 ms |
| Open 10k-node file | < 500 ms |
| Pan/zoom with 10k nodes in doc | 60 fps (≤ 16.6 ms frame) on integrated GPU |
| Add/edit node (5k nodes), incl. relayout | < 8 ms |
| Memory, 10k nodes | < 150 MB native |
| WASM bundle (gzipped) | < 3 MB |

Enforced by `criterion` benches in CI with regression thresholds, plus a built-in frame-time overlay (`F12`) fed by `tracing` spans / Tracy.

### 7.2 Reliability
- No data loss: autosave every few seconds to journal; atomic saves; on crash, offer recovery.
- Core is `#![forbid(unsafe_code)]`; no panics on malformed files (fuzz the parser).

### 7.3 Internationalization
- Full Unicode, RTL/bidi, emoji, CJK with IME composition in the inline editor. UI strings via `fluent`-style resource files (English only at launch, but externalized).

### 7.4 Accessibility
- AccessKit integration (egui provides this); expose the tree structure (levels, expanded/collapsed) as a tree view. Keyboard-only operation complete. Respect OS dark mode, reduced motion, and font scaling.

### 7.5 Security / privacy
- No telemetry. No network access in v1 beyond optional update check (off by default). Attachments/links never auto-fetched.

---

## 8. Platform matrix
| Platform | v0.1 | Notes |
|---|---|---|
| Linux (X11/Wayland) | ✅ | AppImage + `.deb` |
| macOS (arm64/x64) | ✅ | Signed `.app`/dmg |
| Windows 10+ | ✅ | MSI / portable exe |
| Web (WebGPU, WebGL2 fallback) | ✅ | Static hosting; File System Access API |
| iOS/Android | ❌ | Revisit post-v1 |

---

## 9. Testing strategy
- **Unit + property tests** on `mm-core`: random command sequences preserve invariants (acyclic, parent/child symmetry) and `apply(cmd); apply(inverse)` is identity.
- **Layout tests**: snapshot (`insta`) positions for canonical trees; property: no overlapping siblings; incremental result == full relayout result.
- **Serialization**: round-trip, golden files per version, unknown-field preservation, `cargo-fuzz` on the loaders (native JSON, interchange JSON, CSV).
- **Import/export**: property test `export(fmt) → import(fmt)` is structurally identical for random documents (CSV and JSON, nested and flat); golden CSV fixtures covering quoting, embedded newlines, BOM, `;`/tab delimiters, ragged rows, UTF-16; spreadsheet-injection cases; malformed inputs (cycles, duplicate IDs, orphan parents, multiple roots) assert specific warnings/errors.
- **Rendering**: headless wgpu snapshot images with tolerance; SVG export compared textually.
- **UI flows**: Action-level tests drive `update()` without a window (keyboard scripts like "Tab, type, Enter, …" → expected doc).
- **Perf**: `criterion` benches for layout, cull, hit-test, load; CI fails on >10% regression.
- CI: `cargo fmt --check`, `clippy -D warnings`, tests on 3 OSes, wasm build, `cargo deny`.

---

## 10. Extensibility & future (design hooks, not v1 work)
- **Sync/collab**: because every change is a `Command` on stable `NodeId`s, the document can be swapped for / mirrored into a CRDT (e.g. `loro` or `automerge` tree types) with the command log as the translation layer. Sync transport (file-sync folder, WebSocket, peer-to-peer) stays outside `mm-core`.
- **Scripting/plugins**: a WASM-component or Lua host exposing the `Command` API.
- **Themes**: TOML theme files loaded from user config dir.
- **CLI**: `mm-cli import`/`export` for CSV and JSON (§6.4), `mm-cli convert in.opml out.mm.json`, `mm-cli export --png`, enabling git hooks/CI doc rendering.

---

## 11. Milestones

| # | Milestone | Deliverable | Exit criteria |
|---|---|---|---|
| M0 | Skeleton (1 wk) | Workspace, CI, window with pan/zoom canvas, FPS overlay | 60 fps empty canvas on all 3 OSes + web |
| M1 | Core + layout (2 wk) | `mm-core` with commands/undo, tidy-tree layout, native JSON I/O, **CSV + JSON import/export in `mm-io` and `mm-cli`**, CLI validate | Property tests green; 10k-node layout < 10 ms |
| M2 | Editable map (2 wk) | Render nodes/edges, selection, inline edit, full default keymap, collapse | Build a 200-node map using only keyboard |
| M3 | Polish (2 wk) | Drag/drop re-parent, styles, themes, search, palette, minimap, autosave/recovery, **import/export dialogs with column mapping and preview** | Dogfood for a week without data loss |
| M4 | Export + web (1–2 wk) | PNG/SVG/Markdown export, WASM build, packaging | Installers produced by CI; web demo live |
| M5 | v0.1 release | Docs, sample maps, perf report | All §7.1 budgets met |

---

## 12. Risks & mitigations
| Risk | Impact | Mitigation |
|---|---|---|
| egui text editing / IME limitations | Poor input for CJK/complex scripts | Isolate inline editor behind a trait; prototype with `cosmic-text` editor early (M2); fall back to `iced`/custom if needed |
| Text measurement cost at 10k nodes | Slow layout/load | Lazy measurement (only visible + estimate for off-screen), cache by `(text,size,width)`, measure on rayon pool |
| Tessellation cost for many Béziers | Frame drops when zoomed out | LOD + edge culling + cached meshes; switch backend to `vello` if profiling demands |
| WASM file access & perf | Reduced web UX | Treat web as first-class in CI from M0; fall back to download/upload |
| Scope creep (features of XMind/Miro) | Never ships | Hold MVP table in §3.1 as the contract; everything else goes to §3.2 |

---

## 13. Open questions
1. Strict tree only for v1, or allow floating nodes from the start (affects layout + file format)?
2. Is the web build a launch requirement or a fast-follow?
3. Preferred license (MIT/Apache-2.0 dual is the Rust default)?
4. Should the on-disk format be a single JSON file or a folder (to hold attachments)? Current plan: single file for v1, zip container (`.mmz`) when attachments land.
5. CSV cross-links: separate edges file (`source_id,target_id,label`) or extra rows in the same file? Current plan: separate file, v0.2.
