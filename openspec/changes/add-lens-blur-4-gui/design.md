## Context

The series' decisions are in `add-lens-blur-1-depth/design.md`; this slice implements D15 on slice 3. The GUI sends an
operation as `{ family, codename, precision, parameters }` and `enhance/operation.rs` builds it through the catalogue,
so the Rust side needs no change for the operation. Option panels are Radix popovers anchored to their row
(`OptionsPanel.tsx`); sliders commit on release (`IntensityControl.tsx`); the preview maps a pointer to fractions of the
framed photograph for its wheel zoom (`features/preview/geometry.ts::imageFraction`, `PreviewImage.tsx`);
`FaceBoxes.tsx` positions overlays in percentages of the framed photograph.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | **this change** |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |

## How the work divides

`opai` resolves the photograph's own lens (`own_lens`); `gui/src/lens.rs` exposes it as a command; the frontend draws
the panel, the slider and the focus mode, and writes the three parameters onto the operation.

## Decisions

### G1. `lens_info`

```rust
#[tauri::command] async fn lens_info(source: String /* identity */, crop: Option<Crop>, opened: State<..>) -> Result<LensInfo, LensError>
#[serde(rename_all = "camelCase")] struct LensInfo { own_f_number: f32, focal_35mm: Option<f32>, origin: LensOrigin }
```

EXIF only (`opai::own_lens(path, framed)`), as the photograph is framed, so it answers instantly and never runs the depth model. `focal_35mm` is `None` when only the depth model could say; the panel does not show it then. The open picture's path is looked up as `detect_faces` looks it up.

### G2. The slider

Third stops from f/1.0 to f/32, shown from f/1.0 up to **the first stop at or above** the photograph's own equivalent f-number (the right end, "As taken"; see As built for why not the nearest). The control's value is the stop's index; the parameter is the stop's nominal value (f/1.4 is 1.4). A new operation's `f_number` is set to that right-end stop when `lens_info` answers (`startingFNumber`); until then the slider is disabled. Label: `f/{{f}}`, with "full-frame equivalent" under it, and "As taken" at the right end; when `origin` is `assumed`: "This photograph has no lens information; f/8 assumed". Commits on release.

### G3. Set focus

`stores/focusPick.ts` (Zustand): `{ armed: boolean, imageId?: string, arm(id), disarm() }`. The panel's **Set focus**
button closes the popover (`PopoverClose`) and arms. `FocusPick.tsx`, rendered by each `PreviewImage` pane over the
photograph while armed: crosshair cursor, a hint chip, pointer down/up within 4 px = a click: the fractions from
`imageFraction` (the same computation as the wheel anchor), written to the lens blur operation with `withParameter`
(`focus_x`, `focus_y`) of the armed image, then disarm; a drag pans as before; Escape disarms. The ring
(percent-positioned, as `FaceBoxes` is) shows while armed (at the current focus) and for 2 s after a click. Switching
image or closing it disarms. Keyboard: the button is reachable and Escape cancels; choosing a point needs a pointer
(recorded).

### G4. List row, Add enhancement, settings

`ENHANCEMENTS` gains `{ family: "lens_blur", nameKey: "enhancements.lensBlur.name", shortNameKey:
"preview.progress.lensBlur", icon: Aperture, manual: true }`; not suggestible (the Settings Autopilot switch for it is
disabled); row info `enhancements.infoLensBlur`: `"{{name}}, f/{{f}}"`.

### G5. i18n keys

`enhancements.lensBlur.name` ("Lens Blur"), `.fNumber` ("Aperture"), `.equivalent` ("full-frame equivalent"), `.asTaken` ("As taken"), `.assumed` ("This photograph has no lens information; f/{{f}} assumed"), `.setFocus` ("Set focus"), `.focusHint` ("Click the photograph where it should be sharp. Esc to cancel."), `.models.lisbon` (the info text: MoGe-2 ViT-B, depth from one photograph, 324 MB, first use compiles for about a minute on a Mac), `enhancements.infoLensBlur` (`"{{name}}, f/{{f}}"`), `preview.progress.lensBlur` ("Lens Blur"). No `focusRing` key: the ring is hidden from assistive technology (As built). English written by hand; the other 13 locales translated by machine (flagged in the PR, as the shortcut series did).


## Risks / Trade-offs

