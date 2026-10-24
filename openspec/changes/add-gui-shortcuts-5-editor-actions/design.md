## Context

Every action here already exists as a control; the keys only reach it.

- Autopilot is the sidebar's `Switch id="autopilot"` writing `useEnhancementStore.toggle`
  (`features/sidebar/Sidebar.tsx`).
- Add enhancement is a Radix `DropdownMenu` whose open state was `useState` in
  `features/enhancements/AddEnhancement.tsx`, disabled with no image open.
- Each enhancement row is a Radix `Popover` trigger (its options panel, open state `useState` in
  `EnhancementRow.tsx`) with a remove button over its trailing edge; removal is `removeEnhancement` plus an
  `enhancement_removed` event in `EnhancementList.tsx`.
- Crop/Rotate opens from the miniature's button (`SidebarMiniature.tsx`, `useState`), drawn only for an image with
  pixels.
- The comparison is `usePreviewStore.previewMode`, chosen by the drawer header's toggle group; the fold toggle writes
  `useDrawerStore.toggle`; both are disabled with nothing open (`DrawerHeader.tsx`).
- Reveal is the file options menu's `revealImage` with an error toast (`features/files/FileOptionsMenu.tsx`).
- Panning is a pointer drag in `features/preview/PreviewImage.tsx`, moving from the *drawn* position and constraining
  it to the pane; the stored position is not the drawn one after an anchored zoom.
- The window's keys are one listener (`hooks/useShortcuts.ts`) over `SHORTCUT_ACTIONS`/`DEFAULT_SHORTCUTS`
  (`lib/shortcuts.ts`), guarded by `isWindowShortcut`; the Keyboard page and the `?` list are generated from those
  tables.

## Goals / Non-Goals

**Goals:** a key for each editor control, behaving exactly as the control; nothing new to learn about rebinding,
guarding or listing; no key taken from a control that needs it.

