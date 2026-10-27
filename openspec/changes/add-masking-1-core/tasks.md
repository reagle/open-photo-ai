## 1. imaging

- [ ] 1.1 Add `imaging/src/plane.rs` and the `bilinear::Tap` `pub(crate)` fields byte for byte from `add-lens-blur-2-render`, and `pub mod plane;` in `lib.rs` at the same place, as the slice's first commit (design.md - D9); its own tests come with it
- [ ] 1.2 Add `imaging::stroke::painted` (capsule distance to a polyline, hard core, smoothstep edge at least 1 px, max for paint and min of one-minus for erase, per-segment bounding boxes, `rows::for_each_row`) (design.md - D3); tests: a dot's profile, a straight stroke's width, erase undoes paint, painting twice is idempotent, a stroke off the frame touches nothing
- [ ] 1.3 Add `imaging::mix::masked(source, corrected, weight: &Plane)` (bilinear read of the weight when sizes differ, exact copies at 0 and 1, generic over `Channel`) and extend the module comment (design.md - D4); tests: weight 0 gives the source byte for byte, 1 the correction, a half-weight plane mixes proportionally, a quarter-size plane read at full size, 16-bit

## 2. opai

- [ ] 2.1 Add `opai::mask` (`Mask`, `Stroke`, `Base`, serde camelCase with clamping, `is_empty`, `rasterised`, `signature`, `from_image` with Rec. 709 luminance and 16-bit kept) and re-export at the crate root (design.md - D2, D3, D5, D7, D8); tests: the spec's resolution-independence, erase and inversion scenarios, clamping, signature changes with any stroke field and with the base, serde round trip
- [ ] 2.2 Add `ProcessOptions::mask: Option<Arc<Mask>>` with its documentation and `Debug` (signature only), and `identity_masked` beside `identity_after` with the `mask` discriminator (design.md - D5, D8); tests: no collision with a chain key or a data key, an empty mask leaves the identity unchanged
- [ ] 2.3 In `process_with`, after `run`: skip for an absent or empty mask; otherwise rasterise at the source's size, enlarge the source with Lanczos3 when the result's size differs, `mix::masked`, the aspect guard with its warning, the cancellation check, the debug record (design.md - D4, D6, D10, D11)
- [ ] 2.4 Add `inference/process/tests/mask.rs` with the fake backend: the spec's black, white and half-and-half scenarios; a 2x upscale's masked half equals a Lanczos3 enlargement; a second run with one more stroke executes no model and has a new identity; an empty mask's run equals the unmasked run

## 3. CLI

- [ ] 3.1 Add `--mask <FILE>` to `args.rs` (`requires` an enhancement or `--autopilot`, under the same help heading as the enhancements on the fork's `cli-help-headings`); tests for the parse and the refusal (design.md - D7)
- [ ] 3.2 In `run.rs`, read the mask once (`Mask::from_image`), and per photograph check the aspect ratio (refuse that photograph only, with both ratios in the error) and pass it in `ProcessOptions::mask`; tests with in-test images for a matching and a mismatched mask
- [ ] 3.3 Document `--mask` in `docs/CLI_REFERENCE.md` (a Masks section: white and black, aspect rule, the Lanczos enlargement under an upscale) and in the Summary

## 4. Checks

- [ ] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
- [ ] 4.2 Release-build the CLI and run it on a photograph outside the repository with a hand-made half-white mask: sharpen, then 2x upscale; record the blend time, the peak RSS (`/usr/bin/time -l`) and a second run's cache hits in design.md; outputs to `~/dev/opai-notes/masking/out/` only
- [ ] 4.3 After Joseph's checks, record the results and any fixes in design.md; note Windows and Linux as untested
