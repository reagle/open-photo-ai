## Context

See proposal.md - Why. Slices 1 and 2 are implemented on bookmarks `add-gui-shortcuts-1-defaults` and
`add-gui-shortcuts-2-settings`, and this slice builds on both: bindings are strings matched by `matches`, the actions
are `SHORTCUT_ACTIONS`, what is in force is `resolveShortcuts(stored, mac)`, `useShortcuts` is the one window listener
with the `isWindowShortcut` guard, and the Keyboard settings page draws one row per action. These facts were checked
in the code and shape this slice:

- **Scale 1 is *fitted*, not 100%.** `geometry.ts` - `fitInside` draws the photograph at the size that fits the pane,
  enlarging a small one, and every transform's `scale` multiplies that. So "actual pixels" depends on the pane's
  measured size and the photograph's own size, which only `PreviewImage` knows. The enhanced pane is already the one
  that publishes what it draws (`publishViewport` writes `viewport` in `stores/transform.ts` for the sidebar).
- **The photograph's size is the original's** (`ImageRecord.width/height`, with the decoded rendition's as fallback),
  so a fit ratio computed there is in the photograph's own pixels even when the rendition served is smaller.
- **The range is 1x..8x** (`ZOOM_MIN`, `ZOOM_MAX`), clamped by `setTransform`.
- **The drawer lists `useFileStore`'s `files` in order** (`DrawerBody.tsx`, virtualised with
  `@tanstack/react-virtual`), and `currentIndex` is the current image. Nothing scrolls the strip to the current image
  today; clicking a thumbnail is the only way to change it, and that thumbnail is on screen.
- **Arrow keys are already in use** by the split's divider (`PreviewImage.tsx`, prevents default), the Radix slider
  (prevents default) and the Radix toggle group's roving focus. Slice 1's guard leaves an event a control already
  prevented alone, but it cannot rely on every control doing so.
- **Tooltips** are the vendored shadcn `Tooltip`; the comparison toggles already use it.

## Goals / Non-Goals

**Goals:**

- A user can find every shortcut from the window itself, in the form they press it, including after rebinding.
- Actual pixels means what it means in Topaz Photo AI and Lightroom Classic: one photograph pixel per display pixel.
- The new actions are slice 2's ordinary actions: listed, rebindable, stored, repaired, reset.

**Non-Goals:**

- Cycling the comparison mode from the keyboard. Slice 1's plan named it; slice 2's Slices list dropped it, and
  Joseph's list for this slice does not include it. It needs a choice of key that collides with nothing in either
  reference app, and the three-button toggle is one click away. Left for a later change if wanted.
- A menu bar with accelerators (slice 1, Paths considered 2).
- Opening Settings on the Keyboard page from the list: the settings dialog always opens on General (a requirement of
  `gui-settings`), and changing that is out of scope. The list names where to go instead.

## How the work divides

**All in `gui`, all in the frontend**, for slice 1's reasons. `opai` and `cli` are unchanged. No Rust changes.

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| Key matching, the list, tooltips | nothing; DOM and React in TypeScript, as in slices 1 and 2 | - |
| Device pixel ratio | `window.devicePixelRatio`, read at the moment of the action | - |

`rust-sak` is not used or changed. No crate or npm package is introduced.

## Decisions

### D1. Actual pixels is one photograph pixel per *device* pixel, from a fit ratio the pane publishes