**Non-Goals:** keys inside dialogs (Crop/Rotate's rotate and flip, Enter to apply, Enter to export) - a possible
slice 6; reordering enhancements from the keyboard; a key per comparison mode (one cycling key instead).

## How the work divides

All in `gui`'s frontend. Every action is frontend state (stores and Radix components); nothing is shared with the CLI,
and no Rust changes: none of the keys is a macOS menu item.

## `rust-sak` coverage

Nothing: `rust-sak` has no UI module.

## Decisions

### D1. The bindings, and what was checked against them

| Action (`SHORTCUT_ACTIONS`) | Default | Note |
|---|---|---|
| `toggleAutopilot` | `a` | |
| `addEnhancement` | `e` | |
| `removeEnhancement` | `Delete`, `Backspace` | a Mac's Delete key sends `Backspace`, its Forward Delete `Delete` |
| `openOptions1`-`openOptions7` | `1`-`7` | seven: one per enhancement family; bare digits, since `Mod+1` is actual pixels |
| `cropRotate` | `c` | |
| `cyclePreview` | `v` | |
| `toggleDrawer` | `Mod+\` | not the bare `\`, which shows the original |
| `revealImage` | `Mod+Shift+r` | Shift because `Mod R` alone reloads a web page |
| `panLeft`, `panRight`, `panUp`, `panDown` | `Mod+ArrowLeft` and so on | with Mod, since the bare arrows step through the images |

Checked against, with no conflict found, so the table is the one agreed with Joseph unchanged:

- **Existing bindings.** Bare digits do not match `Mod+1` (actual pixels): the matcher requires every unnamed
  modifier up, and Shift with a digit types a symbol. `Mod+\` does not match the bare `\` (show original) and vice
  versa. `Mod+arrow` does not match the bare arrows (previous/next image). A unit test asserts that no two defaults
  overlap on either platform and that resolving the defaults drops none.
- **Refused keys.** None of them is `navigation`, `system` (Command with Q/W/H/M; Alt+F4) or `application` (Open,
  Export, Settings, Close Image, Close All Images, Quit).
- **macOS menu accelerators** (`crates/gui/src/menu.rs`): the custom items hold Command with O, S, comma, W and Shift
  W; the predefined Edit items hold Command with Z, Shift Z, X, C, V and A, View holds Control-Command-F, Window
  Command-M, the app menu Command-H, Option-Command-H and Command-Q. The new keys are bare letters, digits and Delete,
  or Command with `\`, Shift R and the arrows: none of them.
- **Radix and the system.** Radix's menu typeahead and its arrow handling act only inside an open menu, which the
  guard already treats as an overlay; sliders, radio groups and the split divider keep their arrows
  (`ARROWS_OPERATE`). Command with an arrow moves the caret in a macOS text field, which the guard leaves to the
  field. Control with an arrow (Mission Control) and Windows' Win with an arrow are not `Mod`.

Known limits, not conflicts: `\` needs Option on a German Mac, and the matcher does not ignore Option on a binding with
Command, so `Mod+\` cannot be pressed there (rebind it); and WebView2 treats Ctrl+Shift+R as a hard reload - whether
preventing the page's `keydown` stops it is untested (Windows is untested throughout).

### D2. One table of what each action does, and "nothing to do" leaves the key alone

The actions live in `lib/editorActions.ts`, outside components, reading the stores when called (the shape of
`stepZoom`). Each answers `false` when it did nothing, and the listener then does not prevent the key's default: a
`Backspace` away from the enhancement list, a digit past its end, a pan of a fitted photograph or `E` with no image is
left alone. Toggles, Delete and the digits are ignored on key repeat (a held Delete would otherwise empty the list row
by row); the pans repeat like the zoom steps.

The open state of the Add enhancement menu, of an enhancement's options panel and of Crop/Rotate moves into three
small stores (`stores/addEnhancementMenu.ts`, `stores/optionsPanel.ts`, `stores/cropDialog.ts`), as slice 4 did for the
export and settings dialogs. The options panel store holds one family; closing is conditional on the family so a
late close from the panel a second one replaced cannot undo the open. Removing an enhancement
(`removeEnhancementOf`) and revealing a file (`revealInFileManager`) are shared by the control and the key.

### D3. Delete removes the row with the focus, and the focus moves on

Each row carries `data-family`; Delete and Backspace act only when the focus is inside a row (on its trigger or its
remove button). Before removing, the focus moves to the next row's trigger, else the previous one's, else the Add
enhancement button, so a second press removes the next enhancement rather than nothing and the focus never drops to
the page.

### D4. A digit opens the options of the enhancement in that place

`openOptions1`-`7` open the panel of the Nth enhancement in the order the sidebar draws them (the store's apply order,
filtered to the families it draws). Radix moves the focus into the panel and, on Escape, back to the row's trigger, so
"focus the row and open its panel" needs no focus code of its own. Nothing happens past the end of the list, with no
image, or while Autopilot is analysing and the list is replaced by the analysing row.

### D5. Panning from where the photograph is drawn

A pan is a question about the drawn position, which only the pane knows, so the pane that publishes the viewport
registers a panner (`registerPanner` in `stores/transform.ts`, the shape of `registerFileOwner`) and `panCurrent` asks
it. A step is a tenth of the pane (`PAN_KEY_STEP`), so ten presses cross it in any window size; the view moves the way
the arrow points (the photograph moves the other way), and the position is constrained exactly as a drag's. A press
that lands before the previous one has been drawn continues from the position the previous one stored. Fitted, the
panner answers `false`.

### D6. `V` cycles the comparison

Slice 3 left keyboard cycling of the comparison out (its Non-Goals). It is added here as one key stepping full, side by
side, split and round again (`nextPreviewMode`), recording `preview_mode_changed` as the toggles do. One key rather
than three: the three toggles are one click away, and three more single keys would crowd the digits and letters.

### D7. Where the keys are named

Generated as before: the Keyboard page and the `?` list draw every entry of `SHORTCUT_ACTIONS`, so the new actions
appear in both without changes there; the list scrolls now that it is longer. As slice 3 did for the zoom buttons, the
icon-only controls - the crop button and the fold toggle - name their first key in a tooltip, and the Autopilot switch,
Add enhancement, the remove buttons and the comparison group carry `aria-keyshortcuts`. macOS shows Delete and
Backspace as `⌦` and `⌫`.

Every other control that has a key reveals it the same way, through one pair of hooks (`useShortcutHint` for the
rebindable actions, `useAppShortcutHint` for the application's fixed ones) and one `HintTooltip`: the Autopilot switch
(on its label, since a tooltip's own `data-state` would overwrite the switch's), Add enhancement, each enhancement row
(its place, `1` to `7`) and its remove button, Export, Settings and Add images. The comparison toggles read "Full (V
cycles)" because `V` steps through them rather than choosing one. Menu items follow the menu convention instead of a
tooltip: the key sits at the item's trailing edge (`DropdownMenuShortcut`), for Close image, Close all images and Show
in Finder. A hint names the first binding the action has now, or nothing when it has none; on macOS a Backspace binding
is shown in preference to Delete, because that is the key a Mac prints Delete on. There is no previous/next image
button and no show-original button, so those keys have nothing to name.

## Risks / Trade-offs

- [Single letters taken from a control that wants them] -> the guard leaves keys to fields, menus and dialogs; no other
  control in the window uses bare letters or digits.
- [Twenty-seven action rows plus six application rows in the `?` list] -> it scrolls; grouping the list is left for a later change if wanted.
- [WebView2 hard reload on Ctrl+Shift+R] -> untested on Windows; rebindable.
- [Validated against] macOS by unit and component tests in jsdom; the hand checks in tasks.md are open. Windows and
  Linux untested.

## Difficulty and likely paths

Moderate (about two hours of agent time). Each action is small; the work is lifting three pieces of component state
into stores without changing their behaviour, the focus handling of Delete, and panning from the drawn position.

## Slices

1. **`add-gui-shortcuts-1-defaults` --- done.** Zoom in/out, fit and hold-to-show-original; the binding format, the
   matcher and the window-level guard. Depends on nothing.
2. **`add-gui-shortcuts-2-settings` --- done.** The Keyboard settings page: recording, removal, conflicts, refused keys,
   reset, persistence with repair. Depends on slice 1.
3. **`add-gui-shortcuts-3-discoverable` --- done.** Tooltips, the `?` list, actual pixels, `Z`, previous/next image,
   arrows left to controls. Depends on slices 1-2.
4. **`add-gui-shortcuts-4-app-actions` --- done (implemented; by-hand checks open).** Fixed application keys (Open,
   Export, Settings, Close Image, Close All Images, Quit), the macOS menu, Command-slash for the list. Depends on slices
   1-3.
5. **`add-gui-shortcuts-5-editor-actions` --- current (this change; implemented, by-hand checks open).** Keys for the
   editor's controls: Autopilot, Add enhancement, remove, open options 1-7, Crop/Rotate, cycle comparison, show/hide
   the drawer, reveal, pan. Depends on slices 1-4.
6. **`add-gui-shortcuts-6-dialog-keys` --- possible, not proposed.** Keys inside dialogs: in Crop/Rotate, `R` to rotate,
   `H`/`V` to flip and Enter to apply; in Export, Enter to export. Needs a dialog-scoped listener, since slice 1's guard
   deliberately turns the window's keys off while a dialog is open.
