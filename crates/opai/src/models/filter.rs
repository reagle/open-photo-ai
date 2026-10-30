//! The contract every image filter runs: the photograph through a tile grid at its own resolution, then moved towards
//! the result by the run's strength.

// Cross-family, because denoise and sharpen both run it unchanged: the reference's `sharpen/process.go` and
// `sharpen/variant.go` are its denoise files line for line with the prefix changed. What stays with each family is its
// range and the measured profiles. What stays with each model is whether it is guarded and anchored, at what threshold
// and radius, and the prose behind each: a model retrained upstream drops what it no longer needs in its own file.
//
// What a run is:
//
//   tile        the whole photograph through `run_tiled` at the default 256/16 geometry, scale 1, in the caller's range
//   guard       per tile, where the variant has one: a diverged output is retried where the model has a rescue, and a
//               tile still diverged keeps its own input
//   anchor      per tile, where the model declares one: what the model changed at scales coarser than a box is taken
//               back out, so every tile keeps its input's local brightness and neighbouring tiles agree at the seam;
//               at a small enough box, also the coarse part of what the model changed inside the tile
//   blend       the photograph moved towards — or past — the tiled result by the run's strength
//
// **Only the guard and the anchor are this file's own.** The grid is `imaging`'s and the blend is `imaging::mix`. The
// reference's `RunPipeline` then `BlendWithIntensity`, in that order; it has no anchor.
//
// The range and the strength arrive from the caller rather than as one family's `RANGE` and `*Params`: both families
// read `[0, 1]` because each of the reference's two `process.go` files passes `standardize: false`, and a shared file
// that hard-coded it would be right for two families by coincidence.

use std::sync::Arc;

use image::DynamicImage;

use crate::error::InferenceError;
use crate::models::ArtifactId;
use crate::models::strength::Strength;
use crate::pipeline::Backend;
use crate::pipeline::{ImagePipeline, OnOneGraph, Shared, SingleGraph, reporter};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;
use imaging::boxes::box_mean;
use imaging::mix::blended;
use imaging::tensor::{Channel, Normalisation, Sampler};
use imaging::{ChannelDepth, RunTile, TileGeometry, run_tiled};

// A fixed share rather than one fitted to a measured split, for the reason `light_adjustment::process::STEPS` gives:
// a constant correct for one image size on one provider is a number nobody can re-derive from the code. The blend is
// one full-resolution loop against hundreds of model runs, so the tiles take nearly all of it; what the share buys is
// that the end of the range is reported only once the result exists, rather than on the last tile with the blend
// still to run behind a bar that already says finished — which is what the reference does.
/// How much of the operation's progress range the tile grid reports across. The blend reports the rest.
const TILES: f64 = 0.95;

/// Whether a tile's raw model output has blown up: any value whose magnitude is **strictly** past `threshold`, or that
/// is not a number at all.
///
/// Strict, as the reference's comparison is, so a tile whose largest value is exactly the threshold is kept.
fn diverged(output: &[f32], threshold: f32) -> bool {
    // Read off the raw output, before the decode: `from_unit` clamps every value into the channel, so a check made
    // against the decoded pixels could not tell a blow-up from a legitimately bright tile.
    //
    // NaN as well, which the reference's comparison lets through because every comparison with it is false. It is what
    // an FP16 graph's blow-up looks like: the value that reaches four figures at FP32 overflows to infinity at FP16 and
    // becomes NaN a node later. Decoded, NaN is black, so the tile came out as a black square. Measured on Petersburg
    // FP16 over a synthetic 3000x2000 gradient with fine grain: 19 of its 117 tiles black, every one of them among the
    // 21 on which FP32 went past the threshold and was kept, and the other two caught by the threshold at FP16 too.
    output.iter().any(|value| value.is_nan() || value.abs() > threshold)
}

/// The side of the square blocks [`drift`] compares a tile's input and output over.
const DRIFT_BLOCK: usize = 16;

/// How far a tile's output has moved away from its input's local brightness and colour: over the tile's
/// [`DRIFT_BLOCK`]-pixel blocks, the median of each block's largest per-channel shift in mean.
///
/// `input` and `output` are one planar CHW tile each, three square planes of one shape.
fn drift(input: &[f32], output: &[f32]) -> f32 {
    // A filter removes noise or adds edges, both of which average out over a block, so a working tile barely moves a
    // block's mean. A diverged one moves most of them: its blow-up is a colour cast over the whole tile. The median
    // rather than the mean or the largest, so that a few blocks legitimately changed — a street light's halo, an edge
    // sharpened — cannot pass for a cast.
    let plane = input.len() / 3;
    let side = plane.isqrt();
    let block = DRIFT_BLOCK.min(side);

    if block == 0 {
        return 0.0;
    }

    let blocks = side / block;
    let area = (block * block) as f32;
    let mut shifts = vec![0.0_f32; blocks * blocks];

    for channel in 0..3 {
        let (input, output) = (&input[channel * plane..][..plane], &output[channel * plane..][..plane]);

        for (index, shift) in shifts.iter_mut().enumerate() {
            let (top, left) = ((index / blocks) * block, (index % blocks) * block);
            let sum: f32 = (top..top + block)
                .flat_map(|y| (left..left + block).map(move |x| y * side + x))
                .map(|at| output[at] - input[at])
                .sum();

            *shift = shift.max((sum / area).abs());
        }
    }

    let middle = shifts.len() / 2;
    *shifts.select_nth_unstable_by(middle, f32::total_cmp).1
}

// Fixed here rather than read off the model's anchor, though it began as Moscow's: the limit Petersburg's guard sets is
// a ratio measured at this scale, and it would mean something else at another. Petersburg's own anchor is now finer, so
// part of what is judged here no longer reaches the photograph; the speckle the measure exists for is finer than both.
/// The radius of the box [`invention`] splits a tile into fine detail and coarse brightness with.
const INVENTION_RADIUS: usize = 16;

/// The fine detail an input counts as having even where it has none, as an RMS in `[0, 1]`: one 8-bit level, so a
/// featureless tile — a clipped sky, a black border — is judged against the smallest change anyone could see rather
/// than divided by nothing.
const INVENTION_FLOOR: f64 = 1.0 / 255.0;

