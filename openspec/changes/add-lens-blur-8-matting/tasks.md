## 1. The Matting family

- [ ] 1.1 Add `Family::Matting` (prefix `mt`, label "Matting", in `ALL` after Depth, not applied; the `APPLY_ORDER` assertions and every exhaustive match: catalogue, build, cli events and look, `perf` selection, autopilot scope tests, the frontend `Family` union and progress label) (design.md - X1)
- [ ] 1.2 Add `models/matting/` (`Matting`, `MattingVariant::Porto(FloatPrecision)` with only `Fp32` published, the catalogue row, `ArtifactId` `mt_porto_fp32`, `Analysis::Matting`, `DataModel for Matting` with `Output = Matte`) (design.md - X1, X2, X3)
- [ ] 1.3 Add Porto's pipeline (`porto/tiles.rs`: tiles over the unknown band, normalisation, zero padding off the frame, known pixels from the trimap, feathered overlap; one session for all tiles; a progress report per tile) (design.md - X3); tests with a fake backend: tile selection, cross-fade, known pixels exact, normalisation of a known pixel value
- [ ] 1.4 Add `Matte` (16-bit, compact serde), `Opai::matte`, the trimap signature in the data key (design.md - X4); tests: serde round trip, the spec's cache scenario with the fake backend; measure a 24 MP matte's cache read and add the 16-bit PNG form only if it exceeds 100 ms

## 2. The lens blur

- [ ] 2.1 Add `models/lens_blur/trimap.rs` (band 1/1.1 to 1.5 of the focus distance on the guided depth, erosion and dilation by 0.7 percent of the long side at quarter size, nearest upsample) and the X6 rule (design.md - X5, X6); tests on a synthetic depth step and for the threshold
- [ ] 2.2 Inject the matte after the depth in `inference/depth_map.rs` (`with_matte`, the `-a` and `-e0` tag parts, progress within the depth's slot) (design.md - X7); tests with the fake backend: an f-number change runs neither depth nor matte, a focus change runs the matte only, `--lb-edges depth` runs no matte
- [ ] 2.3 Render with a matte in `render.rs` (alpha to the input's size, colour and disparity fills with the ring, blur fusion, subject radius from the confident interior, geometric spacing, `Subject`) (design.md - X8); tests: a synthetic subject with a soft edge composites as the spec says, without a matte the output is bit-identical to slice 3's

## 3. CLI

- [ ] 3.1 Add `--lb-edges <matte|depth>` (`requires` `--lens-blur`) mapped onto the operation; parse and refusal tests (design.md - X9)
- [ ] 3.2 Document `--lb-edges` and the missing-graph case in `docs/CLI_REFERENCE.md`

## 4. Checks

- [ ] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
- [ ] 4.2 Live (ignored) tests with `OPAI_LIVE_DIR` and the installed graph: a matte on the portrait photograph's hair band has strands (alpha between 0.1 and 0.9 on at least 5 percent of the unknown band); record Core ML and CPU times per tile, and FP16 against FP32 if tried, in this design
- [ ] 4.3 Release-build the CLI and run the three test photographs at f/1.4 with `--lb-edges matte` and `depth` (outputs and 100 percent crop strips to `~/dev/opai-notes/lens-blur/out/` only); record total times and peak RSS; write the hand checks into `opai-notes/TESTING.md`
- [ ] 4.4 After Joseph's checks, record the results and any fixes in design.md; note Windows and Linux as untested
