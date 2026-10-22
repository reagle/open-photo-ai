## 1. `rust-sak` (fork, in `~/dev/rust-sak-ws/uhdr`)

- [ ] 1.1 Add `image::jpeg_quality(bytes) -> Option<JpegQuality>` over `blocks/jpeg.rs` `segments()` (design.md - D1): DQT parsing at 8 and 16 bits, IJG table matching, nearest-quality fallback; tests with JPEGs encoded by `JpegEncoder` at 10, 50, 75, 90, 100 (exact), a hand-built non-IJG table (approximate), a PNG and a truncated file (`None`); verify `cargo test --features image jpeg_quality` is green
- [ ] 1.2 Document it in `src/image/README.md`, run `cargo fmt --check`, `cargo clippy --all-features` and the full tests, then (after Joseph's go-ahead) tag `26.10.6-meta.7` and push to the `reagle/rust-sak` fork; verify the tag is on the fork

## 2. `opai`

- [ ] 2.1 Move the workspace `[patch]` for `rust-sak` to `26.10.6-meta.7`; verify `cargo build --workspace` resolves a single `rust-sak` in `Cargo.lock`
- [ ] 2.2 Record the estimate when loading a JPEG and carry it through derived pictures (D2); add tests for a JPEG, a PNG and a cropped JPEG; verify `cargo test -p opai` is green
- [ ] 2.3 Replace `WriteOptions.quality` with `EncodeQuality { mode, fixed }`, resolve it in `export::write`, and add `Written.quality` and its source, logged (D2, D3); add tests for each row of the gui-export scenarios and that an Ultra HDR gain map is still written at 90 for a quality-70 source; verify `cargo test -p opai export` is green

## 3. CLI

- [ ] 3.1 Add `--output-quality <1-100|source>` to `crates/cli/src/args.rs` and pass it in `output.rs` (D4), and show the quality used in the `-v` report; add parsing tests (number, `source`, 0, 101, a word) and an end-to-end test on a quality-85 JPEG fixture; verify `cargo test -p cli` is green

## 4. GUI

- [ ] 4.1 Pass the mode through the `export` command and its TypeScript binding (`crates/gui/src/export`, `frontend/ipc/export.ts`) and return the quality used; add a Rust test and an `ipc/export.test.ts` case; verify `cargo test -p gui export` and `pnpm test ipc/export` are green
- [ ] 4.2 Add `qualityMode` (Fixed / Match source) to `frontend/stores/settings.ts` with its repair, a select on `features/settings/ExportPage.tsx` and in `features/export/ExportSettingsPanel.tsx`, and the used quality on `ExportRow.tsx` (D5); add tests for persistence, repair of an unknown value, the fallback label and the row text; verify `pnpm test features/settings features/export stores/settings` is green
- [ ] 4.3 Add the i18n keys (mode label, two options, fallback note, row text) to all 14 locale files; verify `pnpm test i18n` (parity) is green

## 5. Verification

- [ ] 5.1 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; in `crates/gui` run `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`; verify all are green
- [ ] 5.2 Verify by hand: export the same JPEG (checked with `magick identify -format %Q`) as JPEG in both modes and compare the reported quality and size; a PNG in Match source uses the slider; an Ultra HDR JPEG keeps its gain map; the CLI with `--output-quality source`; record the platform tried
- [ ] 5.3 After implementation run `cargo clean` in the workspace used, and add the cases to `~/dev/opai-notes/TESTING.md`; verify the checklist lists them
