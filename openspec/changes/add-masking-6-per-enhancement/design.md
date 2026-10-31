## Context

The series' decisions are in `add-masking-1-core/design.md` (D1 to D14), the brush's in `add-masking-2-brush/design.md`
(G1 to G9) and the lens blur's in `add-masking-3-lens-blur/design.md` (L1 to L6); Remove's in
`add-remove-1-core/design.md` (D1 to D14) and `add-remove-2-gui/design.md` (R1 to R9). This file records slice 6's
(D1 to D18). It replaces the series table's "per-enhancement scope" (whole photograph, inside or outside the one mask)
with a mask of each enhancement's own, which is what was agreed with Joseph on 2026-10-30, and takes from slice 7 the
one piece it needs, masks kept in the photograph's own coordinates, for the new masks only.

State of the code (`integration`, 2026-10-30, `crates/opai/src/inference/process/mod.rs`):

- **One limiting mask per run**, `ProcessOptions::mask`. `process_with` puts the chain in `Family::APPLY_ORDER` (Remove,
  Denoise, Face Recovery, Colorization, Light Adjustment, Color Balance, Sharpen, Upscale, Restore, Lens Blur), counts
  the leading removals and the trailing lens blurs, and blends the result of what lies between with the photograph
  **after its removals** (`Steps::keep`, `Ran::kept`), through `masked` (rasterise, Lanczos3 enlargement under an
  upscale, `mix::masked`, `masked_gain_map`). A limiting mask before lens blurs **splits the chain** there: the head is
  run, blended, and the tail runs over the blended picture through a `RunCache` keyed from
  `identity_masked(identity_after(head), signature)`.
- **Step keys** are `identity_after(source, operations[..=index], depth)`, a fold of each operation's `cache_tag` and
  the depth tag; `run` walks forward through stored prefixes and decodes only the newest.
- **Regions owned by one enhancement**: Remove's `Arc<Mask>` (white is removed, `rm-lima-fp32-m<signature>`), and the
  lens blur's `Refine` (focus and blur strokes, `-r<signature>`), whose depth and matte are estimated on the chain
  input after its removals, so neither a mask nor an enhancement before the blur estimates them again.
- **GUI**: `stores/mask.ts` keeps three regions per path (`Target = "limit" | "removal" | "refine"`), each a
  `MaskEntry` with its own undo and redo, painted with one brush and one panel; strokes are fractions of the **framed**
  photograph, so a crop change takes them away with an Undo toast. `lib/batch.ts` `keyOf` hashes the operations (which
  carry the removal and the refinement) and the limiting mask. The crop is an `opai::image::Affine` applied by the
  GUI's Rust before the run (`crates/gui/src/images/crop.rs`); the CLI has no framing.
- **Gain maps**: only Remove and Lens Blur rewrite one (`ImagePipeline::rewrites_gain_map`); the limiting mask blends
  the map only where a step after the removals rewrote it.
- **Export record**: `opai:Enhancements` lists each operation and its settings (the `mask` parameter left out),
  `opai:Masked` says a limiting mask was used.

## Goals / Non-Goals

**Goals:**

- Sharpen the subject and denoise the background in one stack, preview and export agreeing.
- The limiting mask keeps its meaning and its cost: nothing a user has today changes.
- A mask edit never runs its own step's model again.
- Step masks survive a crop change.

**Non-Goals:**

- Moving the limiting mask, the removal and the refinement into the photograph's coordinates (slice 7, which can reuse
  D7's `Mask::reframed`).
