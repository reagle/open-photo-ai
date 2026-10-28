## Context

The series' decisions (D1 to D14) are in `add-remove-1-core/design.md`; this file adds the GUI's (R1 to R9). Slice 1
gives a Remove family (`Family::Remove`, one model, Lima, FP32), a catalogue row whose one parameter is
`{ name: "mask", kind: "mask" }`, `ParameterValues::with_mask`, and a chain that drops a removal with an empty mask
before planning, runs Remove first, and blends a limiting mask in after the removals.

The window already has the masking series' brush (`add-masking-2-brush`): a store of strokes per photograph with undo
and redo (`stores/mask.ts`), the panel and its keys (`features/mask/MaskPanel.tsx`, `useMaskKeys.ts`), the cursor, the
overlay shaped by Rust's raster (`MaskOverlay.tsx`, `mask_preview`), painting in `PreviewImage.tsx`, a debounced re-run
in `useEnhancementRun.ts`, the export batch, and a crop change that clears the mask with an Undo toast. What it paints is
the **limiting** mask: the run's own argument, which keeps the chain's result where it is white.

## Goals / Non-Goals

**Goals:** add Remove from the menu, paint over what is to go and see it gone when the stroke ends; undo, erase and
clear; the export shows what the preview showed; the limiting mask and the removal never mistaken for each other.

**Non-Goals:** Ultra HDR gain maps (slice 3), a refinement pass, a grow control, MI-GAN for small strokes (slice 4); a
removal that survives a crop change or a relaunch (as the limiting mask, until masking slice 7); a second removal in one
chain; Core ML for Lima (D7's open question stays open).

## Decisions

### R1. One store, two regions

`stores/mask.ts` gains a `Target`, `"limit" | "removal"`: a second map, `removals`, beside `masks`, keyed by path in the
same way and holding the same `MaskEntry` (strokes, undo, redo, version). `editing` becomes `{ path, target }` and the
live stroke records the target it began in, so a stroke can never end in the other region. The actions that read a
region take an optional `target` that defaults to `"limit"` (`clear`, `undo`, `redo`, `takeMask`, `restoreMask`,
`startEditing`, `useFileMask`, `useHasMask`); `invert` stays the limiting mask's alone. `forgetTarget(path, target)`
forgets one region; `forget` and `forgetAll` (closing files) forget both.

**Not a second store instance**: the brush's tool settings, Space-to-pan and the live stroke are one for the window, and
the preview, cursor and keys would otherwise have to know which store is "the" one. Defaults keep every existing call
site and test meaning the limiting mask.

### R2. Remove in the list: one model, a brush, no settings

`ENHANCEMENTS` gains Remove first (it runs first, and the menu and the chain keep one order), icon `Bandage` (the
healing brush; `Eraser` is the brush's erase tool), and a flag `painted: true`. A painted enhancement:

- is in neither the settings dialog's table nor what Autopilot is asked to suggest from (`SETTABLE`, which
  `familiesWhere` now reads): one model, so no default to choose, and what is removed is never an analysis's choice;
- is added as Lima with no parameters (`newOperation` as for any family) and **opens its brush at once**: adding it is
  asking to paint, and a Remove with nothing painted does nothing;
- has a row that **toggles the brush rather than opening a popover** (a popover would close on the first press in the
  canvas), drawn pressed while the brush is up, and an info line "Lima, nothing painted" or "Lima, area painted";
- takes its strokes with it when its row's X takes it off the stack (`forgetTarget`): they are its whole setting, as an
  upscale's scale is. The limiting mask keeps its strokes when the stack empties, because they are not an
  enhancement's.

Only one Remove per chain, as for every family: the menu draws a family already in the stack unavailable.

The frontend chooses which operation carries the region by `isPainted(family)`, its own presentation list, as it does
for which panel a row opens; Rust chooses which parameter receives it by kind (R4), so no family name is spelled there.

### R3. The panel, in the mask section's place

`MaskSection` draws the removal's panel in place of the whole section while the removal is edited: one region at a time,
and the limiting mask's buttons come back on Done (or Escape). `MaskPanel` takes `target`; for the removal it shows a
visible title, **"Paint area to remove"**, with a dot of the removal's colour, hides Invert, and its hint says that what
is painted is removed when the button is released. Everything else is the same panel and the same keys: B and E, X,
`[` `]` for size, Shift+`[` `]` for softness, Option/Alt to erase one stroke, Space to pan, O for the overlay,
Cmd/Ctrl+Z and Shift+Cmd/Ctrl+Z for the region's own undo and redo (`useMaskKeys(path, target)`), Escape for Done.

The limiting-mask section is offered only for a stack holding at least one enhancement that is not painted: over a
removal alone there is nothing for it to limit (D9 blends it after the removals, so it never brings back what was
removed). Painting stops when the photograph changes or the stack no longer holds what the region is for.

### R4. On the wire

`Requested` (`crates/gui/src/enhance/operation.rs`) gains `mask: Option<opai::Mask>` (the library's camelCase wire
shape, `{ strokes, inverted }`, no base). `resolve` hands it to `ParameterValues::with_mask` when the catalogue row
publishes a parameter of kind `ParameterKind::Mask`, and refuses a region sent for a row that publishes none as
`UnknownOperation::Parameter { name: "mask" }`, as it refuses a range name nothing publishes: dropping it would run a
chain other than the one painted.

The frontend's `Operation` gains `mask?: Mask`. The stack never holds it; `withRemoval(operations, region)`
(`lib/mask.ts`) puts it on the Remove on the way to `enhance` and `export`, as `withChoice` puts the face choice in.
`regionOf(entry)` is the region as sent: **always a mask, empty when nothing is painted** (slice 1 drops an empty
removal, so no model is opened), never inverted, with the coordinates rounded as the limiting mask's are. The limiting
mask stays `enhance`'s and `export`'s own argument; the two are never sent in each other's place.

### R5. When the preview re-runs

A removal is the chain's first step, so a new region is new work for every model after it, unlike a limiting-mask
change, which is served from the cache and only re-blended. The region is therefore a dependency of the run effect like
the stack, behind a **300 ms settle** (`useSettledRegion`): a stroke that ends starts a run after a pause, a flurry of
strokes costs one run, and a run still computing an older region is stopped for the new one (its work is for a region
nobody wants any more). The settled value is keyed by the photograph, so a change of photograph sends the new one's
region at once and never the previous one's; a region taken away or cleared is not waited for (nothing to compute, and a
crop change must not run the old strokes over the new framing). The progress chip names the family `remove` with the
enhancement's short name ("Remove").

