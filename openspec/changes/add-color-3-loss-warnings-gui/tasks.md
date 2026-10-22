## 1. `opai` and the commands

- [x] 1.1 Add `export::predict_blocking(path, format)` in `export/loss.rs`, reading the file's fidelity and profile through `Fidelity::read` without decoding pixels (design.md - D1); add tests that it agrees with `losses` on a loaded picture for the Ultra HDR fixture, a 16-bit PNG and an sRGB JPEG, and that an unreadable path is an error; verify `cargo test -p opai export` is green
- [x] 1.2 Add the `export_losses(paths, naming)` command in `crates/gui/src/export/` on a blocking thread, answering `{ path, losses }` per path, and register it in `lib.rs` (design.md - D2); add a test with a 16-bit PNG and an sRGB JPEG; verify `cargo test -p gui export` is green
- [x] 1.3 Add `losses` to `Exported::Exported` from `Written::losses`; update the `export_with` tests; verify `cargo test -p gui export` is green

## 2. Frontend

- [x] 2.1 In `ipc/export.ts`, add the `Loss` type, `exportLosses` and `losses` on `Exported`; add the command name to `export.test.ts`'s pins; add `formatLoss(t, loss)` in `lib/`; verify `pnpm test ipc/export` is green
- [x] 2.2 Keep the losses on a done row in the export batch store, and add `useExportLosses(paths, format)` keyed on the paths and the format choice; add tests; verify `pnpm test features/export stores` is green
- [x] 2.3 Show the warning icon with a tooltip on `ExportRow` (prediction before writing, the result after) and the notice in `ExportDialog`; add component tests for an affected row, an unaffected one, a written one and the notice; verify `pnpm test features/export` is green
- [x] 2.4 Add the `export.loss.*` and notice keys to all 14 locale files without plural forms (design.md - D3); verify `pnpm test i18n` is green
- [x] 2.5 Review fix: read only a JPEG's or PNG's header in `predict_blocking` (`loss::header`), correct its doc to a lower bound, and keep one question in flight in `useExportLosses`, sending only the latest waiting one (design.md - D1, D4); add tests that the bounded and whole-file predictions agree and that the header is a fraction of a large file, and hook tests for a superseded, a failed and an unmounted question; verify `cargo test -p opai export` and `pnpm test features/export` are green
- [x] 2.6 Review fix: word `ProfileCause::NotWritten` (`export.loss.profileNotWritten`) in all 14 locales and add `notWritten` to the `Loss` type (design.md - D5); add a `formatLoss` test; verify `pnpm test lib i18n` is green

## 3. Checks

- [x] 3.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm check`, `pnpm typecheck`, `pnpm test` and `pnpm build`; record that the dialog was not checked in a real window
