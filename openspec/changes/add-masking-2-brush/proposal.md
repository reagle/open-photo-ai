> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

Slice 1 makes a masked run possible and gives the CLI a mask file. In the GUI a mask is painted where you look: over the
preview, with the result updating as you go.

## What Changes

Task estimate: **14**. Slice 2 of the `add-masking` series (see `add-masking-1-core/design.md` - Slices).

- **A Mask section in the sidebar**, under the enhancement list: "No mask" or "Mask" with Edit and Clear, and while
  editing a panel with the tools: Paint and Erase, Size, Softness, Invert, Undo and Redo, Clear, Show overlay, Done.
- **A brush on the preview** while the panel is open: a drag paints (or erases), a ring under the pointer shows the
  brush's size and hard core, Space held pans, the wheel still zooms. Keys while editing: `B` paint, `E` erase, `X`
  swap, `[` and `]` size, `Shift+[` and `Shift+]` softness, `Cmd/Ctrl+Z` undo, `Shift+Cmd/Ctrl+Z` redo, `O` overlay,
  Escape or Done leaves.
- **A tinted overlay** of the painted area (the exact weight the run will use, rasterised by `opai`), with the stroke
  being drawn shown live on top.
- **The mask goes with every run**: the preview re-runs when a stroke ends (models served from the cache; only the blend
  is redone), and export uses each photograph's mask.
- **One mask per photograph**, kept while it is open, with up to 100 steps of undo. Changing the crop clears the mask
  (it is tied to the framing until slice 7); the crop panel says so first.
- **i18n**: the new keys in all 14 locales.

Crates touched: `gui` (two commands gain an argument, one command added, the frontend). The command that draws the
overlay calls `opai::mask`; the GUI holds the strokes and the undo history, which are editing state, not logic either
front end shares.

`rust-sak` coverage found: nothing needed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gui-enhance`: a photograph can carry a mask, painted over the preview, that limits its enhancements in the preview
  and the export.

## Impact

- `gui` Rust: `src/enhance/{mod.rs,run.rs}` and `src/export/{mod.rs,run.rs}` take `mask: Option<opai::Mask>` and set
  `ProcessOptions::mask`; new `src/mask.rs` (`mask_preview` command) registered in `lib.rs`.
- Frontend: new `stores/mask.ts`, `lib/mask.ts`, `ipc/mask.ts`,
  `features/mask/{MaskSection,MaskPanel,MaskOverlay,BrushCursor}.tsx`; `ipc/enhance.ts` and `ipc/export.ts` (the
  argument), `hooks/useEnhancementRun.ts` (the mask in the run's inputs), `features/export/batch.ts` (each photograph's
  mask), `features/preview/PreviewImage.tsx` (paint mode beside the focus pick's armed mode),
  `features/sidebar/Sidebar.tsx`, the crop panel's notice, `test/support.tsx`; 14 locale files.