/// How much fine detail a tile's output has that its input did not: the RMS of the change finer than an
/// [`INVENTION_RADIUS`] box, for each unit of the input's own detail at the same scale (plus [`INVENTION_FLOOR`]).
///
/// `input` and `output` are one planar CHW tile each, three square planes of one shape, and `output` finite.
fn invention(input: &[f32], output: &[f32]) -> f32 {
    // A sharpener restores edges that are there and a denoiser takes out grain that is there, so what either changes
    // finer than the box is of the order of the detail the tile already had: measured on Petersburg over thirteen
    // AIM-500 photographs and the other tiles of a synthetic gradient, at most 1.77 and 1.97 of it. A NAFNet that has started
    // to blow up without reaching the magnitude paints texture of its own over a flat area: the synthetic gradient's
    // tile at (2640, 1200) changed by 13 times its input's detail at FP16 and 33 at FP32, with its largest value 1.4
    // and 2.5, under the magnitude, and decoded it was a speckled patch a tile wide. A change coarser than the box is
    // an anchor's to take out, not this.
    let plane = input.len() / 3;
    let side = plane.isqrt();

    if side == 0 {
        return 0.0;
    }

    let fine = |values: &[f32]| -> f64 {
        let coarse = box_mean(&box_mean(values, 1, side, side, INVENTION_RADIUS), 1, side, side, INVENTION_RADIUS);

        values.iter().zip(&coarse).map(|(value, coarse)| f64::from(value - coarse).powi(2)).sum()
    };

    let (mut added, mut own) = (0.0_f64, 0.0_f64);

    for channel in 0..3 {
        let (input, output) = (&input[channel * plane..][..plane], &output[channel * plane..][..plane]);
        let change: Vec<f32> = output.iter().zip(input).map(|(after, before)| after - before).collect();

        added += fine(&change);
        own += fine(input);
    }

    let count = (3 * plane) as f64;

    ((added / count).sqrt() / ((own / count).sqrt() + INVENTION_FLOOR)) as f32
}

/// How a guarded model's tiles are judged, and what is tried before a diverged one keeps its own input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Guard {
    /// The magnitude past which a tile's raw output is a blow-up.
    pub(crate) magnitude: f32,
    /// The [`invention`] past which an output that did not reach the magnitude is still a blow-up, for the models
    /// measured to need it; `None` judges by the magnitude alone.
    pub(crate) invention: Option<f32>,
    /// The second chance a diverged tile gets, for the models measured to need one; `None` keeps it at once.
    pub(crate) rescue: Option<Rescue>,
}

impl Guard {
    /// A guard that judges a tile by its magnitude alone, and keeps a diverged tile's input at once.
    pub(crate) const fn magnitude(magnitude: f32) -> Self {
        Self { magnitude, invention: None, rescue: None }
    }

    /// This guard, also judging a tile diverged whose [`invention`] is past `invention`.
    pub(crate) const fn with_invention(self, invention: f32) -> Self {
        Self { invention: Some(invention), ..self }
    }

    /// Whether `output` is a tile the model produced correctly from `input`.
    fn accepts(&self, input: &[f32], output: &[f32]) -> bool {
        // The magnitude first: it is what makes the output finite, which the two measures after it assume.
        !diverged(output, self.magnitude)
            && self.invention.is_none_or(|limit| invention(input, output) <= limit)
            && self.rescue.is_none_or(|rescue| drift(input, output) <= rescue.drift)
    }
}

/// A diverged tile's second chance: the model shown the tile again, brighter, and the result judged again.
///
/// Only for a model that reads `[0, 1]`, where multiplying a tile brightens it about black.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rescue {
    /// The [`drift`] past which an output that did not blow up is still treated as diverged.
    pub(crate) drift: f32,
    /// The mean brightness a dark tile is lifted towards for its retry.
    pub(crate) exposure: f32,
}

impl Rescue {
    /// The gain a retry of `input` runs at, or `None` where the tile cannot be brightened: already at the exposure,
    /// or already reaching the top of the range.
    fn gain(&self, input: &[f32]) -> Option<f32> {
        // Capped at what takes the tile's brightest value to 1, so the retry is still a tile inside the range the
        // model was trained on.
        // Summed in `f64`: two hundred thousand `f32` additions lose enough to move the gain.
        let mean = (input.iter().map(|&value| f64::from(value)).sum::<f64>() / input.len().max(1) as f64) as f32;
        let peak = input.iter().copied().fold(0.0_f32, f32::max);

        if mean <= 0.0 || peak <= 0.0 {
            return None;
        }

        let gain = (self.exposure / mean).min(1.0 / peak);

        (gain > 1.0).then_some(gain)
    }
}

/// How a tile is held to its input's local brightness: what the model changed, averaged over a box twice, is taken
/// back out of its output.
///
/// For a model that only adds or removes detail — a sharpener restores edges, and the blur it undoes moved no pixel's
/// local mean — so whatever its output changes at scales coarser than the box is not detail but something of the
/// model's own: at a radius of 16, the tile's drift in brightness; at a few pixels, also the low-frequency texture a
/// model paints over a flat area. Declared by each model that is measured to need it, at the radius measured for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Anchor {
    // A box twice rather than a Gaussian: the box is a running sum, so its cost does not grow with the radius, and two
    // passes make it a tent, whose response falls away as a Gaussian's does, with sidelobes under 5% of its peak. Its
    // spread is `sqrt(2r(r + 1) / 3)`: 2.8 pixels at a radius of 3, 13.5 at 16.
    /// The radius of the box, in tile pixels: a change much smaller than it is kept, one much larger is taken out.
    pub(crate) radius: usize,
}

impl Anchor {
    /// Takes the low frequencies of `output - input` back out of `output`: one planar CHW tile each, three square
    /// planes of one shape.
    fn hold(&self, input: &[f32], output: &mut [f32]) {
        // Twice over the box rather than once: a box's own edges are a step in its response, and a second pass
        // makes it a tent, which leaves no ring around a sharpened edge. The box is clipped at the tile's edges
        // rather than reflected, which is what an edge of a tile needs: the change near it is compared with what the
        // model did inside the tile, not with a mirror image of it.
        let plane = input.len() / 3;
        let side = plane.isqrt();

        if side == 0 {
            return;
        }

        for channel in 0..3 {
            let (input, output) = (&input[channel * plane..][..plane], &mut output[channel * plane..][..plane]);
            // A value that is not finite is no change: the box means are running sums, so one NaN or infinity left in
            // would reach every mean whose box covers it and blacken a strip of the tile. Kept as its input, it is
            // the one pixel the model failed on and nothing more.
            for (after, &before) in output.iter_mut().zip(input) {
                if !after.is_finite() {
                    *after = before;
                }
            }

            let change: Vec<f32> = output.iter().zip(input).map(|(after, before)| after - before).collect();
            let coarse = box_mean(&box_mean(&change, 1, side, side, self.radius), 1, side, side, self.radius);

            for (value, drift) in output.iter_mut().zip(coarse) {
                *value -= drift;
            }
        }
    }
}

