> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5, high effort) at my behest; I tested its application in
> practice.

## Why

A copper tag hanging off a bonsai, a passer-by walking through a street photograph, a sign in the corner of a
landscape: the commonest retouch after exposure and colour is taking something out. Open Photo AI can sharpen, denoise
and restore a photograph, but has no way to remove anything from it.

On 2026-10-27 three inpainters were tried against hand-drawn masks on the fork's test photographs (notes:
`~/dev/opai-notes/infill/README.md`): Big-LaMa, MI-GAN and a PatchMatch, with OpenCV's Telea as a baseline. LaMa won
every removal and was the only one that handled a large one (a person a fifth of the frame): the pavement, kerb and
bollards continue, where MI-GAN painted a sheet of invented lettering and PatchMatch a flat slab. Its fill is soft when
a large region is inferred at 512, but plausible. This change makes that the Remove enhancement.

**Fork-only.** The Carve/LaMa-ONNX weights are labelled Apache-2.0 (the advimman/lama code is Apache-2.0), but they were
trained on Places365, whose terms allow non-commercial research use. Upstream's CONTRIBUTING asks for AGPL-compatible
models, and vegidio/open-photo-ai#72 asks how to treat weights trained on research-only data; until that is answered
Remove stays on the fork, with no PR, as Kazan (Restore) does.

## What Changes

Task estimate: **14**. Slice 1 of the `add-remove` series (see design.md - Slices).

- **A Remove family (`rm`, label "Remove") with one model, Lima** (Big-LaMa, `rm_lima_fp32`, 210 MB, FP32 only),
  exported by the fork and installed through the fork's local model listing (`fork-model-listing`), like Kazan, Lisbon
  and Porto. The fork export packs the source graph's two inputs (image and hole) into one 1x4x512x512 tensor and
  scales its output to [0, 1], so it runs through the existing one-tensor seam.
- **The operation carries its own region**, an `opai::Mask` (white = remove), the same type as the masking series'
  limiting mask, so the GUI's brush and the CLI's mask file serve both. It is not `ProcessOptions::mask`.
- **Remove runs first**, on the photograph's own pixels, before every other enhancement.
- **Per region**: the marked pixels are grown by 8 px, split into connected regions, and each region (merged with any
  whose context overlaps) is cropped with generous context, inferred at 512, read back bicubically and pasted into the
  photograph through a feather confined to the hole the model filled (16 px beyond the marks).
- **CPU by default**: under `Auto` the model opens on the CPU, which measured faster than Core ML for this graph.
- **The removal is cached** under a tag that carries the mask's signature, so editing a later enhancement never runs
  LaMa again.
- **A limiting mask does not bring back what was removed**: the masked blend uses the photograph after its removals.
- **Faces in the removed region are not restored** by a face recovery in the same chain.
- **CLI**: `--remove <FILE>` (`--rm`), a mask image whose white part is removed, with `--mask`'s aspect-ratio rule.

Crates touched: `opai` (the family, its pipeline, the CPU preference, the chain's intermediate for the limiting mask,
the faces), `cli` (the flag), `perf` (Remove is not measured by the harness), `docs`. `gui` gains nothing but a label
match; the panel is slice 2.

`rust-sak` coverage found: `crypto` (the mask's signature, already `Mask::signature`). Nothing is missing.

## Capabilities

### New Capabilities

- `object-removal`: what Remove does with a region, where it runs in a chain, and how its result is cached.

### Modified Capabilities

- `cli-enhance`: `--remove <FILE>` removes the white part of a mask image.

## Impact

- `opai`: new `models/remove/` (`mod.rs`, `variant.rs`, `lima/`, `region.rs`, `process.rs`, `live.rs`);
  `models/artifact.rs` (the family, first in the apply order), `models/operation.rs`, `models/build.rs`
  (`ParameterValues::with_mask`), `models/catalogue.rs` (`ParameterKind::Mask`), `providers/profile.rs`
  (`cpu_on_auto`), `sessions/mod.rs` (honour it), `inference/process/mod.rs` (drop an empty removal; the photograph after
  the removals for the limiting mask; faces in the region), `lib.rs` re-exports.
- `cli`: `args.rs` (`--remove`), `run.rs` (read once, aspect check, prepend the operation), `look.rs`, `events.rs`,
  `docs/CLI_REFERENCE.md`.
- `perf`: Remove is skipped, as Lens Blur is on its own line.
- No change to the cache's storage format or to any other model.
