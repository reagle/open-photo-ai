## Context

- What is open is frontend state: `stores/files.ts` (`files`, `currentIndex`, `selectedPaths`), and it is "not
  persisted" by decision - its own comment says the reference does not restore it either.
- Per photograph, the edit state is spread over four stores, each registered as a `FileOwner` that forgets its entry
  in the same synchronous block as `closeFile` removes the row: `stores/enhancements.ts` (the stack by path, the wire's
  own `Operation[]`, which already carries every parameter - lens blur's `f_number`, `focus_x`, `focus_y` included),
  `stores/crop.ts` (`CropInfo` by identity), `stores/mask.ts` (the limiting mask and the removal by path, each
  `strokes` + `inverted` plus an undo history) and `stores/faces.ts` (`choices` by identity). Faces found, transforms,
  Autopilot runs and the focus pick are derived or momentary and are not edit state.
- The macOS menu bar is Rust's (`src/menu.rs`, from `add-gui-shortcuts-4-app-actions`): items of the application's own
  are forwarded to the window as `app-menu` with the item's id and acted on by `hooks/useAppActions.ts`; labels arrive
  from the window through `configure_menu`. Windows and Linux have no menu bar: the same actions are window keys, and
  Open is offered by the drawer's Add images and the empty canvas's Browse images.
- The configuration directory already holds `gui-settings.json` (`opai::settings`, written by `src/gui_settings.rs`)
  and `analytics.json` (`src/analytics.rs`), both found with `rust_sak::fs::user_config_dir` and written through a
  temporary file renamed over the old one.
- Every photograph has a content identity (`ImageRecord.identity`, XXH3-64 of its bytes) computed when it is admitted.

## Goals / Non-Goals

**Goals (slice 1):** File > Open Recent as macOS applications have it; the same list on Windows and Linux where Open is
offered; one rule for what counts as opened, whatever opened it; a state file that slice 2 extends rather than
replaces.

**Goals (slice 2, not built):** the gallery and its edit state back at launch, without running anything that was not
asked for.

**Non-Goals:** storing result pixels (the run cache, `memo.redb`, already outlives the process); recent folders; a
recent list shared with the command line; the macOS Dock's own recent-documents list (`NSDocumentController`), which
Tauri does not expose.

## How the work divides

Rust owns the list, the file and the menu: it reads the file at setup, keeps the list as application state, writes it on
every change, and rebuilds the submenu from it. The window owns what opening means: it tells Rust which photographs
entered the gallery, and it opens the one chosen through the same route a drop takes.

## `rust-sak` coverage

`fs::user_config_dir` and `fs::mk_temp_file_in`, as for `analytics.json`. Nothing to add.

## Decisions

### D1. One state file, `session.json`, versioned

Beside `gui-settings.json`, at `rust_sak::fs::user_config_dir(opai::APP_NAME, "session.json")`. Slice 1 writes

```json
{ "version": 1, "recent": [{ "path": "/Users/me/Pictures/a.jpg", "identity": "9f3c…" }] }
```

and slice 2 adds a `session` member to the same object (D8). Written through a temporary file in the same directory
and renamed over the old one, so a reader sees the old list or the new one and never a torn file; a write that would
change nothing is skipped. Read once, at setup. **A file that is absent, unreadable, not JSON, or of another `version`
is an empty list**, logged as a warning with the reason, and replaced at the next change - never an error at launch.
Unknown members are ignored on read, so a file written by a later slice still yields its list.

Alternative: a second key in the window's `localStorage`. Rejected: the menu bar is built before the page loads, and
Rust cannot read the webview's storage.

### D2. The list: ten, newest first, by path

Noting a batch puts its photographs at the front in the batch's own order (the first of a batch is the one the window
makes current), removes any earlier entry at the same path, and keeps the first ten. The identity is kept beside the
path - refreshed on every note - for slice 2's check (D10); slice 1 does not use it.

### D3. What counts as opened: entering the gallery

The window notes the photographs that entered the file store, found by one subscription comparing the paths before and
after each write, so the picker, a drop, Open With, `--gui`, the Open Recent list itself and Browse images all count the
same way. A batch that adds nothing new (the folder opened again) notes nothing. Choosing a photograph that is already
open makes it current and notes it again, so it moves to the top.

Alternative: note in Rust's `describe_images`. Rejected: describing is not entering - the window de-duplicates after
it, and slice 2's restore describes photographs that should not be re-noted.

