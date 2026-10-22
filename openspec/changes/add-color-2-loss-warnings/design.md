## Context

See proposal.md - Why. After slice 1 and the `preserve-metadata` series, an export carries the source's ICC profile
(JPEG, PNG, TIFF, WebP), EXIF, XMP and IPTC, and `rust-sak` narrows depth where a codec forces it (`make_compatible`:
JPEG, BMP, GIF and WebP are 8-bit; PNG, AVIF and HEIF take 16 but not float; TIFF takes float). What reaches the user
when something is lost: nothing. `image::save` logs `colour profile not embedded`; a narrowed depth is not logged at
all; a gain map is never seen, because `rust_sak::image::decode_bytes` decodes the primary JPEG and the MPF secondary
image after its `EOI` is never read, and `preserve-metadata-3-xmp` already drops the `hdrgm:` description from the
written XMP so that the file does not claim a gain map it lacks.

The real case: `18-1603-hawk-bird-mt_auburn-cemetery.jpg`, an iPhone-style Ultra HDR JPEG - `APP2` ICC (Display P3),
`APP1` XMP with `hdrgm:Version="1.0"`, `APP2` MPF with two entries (the second of type `0x050000`, Gain Map, 233897
bytes), and an `APP2` `urn:iso:std:iso:ts:21496:-1` version marker.

## Goals / Non-Goals

**Goals:**

- One assessment in `opai` decides, per export, what the written file lost against its source: HDR, bits per channel
  (and float), and the ICC profile. Both front ends show its answer; neither restates the rule.
- HDR is detected from the file's bytes without a decode, in `rust-sak`, for the cases that exist today: JPEG gain
  maps (ISO 21496-1, Ultra HDR/Adobe `hdrgm`, MPF Gain Map), and PQ/HLG declared by CICP or an ICC `cicp` tag.
- The CLI shows every loss in the terminal, in plain and live modes and under `NO_COLOR`, without disturbing the row
  layout, and counts them for a batch. The exit code is unchanged: a loss is a warning, not a failure.

**Non-Goals:**

- Preserving or producing HDR (slice 7, Ultra HDR), embedding profiles in AVIF/HEIF (slice 4), a depth choice (slice
  5). This slice only reports; when those slices land, the assessment stops reporting what they fix.
- The GUI's notice and indicator: slice 3, `add-color-3-loss-warnings-gui`.
- Alpha, EXIF/XMP/IPTC losses (the `preserve-metadata` series logs those), and resolution changes (deliberate).
- AVIF/HEIF gain maps (`tmap` items) and Apple's pre-ISO private gain maps (an MPF secondary with only `apdi:` XMP and
  no Gain Map type): recorded gaps, not detected.

## Slices

The `add-color` series, in order. Each is its own change. Slices 4-8 were numbered 2-6 before the loss warnings were
inserted; they are not yet proposed.

| # | Change | Delivers | Depends on | Status |
|---|---|---|---|---|
| 1 | `add-color-1-icc-profile` | Source ICC profile read at load, carried on `Picture` through crop, enhancement and the run cache, embedded on export (JPEG, PNG, TIFF, WebP) and in GUI previews; `rust-sak` ICC read/embed | - | done (bookmark `add-color-1-icc-profile`) |
| 2 | `add-color-2-loss-warnings` | `rust-sak` `hdr_bytes`; source fidelity (depth, float, HDR, discarded profile) read at load and carried on `Picture`; the per-export loss assessment in `opai`; CLI warning lines and batch count | 1 | **current** |
| 3 | `add-color-3-loss-warnings-gui` | GUI: the export dialog's notice before export and a per-image warning after it, from slice 2's assessment; i18n in every locale | 2 | pending (proposed with this slice) |
| 4 | `add-color-4-avif-heif` | ICC and CICP (`nclx`) colour descriptions embedded in AVIF and HEIF, CICP read at load (AVIF/HEIF `nclx`, PNG `cICP`), 10/12-bit AVIF and HEIF encode; extends `avif-rs`/`heif-rs` and `rust-sak` | 1 | pending |
| 5 | `add-color-5-output-depth` | A chosen export depth (Source / 8 / 16 / 32-bit float) in `opai`, the GUI export dialog (all locales) and a CLI `--depth` flag; a float path out of `imaging`; float TIFF; optional OpenEXR as a new `rust-sak` format | 1 | pending |
| 6 | `add-color-6-working-space` | Colour conversion with `moxcms`: source profile to the models' sRGB working space before inference and back afterwards (or to a chosen output profile), with gamut handling; a "convert to sRGB on export" option; RAW developed to a wide-gamut space | 1, 5 | pending |
| 7 | `add-color-7-jpeg-xl` | JPEG XL decode and encode (8/16/float, ICC and CICP, PQ/HLG tagging) as a new `rust-sak` format and an export format in GUI and CLI | 4, 5 | pending |
| 8 | `add-color-8-ultra-hdr` | Ultra HDR / ISO 21496-1 gain-map JPEG: an input's gain map preserved through crop and upscale, and HDR output from HDR-capable sources | 5, 6 | pending |

