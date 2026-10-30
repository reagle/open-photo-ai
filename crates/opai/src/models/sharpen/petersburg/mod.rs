//! Petersburg: the NAFNet sharpener — the magnitude past which its output is discarded, the anchor that takes out the
//! texture it paints over flat areas, and the execution-provider profile measured for it. The sharpening it runs is the
//! shared [`filter`](crate::models::filter).

// Deleting the model is deleting this directory plus the arms in `SharpenVariant` that name it.

use crate::models::filter::{Anchor, Guard};
use crate::models::precision::Precision;
use crate::providers::profile::EpProfile;

// Petersburg is NAFNet, the architecture Stockholm is, and it shares the failure: on a tile unlike anything it was
// trained on its raw output can go to 1000 and beyond, and decoded it would be a solid saturated block in the middle
// of the photograph. Legitimate output is of order 1 — the graph reads and writes `[0, 1]` — so 3.0 sits safely above
// anything a working tile produces and far below the blow-up.
//
// **The threshold is the reference's, carried across on trust.** No real photograph known to make Petersburg diverge
// has been observed in this project, so what is verified here is the mechanism — against a fake model made to
// explode, in `models::filter` — and not that 3.0 is where to catch it. The live check reports how many tiles the
// guard kept on the photograph it runs; one that trips it can be added as a fixture without the contract moving.
//
// **The invention limit is measured, and the magnitude alone was not enough.** A synthetic 3000x2000 gradient with fine
// grain (`/Volumes/opai-bench/overnight/sharpen/in/synth.png`) makes it diverge on 21 of its 117 tiles at FP32 and NaN
// on 19 at FP16, which the magnitude catches; but one more tile, at (2640, 1200), only starts to: its raw output stays
// finite and within 1.4 at FP16 and 2.5 at FP32, under the threshold, while it paints a speckle up to 1.65 of the range
// over a flat area, a patch a tile wide in the photograph. What it changed finer than the anchor's box was 13 times
// the detail its input had there at FP16 and 33 at FP32. A working tile measured at most 1.77 over thirteen AIM-500
// photographs (`o_093764ca`, `o_508fa8da`, `o_f12a8afc` at both precisions, and every fiftieth of the set at FP16),
// and at most 1.97 over the gradient's other tiles, so 5 sits between them by a factor of about 2.5 each way. On the
// three photographs run end to end, the result is identical to the magnitude-only guard's to the last bit.
/// How a tile's raw output is judged a blow-up, after which the tile keeps its own input: past the reference's
/// magnitude, or inventing more than five times the fine detail its input had.
pub(crate) const GUARD: Guard = Guard::magnitude(3.0).with_invention(5.0);

// **Petersburg paints a faint mottle over flat areas, and its anchor is set to take it out.** Over a smooth gradient or
// a sky its change is largest between 16 and 64 pixels, where the input has almost no grain to strengthen, and it is
// uncorrelated with the input in every band from 4 pixels up: not sharpening but texture of the model's own, at FP16
// and FP32 alike. Measured at the default strength, held at the radius of 16 the tile drift needs, over the synthetic
// 3000x2000 gradient and `o_f12a8afc`'s sky: about half a grey level RMS in each of those bands, and 4.5 and 3.2 levels
// from the 1st to the 99th percentile once low-passed at 3 pixels, against Moscow's 0.5 — visible on a smooth 8-bit
// gradient, where a change of about one level at these scales is at the edge of what can be seen. Its tiles drift too,
// by at most 1.5 levels, which any anchor takes out.
//
// The radius is measured against both halves of the job: the mottle above 16 pixels, and what Petersburg adds to strong
// edges between 2 and 8. Simulated over the whole photograph, then confirmed end to end at the radius chosen: at 2 (a
// spread of 2 pixels) the mottle goes to 0.5 but 4 to 8 pixels at edges starts to go and 8 to 16 loses half its gain;
// at 4 and 5 the mottle stays at 1.1 and 1.4, about the level that can be seen; at 7, 2.1. At 3 the mottle is 0.7 to
// 0.8, between what Moscow and Novgorod leave, the gain at edges between 2 and 8 pixels moves by at most 0.01 on six
// photographs, and between 8 and 16 it gives up at most 0.03 of a gain of up to 1.09. Over the whole tile rather than
// weighted to flat areas: applied everywhere it already leaves the edges' fine gain alone. End to end at 3, against 16:
// the mottle 4.5 to 0.7 over the gradient and 3.2 to 0.7 over the sky, the seam steps 0.17 to 0.02, and the run no
// slower, the box's cost not depending on its radius.
/// How Petersburg's tiles are held to their input: what it changed coarser than a box 3 pixels in radius, twice, is
/// taken back out.
pub(crate) const ANCHOR: Anchor = Anchor { radius: 3 };

