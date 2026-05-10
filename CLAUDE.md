# CLAUDE.md

This repository is a data-science course (DAT4). Most of the tree (`code/`,
`data/`, `homework/`, `notebooks/`, `slides/`) is course material — see
`README.md` for the course-level guide.

## Active subprojects

- **`backpack/`** — an agentic LEGO Mindstorms robot. Raspberry Pi cortex
  running Claude Opus 4.7, Rust BLE daemon, LEGO 51515 hub, MuJoCo digital
  twin for RL training. Lives entirely under `backpack/` so it doesn't
  clobber the course material above.

  **Read `backpack/CLAUDE.md` before doing any work in `backpack/`.**
  It documents the architecture, the IPC + gRPC protocols, build/test
  commands per layer, the model defaults, and the known stubs.

## Branch protocol

- `master` is the course's primary branch. Don't push to it directly.
- Subproject development happens on dedicated feature branches
  (e.g. `claude/lego-robot-raspberry-pi-2BJ6f` for `backpack/`).
- Don't merge a subproject into `master` without explicit confirmation —
  the course repo's owner may not want experimental code mixed in.
