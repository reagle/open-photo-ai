## 1. imaging

- [x] 1.1 Keep `imaging::grain`'s `Profile`, `measured` and the coordinate hash; add `lacking` (per 32 x 32 tile of the result, the share of the reference's deviation it lacks at each pixel's luminance, median of the normalised residual), `added_where` (the deviation times the lack, bilinear between tile centres), `sensor` (a Poisson-Gaussian sensor through the sRGB curve, normalised at mid-grey) and `Profile::floored`; drop the whole-picture `missing` (design.md D3, D4); tests: a clean result lacks all of it, the reference lacks little, grain back on the half that lost it and not on the half that kept it, the lack read between tile centres, the sensor shape

## 2. opai

- [x] 2.1 `Family::Grain` (`gr`, last in `APPLY_ORDER`), `models/grain/` (`Grain`, `GrainAmount`, `GrainMode`, `GrainVariant::Sensor`, the pipeline), `Operation::Grain`, the catalogue row (`amount` 0-1 default 1, `match` 0-1 default 1), `ParameterValues::with_amount` / `with_grain_mode` and the by-name setter; Autopilot never suggests it (D1, D2, D5)
- [x] 2.2 `ImagePipeline::measures_against_photograph` / `run_against`; the chain driver keeps the photograph (the input, or the last denoise's result, decoded from the store only when the grain is computed; a stored denoise that will not decode is run again) (D6)
- [x] 2.3 Remove Restore's grain: `Restore.grain`, `with_grain`, the `-g` tag, `models/restore/grain.rs`
- [x] 2.4 Tests: matched within 10 percent at 1x and 15 percent at 2x, half the amount, a result that kept its grain, a clean photograph, determinism, add mode, no session; chain tests with a smoothing fake: grain after a smoothing step, after a denoise (computed and stored, the stored one corrupt), opens no session

## 3. CLI

- [x] 3.1 `--grain` (`--gr`) `[MODEL]`, `--gr-amount <0-100>`, `--gr-add`; refused with `--autopilot`; `--rs-grain` removed; parse and plan tests; `docs/CLI_REFERENCE.md`

## 4. GUI

- [x] 4.1 Grain in `ENHANCEMENTS` (manual, `Grip` icon), `GrainOptions.tsx` (model tray, Amount, Match source switch and its line), the row's info line; the Restore panel's slider removed; tests
- [x] 4.2 `enhancements.grain.*`, `enhancements.infoGrain` and `preview.progress.grain` in all 14 locales

## 5. Integration (merge-only)

- [x] 5.1 The limiting mask splits before the trailing lens blurs and grain; the head hands the tail its photograph (`Ran::photograph`, `Steps::measured_after`); a grain leaves the gain map shared; the edit record lists it (`algorithmicallyEnhanced`); the CLI's applied line; `--gui` ignores `--grain`; Grain in the add menu's Effects group (D7, D8)

## 6. Checks

- [x] 6.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `TZ=UTC cargo test -p opai -p gui -p cli --release`, frontend tsc, biome and vitest, on the branch and on `integration`
- [x] 6.2 Release CLI on two AIM-500 photographs with synthetic noise, through Restore, Kyoto 2x, Lens Blur and Denoise, with and without Grain; outputs and a crop sheet under `/Volumes/opai-bench/overnight/grain/`; results in design.md
- [ ] 6.3 Joseph's hands-on check in the GUI