Slices 4, 5 and 6 each make some of slice 2's warnings disappear (AVIF/HEIF profiles, depth, HDR); the assessment is
written so that each capability change is one table entry.

## How the work divides

- **`rust-sak`** (`image` feature): `hdr_bytes(bytes) -> Option<Hdr>` and `Hdr { GainMap, Pq, Hlg }`. Codec and
  container knowledge only.
- **`opai`**: `image::Fidelity` read at load and carried by `Picture::derive`; `export::Loss` and `export::losses`
  (what writing a picture as a format loses against its source); `export::write` answers the losses on `Written` and
  logs each one.
- **`cli`**: turns `Written::losses` into terminal lines and a count. Rendering only.
- **`gui`**: compile fix only in this slice (`Written { path, bytes, .. }`).

## `rust-sak` coverage

Uses `image` (`probe_bytes`, `metadata_bytes`, `icc_profile_bytes`, `encode_writer_with`) as they are, and extends it
with **`hdr_bytes`**, because HDR detection is container knowledge any converter needs, and `rust-sak` already owns
the JPEG segment walk (`blocks/jpeg.rs`), the PNG chunk walk (`blocks/png.rs`), the ISO-BMFF `colr` walk
(`container.rs`) and the ICC tag table (`icc.rs`). It reads, in priority order:

1. **JPEG**, the segments before the first scan: an `APP2` `urn:iso:std:iso:ts:21496:-1\0` marker; an `APP2` `MPF\0`
   whose MP Entry table has an entry of type `0x050000` (Gain Map, CIPA DC-007); an `APP1` XMP packet declaring the
   `http://ns.adobe.com/hdr-gain-map/1.0/` namespace. Any one gives `GainMap`.
2. **PNG** `cICP` (transfer byte 16 or 18), **AVIF/HEIF** `nclx` (transfer 16 or 18) through the existing
   `container::declared`, giving `Pq` or `Hlg`.
3. **Any format's ICC profile** with a v4.4 `cicp` tag whose transfer is 16 or 18.

Bounded like the rest of the module: only segments before `SOS`, every offset checked, the MP Entry count capped by
the segment's own length, no allocation proportional to a claimed size. Released as fork tag `26.10.6-meta.4` (on
`26.10.6-meta.3`), adopted by the workspace `[patch]`. No new crate.

## Decisions

### D1. Fidelity is recorded at load, not re-read at export

`Picture` gains `fidelity: Fidelity` (`Copy`, a few bytes beside the `Arc<Metadata>`), set by `load` from the same
buffer it hashes and decodes, kept by `derive`, and computed from the pixels by `Picture::new` (no HDR, no discarded
profile). The export may run long after the file changed, the GUI's crop and the run cache never touch the file again,
and the loader already holds every byte. Bits come from `probe_bytes` (true 10/12 for AVIF/HEIF), falling back to the
decoded pixels; float from the decoded pixels; a developed RAW is 16-bit, no HDR.

### D2. The assessment predicts what `rust-sak` writes, and a test holds it to that

`export::losses(picture, format)` compares the source's fidelity with what writing `picture.pixels()` as `format`
produces: the written depth by `make_compatible`'s table (8 for JPEG, BMP, GIF, WebP; float narrowed to 16 for PNG,
AVIF, HEIF; TIFF as given), the profile by slice 1's `profile::embeddable` and the formats that hold one, HDR always
lost. It runs before the encode, so `image::save` keeps its signature; the write then reconciles it with what was
embedded (D6). (Implementation: the drift test found that AVIF and HEIF are written at 8 bits today - their encoders
take 16-bit pixels but write 8-bit files - so the table has them with JPEG, BMP, GIF and WebP, and the image module
docs and CLI reference that claimed 16 were corrected.) A drift test encodes every format and colour layout through
`rust-sak`, reads the result back with `probe_bytes` and `metadata_bytes`, and fails if the prediction and the file
disagree - the same guard slice 1 uses for `written_in_colour`.

### D3. Losses are values; words are the front end's

`Loss` is an enum (`Hdr { hdr, format }`, `Depth { from, to, format }`, `Profile { profile, format, cause }`) with a
`Display` giving the English line the CLI and the log use, and `Serialize` so slice 3 can translate each kind in the
GUI. `export::write` logs each with `tracing::warn!` (op `export`, the path, the loss) and returns them on `Written`.

