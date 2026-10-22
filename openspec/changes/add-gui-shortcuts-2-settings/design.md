## Context

See proposal.md - Why. Slice 1 (`add-gui-shortcuts-1-defaults`, design.md D1--D5) is implemented on bookmark
`add-gui-shortcuts-1-defaults` and is what this builds on: bindings are strings (`"Mod+="`, `"Space"`, `"\\"`) read by
`parseBinding`/`matches` in `lib/shortcuts.ts`, the table is `DEFAULT_SHORTCUTS`, and `hooks/useShortcuts.ts` is the
one window listener. These facts were checked in the code and shape this slice:

- **Settings are a frontend store.** `stores/settings.ts` persists `SettingsData` to `localStorage`
  (`settings-storage`) through zustand `persist`, with a `merge` that repairs every field on rehydrate. Only `apply`
  (Save) writes it. A restart is a rehydrate, which is how the existing tests prove persistence.
- **The dialog edits a draft** (`features/settings/draft.tsx`): rows read `values` and call `update`; Save applies the
  draft, Cancel drops it.
- **Every `SettingsData` field must be on exactly one page.** `pages.ts` makes a field on no page a compile error
  (`_EveryPreferenceIsOnAPage`) and `pages.test.ts` catches one on two. The footer's Reset to defaults resets exactly
  the current page's `keys`.
- **The settings dialog is a Radix dialog**, so slice 1's guard already ignores every key while it is open: a recorder
  inside it cannot trigger the shortcut it is recording. Radix closes the dialog on `Escape` through a capture-phase
  listener on the document, before any handler on the focused element runs; its `onEscapeKeyDown` prop can veto that.
- **`isMacOs()` throws outside Tauri** unless a test mocks it, and the settings store rehydrates at import. So
  anything platform-dependent has to happen where bindings are *used*, not in `merge`.
- **`settings_saved`** names every changed preference but analytics (`lib/events.ts`), so a new field is reported by
  name with no change there.

## Goals / Non-Goals

**Goals:**

- Bindings are the user's, edited and kept exactly like every other preference: a draft, Save, Cancel, per-page Reset,
  a repair on rehydrate.
- No key press is ever claimed by two actions, whatever the stored state.
- No binding can take a key the user needs to get around the window.

**Non-Goals:**

- Tooltips, a cheat sheet, further actions (slice 3).
- Per-platform bindings stored separately. A binding names `Mod`, which already means the right modifier on each
  platform; a user who binds `Meta+…` on Windows has chosen the Windows key on purpose.
- Chords or sequences (`g g`). Nothing in the action list needs one.

## How the work divides

**All in `gui`, all in the frontend.** `opai` and `cli` are unchanged, for the reasons slice 1 gives: key bindings are
presentation, and the settings they live in are already a frontend store. No Rust changes.

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| Persisting a preference | nothing in `rust-sak`; the existing `settings-storage` zustand store, as every preference does | - |
| Key event matching, conflict detection | nothing; DOM event handling in TypeScript, as in slice 1 | - |

`rust-sak` is not used or changed. No crate or npm package is introduced.

## Decisions

### D1. Keyboard is a fifth settings page, last in the list

