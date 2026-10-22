## Context

Root cause of Command-W quitting, with evidence:

- `crates/gui/src/lib.rs:96-190` builds the app with no `.menu(...)` and no `enable_macos_default_menu(false)`, so
  Tauri installs `Menu::default` on macOS (`tauri-2.11.5/src/app.rs:2245-2249`). That menu's File submenu is
  `PredefinedMenuItem::close_window` with Command-W (`tauri-2.11.5/src/menu/menu.rs:203-213`; also in its Window menu,
  `menu.rs:152-163`).
- Closing the only window ends the event loop: `run`'s callback (`lib.rs:178-189`) handles only `RunEvent::Exit` and
  never calls `ExitRequestApi::prevent_exit`, and there is no `on_window_event` to intercept `CloseRequested`.
- Command-O: the default menu has no Open item, and the frontend's `DEFAULT_SHORTCUTS`
  (`crates/gui/frontend/lib/shortcuts.ts:37-51`) bind nothing to `Mod+O`. **Command-O currently does nothing.** The
  picker is reached only through `useOpenImages` from the drawer's Add images and the empty canvas's Browse images
  (`hooks/useOpenImages.ts`).
- Settings: the dialog's open state is `useState` inside `useSettingsDialog`, owned by `Navbar.tsx:33`; nothing
  outside the navbar can open it.
- The shortcuts stack already refuses Command with Q, W, H or M on macOS and Alt+F4 elsewhere
  (`lib/shortcuts.ts:293-317`, `refusal`), so the window-level system never competes with those keys.
- Closing images is a store action: `closeFile(path)` and `closeAll()` in `stores/files.ts:183-223`, which also make
  every registered owner forget the file. The new actions call these and nothing else.

## Goals / Non-Goals

**Goals:** standard macOS menu and key behaviour; the same actions on Windows and Linux by keyboard; one dispatch path
for each action whatever delivered it; the Keyboard page tells the truth about which keys are taken.

**Non-Goals:** rebinding application keys (a later change could use `MenuItem::set_accelerator`); a menu bar on
Windows or Linux (the design has none, and a native one would sit under the custom navbar); undo/redo of a close.

## How the work divides

All in `gui`. Rust owns the native menu and window lifecycle; the frontend owns what each action does, because every
action is frontend state (the file store, the picker hook, the settings dialog). Rust forwards a menu click as an
event naming the action; it never closes an image itself.

## `rust-sak` coverage

Nothing: `rust-sak` has no UI module, and a menu is not shared infrastructure.

## Decisions

### D1. Application-level actions, per platform