### D4. The macOS submenu

File holds Open..., then **Open Recent**, a submenu: one item per entry, ids `recent:0` ... `recent:9`, labelled with
the file name, and with " - <parent folder>" where two entries share a name, as Finder disambiguates; a separator; and
**Clear Menu**, unavailable while the list is empty. The submenu's title and Clear Menu are relabelled by
`configure_menu` (keys `openRecent`, `clearRecent`); the entries are file names and are never translated. On every
change of the list Rust removes the entries and inserts the new ones above the separator, leaving the two labelled
items in place. A click on an entry is resolved in Rust to its path and forwarded as `open-recent` carrying the path,
not the index, so a list changed between the click and the window's answer cannot open the wrong photograph; Clear Menu
is acted on in Rust. Every change is published to the window as `recent-changed` with the list.

### D5. Opening a recent photograph

`openRecent(path)` in `lib/recent.ts`, shared by the menu and the Windows/Linux button: a photograph already open is
made current; otherwise `check_recent(path)` asks Rust whether it is still there - where it is not, the entry leaves the
list and the window says so (`toasts.recentMissing`) - and where it is, it is opened by `openPaths([path], "recent")`,
the route a drop takes, so the refusal and `files_added` (`source: "recent"`) are the same. Like the other menu items,
an entry does nothing while a dialog or menu is open over the window.

### D6. Slice 1 does not bring a stack back

Asked for "only if cheap", and it is not: closing a photograph makes each of the four stores forget it in the same
synchronous block, before anything after the close could read it, so keeping a closed photograph's stack means a
capture that runs before the owners do and a serialization of all four stores - which is slice 2's whole mechanism
(D9). Slice 1 therefore reopens the file only; the identity it already records is what slice 2 checks the saved state
against.

### D7. Windows and Linux: a button where Open is

They have no menu bar, so the list is a menu button, "Open Recent" (a clock icon), beside the drawer's Add images,
offered there only off macOS (macOS has File > Open Recent). It lists the same entries and Clear Menu, and is
unavailable while the list is empty. No new key: Open Recent has none on macOS either.

### D8. Slice 2 (not built): what is saved

`session: { files: [{ path, identity, edits }], current, picked }`, `edits` being `{ stack, crop, mask, removal, faces }`
in the stores' own shapes - the stack as `Operation[]`, the mask and removal as `strokes` + `inverted` without their
undo history, which is editing state. Saved debounced (500 ms after the last change to any of the five stores) and once
more when the window is closing; written by Rust through the same atomic write. A **Reopen last session** switch on the
Settings dialog's General page, on by default; off means nothing is saved and the next launch is empty.

### D9. Slice 2 (not built): capturing a closing photograph

`stores/files.ts` gains a "before closing" hook called with the closing records **before** the owners forget, and the
session module answers it by writing those photographs' edits into their recent entries. With that in place Open
Recent also brings a photograph's saved state back (D10).

### D10. Slice 2 (not built): restore

At launch, with the switch on, the window asks Rust to `restore_session`: Rust drops entries whose file is missing
(counted, one notice "N photographs could not be found"), describes and admits the rest - computing their identity -
and drops the `edits` of any whose identity no longer matches, keeping the photograph. The window adds the files in the
saved order, sets current and picked, and writes the four stores. Nothing is run: the run effect already runs only for
the current photograph, from the run cache where it can, so the rest of the gallery runs when it is selected - which is
also how opening a photograph behaves today. A photograph restored with a stack key, even an empty one, is not
re-analysed by Autopilot.

## Risks / Trade-offs

- [An entry's label names a file the user has since renamed] -> choosing it says it is missing and removes it.
- [The submenu is rebuilt on every opening] -> at most ten removals and insertions, on a list change only.
- [A slice-1 build rewriting a slice-2 file drops its `session`] -> only after downgrading a build; the list survives.
- [Validated against] macOS by tests of the plan and by hand; Windows and Linux by tests of the button only.

## Difficulty and likely paths

Slice 1: small (one agent session). Slice 2: moderate - the capture hook, serializing four stores, the restore path and
its tests; about a day of agent time.

## Slices

1. **`add-gui-open-recent` --- built (this change's slice 1; by-hand check open).** The recent list, its state file,
   File > Open Recent on macOS and the button on Windows and Linux.
2. **`add-gui-session-restore` --- proposal only, pending Joseph's decision.** Reopen last session (D8-D10) and Open
   Recent bringing a saved stack back.