- A mask for Upscale, an enlarging Restore, Face Recovery, Remove or Lens Blur (D8 to D10).
- Click-to-select and edge refinement for step masks (slices 4 and 5 apply to any region once they exist).
- Several masks for one enhancement, or one family twice in a chain (the chain already allows one per family).
- Luminosity or colour-range masks.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 6.1 | `add-masking-6-per-enhancement-1-core` | `ProcessOptions::step_masks`, the per-step blend in `run`, the masked step keys, `Family::takes_step_mask`, `Mask::reframed`, the export record | masking 3, remove 2, refine | local merge of `add-masking-3-lens-blur` and `fix-lens-blur-trimap-7-refine` | proposed |
| 6.2 | `add-masking-6-per-enhancement-2-cli` | `--dn-mask`, `--cl-mask`, `--la-mask`, `--cb-mask`, `--sh-mask`, `--rs-mask`; docs | 6.1 | 6.1 | proposed |
| 6.3 | `add-masking-6-per-enhancement-3-gui` | Paint mask on a row, the badge, the overlay of the row edited, the masks on the wire, the crop keeping them, i18n | 6.1 | 6.1 | proposed |

Library first, then the CLI (which proves the library on real photographs before any UI exists), then the GUI. Each
slice is one bookmark of one or a few commits; `integration` takes 6.3's tip in place of its base's parents.

## Decisions

### D1. A step mask blends one step with its own input

A maskable step `k` given a mask `m` produces `out_k = m x step_k(out_{k-1}) + (1 - m) x out_{k-1}` per pixel and per
channel, in unit space, with `m` the mask's weight (slice 1's `mix::masked`, exact copies at 0 and 1). The step's own
strength blend (Denoise, Sharpen, Restore) happens first, inside its pipeline, as today; the mask then limits where
that result lands. The next step reads `out_k`.

The obvious alternative, blending every masked step's result after the chain, is not the same and is wrong: with
"sharpen the subject, denoise the background", a sharpen that ran over the denoised background and was masked out
afterwards would still have run over it, and an upscale after it would have been fed a picture nobody sees. Blending
in place is what the user means and what a layer stack in an editor does.

**Which families** (`Family::takes_step_mask`): Denoise, Colorization, Light Adjustment, Color Balance, Sharpen, and
Restore when its scale is 1. A step mask on any other operation is refused by `plan` as
`InferenceError::Unsupported("a <family> takes no mask of its own")` before anything runs, so a front end that sends
one by mistake is told rather than ignored.

### D2. The limiting mask stays "all enhancements", blended where it is today

The photo-wide mask keeps its place and its cache behaviour: after the removals, before the lens blurs, over the
chain's result (with step masks applied inside it). Its meaning for the UI: **the mask for all enhancements**.