/// The execution-provider tuning measured for this model, at the precision it carries: the provider defaults at both
/// precisions, because nothing measured beat them, except for the nodes WebGPU cannot run correctly at FP16.
pub(crate) fn profile(precision: Precision) -> EpProfile {
    // **Measured, and the default won.** Ported from the reference's `petersburg.go` (not re-measured here), whose
    // sweep is written down so that nobody pays for it again — and so that nobody "fills in" Moscow's setting because
    // the three models share a family.
    //
    // The fact that decides everything else: CoreML takes this graph as **one** partition and places all of it on the
    // GPU, at both precisions, without being told to. That is the opposite of Moscow and Novgorod, and the reason is
    // architectural. They are Restormers, whose blocks are normalization and transpose around a channel-attention
    // matmul, and CoreML reaches for the Neural Engine there and loses. NAFNet is a plain convolutional U-Net — 226
    // convolutions, half the estimated graph cost — and CoreML declines the Neural Engine on its own, so the default
    // compute units and `CpuAndGpu` are the same session.
    //
    // Measured by the reference on an M2 Max, macOS 26.6, ONNX Runtime 1.26, per 256x256 tile:
    //
    //   MLComputeUnits           FP32               FP16
    //   ALL (default)            34.4ms             34.3ms
    //   CPUAndGPU                34.2ms   (-0.5%)   34.4ms  (+0.2%)
    //   CPUAndNeuralEngine      248.1ms  (+622%)    37.0ms  (+7.7%)
    //   CPUOnly / CPU provider  471.9ms            468.6ms
    //
    // So the setting worth 45% on Moscow is worth nothing here, and declaring it would only restate a choice CoreML
    // has already made.
    //
    // Fast-prediction, sequential execution mode and low-precision GPU accumulation all measured within run-to-run
    // spread at both precisions, with the order alternated per round.
    //
    // One finding that is not a tuning result but changes what to recommend: the FP16 export is not faster. End to end
    // over the 640x640 sample it is 328ms against FP32's 328ms, because CoreML compiles both to the same GPU program.
    // Unlike Novgorod, where the precision choice is also a speed choice, FP16 here buys download size and nothing
    // else.
    //
    // Nothing is declared for speed, at either precision. CUDA and TensorRT were not measured.
    //
    // The WebGPU nodes at FP16 are correctness, not tuning. The published FP16 export keeps its layer norms' squares
    // and variances in FP32 — they reach 10.9 million, which FP16 cannot hold, and the CPU and CoreML only hid that by
    // promoting them on their own. What WebGPU still gets wrong after that is its own: it sums the full-resolution
    // blocks' two channel-attention `ReduceMean`s over the whole 256x256 plane in FP16, which leaves the tile at 34 dB
    // against FP32. Left to the CPU provider those two bring it to 67 dB. Measured on an M2 Max against the WebGPU
    // plugin 0.4.0 and ONNX Runtime 1.30, per graph: +10.2% on WebGPU, 67.4 ms to 74.3 ms. FP32 needs none.
    EpProfile::default().with_webgpu_cpu_nodes_at_fp16(precision, WEBGPU_CPU_NODES_FP16)
}

