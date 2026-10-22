## 1. The flag and the description of a chain

- [x] 1.1 In `crates/cli/src/args.rs`, add `verbose: bool` as `--verbose`, long only (`-v` stays `--version`), with help saying it names each written image's enhancements on its line (design.md - D6); add tests that `--verbose` parses alone and with `--ap`, that it defaults to off, and that `-v` still prints the version; verify `cargo test -p cli args` is green
- [x] 1.2 In `crates/cli/src/chain.rs`, add `Applied { family, model: &'static str, detail: Option<String> }` and `Plan.applied` (one per operation, same order), returned by `build` beside the `Operation` from the `VariantEntry::label` it looked up and the value it set: `strength <n>`, `bias <n>`, `<factor>x` written without a trailing `.0`, none for face recovery and colorization (D2); add `fn describe(&[Applied]) -> String` joining `<family> <model>[ <detail>]` with `, `, and `no enhancement needed` for an empty slice; add tests: every family's bare flag, a given strength and a negative bias, `--cb SaoPaulo` reading `São Paulo`, `--scale 1.5` reading `1.5x`, an auto-chosen scale matching the operation's, an autopilot `Selection` worded like the equivalent flags, and a skipped upscale reading `no enhancement needed`; verify `cargo test -p cli chain` is green

## 2. The cache tally

- [x] 2.1 In `crates/cli/src/progress.rs`, make `ImageProgress::report` keep the set of families whose enhancement (not detection) subject reported `Stage::Cached`, without changing the row's words or fraction, and expose the count (D4); add a function giving the marker for `k` of `n` (` (cached)`, ` (<k> of <n> cached)`, nothing); add tests: a cached upscale after a running denoise counts one, a cached detection counts none, and the three marker forms; verify `cargo test -p cli progress` is green

## 3. The ended row

- [x] 3.1 In `crates/cli/src/look.rs`, change `ImageEnd::Enhanced` to carry `Option<String>`, and make `row` and `plain_row` append `: <text>` after the path for a written image with a text, in the terminal's default colour without style, uncoloured under `NO_COLOR` and in plain mode (D3); `Enhanced(None)` keeps today's exact row; keep failed and stopped rows unchanged; extend the alignment test so paths still start in one column, and add exact-text tests for the coloured, uncoloured and plain forms, including `no enhancement needed`, a `(cached)` tail and `None`; verify `cargo test -p cli look` is green
- [x] 3.2 In `crates/cli/src/progress.rs`, store the text on `ImageProgress` when `end_line` receives `Enhanced(Some(text))` so the live line redraws it, and return the plain line with it off a terminal; add tests that a live ended row and a plain line carry the text whole, that `Enhanced(None)` gives today's line, and that a failed image's line carries only its reason; verify `cargo test -p cli progress` is green

## 4. Wiring

- [x] 4.1 In `crates/cli/src/run.rs`, after the write, build the text from `plan.applied` and the image's cache tally, forcing ` (cached)` when the verdict is `NothingExecuted` and the chain is not empty (D4), pass it as `ImageEnd::Enhanced(args.verbose.then_some(text))` (D6), and add `; applied <text>` to the `wrote …` log record whatever the flag (D5); update the module diagram; add tests for the forced marker and the log text over hand-built plans; verify `cargo test -p cli run` is green and `cargo doc -p cli --no-deps` builds without warnings

## 5. Documentation

- [x] 5.1 In `docs/CLI_REFERENCE.md`: add `--verbose` to the options (no short form; `-v` is the version), and in "Terminal output" describe its `: <applied>` tail with an example (family, model, `strength`/`bias`/`<factor>x`, `(cached)`, `(<k> of <n> cached)`, `no enhancement needed`), that it is cut at the terminal's edge in the live view but whole off a terminal and in the log, that failed rows carry none, and that without the flag the rows are unchanged; in "Autopilot", say `--verbose` shows each image's choice, with an example, and that the log records it either way; in "Run cache", say a re-run's row ends `(cached)` under `--verbose`; verify `cargo test -p cli every_example_in_the_reference_parses` is green

## 6. Verification

- [x] 6.1 Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace --locked`; verify all three are green
- [x] 6.2 Verify by hand on macOS with a copy of a photograph: without `--verbose` every row is as before; `opai photo.jpg --dn --la -o out.png --verbose` ends `: Denoise Stockholm strength 100, Light Adjust Paris bias 50`; the same command again ends `(cached)`; adding `--sh` ends `(2 of 3 cached)`; `--ap --verbose` shows the image's own choice; `2>log.txt` and `NO_COLOR=1` give the same text uncoloured; the log's `wrote` record carries `applied` without the flag. Windows and Linux stay untested by hand, as design.md records
