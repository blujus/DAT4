# Mindmap

A fast, keyboard-first mind-mapping app in Rust (egui/eframe on wgpu → Metal on Apple Silicon).
See [SPEC.md](SPEC.md) for the full design; this file says what is actually built.

## Run (macOS, Apple Silicon)

```bash
cargo run --release -p mm-app          # opens the app
cargo run --release -p mm-app -- map.mm.json
scripts/bundle-macos.sh --dmg          # dist/Mindmap.app + dist/Mindmap-<ver>-arm64.dmg
```

Needs Rust 1.95+ (`rustup update stable`) and the Xcode command-line tools. Native target is `aarch64-apple-darwin`;
the same code builds on Linux/Windows (`cargo run --release -p mm-app`).

## Workspace

| Crate | What it is |
|---|---|
| `mm-core` | Document model (arena, stable ids), reversible `Command`s, undo/redo with grouping, outline parser, thinking templates. No UI, no I/O. |
| `mm-layout` | Pure layout: mind map (two-sided), logic chart (left→right), org chart (top-down), **fishbone**; collapse, manual offsets, spatial navigation, hit-testing. |
| `mm-io` | Native `.mm.json`, JSON interchange (nested/flat/generic), CSV (adjacency/path/level), Markdown outline. Atomic writes. |
| `mm-app` | `Model` (UI-free editing state machine, fully unit-tested) + egui front end. |
| `mm-cli` | `mm-cli convert / validate / templates / new` using the same code as the GUI. |

## Keyboard

| Key | Action |
|---|---|
| `Tab` | New child (while typing: commit, then child) |
| `Enter` | New sibling (while typing: commit, then sibling) |
| `Enter` on an empty new node | Climb one level out; repeat to keep climbing |
| `Tab` on an empty new node | Indent under previous sibling |
| `Esc` | Finish editing; cancels an empty new node cleanly |
| `Shift+Enter` | Newline while typing · sibling above when navigating |
| Arrows | Spatial navigation (`Shift` extends selection) |
| `Alt+↑/↓` | Reorder among siblings |
| `Alt+←/→` | Outdent / indent — move between branches |
| `Alt+M` | **Grab**: `↑/↓` pick a new parent anywhere in the map, `Enter` drops, `Esc` cancels |
| `⇧⌘M` | Move selection under a node found by name |
| `⌘F` / `⌘P` | Find and jump |
| Just type | Replace the selected node's text · `F2`/`Space` edits it |
| `⌘.` | Collapse / expand · `⌘1…9` show N levels · `⌘0` all |
| `⌘C ⌘X ⌘V` | Copy/cut/paste branches; pasting indented text creates a tree (`⇧⌘V` = sibling) |
| `⌘Z` / `⇧⌘Z` | Undo / redo |
| `⇧⌘1` / `⇧⌘2` | Fit all / fit selection |
| `⌘S ⇧⌘S ⌘O ⌘N` | Save, save as, open, new from template |

Mouse: drag background to pan, scroll/pinch to zoom, drag a node onto another to re-parent,
`Shift`+drag for marquee, double-click to edit.

## Import / export

File ▸ Import accepts CSV, JSON and indented-text outlines, shows a warning report and lets you open the
result as a new map or insert it under the selected node (one undo step). File ▸ Export writes CSV
(whole map or selected branch), JSON (nested or flat) and a Markdown outline. Same via CLI:

```bash
mm-cli convert map.mm.json map.csv
mm-cli convert data.csv map.json --flat
mm-cli convert api-response.json map.mm.json     # any JSON becomes a tree
mm-cli new "fishbone (6m)" cause.mm.json
```

CSV rules are in SPEC §6.3. Cells starting with `= + - @` are quoted on export and restored on import.

## Status vs. the spec

Built and tested: everything in the table above, fishbone/org/logic/mind-map layouts, templates
(fishbone 6M/4P, 5 Whys, SCAMPER, Six Hats, SWOT, pros/cons), notes, colours, shapes, autosave
(every 5 s next to the file, `File ▸ Recover autosave`), unsaved-changes prompts.

Not built yet (still in the spec): file/image attachments and the `.mmz` package, links between nodes,
tags/markers/tasks, concept-map/timeline/matrix layouts, SVG/PNG export, command palette, focus mode,
Vim keymap, remappable keys, optional AI assist, web (WASM) build, unknown-field preservation on save.
Rendering is immediate-mode egui with viewport culling (no R-tree yet); layout of 10k nodes is a
few ms in release builds.

## Verified vs. unverified

- **Tested (41 tests, run in CI):** model, undo/redo (property-tested), all layouts (no-overlap checks incl. a fishbone
  regression), CSV/JSON/native round-trips (property-tested), hostile-input cases, and scripted keyboard flows against
  the UI-free `Model`.
- **Run for real on Linux** under a virtual display with software rendering and simulated keystrokes: Tab/Enter
  rapid-fire typing, climbing out with Enter, typing over a node, fishbone view, Alt+M grab-and-drop between
  branches. That run found and fixed three bugs (font-init crash on startup, lost first character when typing over a
  node, overlapping fishbone labels).
- **Type-checks for `aarch64-apple-darwin`** (all five crates), but has **not been linked or launched on a Mac** — this
  environment has no macOS SDK. The `macos-14` CI job builds, tests and bundles it; first launch on Apple Silicon is
  the real test. Not exercised anywhere yet: native file dialogs, import/export through the GUI, mouse drag-and-drop,
  clipboard, autosave recovery, and the Metal backend.
