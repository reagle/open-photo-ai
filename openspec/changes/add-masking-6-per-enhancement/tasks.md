## 1. Library (slice 6.1, `add-masking-6-per-enhancement-1-core`)

- [ ] 1.1 Make the base: a local merge of `add-masking-3-lens-blur` and `fix-lens-blur-trimap-7-refine` (as `integration` holds them), resolutions recorded in `opai-notes/masking/merge-step-masks.md`
- [ ] 1.2 Add `Family::takes_step_mask` (Denoise, Colorization, Light Adjustment, Color Balance, Sharpen, Restore) and `StepMasks` (a `BTreeMap<Family, Arc<Mask>>`, empty masks dropped, `Debug` by signature) with `ProcessOptions::step_masks` and its documentation (design.md - D1, D3); tests for the dropping and the order
- [ ] 1.3 Refuse in `plan` a step mask on a family that takes none, on a Restore above 1x, and on a maskable pipeline that rewrites the gain map (D1, D8, D14); tests for each refusal
- [ ] 1.4 Add `identity_after_masked` beside `identity_after` (an extra `identity_masked` fold after each masked step) and use it for every step key and the result's identity; tests: no step masks gives today's keys, a mask changes the masked step's key and every later one and no earlier one (D5)
- [ ] 1.5 Blend in `run` after each masked step (rasterise once per run at the chain input's size, read bilinearly at the step's size, `mix::masked` off the runtime, cancel check, `debug` record), storing the unmasked result under its own key and the blended one under the masked key; decode the hit before a served masked step when needed (D4, D5)
- [ ] 1.6 Add `Mask::reframed(framing: &Affine, source, framed)` (points through the affine, radius by the ratio of the longer sides, the base resampled) (D7); tests: identity framing is a no-op, a crop moves a dot, a quarter turn and a flip, a point outside rasterises to nothing
- [ ] 1.7 Add `inference/process/tests/step_mask.rs` with the fake backend: the spec's subject-and-background, upscale-refused, Restore-after-upscale, both-masks and stroke-on-Denoise scenarios; the faces' keys independent of step masks (D13); the gain map's bytes unchanged (D14)
- [ ] 1.8 Record a masked step in `export/metadata/record.rs` (`, masked` in the entry and the history parameters) (D17); tests

## 2. CLI (slice 6.2, `add-masking-6-per-enhancement-2-cli`)

- [ ] 2.1 Add `--dn-mask`, `--cl-mask`, `--la-mask`, `--cb-mask`, `--sh-mask`, `--rs-mask`, `--gr-mask` to `args.rs` (`requires` the family, under its heading, `--rs-mask` refused with `--rs-scale` above 1, listed among the flags `--gui` ignores) (D15); parse tests
- [ ] 2.2 In `run.rs`, read each once, check the aspect per photograph (refuse that photograph only), fill `step_masks`; with `--autopilot`, warn once per unused family mask (D12); tests with in-test images
- [ ] 2.3 Document the flags in `docs/CLI_REFERENCE.md` (the Masks section: one mask per enhancement, how it combines with `--mask`, what a mask edit costs, soften a hard file first)
- [ ] 2.4 Release CLI on two bench photographs (AIM-500, outputs under `/Volumes/opai-bench/overnight/step-masks/`): denoise background + sharpen subject; the same with a Kazan restore after; record times and a rerun's cache hits in design.md

## 3. GUI (slice 6.3, `add-masking-6-per-enhancement-3-gui`)

- [ ] 3.1 Rust: `Requested.scope: Option<Mask>`, moved into `step_masks` after `resolve`, reframed through the crop (D3, D7); `mask_preview` with an optional framing; tests
- [ ] 3.2 Store: `Target` gains `{ step: Family }`, `stepMasks` per path and family with history, kept on a crop change, forgotten with the row or the file, hidden (not forgotten) when Autopilot leaves the family out (D12, D16); tests
- [ ] 3.3 `EnhancementRow`: Paint mask / Edit mask and the badge on maskable rows, Refine in that place on Lens Blur, Restore's button disabled above 1x; tests
- [ ] 3.4 Panel and overlay: title, the step-mask tint token, the overlay of the row edited only, the slow-step hint; the limiting mask's section relabelled (D16); tests
- [ ] 3.5 Wire: `withScopes` on the way to `enhance`, `export` and the batch; the 300 ms settle as for the removal (D6); tests that `keyOf` changes with a step mask
- [ ] 3.6 i18n keys in all 14 locales (parity test)
- [ ] 3.7 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `TZ=UTC cargo test --workspace`, frontend checks; remerge `integration`; Joseph's hands-on check (sharpen subject + denoise background, crop after painting, export record), results recorded in design.md
