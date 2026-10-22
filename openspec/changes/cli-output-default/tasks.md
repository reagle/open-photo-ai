## 1. The flag

- [x] 1.1 In `crates/cli/src/args.rs`, make `output` an `Option<PathBuf>`, change `override_usage` to `opai <INPUT>... [OPTIONS] [--output <OUTPUT>]`, and rewrite the `-o` help and the command's long about to say that without it each result is written beside its source with the suffix `_opai` unless `--prefix` or `--suffix` is given; replace `the_output_is_required` with a test that `opai photo.jpg --ap` parses with `output == None`, and add one that `--suffix ""` parses as `Some("")` (design.md - D1, D2 risk); verify `cargo test -p cli args` is green

## 2. Where the results go

- [x] 2.1 In `crates/cli/src/output.rs`, add `Target::Beside { naming }`, returned by `target()` when `output` is `None`, with the suffix defaulting to `_opai` only when neither `--prefix` nor `--suffix` is given (design.md - D2); map it in `destination()` to `export::destination_for(&source.path, naming, None)` and pass `create_parent: false` in `write()` (D4); update the module diagram; add tests: no `-o` gives `Beside` with suffix `_opai`, `--suffix -big` / `--prefix new-` / `--suffix ""` each replace the default, `--format webp` is accepted, a named `pictures/beach.jpg` and a found `photos/2024/b.jpg` map to `pictures/beach_opai.jpg` and `photos/2024/b_opai.jpg`, and a bare `photo.jpg` maps to `photo_opai.jpg`; verify `cargo test -p cli output` is green
- [x] 2.2 Add a test in `output.rs` writing through `write()` into a temporary directory: an existing `photo_opai.jpg` is kept and the result is `photo_opai_1.jpg`; with `overwrite` the earlier `photo_opai.jpg` is replaced and the source is untouched; verify `cargo test -p cli output` is green

- [x] 2.3 In `crates/cli/src/output.rs`, add `keep_inputs` (canonicalised inputs held on `Target::Beside`) and `may_overwrite`, and in `run.rs` withhold `--overwrite` with a log warning where a destination is another input (design.md - D5); add tests for both input orders, a `-r` re-run over earlier `_opai` results, an image's own source, and that `-o` keeps no inputs; verify `cargo test -p cli output` is green

## 3. Wiring and the summary

- [x] 3.1 In `crates/cli/src/look.rs`, change `summary(written, output)` to take `Option<&Path>` and, with `None` and more than one file, read `✨ <count> images enhanced successfully; files saved beside their sources` (design.md - D3); in `crates/cli/src/run.rs`, pass `args.output.as_deref()`, make `skip` `None` for `Beside`, and have `format_name` report `Beside`'s naming format as `Directory` does (D4); update `run.rs`'s and `main.rs`'s module docs; add exact-text tests for both summary forms; verify `cargo test -p cli` is green

## 4. Documentation

- [x] 4.1 In `docs/CLI_REFERENCE.md`: the synopsis shows `[--output <OUTPUT>]`; "The rules in short" says `-o` is optional and results go beside their sources; the `-o` heading drops "(required)"; `--format`/`--prefix`/`--suffix` say "not accepted with an output file" instead of "only with an output directory"; add a "No `-o`: beside each source" subsection under "Where the results go" (placement for named files and directory trees, the `_opai` default and how `--prefix`/`--suffix`/`--suffix ""` change it, numbering and `--overwrite`, a re-run over a directory picking up earlier `_opai` results, read-only folders, the "files saved beside their sources" line); add a quick-start example without `-o` (`opai photo.jpg --ap`); verify `cargo test -p cli every_example_in_the_reference_parses` is green

## 5. Verification

- [x] 5.1 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace --locked`; verify all three are green
- [x] 5.2 Verify by hand on macOS with copies of `fixtures/test.dat` as `photo.jpg` and `photos/a.jpg`, `photos/2024/b.jpg`: `opai --autopilot --quality sd --suffix _opai photo.jpg` writes `./photo_opai.jpg`; `opai photo.jpg --up` writes `photo_opai.jpg`, then `photo_opai_1.jpg` on a second run; `--overwrite` replaces `photo_opai.jpg` and leaves `photo.jpg`; `opai photos -r --up` from the parent writes beside each source, creates no directory and ends with `files saved beside their sources`; every `-o` example in the reference behaves as before. Windows and Linux stay untested by hand, as design.md records
