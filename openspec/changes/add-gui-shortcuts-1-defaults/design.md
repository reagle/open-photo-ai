## Context

See proposal.md - Why. These facts were checked in the code and shape the design:

- **There is no keyboard handling at window level today.** The only `keydown` handlers are local: the split divider's
  arrow keys (`PreviewImage.tsx`), Enter in two number fields (`ExportPage.tsx`, `CropDimensions.tsx`), the export
  queue's scroll keys and the slider primitive. No `useHotkeys`, no keybinding library, no Tauri menu or accelerator
  (`crates/gui/src` builds no `Menu`, and `tauri.conf.json` sets no `zoomHotkeysEnabled`, so the webview's own zoom
  hotkeys are off).
- **Zoom is a per-photograph transform in `stores/transform.ts`**, keyed by identity, with `scale` clamped to
  `ZOOM_MIN`..`ZOOM_MAX` (1..8) by `setTransform` and `FITTED = { scale: 1, x: 0, y: 0 }`. The drawer's step buttons
  (`DrawerHeader.tsx`) write `scale +/- ZOOM_BUTTON_STEP` (0.5) with no anchor, which the pane resolves about its
  middle. `FITTED` is "drawn whole, centred by `constrain`".
- **The comparison is drawn by `PreviewImage`**: `source` (the original rendition) and `enhancedSource` (the result,
  or `source` while there is none). The enhanced pane is the only pane in `full` mode and the right-hand / revealed
  pane in `side` and `split`. Its chip reads Enhanced exactly when it draws a result.
- **Settings persist in the frontend**, in `localStorage` through zustand `persist` (`settings-storage`, with a
  per-field `merge` repair), not in Rust. Every `SettingsData` field must sit on exactly one settings page (`pages.ts`
  makes that a compile error), which is why changing bindings is its own slice with its own page.
- **The platform is known synchronously** through `ipc/os.ts` (`isMacOs()`), which already exists for the navbar
  inset.
- **Dialogs and menus are Radix** (shadcn), rendered with `role="dialog"` / `role="menu"` in a portal.

## Goals / Non-Goals

**Goals:**

- One table of actions and their default bindings, in a format slice 2 can persist and edit without a second
  representation.
- One window-level listener, mounted once, that a test can drive with synthetic key events.
- The keyboard zoom is the drawer buttons' zoom: the same step, the same anchor, the same clamping.

**Non-Goals:**

- Rebinding, persistence of bindings, a settings page (slice 2).
- Tooltips or a cheat sheet naming the shortcuts, `Mod+1` actual pixels, a Lightroom-style `Z` toggle, next/previous
  image (slice 3, if wanted).
- Native menu items with accelerators. The application has no menu bar and adding one is a design decision beyond this
  feature.
- Global (system-wide) shortcuts; `tauri-plugin-global-shortcut` is for shortcuts while the app is unfocused, which
  none of these are.

## How the work divides

**All in `gui`, all in the frontend.** `opai` is unchanged: zoom and comparison are view state that only the GUI has.
`cli` is unchanged: it has no interactive view. No Rust in `gui` changes either --- no command is needed, because
every action writes a store the frontend already owns.

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| Keyboard event matching | nothing in `rust-sak`, and none is warranted: it is DOM event handling in TypeScript | - |
| Platform detection | `@tauri-apps/plugin-os` through `ipc/os.ts`, as today | - |

`rust-sak` is not used or changed. No crate is introduced.

## Decisions

### D1. Default bindings follow Topaz Photo AI first, Lightroom Classic second

