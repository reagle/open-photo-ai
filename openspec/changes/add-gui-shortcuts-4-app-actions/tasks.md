## 1. Native side (`crates/gui/src`)

- [x] 1.1 Add `src/menu.rs` building the macOS menu of design.md - D3 (predefined items plus `open`, `settings`, `closeImage`, `closeAll` with their accelerators, no Command-W Close Window) and install it in `lib.rs` under `#[cfg(target_os = "macos")]`; add a `MockRuntime` test that the menu holds those ids and accelerators and no item bound to `CmdOrCtrl+W` other than `closeImage`; verify `cargo test -p gui menu` is green
- [x] 1.2 Add `.on_menu_event` emitting `app-menu` with the item id to the main window; add a test that each custom id is forwarded and a predefined one is not; verify `cargo test -p gui` is green
- [x] 1.3 Add the `configure_menu { labels, has_files }` command applying `set_text`/`set_enabled` (D3), a no-op off macOS, registered in `generate_handler!`; add a test for an unknown label key ignored and the two close items disabled with `has_files: false`; verify `cargo test -p gui` is green

## 2. Frontend (`crates/gui/frontend`)

- [x] 2.1 In `lib/shortcuts.ts` add `APP_ACTIONS`, the fixed per-platform `APP_SHORTCUTS` table of D1 and the `"application"` refusal (D4); add `lib/shortcuts.test.ts` cases for each key on both platforms, Linux `Ctrl+Q`, and `repairShortcuts` dropping a stored `Mod+O`; verify `pnpm test lib/shortcuts` is green
- [x] 2.2 Lift the Settings dialog's open state into `stores/settingsDialog.ts`, keeping `useSettingsDialog`'s draft semantics; verify the existing `SettingsDialog` and `Navbar` tests pass unchanged and a new store test opens it from outside the navbar
- [x] 2.3 Add `hooks/useAppActions.ts` (D2): `listen("app-menu")` on macOS, a keydown listener over `APP_SHORTCUTS` on Windows/Linux, the overlay guard, and the five effects; add `hooks/useAppActions.test.tsx` cases for each action by each input, nothing open, an open dialog, and no keydown handling on macOS; verify `pnpm test hooks/useAppActions` is green
- [x] 2.4 Mount it in `App.tsx`, call `configure_menu` on mount, on language change and when the open-image count crosses zero, and add `"menu"` as a `files_added` source; add an `App.test.tsx` case that the command is called with translated labels; verify `pnpm test App` is green
- [x] 2.5 Show the fixed "Application" group read-only on `features/settings/KeyboardPage.tsx` and in `features/shortcuts/ShortcutsDialog.tsx`, and the refusal message for `"application"`; add tests on both platforms; verify `pnpm test features/settings features/shortcuts` is green
- [x] 2.6 Add the i18n keys (menu labels, five action names, group title, refusal message) to all 14 locale files under `crates/gui/frontend/i18n/locales/`; verify `pnpm test i18n` (parity) is green
- [x] 2.7 Add Export... (D1): an `export` item (`CmdOrCtrl+S`) in File between Open and the close items, `configure_menu`'s `can_export` enabling it, `stores/exportDialog.ts` lifted from the sidebar, `APP_SHORTCUTS` holding each action's keys (Export's `Mod+S`, `Mod+Shift+E`, the page matching ⇧⌘E on macOS), the refusals, both keys on the Keyboard page and list, and two i18n keys in all 14 locales; add `Mod+/` as a default of `showShortcuts`; add Rust and frontend tests for each; verify `cargo test -p gui menu` and `pnpm test` are green

## 3. Verification

- [x] 3.1 Run `cargo fmt --check`, `cargo clippy -p gui --all-targets -- -D warnings` and `cargo test -p gui`; verify all are green
- [x] 3.2 In `crates/gui` run `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`; verify all are green
- [ ] 3.3 Verify by hand on macOS: Command-S and Shift-Command-E open Export with an image picked, File > Export greyed with none, neither in a text field; Command-slash and ? open the shortcuts list; Command-W closes one image and the app stays open; Shift-Command-W closes all; Command-O opens the picker; Command-comma opens Settings; Command-Q quits; Command-W with the export dialog open closes nothing; the close button quits; Command-C/V work in the export name field; menu labels in a second language; record which of Windows (Ctrl+O/W/Shift+W/comma, Alt+F4) and Linux (plus Ctrl+Q) were tried — Export keys and ⌘/ confirmed by hand by Joseph on macOS, 2026-10-23; the app-menu checks below are still open.
- [x] 3.4 Add the by-hand cases to `~/dev/opai-notes/TESTING.md`; verify the checklist lists every row of D1's table

## Implementation notes and deviations

Implemented on bookmark `add-gui-shortcuts-4-app-actions` (two commits on `add-gui-shortcuts-3-discoverable`: the menu
and keys, then Export and Command-slash).

- D5 settled by Joseph on 2026-10-23: closing the window quits on every platform. The hide-on-close task (old 1.4) was
  dropped, leaving 13 tasks; design.md, proposal.md and the spec were changed to match.
- 1.1/1.3: muda builds native menu items on the main thread only and panics in a test thread on macOS, so the
  `MockRuntime` tests planned here could not build the menu. Instead the menu is declared as data (`SECTIONS`,
  `ACTIONS` in `src/menu.rs`), and the tests check that table (ids, accelerators, Command-W on Close Image alone, no
  Close Window, the standard menus) and the pure `plan` that `configure_menu` applies (labels by key, unknown keys
  ignored, only the two close items following `has_files`). What AppKit draws is the by-hand check 3.3.
- 1.1: the menu is installed from `setup` with `app.set_menu` under `#[cfg(target_os = "macos")]` rather than with
  `Builder::menu`, so the items can be kept as state for relabelling; `lib.rs` gains five lines (`mod menu`, the
  `setup` call, `.on_menu_event`, the handler entry).
- 1.2: the forwarding test covers `action_for` (each custom id forwarded, generated and other ids not); the emit
  itself is untested for the same main-thread reason.
- 1.3: `configure_menu` is generic over the runtime and is a no-op wherever no menu was installed (Windows, Linux).
  The frontend calls it only on macOS. The menu's predefined items are relabelled too (About, Hide, Quit, the Edit
  items, ...), so 25 `menu.bar.*` keys rather than only the custom items'.