### D4. The CLI keeps its rows and adds lines

The row layout is not touched (the sibling `cli-report-enhancements` change extends `ImageEnd::Enhanced`, and a second
change to the same row would conflict). Off a terminal, the image's `⚠️ <source path>: <loss>; <loss>` line is printed
right after its plain row. On one, the live view owns stderr and keeps only five rows, so the same lines are printed
after the view finishes, before the summary - where failures already are. For more than one image a count follows:
`⚠️ 2 of 5 images lost something: 2 HDR, 1 bit depth, 1 colour profile`. Yellow when colour is on, plain under
`NO_COLOR`; not gated by `--verbose`; the exit code ignores them.

### D5. HDR is reported once, as HDR

An Ultra HDR source written as JPEG reports one HDR loss, not also "bit depth": its base image is 8-bit and the gain
map is what made it HDR. A PQ AVIF written as AVIF still reports HDR lost: no CICP is written until slice 4.

### D6. The prediction is reconciled with what was written

Review finding: `losses` runs before the encode, and `image::save` threw away the `Embedded` that `rust-sak`'s
`encode_writer_with` answers, so a block the encoder left out at write time was never reported - a PNG carrying a
16,740,000-byte sRGB profile exported as JPEG lost it silently, since 255 `APP2` segments cannot hold it. The
requirement says the assessment agrees with what is written, so `export::write` now saves through
`image::save_embedding` (crate-private; `save` and `save_blocking` keep their public signatures) and `loss::reconcile`
adds `Profile { cause: NotWritten }` when the picture carried a profile, the file does not hold it, and no `Format` or
`ColourModel` loss already counts it. `NotWritten` is never predicted: only the write can know it. The CLI words it
`<profile> profile not embedded in JPEG: the encoder could not write it`. In the Ultra HDR gain-map change stacked on
this one (`add-color-4-ultra-hdr-gainmap`) the same reconciliation covers the gain map: the HDR loss is assessed from
`Embedded::gain_map`, not from whether a gain map was prepared.

### D7. Plain sRGB is not lost in AVIF or HEIF

Review finding: every sRGB JPEG exported as AVIF or HEIF warned `sRGB IEC61966-2.1 profile not embedded in AVIF`. An
AVIF or HEIF with no profile and an unspecified `nclx` is read as sRGB - by browsers, by the OS viewers, and by our
own load (`Fidelity::coded` treats sRGB as nothing to record) - so leaving a plain sRGB profile out changes nothing a
viewer shows. `profile::is_srgb` recognises the common sRGB profiles by their description (`sRGB IEC61966-2.1`, its
black-scaled variant, `sRGB2014`, `sRGB built-in`, plain `sRGB`) on an `RGB ` profile; `profile::matters_in` makes
both the loss and `image::save`'s log quiet for those in AVIF and HEIF only (BMP and GIF are not read as anything in
particular, so the loss stays). Judged by name, so a mislabelled profile is believed: the cost is one missed warning,
the safer error than warning on every phone photo.

### D8. A namespace declaration goes wherever it was made

Review finding against the `preserve-metadata` XMP rewrite this slice relies on: removing the `hdrgm:` properties
dropped their declaration only from an `rdf:Description`. A packet declaring `xmlns:hdrgm` on `rdf:RDF` or `x:xmpmeta`
kept it, and `rust-sak`'s `hdr_bytes` (which matches the namespace URI anywhere in the packet) then read Open Photo
AI's own SDR output as a gain-map JPEG, so exporting it again warned `HDR gain map dropped`. `Xmp::remove_namespace`
now removes each declaration of the namespace, on any element, that nothing in its scope still uses (a redeclaration
of the prefix below it ends that scope), and recomputes the scope at `rdf:RDF`. It runs only where properties are
removed: the Ultra HDR gain-map change, which keeps `hdrgm:` when the gain map is written, keeps the declaration too,
wherever it is.

## Risks / Trade-offs

- [The prediction drifts from `rust-sak`] → D2's drift test.
- [False positives: an MPF with a preview image, a phone JPEG with leftover `hdrgm` text] → MPF counts only with type
  Gain Map; XMP counts only on the namespace URI being declared. Over-warning is the safer error.
- [False negatives: AVIF `tmap`, Apple private gain maps, ICC v4.4 profiles without `cicp`] → recorded as gaps.
- [Live-mode warnings arrive at the end of the batch] → accepted; they sit with the failures where a user looks.

## Platforms

Validated on macOS (Apple Silicon) with the CPU and CoreML providers, by the test suite and a manual CLI run on the
hawk JPEG, a 16-bit PNG and an 8-bit sRGB JPEG. Windows and Linux are untested but nothing here is platform-specific
beyond the terminal's rendering of `⚠️`.
