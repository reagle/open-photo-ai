## 1. Model

- [x] 1.1 Write `~/dev/opai-notes/remove/export_lima.py` (check the source's sha256, pack `image` and `mask` into one 1x4x512x512 `input` with a `Split`, divide `output` by 255, check against the source on random input) and `install_lima.py` (clone into the models directory, `.manifest.json`, merge an entry into `.fork-listing.json` keeping every other one); run both (design.md - D1)

## 2. opai

- [x] 2.1 Add `Family::Remove` (`rm`, "Remove") first in `ALL` after Detection, so first in `APPLY_ORDER`, with the apply-order rationale; extend every exhaustive match and the family-count tests (design.md - D1, D2)
- [x] 2.2 Add `models/remove/` (`Remove` carrying `Arc<Mask>`, `RemoveVariant::Lima(FloatPrecision)` at FP32 only, `Remove::lima`, `cache_tag` `rm-lima-fp32-m<signature>`, equality and hashing by signature, serde through the mask's wire shape), `Operation::Remove`, `ParameterKind::Mask` and the catalogue row, `ParameterValues::with_mask`, `VariantEntry::build`'s arm; tests: tags, round trip, equality, catalogue (design.md - D3, D8)
- [x] 2.3 Add `models/remove/region.rs`: marks at 0.5, 8-connected components, grown boxes with context, merging of overlapping crops, the capped exact distance, the feather weight, the any-coverage hole plane, bicubic sampling; unit tests for each (design.md - D4, D5, D6)
- [x] 2.4 Add `models/remove/process.rs`: the pipeline over the regions (present with `imaging::present`, the four-plane tensor, `run_graph`, read back, composite at the run's depth), progress and cancellation; tests with a fake backend: nothing marked runs nothing, unmarked pixels unchanged byte for byte at 8 and 16 bits, the hole is what the graph is shown, two regions two runs, a stop (design.md - D4 to D6, D11)
- [x] 2.5 Add `EpProfile::cpu_on_auto`, set by Lima, and open such a model on the CPU under `Auto` in `sessions`; test (design.md - D7)
- [x] 2.6 In `process_with`: drop a Remove with an empty mask before planning; drop faces whose box centre is marked from every face recovery; keep the chain's input after its Remove steps for the limiting mask; tests with the fake backend (design.md - D3, D9, D10)
- [x] 2.7 Add the live check `models::remove::live` (`#[ignore]`, `OPAI_LIVE_DIR`): Lima on a test photograph with a drawn mask, unmarked pixels unchanged, the fill differs from the object
- [x] 2.8 Re-export at the crate root, update `tests/public_api.rs` and `perf` (Remove is not measured)

## 3. CLI

- [x] 3.1 Add `--remove <FILE>` (`--rm`) to `args.rs`, counting as an enhancement; tests for the parse and the refusals (design.md - D13)
- [x] 3.2 In `run.rs`, read the mask once, check each photograph's aspect ratio as `--mask` does, and put a Lima removal first in the chain; the family's label and event name in `look.rs` and `events.rs`; tests
- [x] 3.3 Document `--remove` in `docs/CLI_REFERENCE.md` (a Remove section: white is removed, the 8 and 16 px, the context, the CPU, the soft large fills, the licence) and in the Summary

## 4. Checks

- [x] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p opai` and `cargo test -p cli`
- [x] 4.2 Release-build the CLI and remove the bonsai tag and the Tokyo man (masks from the comparison) on copies of the photographs; compare with the prototype's LaMa results in `~/dev/opai-notes/infill/out/remove-*-1929.jpg`; record CPU and Core ML times and the second run's cache hit here and in `~/dev/opai-notes/remove/README.md`; strips to `~/dev/opai-notes/remove/out/` only
- [ ] 4.3 After Joseph's checks, record the results and any fixes in design.md; note Windows and Linux as untested