Where both are painted, a single masked step's effect is the **intersection** `M x m`: the limiting mask `M` brings the
photograph back where it is black, whatever the step mask says. With several steps the composition is the nested blend
of D1 inside the limiting blend, which is what a user who painted both expects ("only in the subject, and of the
subject only the face is sharpened").

Not done: folding `M` into each step's weight (`m_k x M`). It gives the same result for one step, but it would put the
limiting mask into every masked step's key, so a stroke on the limiting mask would run every step after the first
masked one again, where today it costs one blend (`add-masking-1-core` D5). Keeping the two apart keeps the existing
promise.

### D3. Step masks are a run option keyed by family, sent on the operation

`ProcessOptions::step_masks: StepMasks`, a small map from `Family` to `Arc<Mask>` (`BTreeMap`, so its iteration order
and `Debug` are stable). Keyed by family because a chain holds at most one operation per family, so the family names
the step unambiguously, and the map survives `in_apply_order` and the removal-dropping and face-sparing rewrites of the
chain without being carried through them. An empty mask is dropped (D6 of slice 1), and a map left empty is no step
masks: the run, its keys and its identity are exactly today's.

Not a field on each family's struct: that would touch six `Operation` arms, their serde, constructors, catalogue rows
and `cache_tag`s, and Autopilot's builders, for a value no model reads. The keys still say it (D5).

On the GUI's wire the mask travels on the operation it belongs to, `Requested.scope: Option<Mask>` (distinct from
Remove's `mask`, the region it removes), and `crates/gui/src/enhance` moves it into `step_masks` after `resolve`. The
CLI fills the map from its flags (D15).

### D4. Where the blend happens: inside `run`, after the step

`run` gains `step_masks: &StepMasks` in `Steps` (a borrowed rasterisation cache, D11). After step `k` produces its
pixels (computed, or decoded from the store), if `operations[k].family()` has a mask: rasterise it (once per run, at the
**chain input's** size), `mix::masked(prev, result, weight)` reading the weight bilinearly at the step's size, and carry
on with the blended picture. The step's `prev` is already in hand (it is what the step ran on) or is the newest decoded
prefix; a served run of hits that ends on a masked step decodes the hit before it as well, the same materialisation the
limiting mask's `keep` already does. The blend runs off the async runtime (`spawn_blocking`), checks the cancel token
before it, and logs at `debug` with the mask's signature and time, as `masked` does.

The limiting mask's split (`split`, head and tail) and its `keep` are unchanged: step masks only change what the head's
steps produce.

### D5. The cache: the unmasked step and the masked step, each under its own key

- **Unmasked**: the step's pipeline result is stored under the key it has today, folded from the previous step's
  (possibly masked) key. A mask edit therefore finds it and runs no model for that step.
- **Masked**: the blended picture is stored under `identity_masked(unmasked key, signature)` (the existing `mask`
  discriminator), and **every later step folds from it**: `identity_after_masked(identity, operations, step_masks,
  depth)` is `identity_after` with that one extra fold after each masked step. The result's identity is the same fold,
  so two runs that differ in one step mask never share a result or a later step.
- A run with no step masks keeps every key it has today, and so does every step before the first masked one.
- Blended pictures are stored like steps (PNG in the store), unlike the limiting blend, which is not stored (slice 1
  D5): here a later step's key hangs off it, and a forward walk that hits the next step's entry must not need to
  recompute the blend to know it may skip past it.

### D6. What a mask edit costs

Editing the mask of step `k`: step `k`'s model is served (unmasked key), one blend is redone, and **steps `k + 1` to the
end run again**, because their input changed. Steps before `k`, the faces, the depth and the matte are served. On the
fork's M4 Pro:

| What follows the edited step | Extra cost per edit (measured elsewhere) |
|---|---|
| nothing, or only the limiting blend | under a second at 24 MP (one blend) |
| Sharpen (Petersburg/Moscow) | a few seconds |
| Lens Blur | the render only: a few seconds; depth and matte served, since they are estimated on the chain input after its removals |
| Upscale 4x | tens of seconds |
| Restore (Kazan, 1x) | 9 to 13 s at 1.3 MP (9 tiles), about 100 s at 12 MP; 2x is 26 to 27 s at 1.3 MP |

So the cheap places to paint are the late steps, and a mask on Denoise under a Kazan restore is expensive per stroke.
The GUI therefore treats a step mask like Remove's region (R5): a dependency of the run behind the 300 ms settle, a
run in flight stopped for the new mask, and a one-line hint in the panel when a slow step follows ("Restore runs again
after each stroke"). The library does not reorder anything to save time.

### D7. Masks in the photograph's own coordinates

A step mask's strokes are fractions of the **photograph as loaded** (oriented, before the crop, turn and flips), not of
the framing. `Mask::reframed(&self, framing: &Affine, source: (u32, u32), framed: (u32, u32)) -> Mask` maps each point
through the framing into fractions of the framed picture and scales each radius by the ratio of the two longer sides (a
turn keeps a circle a circle); a base plane is resampled through the same affine. The GUI's `enhance` and `export` call
it with the crop they already apply; the CLI never frames, so its masks are used as they are.

A crop change therefore **keeps** step masks, unlike the other three regions, which still clear with an Undo toast
until slice 7 moves them the same way. Points that fall outside the framing are kept (they may come back with a wider
crop) and simply rasterise to nothing. The overlay shows a step mask in the framing too, through the same function
(`mask_preview` gains an optional framing).

**After an upscale**: a mask is rasterised at the chain input's size and read bilinearly at the step's size, so a
Restore at 1x after a 4x Upscale gets the same region at four times the pixels without a plane that large (slice 1 D3).

### D8. Upscale and an enlarging Restore take no mask

Technically the blend would work (slice 1 already enlarges the photograph with Lanczos3 for the limiting mask), but:
the masked-out part would be a Lanczos enlargement next to the model's texture, the most visible seam in the app; every
later step would run at the enlarged size over that seam; and "upscale only the subject" is already the limiting mask
with the upscale as the only enhancement inside it. A step that changes the size takes no mask (D1's refusal), which
also keeps D4's blend free of resampling.

### D9. Lens Blur takes no step mask

Blending a blurred picture with the unblurred one shows sharp background where the mask is black: the opposite of a
lens blur, the reason slice 3 moved the limiting blend before it. Where a lens blur applies is said by what it already
has: the limiting mask's "keep in focus" (`keep_sharp`) and its Refine brush (focus and blur strokes, matted). In the
GUI, the Lens Blur row's **Paint mask** opens the Refine brush, so every row with a region opens it from the same place
(D16).

### D10. Remove and Face Recovery take none either

Remove's region is its mask. Face Recovery's region is its face choice (which faces it restores, `FaceSelection`),
and it already composites each face through a feathered face plane; a second mask over that adds nothing a user asked
for. Both rows keep the actions they have.

### D11. Feathering

A step mask is painted with the same brush (radius, softness, paint and erase) and rasterised by the same code, so its
edge is the brush's softness; the panel's default softness for step masks is 0.5, as for the limiting mask. A CLI mask
file's greys are partial weights, as for `--mask`. No separate feather control: a seam between a denoised and a grainy
area is soft as painted, and a hard-edged file can be blurred before use (documented). Each mask is rasterised once per
run and held for the run (a 24 MP plane is 96 MB of `f32`; a chain has at most six).

### D12. Autopilot

Autopilot chooses families, models and strengths; it never paints, clears or moves a mask. Its analysis reads the
whole photograph, as it does with a limiting mask.

- GUI: masks are kept per photograph **by family**. Autopilot replacing a family's model keeps that family's mask;
  Autopilot leaving a family out hides its mask (not sent) without forgetting it, as the limiting mask's strokes are
  kept when the stack empties; taking the row off with its X forgets it, as Remove's region is forgotten.
- CLI: `--sh-mask` with `--autopilot` limits whatever sharpen Autopilot chooses; if it chooses none, the photograph
  runs and one warning says the Sharpen mask had nothing to limit (the batch goes on).

### D13. Face recovery in a chain with step masks

Faces are detected on the source before the chain (unchanged), and a face recovery sits after Denoise: a denoise masked
to the background leaves the faces' own grain for the face recovery, which then restores as it does over an undenoised
photograph. Nothing special is done; a test pins that the faces' boxes and keys do not depend on any step mask.

### D14. Ultra HDR gain maps

No maskable family rewrites the gain map (`rewrites_gain_map` is false for all six), so a masked step hands the map on
unchanged, and both halves of its blend share it. `plan` asserts it: a maskable family whose pipeline rewrites the map
is refused with `Unsupported` until its blend also blends the map (as `masked_gain_map` does for the limiting mask). A
test pins that a masked step leaves the map's bytes untouched.

### D15. CLI: `--<family>-mask <FILE>`

`--dn-mask`, `--cl-mask`, `--la-mask`, `--cb-mask`, `--sh-mask`, `--rs-mask`, each `requires` its family's flag (as
`--sh-str` requires `--sharpen`), with the help heading of its family's other flags. Read once per command
(`Mask::from_image`), the aspect rule of `--mask` per photograph (refuse that photograph only), white applies. `--rs-mask`
with `--rs-scale` above 1 is refused at parse time (D8). No `--up-mask`, `--fr-mask`, `--lb-mask`, `--rm-mask`.

Why this spelling rather than `--mask sharpen=FILE`: every per-family setting is already a prefixed flag (`--dn-str`,
`--la-bias`, `--rs-scale`, `--lb-refine`), so the six read like their neighbours, sit under their family's heading,
are validated by clap's `requires`, and need no parsing of a value that could be a file name containing `=`;
`--mask FILE` keeps its one meaning. `--gui` does not carry them (the open request names enhancements, not regions):
listed with the other flags `--gui` ignores, as `--lb-refine` is.

### D16. GUI

- **Paint mask on a row**: each maskable row (D1) gains a brush icon button beside its settings, **Paint mask** (tooltip
  "Limit Sharpen to part of the photograph"); once painted, the row shows a small mask badge and the button reads
  **Edit mask**. Lens Blur's row offers **Refine** in the same place (D9); Remove's row already toggles its brush.
  Restore's button is disabled while its scale is above 1 (tooltip says why), and its strokes are kept.
- **The brush**: the existing panel in the mask section, titled "Mask for Sharpen" with a dot of the step-mask tint,
  Paint/Erase, Size, Softness, Invert, Undo/Redo, Clear, Show overlay, Done; one region edited at a time, as now.
- **Overlay**: the tint of the row being edited only (a new token `--step-mask`, amber at 40 percent, apart from the
  limiting mask's blue, the removal's magenta and the refinement's green and orange), drawn through D7's framing.
- **Store**: `Target` gains `{ step: Family }`; `stepMasks: Map<path, Map<family, MaskEntry>>`, each with its own
  history, forgotten with its row or its file, **kept on a crop change** (D7). The limiting mask's section is relabelled
  "Mask for all enhancements".
- **Wire**: `withScopes(operations, entries)` puts each family's mask on its operation as `scope`, on the way to
  `enhance`, `export` and the batch, as `withRemoval` and `withRefine` do; `keyOf` then covers it through the
  operations.
- **Re-runs**: D6's settle, and the hint when Upscale, Restore or a Lens Blur follows the edited step.
- **i18n**: `mask.step.{title, paint, edit, badge, hint, slowHint, disabledScale}`, `mask.section.all`; all 14 locales.

### D17. The export record

Each `opai:Enhancements` entry of a masked step gains `, masked` after its settings
(`Sharpen Petersburg (FP16): strength=1.0, masked`), and the history event's parameters name it the same way
(`enhanced with Denoise, Sharpen (masked)` names the step, the trailing ` (masked)` of the whole chain staying the
limiting mask's). `opai:Masked` stays the limiting mask's. The digital source type is unchanged (a mask makes nothing
more generative).

### D18. Fork-only for now

Built on fork-only lines (Remove, the lens blur and its refinement). The library half (D1 to D5, D7) would go upstream
cleanly after masking slices 1 to 3; whether to offer it is Joseph's later call. HOLD: no upstream issue or PR.

## Risks / Trade-offs

- **Seams between differently processed areas** (a denoised background beside a grainy subject): the brush's softness
  is the tool; the panel's help says so. A later grain step (`add-restore-grain`) can even out the two.
- **Cost of an early mask under a slow late step** (D6): the hint, and the settle.
- **More stored entries**: a masked step stores two pictures. Each is the size every step already stores.
- **Two coordinate systems in one store** until slice 7: step masks follow the photograph, the other regions the
  framing. The crop dialog's notice names only what it clears.
- **Merges**: `run` and `process_with` are touched by every line; record resolutions in
  `opai-notes/masking/merge-step-masks.md` for `integration`.

## Platforms

CPU work, platform-independent. Validated on macOS (Apple silicon) only; Windows and Linux untested.

## Open Questions

1. **Lens Blur's row**: Refine in the Paint mask place (default), or no button there and Refine only in its panel?
2. **Restore above 1x**: refuse (default), or allow with a Lanczos-enlarged input on the masked-out part?
3. **Tint colour** for step masks (default amber), and whether the badge should show the mask's thumbnail.