The enhanced pane, which already publishes the viewport, also publishes `fit: { identity, ratio }`, where `ratio` is
the fitted width over the photograph's own width (pane pixels per photograph pixel at 1x). `actualPixels()` beside
`stepZoom` in `stores/transform.ts` sets the current photograph's scale to `1 / (ratio * devicePixelRatio)`, with no
anchor (the middle of the pane holds still, as for the drawer's buttons), clamped by `setTransform` to 1x..8x. It does
nothing when the published ratio is for another photograph or absent (a pane not yet measured).

Device pixels rather than CSS pixels because that is what "100%" and "1:1" mean in Topaz Photo AI, Lightroom Classic
and Photoshop on a high-density display: at CSS pixels a Retina screen would show each photograph pixel as four, which
is a 200% view. The clamp means a photograph smaller than the pane cannot be shown below fitted (the preview has no
zoom below 1x) and one more than eight times the pane cannot reach 100%; both are stated in the spec rather than
worked around, since widening the zoom range changes the slider and the wheel too.

### D2. `Z` toggles between fitted and actual pixels

From the fitted size (scale 1), `toggleZoom()` goes to actual pixels; from any other magnification it fits. Key repeat
is ignored for it, so a held `Z` does not flip back and forth. Joseph chose this over Lightroom's exact behaviour
(toggle between fit and the *last* zoom level), which would need a remembered level per photograph for little gain: a
user who wants a particular magnification uses the slider or the step keys. `Z` is a bare letter, safe because slice
1's guard leaves every key in a text field to the field.

### D3. Previous and next follow the drawer's order and stop at the ends

`stepImage(direction)` in `stores/files.ts` moves `currentIndex` by one within `0..files.length-1`, doing nothing at
either end and with nothing open, which is what Lightroom's filmstrip does. No wrap: wrapping silently jumps the strip
from the last photograph to the first, and a user holding `→` to reach the end would cycle forever. The default keys
are `←` and `→`, Lightroom's. `DrawerBody` scrolls the current item into view whenever `currentIndex` changes
(`virtualizer.scrollToIndex(index, { align: "auto" })`, which does nothing when it is already in view), so the
keyboard never moves the current image out of sight; a click never needs it, which is why nothing did before.

### D4. Arrow keys belong to a focused control that moves by them

`isWindowShortcut` gains one rule: an arrow key, `Home`, `End`, `PageUp` or `PageDown` is not the window's when its
target is inside a slider, a radio group or radio, a tab list or tab, a separator (the split's divider), a list box, a
grid, a tree, a menu, a toolbar, a spin button or a scroll bar, by `role`, or a `[data-arrow-keys]` element. This is
the same shape as slice 1's `Space` rule: the control that has the focus keeps the keys it is operated with, and a
plain button or the canvas leaves them to the window. It is independent of whether a given control remembers to
`preventDefault()`.

### D5. The list is a dialog of its own, opened by an action

`showShortcuts` (default `?`) opens `ShortcutsDialog`, a Radix dialog mounted once in `App.tsx`, whose open state is a
small non-persisted store (`stores/shortcutsList.ts`) so the hook can open it without a component in reach. It lists
every action in `SHORTCUT_ACTIONS` order with its bindings in force, through `formatBinding` (`⌘ =` on macOS), and "No
shortcut" for an action with none; a footer line says they are changed in Settings > Keyboard. While it is open slice
1's guard ignores every key, so `?` does not close it; `Escape`, the close button and a click outside do. `?` as a
binding is a symbol, so Shift is ignored and it matches on every layout that types a `?`.

The Keyboard settings page gains one line under its table: "Press {{key}} anywhere to see these shortcuts.", naming
the first binding of `showShortcuts` and left out when it has none.

### D6. Tooltips name the first binding in force

The zoom-out and zoom-in buttons get the shadcn tooltip their neighbours already have, reading "{{name}} ({{key}})"
with the first binding of their action, or the name alone when the action has none. Only the first, because a tooltip
is a glance and the list holds the rest. `aria-keyshortcuts` keeps naming all of them.

## Risks / Trade-offs

- [`←`/`→` taken from a focused control that handles arrows but has none of the listed roles] → D4's
  `[data-arrow-keys]` marker covers in-house controls; the slider, toggle group and divider are covered by role.
- [Actual pixels from a stale fit ratio after a resize] → the pane republishes the ratio on every resolution, which
  runs on every re-measurement; the action reads it at the moment of the key press.
- [A rendition smaller than the original looks soft at actual pixels] → expected: 100% is the photograph's pixels; the
  canvas draws what it was served.
- [Machine-drafted strings in 13 locales] → as for slice 2; a native check before release.
- [macOS's default application menu, which Tauri installs, may take some Command combinations before the page sees
  them] → the defaults use none of its items' keys; a user who records one that never arrives sees it do nothing and
  can remove it. Verify by hand.
- [Validated against macOS WKWebView in reasoning and jsdom only] → Windows WebView2 and Linux WebKitGTK are untested;
  task 5.2 records what was tried by hand. Execution providers are not involved.

## Slices

1. **`add-gui-shortcuts-1-defaults` --- done (implemented on bookmark `add-gui-shortcuts-1-defaults`; its by-hand
   check 5.2 is still open).** Fixed default bindings for zoom in, zoom out, fit and hold-to-show-original; the
   binding format and matcher; the window-level hook and its guard; `aria-keyshortcuts` on the zoom buttons. Depends
   on nothing.
2. **`add-gui-shortcuts-2-settings` --- done (implemented on bookmark `add-gui-shortcuts-2-settings`; its by-hand
   check 5.2 is still open).** A Keyboard page in the settings dialog: each action's bindings, recording, removal,
   conflicts refused with a one-click move, trapping keys refused, Reset to defaults, bindings persisted with a
   repair; i18n in all 14 locales. Depends on slice 1's format and table.
3. **`add-gui-shortcuts-3-discoverable` --- current (this change; implemented on bookmark
   `add-gui-shortcuts-3-discoverable`, its by-hand check 5.2 still open).** Shortcuts named in the zoom buttons'
   tooltips and in a list opened with `?`; actual pixels (`Mod+1`), a `Z` toggle between fitted and actual pixels,
   previous/next image in the drawer's order with the strip following; arrow keys left to controls that use them.
   Depends on slices 1 and 2 (the list and tooltips show the user's bindings, and the new actions appear on the
   Keyboard page).
4. **`add-gui-shortcuts-4-app-actions` --- pending (proposed 2026-10-22).** Fixed application-level keys on every
   platform - Open (`Mod+O`), Settings (`Mod+,`), Close Image (`Mod+W`), Close All Images (`Mod+Shift+W`), Quit
   (Command-Q, Ctrl+Q on Linux, Alt+F4 on Windows) - delivered by a macOS application menu of the app's own (replacing
   Tauri's default, whose Close Window on Command-W quits the app) and by window keys on Windows and Linux; shown
   read-only on the Keyboard page and in the list; on macOS the close button hides the window. Depends on slices 1-3.


## Open Questions

- None blocking. Whether `Mod+1` should also have Topaz's companion `Mod+2` (200%) is left for a user to ask for; the
  Keyboard page can bind nothing that does not exist, so it would be a new action.
