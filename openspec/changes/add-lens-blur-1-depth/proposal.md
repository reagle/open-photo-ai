> [!NOTE]
> **AI disclosure:** this was prepared with Claude Code (Opus 5.5) at my behest; I tested its application in practice.

## Why

A lens blur (a shallow depth of field added after the fact: what you click on stays sharp, the rest blurs by its
distance, as a wider aperture would have) is one of the features that keeps people on Topaz Photo AI, Lightroom and
Photoshop. It needs one thing no model in the application provides: the distance of every pixel from the camera, in
metres, and the camera's field of view. MoGe-2 ViT-B (Microsoft, MIT; DINOv2 backbone, Apache-2.0) estimates both from a
single photograph, and its author publishes an official ONNX export (`Ruicheng/moge-2-vitb-normal-onnx`).

This slice adds that estimate as an analysis, run once per photograph and cached, exactly as face detection is; the blur
itself, which only re-renders when the focus point or the aperture changes, follows in later slices.

## What Changes

Task estimate: **11**. This is slice 1 of the `add-lens-blur` series (see design.md - Slices). The series is proposed
together because it is implemented in one pass; each slice is still its own change, at most 15 tasks, and leaves the
project working.

- **A new data family, Depth (`dp`), with one model, Lisbon** (MoGe-2 ViT-B). Like Detection it is never placed in a
  chain; its result is not an image. `Family::ALL` gains it beside Detection, and the rule that `APPLY_ORDER` is `ALL`
  without the families that are never applied is kept, now for two of them.
- **Lisbon's graphs** are MoGe-2 ViT-B re-exported at a fixed size with its token count fixed (3600), its shape arithmetic folded and its edge-mode `Pad`s rewritten as `Slice` + `Concat`, so Core ML takes 1041 of its 1043 nodes in 3 partitions; FP16. Two graphs, landscape (1280x1024) and portrait (1024x1280): `dp_lisbon_landscape_fp16` and `dp_lisbon_portrait_fp16`. **A photograph is stretched to fill the graph of its orientation** (not letterboxed: measured on the three test photographs, stretching kept the metric scale within 1 to 3% of the dynamic graph at the photograph's own aspect, where letterboxing with replicated edges was 8 to 14% off on the two that letterbox at all), so there is no padding to cut from the result.
- **`DepthMap`**, the result: metric depth in metres at the graph's resolution resampled (in disparity) back to the photograph's aspect, 1280 on the long side and never larger than the photograph (pixels the model marks invalid, such as sky, at the farthest finite depth), plus the 35 mm-equivalent focal length MoGe-2's points imply. The affine-invariant point map is turned into metric depth as MoGe-2's own `recover_focal_shift` does: the shift along z, and the focal length, by least squares on a 64x64 subsample of the valid points, on the graph's own image plane, then the graph's metric scale.
- **The analysis carrier names it, and the run path is generic over it.** `Analysis` gains a `Depth` arm so progress, required artifacts and the cache name a depth run as they name a detection; the data-model contract is implemented by the typed `Depth` operation itself (its output is a `DepthMap`, a detection's stays `Faces`), and `Opai::estimate_depth` is the public entry, next to `Opai::execute`, whose signature does not change.
- **Cached like a detection**: under the picture's identity and the operation's tag, as a JSON value (the depth quantised to 16-bit disparity: 7.5 MB for the largest map, encoded in 8 ms and decoded in 10 ms, so the binary alternative was not needed). Like a detection's, its key carries no version of the recovery maths or the graphs; the 24 h TTL bounds a stale map.
- **The sealed data-model contract is told the picture's dimensions** (`DataModel::pipeline(&self, dimensions)`), because every session a model names is opened and Lisbon has two graphs, of which a run needs the one for its photograph's orientation. Detection ignores it.
- **Two additive errors**: `UnsupportedReason::OtherResult` for a depth `Analysis` handed to `Opai::execute`, and `InferenceError::Unmeasurable` for a photograph with too few valid samples to recover from.

Crates touched: `opai` only (models, the data-run path, the public API). No logic in a front end; nothing user-visible yet. The CLI's exhaustive matches over `Family` gain the arm they need to compile (`cli` events and look, as built, a progress label "Depth" that slice 3 renames), `perf`'s selection skips depth as it skips detection, and the autopilot scope tests list it. The GUI frontend's TypeScript `Family` union does not list `"depth"` yet (the catalogue publishes the row with no `order`, which `lib/enhancements.ts` already skips); slice 4 adds it.

**For the upstream PR text**: `Family` and `Analysis` are not `#[non_exhaustive]`, so adding `Depth` breaks an exhaustive `match` on either in a downstream crate. That is intended, and the PR says so.

`rust-sak` coverage found: `memo` already backs the run cache and stores this value as it stores faces; `crypto` hashes
the key. Nothing is needed from `rust-sak` for this slice. The ONNX Runtime session, EP selection and model install are
this project's own, and are used unchanged.

Not part of this change (fork-only, recorded in design.md - D17): the fork installs Lisbon's graphs locally, through the
`models/.fork-listing.json` mechanism its Kazan work introduced, until upstream decides whether to host them.

## Capabilities

### New Capabilities

- `depth-estimation`: a per-photograph metric depth map and field-of-view estimate, run as a data operation, cached,
  with the Lisbon model.

### Modified Capabilities

None. (`models` and `data-inference` gain a family and a variant through their existing rules; no requirement of theirs
changes.)

## Impact

- `crates/opai/src/models/artifact.rs` (`Family`), `models/operation.rs` (`Analysis`, the data-model contract),
  `models/subject.rs`, `models/catalogue.rs`, `models/build.rs`, `models/mod.rs`; new `models/depth/` (`mod.rs`,
  `variant.rs`, `map.rs`, `recover.rs`, `lisbon/mod.rs`); `inference/execute.rs` (generic entry), `lib.rs`
  (`estimate_depth`, re-exports); `models/test_support.rs`; `tests/public_api.rs`.
- `crates/cli/src/events.rs`, `look.rs` (exhaustive matches only).
- Disk: 2 x 324 MB of graphs, of which only the orientation used is opened; first Core ML compile about 64 s per graph.