/// The nodes WebGPU is kept off at FP16, as the graph names them — see `profile`.
const WEBGPU_CPU_NODES_FP16: &[&str] = &["node_mean_2", "node_mean_177"];

#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::precision::FloatPrecision;
    use crate::models::sharpen::SharpenVariant;
    use crate::providers::profile::{CoreMlComputeUnits, CoreMlSpecialization, ExecutionMode};

    #[test]
    fn petersburg_is_anchored_at_the_radius_measured_to_take_out_its_mottle() {
        // Pinned as a literal, for the reason `ANCHOR` gives, and finer than the family's other two, which is the
        // point of declaring it here.
        assert_eq!(ANCHOR, Anchor { radius: 3 });

        for precision in FloatPrecision::ALL {
            assert_eq!(SharpenVariant::Petersburg(precision).anchor(), Some(ANCHOR), "{precision:?}");
        }
    }

    #[test]
    fn the_guard_is_the_references_threshold_and_the_measured_invention_limit() {
        // Pinned as literals, for the reason `GUARD` gives: the threshold is carried over and the limit measured, so
        // nothing else would notice either moving.
        assert_eq!(GUARD, Guard { magnitude: 3.0, invention: Some(5.0), rescue: None });

        for precision in FloatPrecision::ALL {
            assert_eq!(SharpenVariant::Petersburg(precision).guard(), Some(GUARD), "{precision:?}");
        }
    }

    // The measured outcome, pinned in the shape of Gothenburg's four so that it reads as a measurement rather than as
    // a profile nobody got round to. Each is asked through the variant, whose match is the half a refactor can break.

    #[test]
    fn petersburg_is_left_on_coremls_own_compute_units_at_both_precisions() {
        // The one a reader is most likely to "fix" is `CpuAndGpu`, copied across from the family's two Restormers. It
        // compiles to the same session here, so it would buy nothing and claim a finding that was never made.
        for precision in FloatPrecision::ALL {
            assert_eq!(
                SharpenVariant::Petersburg(precision).profile().coreml_compute_units,
                CoreMlComputeUnits::default(),
                "{precision:?}"
            );
        }
    }

    #[test]
    fn petersburg_does_not_ask_for_one_node_at_a_time_at_either_precision() {
        for precision in FloatPrecision::ALL {
            assert_eq!(
                SharpenVariant::Petersburg(precision).profile().execution_mode,
                ExecutionMode::default(),
                "{precision:?}"
            );
        }
    }

    #[test]
    fn petersburg_does_not_ask_coreml_for_fast_prediction() {
        for precision in FloatPrecision::ALL {
            assert_eq!(
                SharpenVariant::Petersburg(precision).profile().coreml_specialization,
                CoreMlSpecialization::default(),
                "{precision:?}"
            );
        }
    }

    #[test]
    fn petersburg_declares_only_its_webgpu_nodes_and_only_at_fp16() {
        // Compared as whole profiles, so a field added to `EpProfile` later is covered by existing.
        assert_eq!(
            SharpenVariant::Petersburg(FloatPrecision::Fp16).profile(),
            EpProfile::default().with_webgpu_cpu_nodes_at_fp16(Precision::Fp16, WEBGPU_CPU_NODES_FP16)
        );
        assert_eq!(SharpenVariant::Petersburg(FloatPrecision::Fp32).profile(), EpProfile::default());
    }

    #[test]
    fn the_webgpu_list_is_the_two_means_measured() {
        // Pinned as literals: the plugin ignores a name it does not find, so a rename in a re-export would bring the
        // FP16 error back with nothing failing.
        assert_eq!(WEBGPU_CPU_NODES_FP16, ["node_mean_2", "node_mean_177"]);
    }
}
