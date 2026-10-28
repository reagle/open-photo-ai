## 1. Base

- [x] 1.1 Merge `add-remove-2-gui` and `add-color-4-ultra-hdr-gainmap` locally (`pxnzlutu`): `crates/cli/src/run.rs` (both masks checked, `Written` returned), `inference/process/mod.rs` (the masked identity on `source.derive`), the tests' module list; the API changes the colour line made (`image::save`, `encode_blocking` taking metadata; `Exported.losses` in a frontend test)

## 2. Library

- [x] 2.1 `imaging::fill` (push-pull) and `imaging::boxes` (reductions), from the lens-blur line (design.md - G1)
- [x] 2.2 `ImagePipeline::rewrites_gain_map` / `run_with_gain_map`, the chain carrying the map, `<step key>|gm` in the run cache, `Picture::with_shared_gain_map`, `GainMap::rewritten` / `shared_pixels`, `InferenceError::GainMap` (design.md - G6)
- [x] 2.3 `models/remove/gain_map.rs`: the per-cell hole and feather, the fill per crop, the drop reasons; `Removal` implements `ImagePipeline` with the regions computed once; tests: a full-resolution map (grayscale and RGB) filled from the surroundings and unchanged past the hole, a quarter-resolution map, a larger map refused, nothing to fill, the cell footprints (design.md - G1 to G5)
- [x] 2.4 Chain tests: fresh and cached runs carry the same rewritten map and export it with no loss; a wider map is dropped fresh and cached and the export reports it; a chain without a removal keeps the source's map shared

## 3. Checks

- [x] 3.1 `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm test`, `pnpm typecheck`
- [x] 3.2 End to end on an Ultra HDR photograph (a ceiling light and an EXIT sign removed): no drop warning, the written map holds the surroundings' gain where they were and differs by at most 3 levels elsewhere (re-encoding)
- [ ] 3.3 Joseph: look at the result on an HDR display (no bright ghost where the objects were)
