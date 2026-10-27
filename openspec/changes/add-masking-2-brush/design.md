## Context

The series' decisions (D1 to D14) are in `add-masking-1-core/design.md`; this file adds the GUI's (G1 to G9). Slice 1
gives `opai::Mask` (strokes in fractions of the framed photograph, an optional base, inversion), `ProcessOptions::mask`,
and a masked run whose models are served from the cache when only the mask changed.

The preview (`features/preview/PreviewImage.tsx`) pans with a primary-button drag and zooms with the wheel around the
pointer; on the fork's lens blur line it also has an armed mode for one click (`stores/focusPick.ts`, `FocusPick.tsx`),
whose positioning in percentages of the drawn photograph box (as `FaceBoxes` does) and whose click-versus-drag test this
slice follows. `hooks/useEnhancementRun.ts` re-runs `enhance` when the image, the stack, the processor, the crop or the
face choice changes. Per-photograph state is in memory, keyed by path, released through `registerFileOwner` when a file
closes.

## Goals / Non-Goals

**Goals:** paint and erase a mask over the preview at any zoom, see exactly what the run will use, undo freely, and have
the preview and the export agree.

**Non-Goals:** click to select (slice 4), refine edges (slice 5), per-enhancement masks (slice 6), a mask that survives
a crop change or a relaunch (slice 7), pressure sensitivity, a live result while the pointer is still down.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-masking-1-core` | `opai::mask`, the masked run, `imaging::{plane, stroke}`, `mix::masked`, CLI `--mask` | - | `main@upstream` | proposed |
| 2 | `add-masking-2-brush` | GUI brush, panel, overlay, mask in preview and export, i18n | 1 | 1 | **this change** |
| 3 | `add-masking-3-lens-blur` | The mask as Lens Blur's "keep sharp"; blend before a trailing lens blur; `--mask-use` and two switches | 2, `add-lens-blur-4-gui` | merge of 2 and `add-lens-blur-4-gui` | proposed |
| 4 | `add-masking-4-select` | Click the subject (Los Angeles) | 2 | 2 | pending, not proposed |
| 5 | `add-masking-5-refine` | Refine edges with the Matting family (ViTMatte-S) | 4, `add-lens-blur-8-matting` | merge | pending, not proposed |
| 6 | `add-masking-6-scope` | Per-enhancement scope | 2 | 2 | pending, not proposed |
| 7 | `add-masking-7-keep` | Mask in photograph coordinates, saved with the session | 2 | 2 | pending, not proposed |

## How the work divides

- `opai` (slice 1): the mask type, rasterising, the masked run. Nothing new here.
- `gui` Rust: the mask argument on `enhance` and `export` (deserialised straight into `opai::Mask`), and `mask_preview`,
  which calls `Mask::rasterised` and encodes a grey PNG.
- Frontend: strokes, undo, tools, pointer handling, drawing the live stroke and the cursor. No rasterising of its own
  for the result: the overlay shows `opai`'s raster, so the user never sees one mask and gets another.

## Decisions

### G1. Where the mask lives in the window

A **Mask** section under the enhancement list in the sidebar (`features/mask/MaskSection.tsx`), shown when a photograph
is open and the stack has at least one enhancement. It reads "No mask" with a **Paint mask** button, or "Mask" with
**Edit**, **Invert** and **Clear**. Edit (or Paint mask) opens the panel (`MaskPanel.tsx`) in place of the section's
buttons, not a popover: the user's attention is on the preview while painting, and a popover would close on the first
click in the canvas (Radix outside-click).

The panel: Paint / Erase (a toggle group), Size (slider, 0.2 to 25 percent of the longer side, logarithmic), Softness (0
to 100 percent), Invert, Undo, Redo, Clear, Show overlay (a switch, on), and **Done**. A one-line hint under the tools:
"Drag to paint. Hold Space to move the photograph."

### G2. Painting on the preview

A store `stores/mask.ts` holds, per path: `strokes`, `inverted`, the undo and redo stacks (snapshots of `{strokes,
inverted}`, at most 100), and the tool settings (mode, size, softness, overlay), which are global rather than per
photograph. `editing: { path } | undefined` says the preview is in paint mode for that photograph, as `focusPick`'s
`armed` does; it is left when the photograph changes, the panel closes, the stack becomes empty, or Escape is pressed.

In paint mode, `PreviewImage.tsx`'s pointer handlers branch before the pan: a primary-button press (without Space held)
starts a stroke at the pointer's position as fractions of the framed photograph (`imageFraction` against the drawn box,
the same arithmetic the focus pick uses), pointer moves append points, release ends the stroke. Option/Alt held while
pressing erases for that stroke. A press outside the photograph's box starts nothing. Space held turns the drag back
into a pan and shows the grab cursor; the wheel zooms as always. Pointer capture keeps a stroke going past the pane's
edge; points are clamped to the photograph. In the split and side-by-side views both panes paint the same mask.

A stroke's points are decimated as they arrive: a point closer than `0.1 r` (in photograph pixels) to the last kept one
is dropped, which keeps a long stroke to a few hundred points. A stroke records the mode, size and softness it was
started with.

### G3. What the user sees

- **The overlay** (`MaskOverlay.tsx`): a translucent tint (the theme's `primary` at 40 percent) over the photograph's
  drawn box, shaped by a CSS `mask-image` of the PNG `mask_preview` returns for the current mask at the drawn box's size
  in device pixels (capped at 2048 on the longer side). Requested after each stroke ends and after Undo, Redo, Invert
  and Clear, at most one in flight (a newer request supersedes an older). While painting, the stroke in progress is
  drawn on top in a `<canvas>` of the same box with the same tint, as a polyline of round caps at the brush's radius
  (hard edge; close enough for the second it shows). Shown while editing; outside paint mode a faint outline is not
  attempted: the section says "Mask" and the result shows its effect.
- **The cursor** (`BrushCursor.tsx`): two circles under the pointer at the brush's radius and its hard core, in screen
  pixels at the current zoom, white with a dark outline, and a minus sign while erasing; the system cursor is hidden
  over the photograph while painting.
- **The result** re-runs when a stroke ends (G5). Show original, split and side by side work as before; the overlay is
  drawn over the enhanced pane only.

### G4. The commands

- `enhance(run, source, operations, processor, crop, mask: Option<Mask>)` and `export(..., mask: Option<Mask>)`:
  deserialised into `opai::Mask` (slice 1's serde, camelCase), placed in `ProcessOptions::mask`. An empty mask is sent
  as absent.
- `mask_preview(mask: Mask, width: u32, height: u32) -> tauri::ipc::Response` (the PNG's bytes, as the image commands
  answer binary): `Mask::rasterised(width, height)`, quantised to 8 bits, PNG-encoded with the `image` crate the GUI
  already uses. A few milliseconds at 2 MP; no state.

### G5. When the preview re-runs

`useEnhancementRun` takes the photograph's mask (the store's strokes and `inverted`) as one more input and compares it
by a cheap version counter the store bumps on each change, not by deep equality. A change while the pointer is down does
not count (strokes are committed on release). A run already in flight is cancelled as for any other input change.
Because the models are served from the run cache, a re-run after a stroke costs reading the cached result and the blend:
under a second on a 24 MP photograph on the fork's test machine (measured in task 3.2).

### G6. Export

`features/export/batch.ts` adds each photograph's mask to its export request, as it adds its crop and its operations.
The export dialog does not mention the mask (it is part of the photograph's settings, as the crop is).

### G7. Undo, Clear, Invert and the crop

- Undo and Redo restore snapshots; a new stroke after an Undo clears Redo. Clear and Invert are undoable steps. Closing
  the photograph forgets its mask and history (`registerFileOwner`).
- **A crop change clears the mask** (strokes are fractions of the framing, D2). The crop panel shows "Changing the crop
  clears the mask" while a mask exists; when a crop change clears one, a toast says so with an Undo that restores the
  previous crop and the mask together (the mask's own Undo cannot bring the strokes back alone, since they belong to the
  old framing). Slice 7 removes this.

### G8. Keys

While editing, and only when no text field, dialog or menu has focus (the rule `PreviewImage.tsx` already applies to
Escape with `OPEN_LAYER`): `B`, `E`, `X`, `[`, `]`, `Shift+[`, `Shift+]`, `Cmd/Ctrl+Z`, `Shift+Cmd/Ctrl+Z`, `O`,
`Escape`, `Space` (held). Registered locally by the panel, not through the fork's keyboard-shortcut settings
(`add-gui-shortcuts-*`), which are not on `main@upstream`; when both are merged, the shortcut series can adopt them
(noted for `integration`: `Cmd+Z` must not reach any other handler while editing).

### G9. i18n

Keys under `mask.*`: `section.none`, `section.some`, `section.paint`, `section.edit`, `section.invert`, `section.clear`,
`panel.paint`, `panel.erase`, `panel.size`, `panel.softness`, `panel.invert`, `panel.undo`, `panel.redo`, `panel.clear`,
`panel.overlay`, `panel.done`, `panel.hint`, `crop.notice`, `toast.cleared`, `toast.undo`, and the keys' tooltip
suffixes. English first; the other 13 locales translated (the existing parity test must pass). "Mask" uses each
language's photo-editing term (de "Maske", fr "masque", ja "マスク").

## Risks / Trade-offs

- **Pointer handling** shares `PreviewImage.tsx` with pan, zoom, the split divider and the focus pick; paint mode must
  take precedence over pan only for an unmodified primary press inside the photograph, and the focus pick and paint mode
  must never be active together (arming one leaves the other).
- **Overlay latency**: one IPC round trip per stroke for the tint; the live canvas covers the gap. If PNG encoding at
  2048 px is slow on Windows, lower the cap.
- **Clearing on crop change** loses work; the notice and Undo soften it until slice 7.
- **Large strokes counts**: a mask of hundreds of strokes is still tens of kilobytes per request; rasterising is per
  segment box, so it stays fast.

## Platforms

Validated on macOS only (trackpad and mouse). Windows and Linux untested; Space-to-pan and Option/Alt-to-erase follow
common editors on all three.

## Open Questions

1. **Tint colour** (default: theme primary at 40 percent). Photoshop's quick mask is red; red fights with skin tones in
   this app's dark theme.
2. **Where the section sits** (default: under the enhancement list). Alternative: a mask icon in the preview's toolbar.
3. **Should a mask exist without enhancements?** (default: no: the section hides with an empty stack; the strokes are
   kept if the user adds an enhancement again).