// Test-only, and read by each family's live check to report whether its guard fired on a real photograph. A static
// rather than a field because a live check goes through `Opai::process`, which builds its pipeline where the test
// cannot reach it. Shared by every family that runs this file, and the fake-backend suite below trips the guard too,
// which is why each live check is `#[ignore]`d, run alone, and counts only what its own runs added.
/// How many tiles the guard has kept since the process started.
#[cfg(test)]
pub(crate) static GUARDED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many diverged tiles a rescue has saved since the process started; see [`GUARDED`].
#[cfg(test)]
pub(crate) static RESCUED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The version of what a filter run makes from its tiles, in the cache tag of every family that runs this file: raised
/// whenever the guard, the anchor or the blend draws a different picture from the same photograph, model and strength,
/// so the run cache never serves a filtered image an older build drew. 1: the tags that carried no version. 2: every
/// tile held to its input's local brightness, NaN treated as a blow-up, and a non-finite output value kept as its input.
/// 3: Petersburg's guard also rejecting a tile that invents detail its input did not have. 4: each model declaring its
/// own anchor, and Petersburg's at a radius of 3 rather than 16, taking out the mottle it paints over flat areas.
pub(crate) const FILTER_VERSION: u32 = 4;

/// The pipeline running one filter operation, as a family's contract seam hands it back.
#[expect(
    clippy::too_many_arguments,
    reason = "every one is a family's own fact about its run, and a struct of them would be built inline at each seam"
)]
pub(crate) fn pipeline<B: Backend>(
    name: String,
    artifact: ArtifactId,
    profile: EpProfile,
    range: Normalisation,
    strength: Strength,
    guard: Option<Guard>,
    anchor: Option<Anchor>,
    geometry: TileGeometry,
) -> Shared<B> {
    Arc::new(Filter::new(name, artifact, profile, range, strength, guard, anchor, geometry))
}

/// One filter run: the graph it opens, the settings it opens it under, the range it feeds it, the guard its model
/// needs, and how much of the result lands.
struct Filter {
    /// The graph this operation runs, and the name it reports a failure in.
    graph: SingleGraph,
    /// The range the graph reads and writes, as its family states it.
    range: Normalisation,
    // An `Option` rather than the reference's zero-means-off `float32`, so "no guard" is not a sentinel a later reader
    // has to know about.
    /// How a tile's raw output is judged, and what is tried before it is discarded, for the models that need it.
    guard: Option<Guard>,
    /// How each tile is held to its input's local brightness, for the models that need it.
    anchor: Option<Anchor>,
    /// How far the photograph is moved towards the tiled result: past it above 1.
    strength: f32,
    /// The tile the graph accepts and how far tiles overlap: the shared default for every model exported at 256.
    geometry: TileGeometry,
}

impl Filter {
    #[expect(clippy::too_many_arguments, reason = "the arguments of `pipeline`, forwarded")]
    fn new(
        name: String,
        artifact: ArtifactId,
        profile: EpProfile,
        range: Normalisation,
        strength: Strength,
        guard: Option<Guard>,
        anchor: Option<Anchor>,
        geometry: TileGeometry,
    ) -> Self {
        // A retry multiplies the tile, which brightens it about black only in `[0, 1]`.
        debug_assert!(
            guard.is_none_or(|guard| guard.rescue.is_none() || range == Normalisation::Unit),
            "a rescue was handed a model that does not read [0, 1]"
        );

        Self {
            graph: SingleGraph::new(name, artifact, profile),
            range,
            guard,
            anchor,
            strength: strength.as_f32(),
            geometry,
        }
    }
}

impl OnOneGraph for Filter {
    fn graph(&self) -> &SingleGraph {
        &self.graph
    }
}

impl<B: Backend> ImagePipeline<B> for Filter {
    /// One graph, run over a tile grid.
    fn stages(&self) -> usize {
        1
    }

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        let report = reporter(progress);

        // The driver's to leave to its caller, so that a multi-pass run does not return to the start at each pass.
        report(0.0);

        let tile_progress = progress.map(|report| move |fraction: f64| report(fraction * TILES));
        let tile_progress = tile_progress.as_ref().map(|report| report as &dyn Fn(f64));

        let session = &sessions[0];
        let guard = self.guard;

        // The guard lives in the per-tile step rather than in `run_tiled`, which takes the step as a parameter so a
        // caller can do exactly this. At scale 1 the tile the model was shown and the tensor it returned have one
        // shape, so "keep this tile's input" is a copy — and the driver then decodes and blends it like any other,
        // so a kept tile meets its neighbours through the same ramp and at the run's own depth. The reference keeps
        // an 8-bit copy of the tile instead, which quantises a guarded region inside a 16-bit result.
        //
        // The retry is the model's own output, shown a brighter copy of the tile and dimmed back: NAFNet diverges on
        // dark, heavily noisy tiles it was never trained on — a night sky — and the same noise lifted to an exposure
        // it has seen denoises cleanly. A tile the model handles the first time is never retried, so a rescue changes
        // nothing about a photograph the guard never fired on.
        let mut lifted = Vec::new();
        let mut guarded = |input: &[f32], output: &mut [f32]| -> Result<(), B::Error> {
            B::run_tile(session, input, output)?;

            let Some(guard) = guard else { return Ok(()) };

            if guard.accepts(input, output) {
                return Ok(());
            }

            if let Some(rescue) = guard.rescue
                && let Some(gain) = rescue.gain(input)
            {
                lifted.clear();
                lifted.extend(input.iter().map(|value| value * gain));
                B::run_tile(session, &lifted, output)?;
                output.iter_mut().for_each(|value| *value /= gain);

                if guard.accepts(input, output) {
                    #[cfg(test)]
                    RESCUED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    return Ok(());
                }
            }

            output.copy_from_slice(input);

            #[cfg(test)]
            GUARDED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            Ok(())
        };

        // The order, per tile: the model, the guard, the anchor, then the driver's decode and ramp, and the strength
        // last, over the whole photograph. After the guard, so the anchor only ever sees a tile that is going to be
        // used: a kept tile is its own input and has nothing to take out, and a tile that blew up is not one whose
        // drift can be measured. Before the decode, in the graph's own range, where the change is still unquantised.
        // One anchor a model rather than a list: a hold at one radius after another at a smaller one finds almost
        // nothing left to take out, so a model declares only the finest it needs.
        let anchor = self.anchor;
        let mut tile = |input: &[f32], output: &mut [f32]| -> Result<(), B::Error> {
            guarded(input, output)?;

            if let Some(anchor) = anchor {
                anchor.hold(input, output);
            }

            Ok(())
        };

