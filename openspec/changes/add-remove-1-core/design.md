## Context

See proposal.md - Why. This design is the series' own: it records the decisions for the whole of Remove (D1 to D14),
and the later slices' design.md files refer back to it rather than repeating it.

State of the code the series builds on (a local merge of `add-masking-2-brush` and `fork-model-listing`, both on
`main@upstream`):

- **A chain runs at full resolution** on the framed picture. `inference/process/mod.rs::process_with` resolves the
  faces, puts the chain in `Family::APPLY_ORDER`, plans it, runs it (`run` keys each step in the run cache by
  `identity_after(source, operations[..=index], depth)`) and, where `ProcessOptions::mask` is given, blends the result
  with the photograph through the mask.
- **`opai::Mask`** (masking slice 1) is brush strokes in fractions of the framed photograph over an optional base
  weight plane, optionally inverted, rasterised at any size, with a signature. The GUI's brush (masking slice 2) edits
  one per photograph and sends it with every run; the CLI's `--mask` reads a file into a base.
- **The fork's local model listing** (`fork-model-listing`): `models/.fork-listing.json` names the files of artifacts the
  published listing does not, so a fork-exported graph installs like a published one.
- **One seam per graph contract** (`pipeline/session.rs`): one tensor in and one out (`run_graph`), named outputs, and an
  image plus a scalar weight. No contract takes two image-sized inputs.
- **Two-step fills elsewhere.** Face recovery crops, infers at 512 and pastes back through a feathered plane, all private
  to its family.

The prototype this ports is `.../scratchpad/infill/scripts/inp.py` (`lama`) and `runB.py` from the comparison: dilate the
hand-drawn mask 8 px, crop the dilated box with `0.6 * longer side + 64` px of context, dilate 8 px more for the model's
hole, fit the crop's longer side to 512 and reflect-pad the square, feed RGB in [0, 1] and a 1/0 hole, read the [0, 255]
output back with a bicubic resize, and paste through a Gaussian (sigma 4) of the once-dilated mask.

## Goals / Non-Goals

**Goals:**

- Remove what the user marks, plausibly, at the photograph's full resolution, in a few seconds on a laptop CPU.
- One mask type for both brushes: the GUI's slice reuses the masking brush, and the CLI takes a mask file.
- Editing any other enhancement never runs LaMa again.
- An API the GUI's slice can use without new library work: the catalogue says Remove takes a mask, and the mask goes
  in the operation.

**Non-Goals (later slices, or not at all):**

- The GUI (slice 2).
- Ultra HDR gain maps (slice 3, D12).
- A refinement pass at a higher resolution for large regions, a quick small-brush model (MI-GAN), shadow-aware growth of
  the mask, a "grow" control, a subject click to select what to remove: slice 4, not proposed.
