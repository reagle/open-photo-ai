> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

Slice 7 gives the render a way to composite a matted subject; it still needs the matte. The fork's prototype compared
three ways of getting one on a fur hood, dark hair and leaves (`opai-notes/lens-blur/matting/README.md`): a closed-form
matte from a depth trimap (works, blocky), BiRefNet (1.1 GB, minutes on the CPU, soft at fur scale, and it selects
salient things that are not the subject), and **a depth trimap refined by ViTMatte-S**: the best on all three, strands
kept, no curls or haze, 26 M parameters, about 0.35 s per 1024 px tile on the GPU and 0.8 s on the CPU, 17 to 21 tiles
on a 12 to 20 MP photograph. This slice adds it, automatically: no user input, the subject is what the focus point and
the depth say is in focus.

## What Changes

Task estimate: **13**. Slice 8 of the `add-lens-blur` series (see `add-lens-blur-7-matte-render/design.md` - Slices;
this change's design has X1 to X9).

- **A Matting family** (`mt`, an analysis like Depth, never applied in a chain) with one model, **Porto** (ViTMatte-S,
  `hustvl/vitmatte-small-composition-1k`, Apache-2.0 weights, MIT code), FP32, run in 1024 px tiles over a trimap's
  unknown band only. Its graph is fork-only for now, installed locally through `models/.fork-listing.json` as Lisbon's
  are, until the maintainer decides on hosting.
- **`Opai::matte(&Picture, &Matting) -> Executed<Matte>`**: a cached data run keyed by the photograph and the trimap.
- **The lens blur mattes its subject**: from the depth and the focus point it builds a trimap, has Porto matte it (once
  per photograph, framing and focus; cached), and renders through slice 7's subject path: background filled behind the
  subject, subject colour by blur fusion, subject composited as one sharp layer, layers spaced geometrically. Skipped
  when the blur would be under 3 px anywhere.
- **CLI**: `--lb-edges <matte|depth>`, default `matte`; `depth` is the render of slices 1 to 5, for comparison.

Crates touched: `opai` (the family, the model, the trimap, the injection, the render), `cli` (the flag). No GUI change:
the GUI's lens blur gets the matte by default; a switch is left to a later slice if wanted.

`rust-sak` coverage found: nothing needed beyond what the depth already uses (the model install path, `memo` for the
cache).

## Capabilities

### New Capabilities

- `matting`: a matte (per-pixel subject coverage) estimated from a photograph and a trimap.

### Modified Capabilities

- `lens-blur`: the subject is matted from the depth and composited over the blurred background.
- `cli-enhance`: `--lb-edges`.

## Impact

- `opai`: `models/artifact.rs` (`Family::Matting`, prefix `mt`, label "Matting", in `ALL` after Depth, not in
  `APPLY_ORDER`); new `models/matting/{mod.rs,variant.rs,matte.rs,porto/{mod.rs,tiles.rs}}`; `models/operation.rs`
  (`Analysis::Matting`, `DataModel for Matting`); `models/catalogue.rs`, `models/build.rs`; `lib.rs` (`Opai::matte`,
  re-exports); `models/lens_blur/{mod.rs,trimap.rs,render.rs}` (the trimap, `with_matte`, the tag, the subject path);
  `inference/depth_map.rs` (matte after depth); cli events and look (`"matting"`), `perf` selection (skips it as it
  skips depth).
- `cli`: `args.rs`, `chain.rs`, `docs/CLI_REFERENCE.md`.
- Fork-only: `opai-notes/lens-blur/matting/export_porto.py` and `install_porto.py` (notes repo, not this repository), as
  for Lisbon.