The page table is the settings dialog's structure: a page is what the nav lists, what Reset to defaults resets, and
what the type check proves every preference is on. A fifth entry with `keys: ["shortcuts"]` gets the nav entry, the
page-scoped Reset to defaults (Joseph's "reset to defaults" request) and the placement proof for nothing. The other
homes are worse: General already holds three unrelated cards, and a Keyboard section inside it would make its Reset
put the language back as well; a separate shortcuts dialog would be a second draft, Save and Cancel to keep in step
with the first. Topaz Photo AI and Lightroom Classic both give shortcuts a pane of their own in their preferences.
Last in the list, because it is the page least often visited and every existing page keeps its place.

This changes the `gui-settings` requirement that the surface has four pages, and its list of defaults (see the spec
delta).

### D2. Only the actions the user changed are stored

`SettingsData.shortcuts: Partial<Record<ShortcutAction, string[]>>`, `{}` by default --- the shape `quality` and
`models` already have, for the same reason: an action the user never touched reads its default *as the defaults are in
this build*, so a default changed or an action added by a later version reaches every user who did not choose
otherwise. An entry that is `[]` is a choice (no binding), not an absence.

Repair in `merge` (`repairShortcuts`), platform-independent because `merge` runs at import: anything that is not an
object reads as `{}`; an unknown action is dropped; a value that is not an array of strings is dropped (reads as the
default); within a list, a string `parseBinding` rejects, one that is refused on every platform (D5), or an exact
duplicate is dropped, and at most four are kept. As implemented, a list that had bindings and loses every one of them
to the repair reads as the defaults rather than as `[]`: none of what the user chose can be honoured, and "no binding"
is not what they chose.

### D3. What is in force is resolved where it is used

`resolveShortcuts(stored, mac)` in `lib/shortcuts.ts` turns the stored overrides into the full table, and is the only
way anything reads bindings:

1. Each action the user changed takes its stored list, in `SHORTCUT_ACTIONS` order, minus any binding refused on this
   platform (D5) and any that overlaps (D4) one an earlier action already took.
2. Each other action takes its defaults, minus any that overlaps a binding step 1 placed.

So the user's choice beats a default and no press is claimed twice, whatever is on disk --- including a default that a
later version changes to collide with something the user bound. `useShortcuts` calls it on each `keydown` from
`useSettingsStore.getState()`, so Save takes effect at once without remounting the listener; `DrawerHeader` reads it
through a small `useShortcutBindings()` hook for `aria-keyshortcuts`.

### D4. Two bindings are the same when one key press matches both

`overlaps(a, b, mac)` asks `matches` itself: it builds the key event for each binding's key under every combination of
the four modifiers (16) and reports whether any one event matches both bindings. This is exact with respect to slice
1's matching rules by construction --- `Mod+=` and `Meta+=` on macOS, `?` and `Shift+?`, `\` and `Alt+\` (Option is
ignored for a bare symbol) all overlap --- where a hand-written normalisation would have to restate those rules and
could drift from them.

### D5. Keys that would trap the user are refused, with a reason

`refusal(binding, mac)` answers `"navigation"`, `"system"` or nothing:

- `"navigation"` on every platform: `Escape` or `Tab` with any modifiers, and `Enter` with no modifier but Shift. Escape
  closes dialogs and menus, Tab moves the focus, and Enter presses the focused button; slice 1's guard leaves a dialog's
  keys alone, but the window itself has focusable controls, and a window-level `Enter` binding would `preventDefault()`
  every button press made with it. Space is *not* refused --- it is no longer a default (slice 1's D1: a closed menu
  hands the focus back to its trigger button, which takes `Space`), but it is Topaz's key and a user may want it, and
  slice 1's guard leaves it to a focused control.
- `"system"`: on macOS Command with `Q`, `W`, `H` or `M` (quit, close, hide, minimise --- the system or the app menu
  takes them before the page sees them, or the window goes away); on Windows and Linux `Alt+F4`.
- A key the recorder cannot name is never offered at all: a modifier alone (recording continues), and `Dead`,
  `Unidentified` and `Process` (an input method's or a dead key's keydown).

Bare letters and digits are allowed: slice 1's guard leaves every key in a text field to the field, so `Z` as a
binding cannot break typing.

### D6. Recording a binding: one focused control, Escape vetoed at the dialog