- Prompted, diffusion-based generative fill.

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-remove-1-core` | The Remove family, Lima, the region pipeline, CPU on Auto, the cache tag, the limiting mask and faces rules, CLI `--remove` | - | merge of `add-masking-2-brush` and `fork-model-listing` | **this change** |
| 2 | `add-remove-2-gui` | GUI: Remove as an enhancement with its own brush (the masking brush, a second tint), sent in the operation; i18n | 1 | 1 | proposed |
| 3 | `add-remove-3-hdr-gainmap` | An Ultra HDR gain map filled under the removed region from its own surroundings (D12) | 2, `add-color-4-ultra-hdr-gainmap` | merge | built |
| 4 | `add-remove-4-refine` | A second pass at a higher resolution for large regions; a grow control; possibly MI-GAN for small strokes | 2 | 2 | pending, not proposed |

## Decisions

### D1. A family of its own, `rm`, with one model, Lima

Remove is an enhancement a user adds to a photograph's stack like the others, so it is a family (`Family::Remove`,
prefix `rm`, label "Remove") carried in `Operation::Remove`, listed by the catalogue and built from a catalogue row. The
model follows the fork's naming (a city, as Kazan, Lisbon and Porto): **Lima**, artifact `rm_lima_fp32`. Published at
FP32 only: the source export is FP32, and an FP16 conversion was not tried (LaMa's FFT convolutions are where an FP16
overflow would be).

The graph is the fork's own export (`~/dev/opai-notes/remove/export_lima.py`): Carve/LaMa-ONNX's `lama_fp32.onnx`
(sha256 `1faef530…`) with a `Split` in front, so its two inputs `image` (1x3x512x512) and `mask` (1x1x512x512) arrive as
one `input` (1x4x512x512: RGB, then the hole), and a `Div` by 255 behind, so `output` (1x3x512x512) is in [0, 1]. Checked
against the source graph on random input: 4e-5 apart. **Packing rather than a fifth session contract**: a `run_masked`
beside `run_graph` would add a method to `Backend` and to every fake backend in the suite for one model, where a
four-plane tensor goes through `run_graph` as Porto's RGB + trimap already does.

Alternatives: MI-GAN (MIT, 30 MB, 0.1 s) fails on large regions (it painted lettering over the woman); a PatchMatch is
not competitive and has patent questions; diffusion inpainting is OpenRAIL-licensed and gigabytes.

### D2. Remove runs first, on the photograph's own pixels

`Family::APPLY_ORDER` begins with Remove. LaMa was trained on photographs as cameras produce them, so it sees the
source's grain and sharpness rather than a denoised or sharpened version, and every enhancement after it then treats the
fill like the rest of the photograph: a fill pasted after a sharpen would be softer than its surroundings, one pasted
after an upscale would be inferred over invented detail. A face recovery after it never restores a face that is no
longer there (D10).

The cost: Remove's tag is folded into every later step's key, so an edit of the removal region recomputes every step
after it. Removal regions are edited less often than strengths, and a strength edit never reruns LaMa (D8).

### D3. The operation carries its region, an `opai::Mask`

`Remove { variant, mask: Arc<Mask> }`. White (weight at least 0.5 after rasterising) is removed; grey below that is
not: a removal is all or nothing per pixel, and the edge is the pipeline's own feather (D5). The same `Mask` the masking
series made, so the GUI paints it with the same brush and the CLI reads a file into a base, but it is the operation's,
not `ProcessOptions::mask`: the limiting mask says where the enhancements apply, this one says what to take out.

- **Built from the catalogue**: the row publishes one parameter, `mask`, of a new kind `ParameterKind::Mask` ("a region
  painted on the photograph, not a control", as `Faces` is "not a control"), and `ParameterValues::with_mask` sets it.
  A row built without one carries an empty mask.
- **An empty mask removes nothing and is dropped** from the chain before it is planned, so adding Remove before painting
  installs and opens nothing, and the result's identity is the chain's without it.
- **Equality, hashing and the cache tag** go through `Mask::signature`. Serialised, the mask is its wire shape (strokes
  and inversion); a base built in-process from a file does not cross the wire, as for the limiting mask.

### D4. Regions: grow, split, give context, merge

Over the rasterised weight at the chain input's size:

1. The marks are the pixels at or above 0.5. None: the photograph is returned unchanged, and no graph runs.
2. They are split into 8-connected components, each with its bounding box.
3. Each box is grown by the model's hole (16 px, D5) and given context on every side of `round(0.6 * longer side) + 64`
   px, clamped to the frame: the prototype's figure, enough for LaMa to see the surface around a small object and the
   scene around a large one.
4. Crops that overlap are merged (the union of their boxes, context recomputed) until none do, so no crop contains
   another region's marks unfilled and no pixel is pasted twice.
5. Each crop is one inference, in reading order of its top-left corner.

Two small objects far apart are then each inferred at a useful resolution, rather than both shrunk into one box.

### D5. Grow 8 px, fill 16, feather between

The distance from each pixel of a crop to the nearest mark is computed exactly (Euclidean, from per-row distances and a
vertical scan, capped at 17 px). Then:

- **d <= 8**: the fill replaces the photograph. The 8 px cover the object's anti-aliased edge, its halo and JPEG
  ringing; the comparison used 8, and the erase guides found suggest about 10 for automatic masks.
- **8 < d < 16**: the fill and the photograph are mixed, the fill's weight falling from 1 to 0 by a smoothstep.
- **d <= 16** is the hole the model is shown; nothing at 16 or beyond changes, byte for byte.

The amounts are pixels at the photograph's resolution, not fractions of it: what they cover (edges, ringing) is a pixel
-scale property of the capture, and both front ends run at full resolution, so a preview and an export agree.

The prototype pasted through `max(gauss(m8, 4), m8)`, which steps from 1 to about 0.5 at the edge of the 8-px mask; the
smoothstep over the band between the two masks has no step and is confined to the model's hole, as the prototype's
paste was in effect (its fill equals the photograph outside the hole). No control in this slice (slice 4).

### D6. Inference at 512, read back bicubically

Each crop is resampled with Lanczos3 so its longer side is 512 and reflected to the square (`imaging::present`, as light
adjustment and colour balance present theirs); a crop smaller than 512 is enlarged onto it. The hole plane is at the same
planned size, a model pixel being hole where any photograph pixel under it is (so a one-pixel string is not lost), and
reflected the same way. The tensor is the three colour planes in [0, 1] then the hole, 1 for hole and 0 for kept
(LaMa's own polarity; the graph composites the known pixels itself).

The output is read back only where it is used (d < 16) by bicubic interpolation (a = -0.75, OpenCV's `INTER_CUBIC`, as
the prototype resized) over the planned part of the square, and composited at the run's channel depth, 8 or 16 bits.

### D7. The CPU under Auto

The comparison measured, with ONNX Runtime 1.31 in Python, 1.15 s per 512 inference on the CPU and 1.67 s with Core ML,
which takes 3984 of 7917 nodes in 585 partitions (the FFT convolutions fall back to the CPU). Measured again in this
slice, with this crate's runtime on the same M4 Pro (24 GB), release builds:

| Provider | First ever | First in a process | Warm, per region | Peak memory (woman, 21 MP) |
|---|---|---|---|---|
| CPU | 1.6 s | 1.6 s (0.5 s of it opening the session) | 1.07 s | 2.8 GB |
| Core ML | 6.5 s (compile, then cached in `engines/`) | 2.1 s | 0.40 s | 7.2 GB |

Core ML's output agrees with the CPU's to one level on 16 pixels of 19 million. So Core ML is faster once warm, which a
GUI painting stroke by stroke would feel, but peaks 4.4 GB higher beside whatever else a session holds, and a one-shot
CLI run is slower on it. **The CPU stays the default under `Auto`** (`EpProfile::cpu_on_auto`, which Lima sets; the
session layer opens such a model on the CPU when `Auto` is asked for). An explicit provider (`--processor coreml`) is
honoured. Open question for Joseph: switch the GUI to Core ML for the warm speed.

### D8. The cache

The tag is `rm-lima-fp32-m<signature>`, the mask's XXH3-64 in hex (strokes, inversion and base pixels). The step is
stored like any other, so a run that differs only after the removal serves it from the store. A run with a different
region misses from Remove on.

### D9. A limiting mask does not bring back what was removed

`ProcessOptions::mask` blends the chain's result with "the photograph". With a removal in the chain, that photograph is
the chain's input **after its Remove steps**: the enhancements are limited to the mask, the removal is not, and a tag
removed outside the sharpened subject stays removed. The run keeps that intermediate (a step served from the store is
decoded for it) only when a limiting mask is given. A chain of nothing but removals ignores the limiting mask, identity
included.

### D10. Faces in the removed region are not restored

The faces a chain's face recoveries restore are found on the source (`FaceSelection::Detect`), before the chain runs, so
they include faces Remove is about to take out; restoring one would paste a face-shaped restoration over the fill. Each
face whose box centre is marked (D4's weight at 0.5 or more) is dropped from every face recovery's selection.

### D11. Progress and cancellation

Each region reports three steps on the operation's share of the bar (presented, inferred, pasted), weighted equally
across regions, and a stop is honoured at each. A stopped run returns no image, as every pipeline's does.

### D12. Ultra HDR gain maps (slice 3)

On the colour line a step keeps the source's gain map unless its pipeline rewrites it (the lens blur does). Kept under a
removal, the map would lift the removed object's highlights over the fill on an HDR display: a bright ghost. Slice 3
rewrites it: the map's cells under the hole (the distance of D5 measured at the map's resolution) are filled from the
map's own surroundings by a membrane fill (push-pull), through the same feather, and the rest of the map is kept byte for
byte. Not LaMa: a gain map is smooth and low-frequency, generated pixels carry no HDR information of their own, and the
neutral continuation of the surroundings is what an HDR rendition of the fill should get. Until slice 3 lands Remove is
not merged into a build that carries gain maps.

### D13. The command line

`--remove <FILE>` (alias `--rm`): a mask image in any format opai reads, whose luminance marks what to remove (white).
Read once for the whole command; each photograph whose aspect ratio differs from the mask's by more than 1 percent is
refused, naming both, and the batch goes on (`--mask`'s rule and code). It counts as an enhancement on its own, and it
combines with the enhancement flags, with `--mask`, and with `--autopilot`, whose analysis reads the photograph before
the removal. No other flag: the grow and the context are fixed in this slice.

### D14. Fork-only

See proposal.md - Why. The notes record the licence, and the catalogue row exists only on the fork's branches.

## Risks / Trade-offs

- **Large regions are soft**: a crop four thousand pixels across is inferred at 512 and enlarged eightfold. Plausible
  structure, little texture. Slice 4's refinement pass is the answer; until then the GUI can say so.
- **A removal edit recomputes the chain** after it (D2).
- **210 MB on disk; 2.8 GB peak** for a 21-megapixel photograph with a large removal on the CPU (7.2 GB under Core ML).
- **Untested on Windows and Linux**; the CPU path is the same code there.
- **Licence** (D14).

## Open Questions

- Whether the GUI should show the grown region (d <= 8) rather than the painted one while painting.

## Results (2026-10-27)

Release CLI on byte-appended copies of the comparison's photographs, masks drawn by the prototype's own code (not grown),
`-p auto` (CPU): the bonsai tag 2.07 s for the enhancement (0.84 s of it opening the session, cold), the Tokyo man 1.67
s and the woman 1.79 s (0.46 s opening), 2.7 to 4.0 s end to end with decode and PNG encode. A second run of the same
command on Core ML was served from the run cache (0.11 s). Against the prototype's LaMa over the same boxes, the mean
absolute difference inside the grown mask is 2.6 levels (tag), 5.8 (man) and 8.4 (woman), and 0.02 to 0.03 outside it
(JPEG decoding); the fills are the same in kind (the paving shadows continue under the tag; the same half-figure behind
the man; road, kerb and bollards for the woman), differing where the Lanczos resample and the any-coverage hole show the
model slightly different pixels. Strips: `~/dev/opai-notes/remove/out/remove-{tag,man,woman}-cli-2018.jpg`.

Suites: `opai` 1772 passed, 40 ignored, 1 failed (`logging::writer`'s rotation-day test, which compares a UTC and a local
"yesterday" and fails between 20:00 and midnight EDT; unrelated), integration tests 44 passed; `cli` 193; `perf` 129;
`gui` 349. `cargo clippy --workspace --all-targets -- -D warnings` clean. Live check: CPU, Auto and Core ML pass.