        let produced = run_tiled(
            input,
            self.geometry,
            1,
            depth,
            self.range,
            tile_progress,
            cancelled,
            &mut tile as &mut RunTile<'_, B::Error>,
        )
        .map_err(|error| InferenceError::from_tiling(&self.graph.name, error))?;

        // Once more after the last tile: the blend is a full-resolution loop, and a run cancelled during the grid's
        // last tile would otherwise hand back a finished photograph the caller had already asked it to drop.
        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        // Dispatched once rather than per pixel. The driver's result is already at the depth it was asked for, so
        // `into_rgb8` and `into_rgb16` hand its buffer over rather than converting it, and `blended` works in place
        // on it — no third full-resolution buffer. Not skipped at a strength of 1: the loop returns the tiled result
        // exactly there already, and one pass over the pixels is nothing beside the grid that produced them.
        let sampler = Sampler::new(input);
        let result = match depth {
            ChannelDepth::Eight => u8::into_dynamic(blended(&sampler, produced.into_rgb8(), self.strength)),
            ChannelDepth::Sixteen => u16::into_dynamic(blended(&sampler, produced.into_rgb16(), self.strength)),
        };

        report(1.0);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_at_the_threshold_is_not_a_blow_up() {
        // Strict, as the reference's `v > t || v < -t` is, on both signs.
        assert!(!diverged(&[3.0], 3.0));
        assert!(!diverged(&[-3.0], 3.0));
        assert!(!diverged(&[0.5, 3.0, -3.0, 1.0], 3.0));
    }

    #[test]
    fn a_value_past_the_threshold_is_a_blow_up_on_either_side_of_zero() {
        assert!(diverged(&[3.0001], 3.0));
        assert!(diverged(&[-3.0001], 3.0));
    }

    #[test]
    fn one_exploding_value_among_thousands_of_ordinary_ones_is_enough() {
        // What a blow-up looks like in practice: most of the tile is fine and one region goes to four figures.
        let mut output = vec![0.5_f32; 3 * 256 * 256];
        assert!(!diverged(&output, 3.0), "an ordinary tile read as a blow-up");

        output[123_456] = 1200.0;
        assert!(diverged(&output, 3.0), "one exploded value was missed");
    }

    #[test]
    fn an_empty_output_has_not_diverged() {
        assert!(!diverged(&[], 3.0));
    }

    #[test]
    fn a_value_that_is_not_a_number_is_a_blow_up() {
        // What an FP16 graph's blow-up decodes to: black, and every comparison with it false.
        assert!(diverged(&[0.5, f32::NAN, 0.5], 3.0));
        assert!(diverged(&[f32::INFINITY], 3.0));
    }

