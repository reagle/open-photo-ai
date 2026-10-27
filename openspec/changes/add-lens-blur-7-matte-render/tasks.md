## 1. Shared primitives

- [ ] 1.1 Move `guided.rs`'s `box_mean` and `reduced` to a new `imaging::boxes` with their tests, `guided` calling them from there, as the slice's first commit (design.md - M7); `guided`'s tests unchanged and green
- [ ] 1.2 Add `imaging::morph::{eroded, dilated}` (separable square, running min and max) (design.md - M4); tests: against a brute-force min and max for radii 1 to 9, a radius larger than the plane, idempotence of a second erosion by 0
- [ ] 1.3 Add `imaging::fill::filled` (push-pull pyramid) (design.md - M2); tests: the spec's gradient scenario, weight-1 values unchanged, a hole touching the frame edge, all-zero weight returns a constant, multi-channel
- [ ] 1.4 Add `imaging::fusion::foreground` (two-pass blur fusion, running-sum box filters, rows in parallel) (design.md - M3); tests: the spec's red-over-green scenario, alpha 1 returns the photograph, alpha 0 pixels are left at the input

## 2. The render

- [ ] 2.1 Add `Spacing` and `Defocus::spacing` (default `Even`), with geometric layer radii on each side of 0 (design.md - M6); tests: the default reproduces slice 2's layer radii exactly, the spec's geometric scenario, a range entirely on one side of focus
- [ ] 2.2 Add `Subject` and the `subject` argument to `defocus`: one premultiplied layer composited before the first nearer layer, convolved by its own disc when its radius is 0.5 px or more, alpha read bilinearly at any size (design.md - M1); update `models/lens_blur/render.rs` to pass `None`
- [ ] 2.3 Tests for the subject: the spec's half-covered edge, no subject is bit-identical to before (a stored checksum of a fixed render), a nearer layer covers the subject, a subject at a radius between two layers, a blurred subject keeps its total light
- [ ] 2.4 Extend the ignored timing test with a 24 MP render with a full-frame subject, fill and fusion, and geometric spacing over -400..+150 px; record the times in this design

## 3. Docs

- [ ] 3.1 Update `imaging/src/lib.rs`'s module documentation (the matte half of the lens blur) and `defocus.rs`'s module comment (the subject step), in upstream's comment density

## 4. Checks

- [ ] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo test -p imaging --release -- --ignored defocus_timing`
