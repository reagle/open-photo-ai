> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

Slice 3 makes the lens blur runnable from the CLI, where the focus point is two fractions typed by hand. Its natural
home is the GUI: look at the photograph, click what should be sharp, open the aperture until the background is soft
enough.

## What Changes

Task estimate: **11**. Slice 4 of the `add-lens-blur` series (see `add-lens-blur-1-depth/design.md` - Slices and D15).

- **Lens Blur in Add enhancement**, last in the list, never suggested by Autopilot (its Autopilot switch in Settings is
  disabled, as the fork's Restore's is); the row reads `Lisbon, f/2.8`.
- **A Lens Blur panel**: the f-number as a slider in third stops, from f/1.0 to **the first stop at or above** the photograph's own equivalent f-number (not the nearest, so a new lens blur never starts already blurring), which is where it starts (no blur added until it moves); a note when the photograph's own is assumed (no EXIF); the model's info box (what Lisbon is, its size, the first-use compile). The thumb has an accessible name and a spoken value ("f/6.3, As taken").
- **Set focus**: a button in the panel arms the preview; the next click on the photograph (not a drag) sets the focus point; a ring marks the point and a status line says what the click does. Escape cancels, unless it was taken by a dialog or menu that is closing.
- **`lens_info(source, crop)`**, a Tauri command answering the photograph's own lens as it is framed (equivalent focal length and f-number, and whether they were read or assumed), from `opai`.
- **A crop reframes the lens blur**: the slider's end moves with the framing, so a lens blur "as taken" stays as taken, a value below the end is kept, and the focus point is carried to the same place on the subject (clamped to the nearest edge where the new frame leaves it out).
- **The frontend `Family` union** gains `lens_blur` and `depth` (slice 3 left it).
- **i18n**: the new keys in all 14 locales (English by hand, the rest machine translated).

Crates touched: `gui` (one command calling `opai::own_lens`; the frontend) and `opai` (that helper, its own commit). The command holds no logic of its own: the resolution is `opai`'s, so the CLI and the GUI agree on the photograph's own aperture, **as the photograph is framed**.

`rust-sak` coverage found: nothing needed.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gui-enhance`: a lens blur in the list reports its f-number; its f-number and focus point can be changed from its
  options and from the preview.

## Impact

- `gui`: new `src/lens.rs` (command) registered in `lib.rs`; frontend `ipc/lens.ts`, `hooks/useLensInfo.ts`, `stores/focusPick.ts`, `lib/fstops.ts`, `lib/reframe.ts` (called from `features/crop/useCropController.ts`), `features/enhancements/LensBlurOptions.tsx` and `FStopSlider.tsx`, `features/preview/FocusPick.tsx` (overlay, ring and hint) used by `PreviewImage.tsx`; `components/ui/slider.tsx` (`thumbValueText`); `lib/enhancements.ts`, `ipc/catalogue.ts`, `ipc/enhance.ts`, `ipc/autopilot.ts`, `hooks/useAutopilot.ts`, `features/enhancements/EnhancementRow.tsx`, `features/settings/EnhancementsPage.tsx`, `test/support.tsx`; 14 locale files.
- `opai`: `opai::own_lens(path, framed) -> OwnLens` (EXIF only; no depth), sharing one helper with `find_chain_depth`, in its own commit beneath the GUI's.