| Action | macOS | Windows | Linux | Delivered by | Keyboard page | Tested off macOS |
| --- | --- | --- | --- | --- | --- | --- |
| Open... (Browse images) | ⌘O | Ctrl+O | Ctrl+O | macOS: native menu item; Win/Linux: window keydown | Fixed (read-only row) | No |
| Export... (the Export dialog) | ⌘S, ⇧⌘E | Ctrl+S, Ctrl+Shift+E | Ctrl+S, Ctrl+Shift+E | macOS: native menu item (⌘S) and window keydown (⇧⌘E, which the item cannot also hold); Win/Linux: window keydown | Fixed | No |
| Settings... | ⌘, | Ctrl+, | Ctrl+, | macOS: app-menu item; Win/Linux: window keydown | Fixed | No |
| Close Image | ⌘W | Ctrl+W | Ctrl+W | macOS: native menu item; Win/Linux: window keydown | Fixed | No |
| Close All Images | ⇧⌘W | Ctrl+Shift+W | Ctrl+Shift+W | macOS: native menu item; Win/Linux: window keydown | Fixed | No |
| Quit | ⌘Q | Alt+F4 | Ctrl+Q | macOS: predefined Quit; Windows: the system closes the window; Linux: window keydown -> `quit` command | Fixed | No |
| Hide / Hide Others / Minimize | ⌘H / ⌥⌘H / ⌘M | - | - | macOS predefined items only | Not listed (system) | n/a |
| Enter Full Screen | ⌃⌘F | - | - | macOS predefined item | Not listed (system) | n/a |
| Cut / Copy / Paste / Select All / Undo / Redo | ⌘X/C/V/A/Z/⇧⌘Z | Ctrl+... | Ctrl+... | macOS predefined Edit items (text fields need them); Win/Linux: the webview natively | Not listed (system) | n/a |
| Zoom in / Zoom out / Fit / Actual pixels | ⌘= / ⌘- / ⌘0 / ⌘1 | Ctrl+= / Ctrl+- / Ctrl+0 / Ctrl+1 | same as Windows | Window keydown (shortcuts stack, `Mod`) on every platform | Rebindable | No (stack's own 5.2) |
| Toggle zoom / Show original / Previous / Next / Shortcuts list | Z / \ / ← / → / ?, ⌘/ | same, Ctrl+/ for the list | same as Windows | Window keydown on every platform | Rebindable | No |

The application keys are not added to the menu's View menu for zoom/fit: a native accelerator would fire before the
page and break press-and-hold and rebinding (the shortcuts stack's design, slice 1, Paths considered 2). Those stay in
the window's own system.

Delivery on macOS: menu key equivalents are matched by AppKit before the page's `keydown` is relied on, so the menu is
the one route for its keys and the frontend does not also bind them there (no double close). A menu item holds one key
equivalent, so an action's further keys - Export's ⇧⌘E - are the page's keydown on macOS too: `APP_SHORTCUTS` lists
each action's keys with the menu item's first, and on macOS the page matches only the rest. On Windows and Linux the
frontend's keydown handler is the one route. `isWindowShortcut`'s guard applies on every platform: with a dialog or
menu open the actions do nothing - except that a menu click on macOS while a dialog is open is also ignored by the
same overlay check in the dispatcher, so Command-W never closes an image under an open export dialog.

Export follows Topaz Photo AI (⌘S, its save/export key) and Lightroom Classic (⇧⌘E). Its item is available exactly
when the sidebar's Export button is: while at least one image is picked (`selectedPaths` non-empty), which implies one
is open. Command-slash for the shortcuts list follows Topaz (its shortcuts panel). It is a second default binding of
slice 3's rebindable `showShortcuts` rather than a fixed application key: the action is already the user's to rebind,
and a fixed second key would split one action across the read-only and rebindable groups and leave a key the user
could not remove.

### D2. One dispatcher, two inputs

`hooks/useAppActions.ts`, mounted once in `App.tsx`, maps an action id to its effect: `open` -> the
`useOpenImages("menu")` callback; `settings` -> a new `stores/settingsDialog.ts` (`open` lifted out of
`useSettingsDialog`, which keeps its draft logic and reads `open` from the store); `closeImage` ->
`closeFile(currentPath)`, a no-op with nothing open; `closeAll` -> `closeAll()`; `export` -> a new
`stores/exportDialog.ts` (the sidebar's `exporting` state lifted out of `Sidebar.tsx`, opened only while something is
picked); `quit` -> the existing `quit` command (`setup::quit`, `app.exit(0)`). Inputs: `listen("app-menu", ...)`
(macOS) and a `keydown` listener using a fixed `APP_SHORTCUTS` table in `lib/shortcuts.ts` (Windows/Linux).
`files_added`'s `source` gains `"menu"`.

Alternative: Rust performs the actions. Rejected - the file list, selection and owners are frontend state; Rust would
have to call back into the page anyway.

### D3. The native menu (`src/menu.rs`)

Built with `Menu::with_items` and `Submenu`s, `PredefinedMenuItem` for About, Services, Hide, Hide Others, Show All,
Quit, the Edit items, Fullscreen, Minimize, Maximize (Zoom) and Bring All to Front, and `MenuItem::with_id` for
`open`, `export`, `settings`, `closeImage`, `closeAll` with accelerators `CmdOrCtrl+O`, `CmdOrCtrl+S`, `CmdOrCtrl+,`,
`CmdOrCtrl+W`, `CmdOrCtrl+Shift+W`; File holds Open..., a separator, Export..., a separator, then the close items.
Installed with `.menu(...)` on macOS only (`#[cfg(target_os = "macos")]`); on Windows and Linux
`.enable_macos_default_menu` is irrelevant and no menu is set, so no menu bar appears. `.on_menu_event` emits
`app-menu` with the id to the main window. There is deliberately no Close Window item with Command-W; the window can
still be closed by its button.

Labels: Rust builds the menu with English labels, and the frontend calls a new `configure_menu` command with the
translated labels at start and on a language change, plus whether any image is open and whether Export is available;
Rust applies them with `set_text` and `set_enabled` (Close Image and Close All disabled with nothing open, Export with
nothing picked). This keeps the i18n catalogue the frontend's, as `open_images` already does with the picker's
wording.

### D4. Fixed keys are refused and listed

`refusal` gains a third kind, `"application"`, for `Mod+O`, `Mod+S`, `Mod+Shift+E`, `Mod+,`, `Mod+W`, `Mod+Shift+W` on
every platform and `Ctrl+Q` on Linux (Command with Q/W/H/M stays `"system"` on macOS). The Keyboard page shows an
"Application" group of read-only rows from `APP_SHORTCUTS` above the rebindable ones, each with all its keys (Export's
two), and the shortcuts list (slice 3) shows the same group. `repairShortcuts` drops a stored binding refused on both
platforms, as today.

### D5. Closing the window quits the application, on every platform (Joseph's decision)

Unchanged from today: there is no `on_window_event` interception and no `RunEvent::Reopen` handler, so the window's
close button (and Alt+F4 on Windows) ends the event loop through `RunEvent::Exit`, with its cache settle and telemetry
stop. What changes on macOS is only that Command-W no longer reaches the window: no menu item closes it with that key.

Alternative considered: hide the window on macOS and show it again from the Dock (the convention of Preview and
Photos, and it keeps models warm for a later Open With). Rejected by Joseph on 2026-10-23: one predictable rule on
every platform, and no hidden window holding memory. The task it needed was dropped, leaving 13.

## Risks / Trade-offs

- [WebView2 or WebKitGTK consume Ctrl+W/Ctrl+O/Ctrl+, before the page] -> no evidence they do (neither has tabs or an
  open dialog bound), but untested; task 3.3 records it.
- [A menu accelerator and a page `keydown` both firing on macOS] -> on macOS the frontend matches only the keys no
  menu item holds (Export's ⇧⌘E); the menu items' keys only when `isMacOs()` is false.
- [Labels before the frontend loads are English] -> the window is hidden until painted, and `configure_menu` runs from
  the first mount.
- [Validated against] macOS by reasoning and Tauri's source; Windows and Linux untested.

## Difficulty and likely paths

Moderate (one to two days). The menu itself is routine Tauri 2; the work is the dispatcher, lifting the settings
dialog's open state into a store without breaking its draft semantics, the read-only rows on the Keyboard page, and 14
locales. Path: D1-D5 as written.

## Slices

1. **`add-gui-shortcuts-1-defaults` --- done (implemented on bookmark `add-gui-shortcuts-1-defaults`; its by-hand
   check 5.2 is still open).** Fixed default bindings for zoom in, zoom out, fit and hold-to-show-original; the
   binding format and matcher; the window-level hook and its guard; `aria-keyshortcuts` on the zoom buttons. Depends
   on nothing.
2. **`add-gui-shortcuts-2-settings` --- done (implemented on bookmark `add-gui-shortcuts-2-settings`; its by-hand
   check 5.2 is still open).** A Keyboard page in the settings dialog: each action's bindings, recording, removal,
   conflicts refused with a one-click move, trapping keys refused, Reset to defaults, bindings persisted with a
   repair; i18n in all 14 locales. Depends on slice 1's format and table.
3. **`add-gui-shortcuts-3-discoverable` --- done (implemented on bookmark `add-gui-shortcuts-3-discoverable`; its
   by-hand check 5.2 is still open).** Shortcuts named in the zoom buttons' tooltips and in a list opened with `?`;
   actual pixels, a `Z` toggle, previous/next image; arrow keys left to controls that use them. Depends on slices 1
   and 2.
4. **`add-gui-shortcuts-4-app-actions` --- current (this change; implemented, by-hand check open).** Fixed
   application-level keys on every platform (Open, Export, Settings, Close Image, Close All Images, Quit) delivered by
   a macOS application menu of the app's own and by window keys on Windows and Linux; shown read-only on the Keyboard
   page and in the list; Command-slash as a second key for the list; closing the window still quits. Depends on slices
   1-3 (refusal rules, Keyboard page, list). 14 tasks: Export and Command-slash were added as one task (2.7) after the
   first 13 were done.