### R6. What the user sees

- **A tint of its own colour**: a new theme token `--removal`, magenta (`#e040fb`), at 35 percent, against the limiting
  mask's primary blue at 40. Magenta is what editors' heal and remove brushes draw, is far from the blue in hue, and is
  rarely the colour of what is being removed; lighter, because what is under it is the fill being judged. The live and
  pending strokes are drawn in the same colour.
- **One overlay**, showing the region being edited and only that one (`MaskOverlay` reads the target from `editing`);
  outside the brush neither is tinted, as before. O (Show overlay) hides the tint to judge the fill.
- The cursor, Space-to-pan, zoom and the split and side-by-side views are the mask brush's, unchanged.

### R7. Export

`features/export/batch.ts` puts each photograph's region on its Remove (`withRemoval` over `regionOf` of the store's
entry, read when its turn comes, as the stack and the crop are), and sends the limiting mask beside it as before.

### R8. A crop change

The removal's strokes are fractions of the framing, as the limiting mask's are, so a crop change takes both away in the
same Apply. One toast says what was cleared ("the mask", "the area painted for Remove", or both) and its Undo restores
the crop and whatever it took, together, only while neither region has been painted since. The crop dialog's notice
names the same.

### R9. i18n

New keys, in all 14 locales: `enhancements.removal.{name, painted, unpainted}`, `preview.progress.removal`,
`mask.removal.{title, hint}`, `mask.crop.{noticeRemoval, noticeBoth}`, `mask.toast.{clearedRemoval, clearedBoth}`. The
panel's tool names reuse `mask.panel.*`. `enhancements.remove` was already the row's X ("Remove enhancement"), hence
`removal` for the family's keys. Each language names Remove as its photo editors do (de "Entfernen", fr "Supprimer", ja
"除去", ru "Удаление", zh "移除").

## Risks / Trade-offs

- **The tint hides the fill it covers.** At 35 percent the fill shows through; O turns the tint off. If that proves
  clumsy, the removal's committed tint could fade once the run that covers it lands (open question 1).
- **A stroke stops a run in flight.** Painting quickly during a slow first run (the CPU, 1 to 2 s per region) restarts
  it after each pause; one run per pause is the trade for always showing the latest region.
- **Every later step recomputes** after a region edit (D2): a long chain with an upscale pays for it on each stroke.
- **Gain maps**: until slice 3, a build with Ultra HDR must not merge Remove (D12).
- **Merges**: `stores/mask.ts`, `MaskSection.tsx`, `EnhancementRow.tsx`, `lib/enhancements.ts` and the 14 locales are
  touched by both series; masking slices 3 and 6 will need to carry `Target` along.

## Platforms

Validated on macOS only. Windows and Linux untested.

## Open Questions

1. Should the removal's tint fade (or become an outline) once its run has landed, so the fill is seen without pressing O?
2. Should the tint show the grown region (8 px, what the run actually treats as marked) rather than the painted one?
3. Core ML for Lima in the GUI (0.4 s warm per region against 1.07 s, at 4.4 GB more peak): D7's question.
