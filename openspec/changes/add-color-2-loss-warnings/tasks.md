## 1. `rust-sak`: detect HDR (in `../rust-sak`)

- [x] 1.1 In `src/image/`, add `hdr.rs` with `hdr_bytes(bytes) -> Option<Hdr>` and `Hdr { GainMap, Pq, Hlg }` (`#[non_exhaustive]`): JPEG ISO 21496-1 `APP2`, MPF Gain Map entry and `hdrgm` XMP namespace before `SOS`; PNG `cICP`; AVIF/HEIF `nclx` through `container::declared`; ICC `cicp` tag (design.md - rust-sak coverage); add tests for each source, for SDR files of every format, and truncations and byte flips that must not panic; verify `cargo test --features image` is green
- [x] 1.2 Document `hdr_bytes` in `src/image/README.md` and the changelog, run `cargo clippy --all-features -- -D warnings` and `cargo test --all-features`, commit on `metadata`, tag `26.10.6-meta.4` and push the bookmark and tag to the `reagle/rust-sak` fork (not upstream)

## 2. `opai`: fidelity and the loss assessment

- [x] 2.1 Move the workspace `[patch]` to `26.10.6-meta.4` and verify `cargo build --workspace` is green with no other change
- [x] 2.2 Add `image::Fidelity` (bits, float, HDR, discarded profile) in `image/fidelity.rs`; read it in `load.rs` from the loaded bytes (`probe_bytes`, `hdr_bytes`, the profile `fitting` drops) and from the pixels for RAW; carry it on `Picture` (`fidelity()`, `with_fidelity`, kept by `derive`, from the pixels in `new`) (design.md - D1); add tests for an Ultra HDR JPEG, a 16-bit PNG, the DNG fixture and a derived picture; verify `cargo test -p opai image` is green
- [x] 2.3 Add `export::Loss` and `export::losses(picture, format)` in `export/loss.rs` (HDR, depth, profile, with `Display` and `Serialize`) (design.md - D2, D3); add unit tests for each kind and for no loss, and the drift test that encodes every format and colour layout through `rust-sak` and checks the prediction against `probe_bytes` and `metadata_bytes`; verify `cargo test -p opai export` is green
- [x] 2.4 Make `export::write` assess before writing, log each loss with `tracing::warn!` and answer them as `Written::losses`; update the GUI's destructuring of `Written`; add a test that writing the Ultra HDR fixture as JPEG reports exactly the HDR loss; verify `cargo test -p opai export::write` is green
- [x] 2.5 Update `image/mod.rs` and `export/mod.rs` module docs with what is recorded and assessed, and the recorded gaps (AVIF `tmap`, Apple private gain maps); verify `cargo doc -p opai --no-deps` builds without warnings
- [x] 2.6 Review fix: answer `Embedded` from the save to `export::write` (`image::save_embedding`) and reconcile the prediction with it (`loss::reconcile`, `ProfileCause::NotWritten`) (design.md - D6); add a test that a 16,740,000-byte sRGB profile in a PNG written as JPEG is reported and as PNG is not; verify `cargo test -p opai export` is green
- [x] 2.7 Review fix: report no profile loss for plain sRGB in AVIF or HEIF (`profile::is_srgb`, `matters_in`), in the assessment and `image::save`'s log (design.md - D7); add tests for the recognised names, a lookalike, a grayscale profile, and Display P3 still lost; verify `cargo test -p opai` is green
- [x] 2.8 Review fix: remove an unused namespace declaration from any element, not only a description, once its properties are removed (design.md - D8); add tests with the declaration on `rdf:RDF` and `x:xmpmeta`, with a redeclared prefix, and (in the Ultra HDR change) with a written gain map that keeps it; rerun the reviewer's repro of an export exported again; verify `cargo test -p opai xmp` is green

## 3. `cli`: show the losses

- [x] 3.1 Return the losses from `output::write`, print each affected image's `⚠️` line after its plain row off a terminal and after the live view on one, through a new `look::loss_line` (design.md - D4); add tests for the line's text with and without colour and for both modes; verify `cargo test -p cli` is green
- [x] 3.2 Add `look::loss_count` and print it for a batch of more than one image with any loss; add tests; state in `docs/CLI_REFERENCE.md` (Output formats) what is warned about and that the exit status is unchanged; verify `cargo test -p cli every_example_in_the_reference_parses` is green

## 4. Checks

- [x] 4.1 Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; run the CLI on the hawk JPEG (`-q sd --dn`, to JPEG and AVIF), a 16-bit PNG to JPEG and an 8-bit sRGB JPEG to JPEG, and confirm the warnings and their absence
