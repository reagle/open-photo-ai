## 1. Base

- [ ] 1.1 Create the merge (`jj new add-masking-2-brush add-lens-blur-4-gui`), resolve the conflicts listed in proposal.md - Impact so that `cargo test --workspace` and `pnpm test` pass on the merge, and record the resolutions in `opai-notes/masking/merge-masking-lens-blur.md`

## 2. opai

- [ ] 2.1 Add `limits_enhancements` and `keeps_sharp` to `Mask` (serde default `true`, in the signature) and the rule that a use with no effect on the stack is ignored, identity included (design.md - L1); tests for the table's six cells
- [ ] 2.2 Split `process_with` at a trailing lens blur: run the head, blend when `limits_enhancements`, run the tail over the blended picture with its own `RunCache`, continuous progress indices (design.md - L2); tests with the fake backend: the spec's denoise-and-lens-blur scenarios, a mask edit re-runs neither depth nor the head, progress indices for a three-step chain
- [ ] 2.3 Inject the rasterised mask into the lens blur with `with_keep_sharp(weight, signature)` when `keeps_sharp`, and add `-k{signature:016x}` to `LensBlur::cache_tag` (design.md - L3); tests: the tag changes with the mask and not without one, a kept-sharp region renders at radius 0 through the pipeline

## 3. CLI

- [ ] 3.1 Add `--mask-use <both|enhancements|lens-blur>` (requires `--mask`; `lens-blur` refused without `--lens-blur`) and map it onto the two fields; parse and refusal tests (design.md - L5)
- [ ] 3.2 Document `--mask-use` and the subject-in-focus meaning in `docs/CLI_REFERENCE.md`

## 4. GUI

- [ ] 4.1 Add `limitsEnhancements` and `keepsSharp` to `ipc/mask.ts` and `stores/mask.ts` (undoable), and the two switches in `MaskPanel.tsx`, shown with a lens blur in the stack, the first hidden when the lens blur is alone (design.md - L6); tests for both visibility rules and that a switch re-runs the preview
- [ ] 4.2 Add `mask.panel.limits`, `mask.panel.keepsSharp` and their tooltips to `en.json` and the other 13 locales; verify the parity test

## 5. Checks

- [ ] 5.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `crates/gui` `pnpm check`, `pnpm typecheck` and `pnpm test`; release-build the CLI and run the spec's focus-only scenario on the three test photographs (outputs to `~/dev/opai-notes/masking/out/` only); write the hand checks into `opai-notes/TESTING.md`
