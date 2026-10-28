> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code; Claude Opus 5.5). Joseph reviews and is responsible for it.

## Why

Every launch starts with an empty window. A photograph worked on yesterday has to be found again in the file manager
or the picker, and there is no list of what was opened lately - the File menu every macOS document application has
(File > Open Recent) is missing from the menu bar `add-gui-shortcuts-4-app-actions` built. And when the application is
quit with a gallery of photographs in progress, the gallery and every enhancement stack, framing and mask in it are
lost.

## What Changes

Two slices; **slice 1 is built, slice 2 is a proposal only, pending Joseph's decision** (2026-10-27: Open Recent was
asked for first, as its own step).

- **Slice 1 - File > Open Recent (built, bookmark `add-gui-open-recent`).** The ten photographs most recently opened,
  newest first, whatever route opened them (the picker, a drop, Open With, Open Recent itself), plus Clear Menu. Choosing
  one opens it as the picker would - adding it to the gallery if it is not there, making it current if it is. A
  photograph no longer at its path is not opened: the window says so and the entry leaves the list. On macOS the list is
  a submenu of the app's own File menu, under Open...; on Windows and Linux, which have no menu bar, it is a menu button
  beside the drawer's Add images. The list lives in a small versioned state file, `session.json`, beside
  `gui-settings.json` in the application's configuration directory, written atomically. Reopening a photograph **does
  not** bring back its enhancement stack in this slice: keeping a closed photograph's stack needs the capture that
  slice 2 builds (design.md - D6).
- **Slice 2 - Reopen last session (proposal only, not built).** A Settings switch, on by default. At launch the gallery
  comes back with the same photographs in the same order, the same current and picked ones, and each photograph's edit
  state: its stack with every parameter (lens blur focus and f-number included), its framing, its limiting mask and its
  removal strokes, and its face choices. No pixels are stored: results come from the run cache when present and are
  otherwise run lazily for the photograph that is selected, as opening one runs today. Missing photographs are skipped
  with one notice; a photograph whose bytes changed keeps its place but loses its saved state; a corrupt or other-version
  file is an empty session. Open Recent then also brings a photograph's saved stack back when its identity matches.

## Capabilities

### New Capabilities

- `gui-session`: what the window remembers across launches - the recent photographs (slice 1) and the last session
  (slice 2) - the state file that holds them, and how a damaged, foreign or stale file is read.

### Modified Capabilities

- `gui-shell`: File > Open Recent on macOS, and the same list where Open is offered on Windows and Linux.

## Impact

- Crates: `gui` only. Slice 1: a new `src/recent.rs` (the list, the state file, three commands), `src/menu.rs` (the
  Open Recent submenu and its rebuild), `src/lib.rs` (state, commands); frontend `ipc/recent.ts`, `stores/recent.ts`,
  `lib/recent.ts`, `hooks/useAppActions.ts`, `features/drawer/DrawerHeader.tsx`, 14 locale files. Nothing in `opai`.
- `rust-sak`: `fs::user_config_dir` and `fs::mk_temp_file_in`, already used for `analytics.json`; nothing to extend.
- No new crates or npm packages.
- Base: `integration` (it needs the app menu of `add-gui-shortcuts-4-app-actions`). Fork-local; no PR is planned.
