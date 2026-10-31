## 1. imaging

- [x] 1.1 Add `imaging::grain`: `Profile` (eight luma bands x three channels of sigma, correlation), `measured` (3 x 3 residual, median-absolute per band on a stride, correlation within 3 sigma, sparse bands from their neighbours, quantisation floor), `missing`, `added` (salted SplitMix64 normals per output pixel, shared and per-channel draws, [1 2 1] smoothing renormalised, per-pixel deviation interpolated by luma, 8 and 16 bits) (design.md - D3, D4, D9); tests: a measured profile of synthetic signal-dependent noise, a clean image measures nothing, `added` reproduces a target profile within 5 percent, the same coordinates draw the same values, 16-bit

## 2. opai

- [x] 2.1 Add `opai::Grain` (bounded 0 to 1, quantised, serde) and `Restore.grain` with `#[serde(default)]`, `with_grain`, `grain()`, `params()`, and the cache tag's `-g<grain>v<GRAIN_VERSION>` only above 0 (D1, D7); tests: tag unchanged at 0, a Restore deserialised without `grain`
- [x] 2.2 Add `models/restore/grain.rs`: `Grained` around the resampled restorer (measure the unresampled input, run, measure the output, add the share), skipped at 0 (D2, D5, D6, D8); tests with the fake backend: grain 0 equals today, a clean fake source adds nothing, a noisy source's residual is matched within 10 percent at 1x and 15 percent at 2x, determinism, the gain map untouched (the base has no gain-map plumbing in the pipeline: `Grained` returns only the base pixels it was handed; see design.md Results)
- [x] 2.3 Publish `grain` in the catalogue's Restore row and carry it in `ParameterValues` (`with_grain`, the by-name setter, the build); update `tests/public_api.rs`

## 3. CLI

- [x] 3.1 Add `--rs-grain <0-100>` (`requires = "restore"`) and pass it into the chain; parse tests; `docs/CLI_REFERENCE.md`

## 4. GUI

- [x] 4.1 Add the Grain slider and hint to `RestoreOptions.tsx` from the published range; tests
- [x] 4.2 `enhancements.restore.grain` and `grainHint` in all 14 locales (parity test)

## 5. Checks

- [x] 5.1 `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `TZ=UTC cargo test -p opai --release -j 6`, cli and gui tests, frontend checks
- [x] 5.2 Release CLI on one or two AIM-500 photographs (`/Volumes/opai-bench/bench/data/`), Kazan 1x with grain 0, 50 and 100 (and 2x at 100), outputs under `/Volumes/opai-bench/overnight/restore-grain/`; record timings and the measured residuals in design.md
- [ ] 5.3 Remerge `integration`; Joseph's hands-on check in the GUI