- Each slider release re-renders at full resolution (0.5 to 1.4 s for a 24 MP photograph once the depth is served, plus the run cache's PNG write) and the first run of a photograph estimates depth (0.6 s, or about 65 s once per orientation for the Core ML compile, which `gui-descriptive-progress` on the fork reports as "Optimizing"). Slice 6 is the fix for the slider.
- The popover closing on Set focus is a deliberate step; the user reopens the row to change the aperture.
- Choosing a point needs a pointer: there is no keyboard way to place the focus (recorded). Escape is the keyboard's one part in it.
- The slider's right end reads up to a third of a stop wider than the photograph's own f-number, which is the price of a new lens blur never starting already blurring.

## As built

What the implementation settled differently from the text above. The change is two commits: the library's `opai::own_lens` (`feat(lens-blur): Tell a front end a photograph's own lens, as framed`), split out so the GUI commit holds only the window, and the GUI (`feat(gui): Add a Lens Blur panel with an f-number slider and click-to-focus`). The English strings are written by hand and the other thirteen locales machine translated.

- **`lens_info(source, crop)`**, not `lens_info(source)`. The command takes the framing the window draws (`Option<Crop>`; only its size matters) and answers `opai::own_lens(path, framed)`, which scales by the framing exactly as a run does (slice 3's design: a crop is a smaller sensor behind the same lens, so its equivalent f-number is the frame's times the ratio of the diagonals), so the slider ends where the run's own aperture is. It is in `opai`, not `image/lens.rs`'s `own_lens`: one helper, shared with `find_chain_depth`, does the EXIF read, the check against the file's frame and the framing, so the f-number a slider ends at is the one a run blurs from. It answers `exif` or `assumed` and never `estimated` (an EXIF whose crop factor only the depth model's focal estimate gives is the assumed f/8 here, which the run's estimate refines). The window asks once per photograph and framing while the photograph is open (`ipc/lens.ts`, a per-file owner forgetting each framing's answer on close). Tests: a fixture with EXIF written by hand (no EXIF-writer dependency), one without, an unknown identity.
- **The slider's right end is the first stop at or above the own f-number**, not the stop nearest it (`lib/fstops.ts`, `stopsUpTo`): the library adds no blur at or above the own f-number and a good deal just below it (a third of a stop under it is already a third of the full disc), so rounding to the nearest stop would start a new lens blur already blurring (f/5.91 ends at f/6.3, not f/5.6). The stop a hundredth short of the own f-number counts as at it (the library rounds the own f-number to a hundredth); a photograph above f/32 ends at f/32. The stops run to f/32 (31 of them). The control's value is the stop's index and what it writes, on release and only when the stop changed, is the stop's nominal number; a value above the end is drawn at "As taken", where it adds the same nothing.
- **A crop reframes the lens blur** (`lib/reframe.ts`, called from the crop dialog's Apply after the framing is written and not waited for). The f-number is an absolute equivalent value, not a place on the slider, so a crop that moves the slider's end would otherwise change what the value means: a lens blur at "As taken" (at or above the old end) stays at "As taken", the new end; a value below the end keeps its stop, held inside the new range, so undoing a crop does not leave a value past the end. The focus is a fraction of the frame, so a frame that changes moves what it points at: `mapFocus` carries it through the file's own pixels (undoing the old flips, turn and cut and applying the new ones; the turned canvas is the bounding box the Crop/Rotate widget measures, rounded outward) and **clamps a point the new frame leaves out to the nearest edge** rather than sending it to the middle; it is rounded to the thousandths the library keeps. A file whose size is unknown (a flip or turn of a file whose header could not be read) leaves the point as it was. The lens is read from the store when the answer arrives, so an edit made meanwhile is not overwritten with an older stack's.
- **Set focus**, as designed, and: the focus ring is positioned in percentages of the photograph's drawn box in each pane, so it keeps one size and lands on the same point at every zoom; it takes no pointer events; a hint chip (`role="status"`, once for the canvas, not once per pane) says what the next click does. Armed is for one photograph (by identity). The mode is left on a pick, by Escape, by switching photograph, by closing it or by removing the lens blur from the list.
- **Escape.** The key listener (on `window`, only while armed) ignores an Escape that something else took: one that is `defaultPrevented`, or while a dialog, alert dialog, menu or listbox that is not closing is open over the photograph, so the press that closes a popover or menu does not also leave the mode; an Escape nothing else wanted leaves it.
- **Accessibility.** The slider's thumb has an accessible name ("Aperture") and a spoken value (`aria-valuetext`: "f/6.3, As taken" at the right end, "f/2.8" elsewhere; `components/ui/slider.tsx` gains `thumbValueText`, next to `thumbLabel`), because a slider over the stops holds an index and "17" says nothing where "f/6.3" does. The visual "As taken" label under the track is `aria-hidden` so it is not read twice. The focus ring is `aria-hidden`; the hint is the status line a screen reader is told. There is no `focusRing` string for that reason. The Set focus button is a real button in the popover (`PopoverClose`), disabled for a photograph with no identity.
- **Nothing is written on open**: the slider writes on release and the button writes nothing, so opening the panel to look never cancels the run in flight. A failed `lens_info` is the assumed lens (f/8, with the note), which is what a photograph the library knows nothing of is.
- **The frontend `Family` union** gains `lens_blur` and `depth` (here, with the panel: slice 3 left it): `depth` is excluded from operations (`ipc/enhance.ts`, beside `detection`) and `lens_blur` from Autopilot's suggestions (`ipc/autopilot.ts`, `hooks/useAutopilot.ts`), and a depth progress report is named as the lens blur. The Autopilot switch for Lens Blur on the Enhancements settings page is off and unavailable, as Restore's is; its model dropdown offers Lisbon alone.
- **Hand checks** are written in `opai-notes/TESTING.md` ("Lens blur (2026-10-27) (GUI)": add and slider start and end on the bonsai and the no-EXIF copy, a crop, Set focus on Nora's eyes, a drag while armed, Escape, the row text, the first-run compile message, an export matching the preview). They are Joseph's, and the results are not in yet.


## Platforms

macOS checked by hand (Joseph). Windows and Linux untested; the frontend tests run everywhere.