| Action | macOS | Windows / Linux | Source |
|---|---|---|---|
| Zoom in | `Cmd =` (also `Cmd +`, keypad `+`) | `Ctrl =` (also `Ctrl +`, keypad `+`) | Lightroom Classic: `Cmd/Ctrl =` zoom in [1]; Topaz: `+`/`-` step zoom [3] |
| Zoom out | `Cmd -` (also keypad `-`) | `Ctrl -` (also keypad `-`) | Lightroom Classic: `Cmd/Ctrl -` zoom out [1] |
| Fit to window | `Cmd 0` | `Ctrl 0` | Topaz: `Ctrl/Cmd 0` fit to screen [3]; also the browser and Photoshop convention for "reset zoom" |
| Show original while held | `\` | `\` | Topaz: "click and hold the preview or hit the spacebar to view the original" [2]; Lightroom Classic: `\` shows the Before view [1] |

Lightroom's `\` is a *toggle* and Topaz's spacebar is a *hold*. The user asked for hold, so `\` holds here. `Space` was
a second default at first and was dropped after hand testing (Joseph, 2026-10-24): when a Radix `DropdownMenu` closes,
its `onCloseAutoFocus` returns the focus to the trigger button, and D4 leaves `Space` to a focused button. So after
picking from Add enhancement, the next `Space` opens the menu again and the one after picks its first enabled item,
adding an enhancement; holding `Space` repeats it. The Autopilot switch, the enhancement rows' popover triggers and the
comparison toggle group keep the focus the same way. Giving `Space` to the window on those controls would break keyboard
operation (D4), so it is no default; a user who wants Topaz's key can bind it in slice 2, and the guard still applies to
it. Lightroom's `Z` (toggle between fit and the last zoom) and Topaz's `Cmd/Ctrl 1` (100%) need a notion of "actual
pixels" this canvas does not have yet (`scale` 1 is *fit*, not 100%), so they are deferred to slice 3.

`Mod` is resolved once from `isMacOs()`: `metaKey` on macOS, `ctrlKey` elsewhere. A binding that names `Mod` does not
match when the *other* platform modifier is held, so `Ctrl =` on macOS is not a zoom.

Sources: [1] Adobe, "Keyboard shortcuts for Lightroom Classic",
https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html (Develop
and Loupe tables: zoom in/out, `Z`, `\`, `Y`). [2] Topaz Labs, "User Interface", Photo AI documentation,
https://docs.topazlabs.com/photo-ai/features/user-interface. [3] Topaz community, Ctrl+0 fit / Ctrl+1 100% and
Preferences > Shortcuts > View,
https://community.topazlabs.com/t/released-in-photo-ai-v3-6-0-zoom-improvement-set-preference-to-always-view-at-fit-size-rather-than-100/43224
(a community answer, not official documentation; Topaz's own list is in the app under Preferences > Shortcuts). Topaz
does not support remapping
(https://community.topazlabs.com/t/feature-improvement-custom-shortcut-key-assignment/97365), which is what slice 2
adds here.

### D2. A binding is a string, matched by a small in-house function; no keybinding library

`lib/shortcuts.ts` holds `ShortcutAction` (`zoomIn`, `zoomOut`, `fit`, `showOriginal`),
`DEFAULT_SHORTCUTS: Record<ShortcutAction, readonly string[]>` and `matches(event, binding, mac)`. A binding is
`"Mod+="`, `"Space"`, `"\\"`: modifiers `Mod`, `Shift`, `Alt`, `Ctrl`, `Meta` joined by `+`, then a key. Strings are
what slice 2 persists and what a rebinding control records, so there is one representation from the first slice.

The key is compared on `event.key`, not `event.code`, so a binding means the *character*, which is what users of
non-US layouts expect from `Mod -`. Two equivalences are built in, because the character differs by layout and Shift:
`=` and `+` are one key for matching (as browsers treat zoom), and `-` and `_` likewise. The keypad's `+`/`-` arrive
as `key` `+`/`-` and so match without a rule of their own.

Modifier rules, as implemented: every modifier a binding does not name must be up, except that for a punctuation key
Shift is ignored (it is part of typing `+`, `_` or `|` on some layout), and for a punctuation binding with no
`Mod`/`Ctrl`/`Meta` Alt is ignored too (a German Mac types `\` with Option). AltGr, which Windows reports as Ctrl plus
Alt, is read through `getModifierState("AltGraph")` and never counts as a modifier. So `Mod+Alt+=` does not zoom,
while an Option- or AltGr-typed `\` still holds the original.

*Alternatives:* `react-hotkeys-hook` (scopes, `mod` alias, keyup) and `tinykeys` (tiny, string sequences). Either
would replace about 60 lines; neither handles the hold-with-release-on-blur semantics (D3) or the guard (D4) without
the same code around it, and the project's stack list does not include one. Not worth a dependency for four actions.

### D3. Hold-to-show-original is a non-persisted store the canvas reads

`stores/peek.ts` holds `showingOriginal: boolean`. `useShortcuts` sets it on the bound key's `keydown` (ignoring
`event.repeat`) and clears it on that key's `keyup`, and on window `blur` and `visibilitychange` --- a key released
while another window has focus delivers no `keyup`, and without this the original would stick on screen.
`PreviewImage` reads it: while set, `enhancedSource` is `source` and the enhanced pane's label is Original, in all
three modes. Nothing else changes, so the transform, the divider and a running enhancement are untouched.

As implemented, the hook remembers the *physical* key (`event.code`) that started the hold and releases on that key's
`keyup` whatever the modifiers are by then (Shift pressed mid-hold turns `\` into `|`). It also releases on the
`keyup` of `Meta`, because macOS delivers no `keyup` for a key let go while Command is down. Keyups are handled
regardless of the D4 guard, so a dialog opening mid-hold cannot strand the original on screen.

Not in `stores/preview.ts`, which persists its whole state into the Wails app's `app-storage` key (see the comment
there) and must hold only `previewMode`.

### D4. One guard decides when a shortcut is not the window's

`useShortcuts` (mounted once in `App.tsx`, listening on `window`) ignores an event when any of these holds:

- its target is an `input` (other than checkbox, radio, range), `textarea`, `select` or `isContentEditable`;
- a Radix dialog or menu is open (`[role="dialog"]`, `[role="alertdialog"]`, `[role="menu"]` in the document);
- the key is `Space` (no default since D1, but the user may bind it) and the target is a button, checkbox, slider, tab,
  menu item, or anything with `role` of those --- `Space` activates the focused control, and taking it would break
  keyboard operation of the whole window.
- as implemented, also: an open native `<dialog>`; `Space` on any `input`, `summary`, or a `role` of `radio` (the
  drawer's comparison toggles are Radix radios), `switch` (Autopilot), `option`, `menuitemcheckbox` or
  `menuitemradio`; an event a control already `preventDefault()`ed; and an event fired while an input method is
  composing.

A handled event gets `preventDefault()` so a bound `Space` does not scroll and `Mod+=` does nothing else. Unhandled
events are left alone.

### D5. Zoom and fit go through the same writer as the drawer's buttons

A `stepZoom(direction)` and `fitCurrent()` pair, beside the store in `stores/transform.ts`, read `useFileStore` and
`useTransformStore` via `getState()` and write
`setTransform(identity, { scale: current.scale +/- ZOOM_BUTTON_STEP, x, y })` and `setTransform(identity, FITTED)`.
`DrawerHeader`'s buttons call `stepZoom` too, so the two routes cannot drift. With no current image, or one with no
identity, they do nothing --- the same condition that disables the buttons.

## Risks / Trade-offs

- [WebView2 may still act on `Ctrl =`/`Ctrl -`/`Ctrl 0` as browser accelerators on Windows] → Tauri's
  `zoomHotkeysEnabled: false` disables WebView2's zoom control, and the handler calls `preventDefault()`; verify by
  hand on Windows, and if the webview still zooms, set the window's `zoomHotkeysEnabled` explicitly or switch zoom-in
  to `Ctrl Shift =`.
- [A focused button keeps the focus after a click or after its menu closes, and would take `Space`] → `Space` is not a
  default (D1); `\` is not subject to the button rule. A user who binds `Space` gets the button rule with it.
- [`\` is awkward on some layouts (German, Nordic: AltGr combinations)] → slice 2 lets the user rebind it; matching on
  `event.key` means an AltGr-produced `\` still matches.
- [A `keyup` lost to an OS shortcut (Cmd-Tab) leaves the original showing] → cleared on `blur` and `visibilitychange`
  (D3).
- [Linux WebKitGTK and Windows WebView2 are untested in this design] → validated in reasoning against macOS WKWebView
  only; task 5.2 records which platforms were tried by hand. Execution providers are not involved.

## Difficulty and likely paths

**Difficulty: easy to moderate.** Slice 1 is about a day for someone who knows the codebase (10 tasks, all frontend,
no Rust, no IPC); the logic is small, and most of the time goes into tests and the platform checks. Slice 2 is
moderate (2--3 days): a key-recording control, conflict handling, a fifth settings page that changes an existing
requirement ("four pages"), `merge` repair for a new persisted field, and 14 locale files. Slice 3 is easy and
optional.

**Risks:** webview accelerator behaviour on Windows/Linux (above); i18n volume in slice 2 (machine-drafted strings in
13 non-English locales need a native check); the `_EveryPreferenceIsOnAPage` type check forces the settings page to
land with the persisted field, which is why they are one slice.

**Paths considered:**

1. **Recommended --- frontend-only, defaults first, settings page second.** Ships value immediately, keeps one binding
   format across slices, and changes nothing in Rust.
2. **Native Tauri menu with accelerators** (View > Zoom In / Zoom Out / Fit). Gives macOS users a discoverable menu
   bar and OS-standard accelerator display, but a menu cannot express press-and-hold, would split the shortcuts across
   two systems, and introduces a menu bar the design (Claude Design screens, macOS overlay title bar) does not have.
   Better proposed upstream on its own if wanted.
3. **A config file (JSON/TOML) instead of a settings page.** The application has no settings file: every preference is
   in the webview's `localStorage`, and `opai` has no settings store. A file would need a Rust reader, a watcher or
   restart rule, and a second source of truth beside `settings-storage`. Rejected for the main path; an "export/import
   shortcuts as JSON" button on the slice-2 page is a cheap way to give power users a file without that cost (open
   question).
4. **A keybinding library** (`react-hotkeys-hook`). Rejected in D2.

**Would upstream accept it?** Likely, in this shape: it is small, frontend-only, adds no dependency, follows the
reference apps users already know, and does not alter existing behaviour. CONTRIBUTING.md says new UI features should
get an issue first, so open an issue on vegidio/open-photo-ai describing slices 1--2 and the default table before
sending a PR; slice 1 on its own is the easiest PR to accept. Slice 2 touches the settings dialog's page structure and
every locale, which the maintainer may want to shape (for example, a sixth page versus a section on General). Keep
slice 1 independent of slice 2 so it can be offered alone.

## Slices

1. **`add-gui-shortcuts-1-defaults` --- current (this change).** Fixed default bindings for zoom in, zoom out, fit and
   hold-to-show-original; the binding format and matcher; the window-level hook and its guard; `aria-keyshortcuts` on
   the zoom buttons. Depends on nothing. Pending implementation.
2. **`add-gui-shortcuts-2-settings` --- pending.** A Keyboard page in the settings dialog listing each action with its
   binding(s), a control that records a new binding, clear and per-page reset, conflict detection (a binding already
   in use is refused with the action that holds it named, or swapped --- to decide), bindings persisted in
   `settings-storage` with a `merge` repair that drops unknown actions and malformed strings; i18n keys in all 14
   locales; modifies `gui-settings` (five pages; reset defaults) and `gui-shortcuts` (bindings are the user's).
   Depends on slice 1's format and table.
3. **`add-gui-shortcuts-3-discoverable` --- pending, optional.** Shortcuts named in the zoom and comparison controls'
   tooltips and in a cheat sheet (`?` or `Mod+/`); further actions: 100% (actual pixels, `Mod+1`), a Lightroom-style
   `Z` toggle between fit and the last zoom, cycling the comparison mode, previous/next image. Depends on slices 1 and
   2 (so the cheat sheet shows the user's bindings).
4. **`add-gui-shortcuts-4-app-actions` --- pending (proposed 2026-10-22).** Fixed application-level keys on every
   platform - Open (`Mod+O`), Settings (`Mod+,`), Close Image (`Mod+W`), Close All Images (`Mod+Shift+W`), Quit
   (Command-Q, Ctrl+Q on Linux, Alt+F4 on Windows) - delivered by a macOS application menu of the app's own (replacing
   Tauri's default, whose Close Window on Command-W quits the app) and by window keys on Windows and Linux; shown
   read-only on the Keyboard page and in the list; on macOS the close button hides the window. Depends on slices 1-3.


## Open Questions

- Should `\` hold (as proposed, matching Topaz's spacebar) or toggle (as Lightroom's `\` does)? Either fits slice 1's
  tasks; hold is assumed.