    /// A 256x256 planar tile of reproducible values in `[0.2, 0.8)`, so a change can be seen against something that
    /// is not flat.
    fn textured() -> Vec<f32> {
        let mut state = 7_u32;

        (0..3 * 256 * 256)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                0.2 + 0.6 * ((state >> 8) as f32 / (1 << 24) as f32)
            })
            .collect()
    }

    #[test]
    fn an_anchor_takes_a_shift_over_the_whole_tile_out_entirely() {
        // Moscow's drift, in its simplest form: the tile brighter by a constant, a different one in each channel.
        let input = textured();
        let mut output: Vec<f32> = input
            .iter()
            .enumerate()
            .map(|(index, value)| value + [0.04, -0.02, 0.01][index / (256 * 256)])
            .collect();

        Anchor { radius: 16 }.hold(&input, &mut output);

        let worst = output.iter().zip(&input).map(|(after, before)| (after - before).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-5, "a shift over the whole tile left {worst} behind");
    }

    #[test]
    fn an_anchor_keeps_a_change_finer_than_its_box() {
        // What a sharpener adds: detail, here a checkerboard of a twentieth of the range, which averages out over any
        // box and so is all kept, on top of a shift, which is all taken out.
        let input = textured();
        let detail = |index: usize| if (index % 256 + index / 256).is_multiple_of(2) { 0.05 } else { -0.05 };
        let mut output: Vec<f32> =
            input.iter().enumerate().map(|(index, value)| value + 0.03 + detail(index)).collect();

        Anchor { radius: 16 }.hold(&input, &mut output);

        let worst = output
            .iter()
            .zip(&input)
            .enumerate()
            .map(|(index, (after, before))| (after - before - detail(index)).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.002, "the detail came back off by as much as {worst}");
    }

    #[test]
    fn a_small_anchor_takes_out_a_coarse_change_and_keeps_a_fine_one() {
        // Petersburg's mottle in miniature: over a flat tile, a change that rises and falls over 64 pixels, which is
        // texture the model painted, on top of one that alternates every pixel, which is the sharpening. At a radius of
        // 3 the first is nearly all taken out and the second is all kept; at 16, the radius the tile drift needs, the
        // first is barely touched, which is why Petersburg declares its own.
        let input = flat(0.5);
        let coarse = |index: usize| 0.02 * (std::f32::consts::TAU * (index % 256) as f32 / 64.0).sin();
        let fine = speckle(0.02);
        let model: Vec<f32> =
            input.iter().enumerate().map(|(index, value)| value + coarse(index) + fine(index)).collect();

        // Away from the tile's edges, where the clipped box sees only one side of the wave.
        let left = |anchor: Anchor, part: &dyn Fn(usize) -> f32| -> f32 {
            let mut output = model.clone();
            anchor.hold(&input, &mut output);

            let (mut along, mut energy) = (0.0_f64, 0.0_f64);
            for index in (0..256 * 256).filter(|index| (16..240).contains(&(index % 256))) {
                along += f64::from((output[index] - input[index]) * part(index));
                energy += f64::from(part(index).powi(2));
            }

            (along / energy) as f32
        };

        let (coarse_left, fine_left) = (left(Anchor { radius: 3 }, &coarse), left(Anchor { radius: 3 }, &fine));
        assert!(coarse_left < 0.06, "a radius of 3 left {coarse_left} of the 64-pixel change");
        assert!(fine_left > 0.99, "a radius of 3 kept only {fine_left} of the alternating change");

        let wide = left(Anchor { radius: 16 }, &coarse);
        assert!(wide > 0.5, "a radius of 16 took out all but {wide} of the 64-pixel change");
    }

    #[test]
    fn an_anchor_keeps_a_value_that_is_not_a_number_to_its_own_pixel() {
        // At the radii the shipped models declare and one between them: non-finite values are kept as their input
        // before any box sum, whatever the box.
        let input = textured();

        for (anchor, poison) in [3, 8, 16]
            .into_iter()
            .flat_map(|radius| [f32::NAN, f32::INFINITY, f32::NEG_INFINITY].map(|poison| (Anchor { radius }, poison)))
        {
            let mut clean = input.clone();
            anchor.hold(&input, &mut clean);

            let mut output = input.clone();
            let poisoned = output.len() / 3 + 37;
            output[poisoned] = poison;
            anchor.hold(&input, &mut output);

            assert!(output.iter().all(|value| value.is_finite()), "{anchor:?}: {poison} reached the anchored tile");
            for (index, (held, expected)) in output.iter().zip(&clean).enumerate() {
                if index != poisoned {
                    assert_eq!(held, expected, "{anchor:?}: {poison} at {poisoned} moved pixel {index}");
                }
            }
            assert_eq!(output[poisoned], input[poisoned], "the poisoned pixel keeps its input");
        }
    }

    /// A 256x256 planar tile whose every value is `value`.
    fn flat(value: f32) -> Vec<f32> {
        vec![value; 3 * 256 * 256]
    }

    /// A checkerboard of `amplitude` either side of zero at every value of a 256x256 planar tile: detail finer than
    /// any box.
    fn speckle(amplitude: f32) -> impl Fn(usize) -> f32 {
        move |index| if (index % 256 + index / 256).is_multiple_of(2) { amplitude } else { -amplitude }
    }

    #[test]
    fn an_output_that_matches_its_input_invents_nothing() {
        let input = textured();
        assert_eq!(invention(&input, &input), 0.0);
        assert_eq!(invention(&[], &[]), 0.0);
    }

    #[test]
    fn detail_strengthened_where_the_input_has_it_is_not_an_invention() {
        // What a sharpener does: the input's own detail, made half as strong again, plus a shift the anchor takes out.
        let input = textured();
        let output: Vec<f32> = input.iter().map(|value| value + 0.5 * (value - 0.5) + 0.02).collect();
        let invented = invention(&input, &output);

        assert!(invented < 0.6, "strengthened detail read as {invented} times the input's");
        assert!(Guard::magnitude(3.0).with_invention(5.0).accepts(&input, &output));
    }

    #[test]
    fn a_shift_over_a_flat_tile_is_not_an_invention() {
        // Coarser than the box: the anchor's to take out, not the guard's to reject.
        let input = flat(0.5);
        let output = flat(0.56);

        assert!(invention(&input, &output) < 1e-3);
    }

    #[test]
    fn texture_painted_over_a_flat_tile_is_an_invention_the_magnitude_alone_lets_through() {
        // Petersburg's patch in miniature: an output well inside the magnitude, speckled by a tenth of the range where
        // the input had nothing to sharpen.
        let input = flat(0.5);
        let speckle = speckle(0.1);
        let output: Vec<f32> = input.iter().enumerate().map(|(index, value)| value + speckle(index)).collect();
        let invented = invention(&input, &output);

        assert!(invented > 20.0, "a speckle over a flat tile read as only {invented} times its detail");
        assert!(
            Guard::magnitude(3.0).accepts(&input, &output),
            "the magnitude alone was meant to let it through"
        );
        assert!(!Guard::magnitude(3.0).with_invention(5.0).accepts(&input, &output), "the limit let it through");
    }

    #[test]
    fn the_same_speckle_over_a_detailed_tile_is_within_the_limit() {
        // Judged against the input's own detail, so a change a flat tile could not have is ordinary on a busy one.
        let input = textured();
        let speckle = speckle(0.1);
        let output: Vec<f32> = input.iter().enumerate().map(|(index, value)| value + speckle(index)).collect();

        assert!(Guard::magnitude(3.0).with_invention(5.0).accepts(&input, &output));
    }

    #[test]
    fn an_output_that_matches_its_input_has_not_drifted() {
        assert_eq!(drift(&flat(0.2), &flat(0.2)), 0.0);
    }

    #[test]
    fn a_cast_over_the_whole_tile_drifts_by_its_size_in_the_worst_channel() {
        // Green lifted by 0.1 and blue lowered by 0.05: the worst channel is the drift, whatever its sign.
        let input = flat(0.2);
        let mut output = input.clone();
        let plane = 256 * 256;

        output[plane..2 * plane].iter_mut().for_each(|value| *value += 0.1);
        output[2 * plane..].iter_mut().for_each(|value| *value -= 0.05);

        assert!((drift(&input, &output) - 0.1).abs() < 1e-5, "{}", drift(&input, &output));
    }

    #[test]
    fn a_change_confined_to_a_few_blocks_is_not_a_drift() {
        // A street light's halo: one block moved hard, the other 255 untouched. The median does not see it.
        let input = flat(0.05);
        let mut output = input.clone();

        for y in 0..16 {
            for x in 0..16 {
                output[y * 256 + x] = 0.9;
            }
        }

        assert_eq!(drift(&input, &output), 0.0);
    }

    #[test]
    fn noise_removed_about_its_own_mean_is_not_a_drift() {
        // What a denoiser does: a checkerboard of +-0.04 about 0.1 flattened to 0.1. Every block's mean is unchanged.
        let input: Vec<f32> = (0..3 * 256 * 256).map(|at| if (at + at / 256) % 2 == 0 { 0.14 } else { 0.06 }).collect();

        assert!(drift(&input, &flat(0.1)) < 1e-5, "{}", drift(&input, &flat(0.1)));
    }

    /// The rescue the gain tests run, lifting towards a mean of 0.25.
    const LIFT: Rescue = Rescue { drift: 0.01, exposure: 0.25 };

    #[test]
    fn a_dark_tile_is_lifted_to_the_exposure() {
        assert!((LIFT.gain(&flat(0.05)).expect("a dark tile is lifted") - 5.0).abs() < 1e-4);
    }

    #[test]
    fn a_lift_stops_where_the_brightest_value_reaches_the_top_of_the_range() {
        // A mean of about 0.05 would ask for 5, but one value at 0.5 allows only 2.
        let mut input = flat(0.05);
        input[1234] = 0.5;

        assert!((LIFT.gain(&input).expect("a dark tile is lifted") - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_tile_already_at_the_exposure_or_black_is_not_lifted() {
        assert_eq!(LIFT.gain(&flat(0.25)), None);
        assert_eq!(LIFT.gain(&flat(0.6)), None);
        assert_eq!(LIFT.gain(&flat(0.0)), None);

        // Dark, but with one value already at the top of the range: nothing to lift it by.
        let mut input = flat(0.05);
        input[99] = 1.0;
        assert_eq!(LIFT.gain(&input), None);
    }

    // The fake-backend suite: the whole pipeline — the tile grid at scale 1, the guard, the blend, the progress
    // schedule, the depth dispatch, the cancellation and the refusal — with no ONNX Runtime, no GPU, no model file and
    // no network, which is what keeps every property above checked on every CI platform.
    //
    // Built here directly rather than through a family's seam: a shared module's tests do not reach up into one family
    // to construct what they test. Which family hands which guard to which model is each family's to prove.

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use image::{ImageBuffer, Rgb};

    use crate::pipeline::test_support::{DARKENS, Fake, FakeSession, PAIR, darkened, original, session};

    use crate::models::artifact::Family;
    use crate::models::precision::Precision;
    use imaging::test_support::photograph;

    /// The guard every guarded model this project ships runs at.
    const THRESHOLD: f32 = 3.0;

    /// A filter over `range` at `strength`, guarded by `guard`.
    fn filtering_in(range: Normalisation, strength: f64, guard: Option<Guard>) -> Shared<Fake> {
        let strength = Strength::new(strength).expect("the test supplied a strength in range");
        let artifact = ArtifactId::new(Family::Denoise, "fake", None, Precision::Fp32);

        pipeline::<Fake>(
            "Fake (FP32)".into(),
            artifact,
            EpProfile::default(),
            range,
            strength,
            guard,
            None,
            TileGeometry::DEFAULT,
        )
    }

    /// A filter over `[0, 1]` at `strength`, guarded by `guard`.
    fn filtering(strength: f64, guard: Option<Guard>) -> Shared<Fake> {
        filtering_in(Normalisation::Unit, strength, guard)
    }

    /// A filter over `[0, 1]` at a strength of 1, unguarded and held by `anchor`.
    fn anchored(anchor: Option<Anchor>) -> Shared<Fake> {
        let strength = Strength::new(1.0).expect("the test supplied a strength in range");
        let artifact = ArtifactId::new(Family::Sharpen, "fake", None, Precision::Fp32);

        pipeline::<Fake>(
            "Fake (FP32)".into(),
            artifact,
            EpProfile::default(),
            Normalisation::Unit,
            strength,
            None,
            anchor,
            TileGeometry::DEFAULT,
        )
    }

    /// A filter whose model is guarded by its magnitude alone.
    fn guarded(strength: f64) -> Shared<Fake> {
        filtering(strength, Some(Guard::magnitude(THRESHOLD)))
    }

    /// Runs `pipeline` over `source` with nothing watching and nothing cancelling it.
    fn run(
        pipeline: &Shared<Fake>,
        source: &DynamicImage,
        handle: &SessionHandle<FakeSession>,
        depth: ChannelDepth,
    ) -> DynamicImage {
        pipeline
            .run(source, std::slice::from_ref(handle), depth, None, &|| false)
            .expect("a filter over a photograph")
    }

    #[test]
    fn a_photograph_of_many_tiles_comes_back_at_its_own_size() {
        // 600x400 at 256/16 is three columns by two rows, which is also what the progress test below relies on.
        let handle = session(None);
        let produced = run(&guarded(1.0), &photograph(600, 400), &handle, ChannelDepth::Eight);

        assert_eq!((produced.width(), produced.height()), (600, 400));
        assert_eq!(handle.session().runs.load(Ordering::Relaxed), 6, "the grid was not the six tiles expected");
    }

    #[test]
    fn a_photograph_smaller_than_one_tile_comes_back_at_its_own_size_rather_than_the_tiles() {
        let handle = session(None);
        let produced = run(&guarded(1.0), &photograph(40, 17), &handle, ChannelDepth::Eight);

        assert_eq!((produced.width(), produced.height()), (40, 17));
    }

    #[test]
    fn a_strength_of_zero_returns_the_photograph_exactly() {
        let source = photograph(300, 280);
        let handle = session(None);
        let produced = run(&guarded(0.0), &source, &handle, ChannelDepth::Eight);

        let sampler = Sampler::new(&source);
        for (x, y, pixel) in produced.as_rgb8().expect("an eight-bit run").enumerate_pixels() {
            assert_eq!(pixel.0, original::<u8>(&sampler, x, y), "a strength of zero changed ({x}, {y})");
        }
    }

    #[test]
    fn a_strength_of_one_returns_the_models_output_exactly() {
        // Exact rather than within a level, which is what the refusal of a fast path at 1 rests on: the blend loop
        // already lands on the model's own output there.
        let source = photograph(300, 280);
        let handle = session(None);
        let produced = run(&guarded(1.0), &source, &handle, ChannelDepth::Eight);

        let sampler = Sampler::new(&source);
        for (x, y, pixel) in produced.as_rgb8().expect("an eight-bit run").enumerate_pixels() {
            assert_eq!(pixel.0, darkened::<u8>(&sampler, x, y), "a strength of one moved ({x}, {y}) off the output");
        }
    }

    #[test]
    fn a_strength_past_one_moves_further_along_the_same_line_and_stays_in_range() {
        // The model darkens, so the result at 2 must be darker than at 1 wherever the photograph had anything to
        // darken, and never lighter. `u8` cannot leave its range, so what "in range" rules out is a wrap: a dark pixel
        // pushed below zero coming back as a bright one, which would read as lighter than the run at 1.
        let source = photograph(300, 280);
        let (once, twice) = (session(None), session(None));

        let one = run(&guarded(1.0), &source, &once, ChannelDepth::Eight);
        let two = run(&guarded(2.0), &source, &twice, ChannelDepth::Eight);

        let (one, two) = (one.as_rgb8().expect("eight-bit"), two.as_rgb8().expect("eight-bit"));
        let sampler = Sampler::new(&source);
        let mut moved_further = 0;

        for (x, y, at_two) in two.enumerate_pixels() {
            let at_one = one.get_pixel(x, y);
            let before = original::<u8>(&sampler, x, y);

            for (channel, before) in before.into_iter().enumerate() {
                assert!(at_two.0[channel] <= at_one.0[channel], "({x}, {y}) moved the other way past a strength of 1");

                let (near, far) = (
                    i32::from(before) - i32::from(at_one.0[channel]),
                    i32::from(before) - i32::from(at_two.0[channel]),
                );
                assert!(far >= near, "({x}, {y}) moved less at 2 than at 1");
                moved_further += usize::from(far > near);
            }
        }

        assert!(moved_further > 0, "a strength of 2 produced exactly what 1 did");
    }

    #[test]
    fn a_tile_the_guard_trips_on_keeps_the_photographs_own_pixels_and_the_rest_are_the_models() {
        // The mechanism test, at both depths. A 16-bit source whose values are not multiples of 257 is what shows the
        // kept pixels are the photograph's at the run's own depth rather than an 8-bit copy of them.
        let eight = photograph(PAIR.0, PAIR.1);
        let sixteen = DynamicImage::ImageRgb16(ImageBuffer::from_fn(PAIR.0, PAIR.1, |x, y| {
            Rgb([(x * 131 + y * 7) as u16, (x * 3 + y * 311) as u16, 40_001])
        }));

        for (source, depth) in [(&eight, ChannelDepth::Eight), (&sixteen, ChannelDepth::Sixteen)] {
            let handle = session(Some((0, 4.0)));
            let produced = run(&guarded(1.0), source, &handle, depth);
            let sampler = Sampler::new(source);

            assert_eq!((produced.width(), produced.height()), PAIR, "the guarded run did not complete at full size");

            for y in 0..PAIR.1 {
                for x in (0..224).chain(256..PAIR.0) {
                    let kept = x < 224;
                    let (got, want) = match depth {
                        ChannelDepth::Eight => {
                            let got = produced.as_rgb8().expect("eight-bit").get_pixel(x, y).0.map(u16::from);
                            let want =
                                if kept { original::<u8>(&sampler, x, y) } else { darkened::<u8>(&sampler, x, y) };

                            (got, want.map(u16::from))
                        }
                        ChannelDepth::Sixteen => {
                            let got = produced.as_rgb16().expect("sixteen-bit").get_pixel(x, y).0;
                            let want =
                                if kept { original::<u16>(&sampler, x, y) } else { darkened::<u16>(&sampler, x, y) };

                            (got, want)
                        }
                    };

                    assert_eq!(
                        got,
                        want,
                        "{depth:?} ({x}, {y}): the {} tile's pixels are wrong",
                        ["second", "guarded"][usize::from(kept)]
                    );
                }
            }
        }
    }

    #[test]
    fn an_anchored_run_keeps_a_flat_photograph_flat_across_every_tile_and_seam() {
        // The fake halves every tile, which over a flat photograph is a change with nothing finer than a tile in it:
        // anchored, every tile comes back at its input's level, so the tiles agree with each other across each seam.
        // Unanchored, the same run is the darkened photograph — which is what makes this a test of the anchor.
        let colour = [200, 120, 40];
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(600, 400, Rgb(colour)));

        let held = run(&anchored(Some(Anchor { radius: 16 })), &source, &session(None), ChannelDepth::Eight);
        let loose = run(&anchored(None), &source, &session(None), ChannelDepth::Eight);

        for (x, y, pixel) in held.as_rgb8().expect("eight-bit").enumerate_pixels() {
            assert!(
                pixel.0.iter().zip(colour).all(|(got, want)| got.abs_diff(want) <= 1),
                "({x}, {y}) came out at {:?} rather than {colour:?}",
                pixel.0
            );
        }

        assert_eq!(loose.as_rgb8().expect("eight-bit").get_pixel(300, 200).0, [100, 60, 20]);
    }

    #[test]
    fn a_tile_whose_largest_value_is_exactly_the_threshold_is_the_models() {
        let source = photograph(PAIR.0, PAIR.1);
        let handle = session(Some((0, 3.0)));
        let produced = run(&guarded(1.0), &source, &handle, ChannelDepth::Eight);

        let sampler = Sampler::new(&source);
        let produced = produced.as_rgb8().expect("eight-bit");

        // Anywhere in the first tile but the one pixel the fake set to 3.0, which decodes to white rather than to the
        // darkened photograph.
        for (x, y) in [(0, 0), (10, 150), (200, 30)] {
            assert_eq!(produced.get_pixel(x, y).0, darkened::<u8>(&sampler, x, y), "({x}, {y}) was kept");
        }
    }

    #[test]
    fn an_unguarded_model_passes_an_exploding_tile_through() {
        // The same backend that trips the guard, and nothing is kept: every region is the model's.
        let source = photograph(PAIR.0, PAIR.1);
        let sampler = Sampler::new(&source);

        let handle = session(Some((0, 4.0)));
        let produced = run(&filtering(1.0, None), &source, &handle, ChannelDepth::Eight);
        let produced = produced.as_rgb8().expect("eight-bit");

        for (x, y) in [(0, 0), (10, 150), (200, 30), (400, 100)] {
            assert_eq!(
                produced.get_pixel(x, y).0,
                darkened::<u8>(&sampler, x, y),
                "the unguarded model kept ({x}, {y}) rather than using the model's output"
            );
        }
    }

    // The rescue, through the whole pipeline. The fake halves every tile, which moves every block's mean, so a rescue
    // whose drift is tight judges every tile the fake produces diverged and one whose drift is loose judges none of
    // them — which is what lets these tests pick which half of the judgement they exercise.

    /// A rescue that judges only by magnitude, because the fake's halving is far inside its drift.
    const LOOSE: Rescue = Rescue { drift: 1.0, exposure: 0.25 };

    /// A rescue that judges every one of the fake's tiles diverged.
    const TIGHT: Rescue = Rescue { drift: 0.01, exposure: 0.25 };

    /// A filter guarded at the shipped magnitude, with `rescue`.
    fn rescued(rescue: Rescue) -> Shared<Fake> {
        filtering(1.0, Some(Guard { magnitude: THRESHOLD, invention: None, rescue: Some(rescue) }))
    }

    /// Two tiles of night sky: every value at most a tenth of the range, so both can be lifted.
    fn night() -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(PAIR.0, PAIR.1, |x, y| {
            Rgb([(x % 24) as u8 + 2, (y % 24) as u8 + 2, 10])
        }))
    }

    #[test]
    fn a_tile_that_blows_up_is_retried_brighter_and_the_retry_is_used() {
        // The first run explodes, the retry does not: the tile is the model's, dimmed back to the photograph's
        // exposure, rather than the photograph's own pixels.
        let source = night();
        let sampler = Sampler::new(&source);
        let handle = session(Some((0, 4.0)));
        let produced = run(&rescued(LOOSE), &source, &handle, ChannelDepth::Eight);
        let produced = produced.as_rgb8().expect("eight-bit");

        assert_eq!(handle.session().runs.load(Ordering::Relaxed), 3, "two tiles and one retry");

        for (x, y) in [(0, 0), (10, 150), (200, 30), (400, 100)] {
            let (got, want) = (produced.get_pixel(x, y).0, darkened::<u8>(&sampler, x, y));

            // Within a level: the retry is multiplied and divided back, which need not round to the same step.
            assert!(got.iter().zip(want).all(|(got, want)| got.abs_diff(want) <= 1), "({x}, {y}): {got:?} {want:?}");
        }
    }

    #[test]
    fn a_tile_whose_retry_still_diverges_keeps_the_photographs_own_pixels() {
        // Every run drifts, so each tile is tried twice and then kept.
        let source = night();
        let sampler = Sampler::new(&source);
        let handle = session(None);
        let produced = run(&rescued(TIGHT), &source, &handle, ChannelDepth::Eight);
        let produced = produced.as_rgb8().expect("eight-bit");

        assert_eq!(handle.session().runs.load(Ordering::Relaxed), 4, "each of the two tiles was not retried once");

        for (x, y, pixel) in produced.enumerate_pixels() {
            assert_eq!(pixel.0, original::<u8>(&sampler, x, y), "({x}, {y}) was not kept");
        }
    }

    #[test]
    fn a_tile_that_cannot_be_brightened_is_kept_without_a_retry() {
        // The ordinary photograph reaches the top of the range, so there is nothing to lift it by.
        let source = photograph(PAIR.0, PAIR.1);
        let sampler = Sampler::new(&source);
        let handle = session(Some((0, 4.0)));
        let produced = run(&rescued(LOOSE), &source, &handle, ChannelDepth::Eight);
        let produced = produced.as_rgb8().expect("eight-bit");

        assert_eq!(handle.session().runs.load(Ordering::Relaxed), 2, "a tile with nothing to lift was retried");
        assert_eq!(
            produced.get_pixel(10, 150).0,
            original::<u8>(&sampler, 10, 150),
            "the diverged tile was not kept"
        );
        assert_eq!(produced.get_pixel(400, 100).0, darkened::<u8>(&sampler, 400, 100), "the good tile was not used");
    }

    #[test]
    fn a_rescue_leaves_a_tile_the_model_handled_alone() {
        // Nothing explodes and the drift is loose: every tile is run once and is the model's.
        let source = night();
        let handle = session(None);
        run(&rescued(LOOSE), &source, &handle, ChannelDepth::Eight);

        assert_eq!(handle.session().runs.load(Ordering::Relaxed), 2, "a tile that passed was retried");
    }

    #[test]
    fn the_range_the_caller_hands_over_is_the_one_the_graph_is_fed() {
        // The range is the caller's, so a filter that had gone back to hard-coding one would pass every test above
        // and feed a signed-range family the unit range. The fake halves what it is shown, and halving is not the
        // same operation in the two: under `[-1, 1]` a value is halved about the middle of the channel, not about
        // black, so the two runs differ at every pixel that is not mid-grey.
        let source = photograph(300, 280);
        let sampler = Sampler::new(&source);

        for range in [Normalisation::Unit, Normalisation::Signed] {
            let produced = run(&filtering_in(range, 1.0, None), &source, &session(None), ChannelDepth::Eight);
            let produced = produced.as_rgb8().expect("eight-bit");

            for (x, y, pixel) in produced.enumerate_pixels() {
                let want = sampler
                    .rgb(x, y)
                    .map(|value| u8::from_unit(range.decode(range.encode_unit(value.to_unit()) * DARKENS)));

                assert_eq!(pixel.0, want, "{range:?} ({x}, {y}) was not fed the range it was handed");
            }
        }
    }

    #[test]
    fn progress_opens_on_zero_never_goes_backwards_and_reaches_one_only_after_the_blend() {
        let handle = session(None);
        let reported: Mutex<Vec<f64>> = Mutex::default();

        guarded(0.5)
            .run(
                &photograph(600, 400),
                std::slice::from_ref(&handle),
                ChannelDepth::Eight,
                Some(&|fraction| reported.lock().unwrap().push(fraction)),
                &|| false,
            )
            .expect("a filter over a photograph");

        let reported = reported.into_inner().unwrap();

        // The opening zero, six tiles, and the end.
        assert_eq!(reported.len(), 8, "{reported:?}");
        assert_eq!(reported[0], 0.0, "the run did not open on zero");
        assert!(reported.windows(2).all(|pair| pair[1] >= pair[0]), "progress went backwards: {reported:?}");
        assert_eq!(*reported.last().expect("a run reports"), 1.0, "the last report was not the end of the range");

        // Every report before the last is short of the end, so the end cannot have been reached on a tile.
        assert!(reported[..reported.len() - 1].iter().all(|fraction| *fraction <= TILES), "{reported:?}");
    }

    #[test]
    fn a_cancellation_between_tiles_or_before_the_blend_produces_no_image() {
        // 600x400 is six tiles, so the driver asks six times and the pipeline asks a seventh just before the blend.
        // A cancellation at the second question lands between tiles; at the seventh, after every tile has run.
        for (boundary, tiles_run) in [(2, 2), (6, 6)] {
            let handle = session(None);
            let asked = AtomicUsize::new(0);
            let reported: Mutex<Vec<f64>> = Mutex::default();

            let outcome = guarded(1.0).run(
                &photograph(600, 400),
                std::slice::from_ref(&handle),
                ChannelDepth::Eight,
                Some(&|fraction| reported.lock().unwrap().push(fraction)),
                &|| asked.fetch_add(1, Ordering::Relaxed) >= boundary,
            );

            assert!(matches!(outcome, Err(InferenceError::Cancelled)), "at {boundary}: {outcome:?}");
            assert_eq!(handle.session().runs.load(Ordering::Relaxed), tiles_run, "at {boundary}");

            // And the bar never claimed a finished run it then dropped.
            assert!(!reported.into_inner().unwrap().contains(&1.0), "a cancelled run reported the end at {boundary}");
        }
    }

    #[test]
    fn an_image_with_no_area_is_refused() {
        for (width, height) in [(0, 8), (8, 0), (0, 0)] {
            let handle = session(None);
            let empty = DynamicImage::ImageRgb8(ImageBuffer::new(width, height));

            let outcome = guarded(1.0).run(&empty, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false);

            assert!(
                matches!(outcome, Err(InferenceError::Untileable { .. })),
                "a {width}x{height} image produced {outcome:?}"
            );
            assert_eq!(handle.session().runs.load(Ordering::Relaxed), 0, "a refused run reached the graph");
        }
    }

    #[test]
    fn a_run_produces_the_channel_depth_that_was_asked_for() {
        for (depth, sixteen) in [(ChannelDepth::Eight, false), (ChannelDepth::Sixteen, true)] {
            let handle = session(None);
            let produced = run(&guarded(1.5), &photograph(300, 90), &handle, depth);

            assert_eq!(produced.as_rgb16().is_some(), sixteen, "{depth:?} did not produce the depth asked for");
            assert_eq!(produced.as_rgb8().is_some(), !sixteen, "{depth:?} did not produce the depth asked for");
        }
    }

    #[test]
    fn the_pipeline_is_one_stage() {
        assert_eq!(ImagePipeline::<Fake>::stages(guarded(1.0).as_ref()), 1);
    }
}