Add shortcut is a button that, when pressed, becomes the recorder: it keeps the focus, announces "Press the keys for
{{action}}" through `aria-live`, and takes the next `keydown` that is not a lone modifier.
`bindingFromEvent(event, mac)` writes it in the stored grammar: Command on macOS and Control elsewhere become `Mod`,
the other platform's modifier is written as `Ctrl` or `Meta`, AltGr is never a modifier, Shift is left out for a
punctuation key (it is part of typing `?`, as slice 1's matcher already assumes) and kept for a letter, letters are
lower-cased, and `" "` is `Space`.

- `Escape` ends the recording. The dialog's `onEscapeKeyDown` checks whether the event's target is inside an element
  marked `data-shortcut-recorder` and, if so, prevents the dismissal --- the only way to stop it, since Radix listens
  in the capture phase before the button's own handler.
- `Tab` and `Shift+Tab` end the recording and are not prevented, so the focus moves as it always does; so does any
  blur.
- A refused key (D5) ends the recording with the reason shown in the row; a key another action holds ends it with the
  conflict shown (D7).

Bindings are shown with `formatBinding(binding, mac, spaceName)`: on macOS the modifier glyphs `⌃ ⌥ ⇧ ⌘` and the key
(`⌘ =`), elsewhere `Ctrl+Alt+Shift+key`; arrows as `← → ↑ ↓`; `Space` in the user's language; anything else as
`KeyboardEvent.key` spells it. Each binding is a chip with its own remove button whose accessible name includes the
action.

### D7. A conflicting binding is refused, with a one-click move

When the recorded binding overlaps one another action holds, nothing is added; the row shows "{{binding}} is already
used by {{action}}." with one button, *Use here instead*, and the next recording or a Cancel clears it. The button
removes the overlapping binding(s) from the holder and adds the new one here, both in the draft.

- *Silently move it* (VS Code, many games) was rejected: an action can lose its only key without the user seeing
  which, on a page they may not scroll back up.
- *Swap* (give the holder this action's old key) was rejected: it invents a binding the user never chose, and an
  action with two or no bindings has no single "old key" to give.
- *Refuse outright* was rejected: the user then has to find the other row, remove it there and come back --- three
  steps for the commonest reason to rebind.

Refusal with an explicit, adjacent move keeps every change visible and still costs one click.

### D8. No export/import file

Exporting or importing the bindings as JSON was considered (slice 1, Paths considered 3) and left out. The application
has no `tauri-plugin-dialog` or `tauri-plugin-fs` today: a save/open file pair would add two plugins, their
capabilities and their Rust registration --- a native surface larger than this whole slice --- for a feature that only
matters to someone with many machines. The persisted form is already plain JSON in `settings-storage`, and the page
lists every binding at a glance. Revisit if users ask; the stored shape (D2) is the file format it would use.

## Risks / Trade-offs

- [A key the webview or OS takes before the page (e.g. `Ctrl+Tab`, `Ctrl+W` in some WebView2 builds, media keys) can
  be recorded but may never arrive] → `refusal` lists only the combinations known to close or hide the window;
  anything else that never arrives simply does nothing, which the user sees at once and can undo. Verify `Ctrl+W` on
  Windows by hand.
- [Machine-drafted strings in 13 non-English locales] → kept short and in the existing tone; a native check is needed
  before release, as for slice 1's planned strings.
- [A binding recorded on one layout means a different physical key on another] → bindings mean characters (slice 1,
  D2), which is what a user moving layouts expects; documented, not mitigated.
- [Stored bindings from a future version that adds actions] → unknown actions are dropped on rehydrate, so a downgrade
  loses only those actions' choices.
- [Validated against macOS WKWebView in reasoning and jsdom only] → Windows WebView2 and Linux WebKitGTK are untested;
  task 3.2 records what was tried by hand. Execution providers are not involved.

## Slices

1. **`add-gui-shortcuts-1-defaults` --- done (implemented on bookmark `add-gui-shortcuts-1-defaults`; its by-hand
   check 5.2 is still open).** Fixed default bindings for zoom in, zoom out, fit and hold-to-show-original; the
   binding format and matcher; the window-level hook and its guard; `aria-keyshortcuts` on the zoom buttons. Depends
   on nothing.
2. **`add-gui-shortcuts-2-settings` --- current (this change; implemented on bookmark `add-gui-shortcuts-2-settings`,
   its by-hand check 5.2 still open).** A Keyboard page in the settings dialog: each action's bindings, recording,
   removal, conflicts refused with a one-click move, trapping keys refused, Reset to defaults, bindings persisted with
   a repair; i18n in all 14 locales. Depends on slice 1's format and table.
3. **`add-gui-shortcuts-3-discoverable` --- pending.** Shortcuts named in the zoom controls' tooltips and in a cheat
   sheet (`?`); further actions: 100% (actual pixels, `Mod+1`), a Lightroom-style `Z` toggle between fit and 100%,
   next/previous image in the strip's order. Depends on slices 1 and 2 (the cheat sheet and tooltips show the user's
   bindings, and the new actions appear on the Keyboard page).
4. **`add-gui-shortcuts-4-app-actions` --- pending (proposed 2026-10-22).** Fixed application-level keys on every
   platform - Open (`Mod+O`), Settings (`Mod+,`), Close Image (`Mod+W`), Close All Images (`Mod+Shift+W`), Quit
   (Command-Q, Ctrl+Q on Linux, Alt+F4 on Windows) - delivered by a macOS application menu of the app's own (replacing
   Tauri's default, whose Close Window on Command-W quits the app) and by window keys on Windows and Linux; shown
   read-only on the Keyboard page and in the list; on macOS the close button hides the window. Depends on slices 1-3.


## Open Questions

- Should the Keyboard page offer a per-action reset as well as the page's Reset to defaults? Resolved for this slice:
  no. Not needed for the requirements, and it can be added later without changing the stored shape.