- 2.1: `refusal`'s `"application"` refuses `Mod+Q` off macOS on Windows as well as Linux (the function knows only
  macOS or not, and a binding stored on one PC platform should mean the same on the other); Windows itself still quits
  with Alt+F4 and does not bind Ctrl+Q. On macOS, Command with W or Q stays `"system"` as before.
- 2.2: `openSettings` while the dialog is already open is a no-op, so a stray request never re-seeds (discards) the
  draft. The test setup resets the new store after each test.
- 2.3: the menu-label effect lives in `useAppActions` rather than `App.tsx`, so App only mounts the hook;
  `hooks/useAppShortcuts.ts` holds the platform's fixed table for the Keyboard page and the list.
- 2.7 (added 2026-10-23, at Joseph's request; folded into one task to stay within 15): Export is enabled exactly when
  the sidebar's Export button is, `selectedPaths` non-empty, so the button and the menu read one `canExport`. A macOS
  menu item holds one key equivalent, so `APP_SHORTCUTS` became a list per action with the menu item's key first; on
  macOS the page's keydown listener now runs too but matches only the keys after the first (Export's ⇧⌘E), so no key
  is acted on twice. The listener applies `isWindowShortcut` as for every other application action, so neither key
  acts in a text field or under an open dialog. AppKit sends a menu item's key even from a focused text field, so on
  macOS the Export item alone also waits for the focus to leave one (`isEditing`); the other items keep acting from a
  field, as the first commit left them (the review's open "⌘W in a text field" question). Command-slash is a default
  of the rebindable `showShortcuts` (design.md D1) rather than a fixed application key. The Keyboard page and the list
  show each fixed action's keys as separate key caps. `configure_menu`'s arguments are
  `{ labels, has_files, can_export }`; the Rust `plan` takes an `Availability`. 3.4's checklist in
  `~/dev/opai-notes/TESTING.md` gained an "Export shortcut" section.
- 3.3 stays open for Joseph. Before UI automation was stopped (2026-10-23, no further screen capture or scripting), a
  read of the menu through macOS accessibility showed the bar Open Photo AI / File / Edit / View / Window / Help,
  Settings… ⌘, and Quit ⌘Q in the app menu, Open… ⌘O, Close Image ⌘W and Close All Images ⇧⌘W in File, no Close
  Window, the close items greyed with nothing open; ⌘, opened Settings and ⌘O opened the picker. The bundle at
  `/Users/reagle/dev/opai-ws/appmenu/build/OpenPhotoAI.app` was checked to carry this commit's menu code. Steps for
  Joseph, with no other Open Photo AI running:
  1. Open `/Users/reagle/dev/opai-ws/appmenu/build/OpenPhotoAI.app`. Look at the File menu: Open… ⌘O, Close Image ⌘W
     and Close All Images ⇧⌘W, the two close items greyed out.
  2. Press ⌘O and add three photographs. Press ⌘W: one image closes, two remain, the window stays open.
  3. Press ⇧⌘W: all images close and the empty canvas shows. Press ⌘W again: nothing happens.
  4. Open an image, open the export dialog, press ⌘W: nothing closes. In its file-name field, ⌘A, ⌘C, ⌘V work.
  5. Press ⌘, : Settings opens. Keyboard page: an Application group lists the five keys read-only; Add shortcut on
     Zoom in, then ⌘O: refused with "⌘ O is used by the application." Cancel.
  6. Settings, General, Language French, Save: the menu bar reads Fichier, Édition, ...; switch back to English.
  7. Click the window's red close button: the application quits. Relaunch and press ⌘Q: it quits.
- 3.4 not done here: `~/dev/opai-notes/TESTING.md` is outside this change. Proposed section:

  ### Keyboard shortcuts 4: application actions (GUI, macOS)

  - [ ] The menu bar shows Open Photo AI, File, Edit, View, Window, Help; File has Open… ⌘O, Close Image ⌘W, Close
    All Images ⇧⌘W; the app menu has Settings… ⌘, and Quit ⌘Q.
  - [ ] With no image open, Close Image and Close All Images are greyed out.
  - [ ] `Cmd O` opens the Browse images picker; chosen images are added.
  - [ ] `Cmd ,` opens Settings.
  - [ ] With three images open, `Cmd W` closes the current one; the app stays open with two.
  - [ ] `Shift Cmd W` closes all images; the app stays open on the empty canvas.
  - [ ] `Cmd W` with no image open does nothing, and the window stays open.
  - [ ] With the export dialog open, `Cmd W` closes nothing.
  - [ ] `Cmd C`/`Cmd V` work in the export name field.
  - [ ] Switch Settings, General, Language to French and save: the menu labels are in French.
  - [ ] Settings, Keyboard lists the Application group read-only; recording `Cmd O` for Zoom in is refused.
  - [ ] The window's close button quits; `Cmd Q` quits.
  - [ ] Windows (Ctrl+O, Ctrl+comma, Ctrl+W, Ctrl+Shift+W, Alt+F4) and Linux (plus Ctrl+Q): untested.

