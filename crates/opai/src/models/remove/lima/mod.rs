//! Lima: Big-LaMa (Suvorov et al., advimman/lama), the inpainter the remove family runs — the graph's contract and the
//! execution-provider profile declared for it. The removal it runs is the family's [`process`](super::process).

// Deleting the model is deleting this directory plus the arm in `RemoveVariant` that names it.
//
// The graph is the fork's own export, not a published one: `export_lima.py` in the fork's notes packs the two inputs of
// Carve/LaMa-ONNX's `lama_fp32.onnx` (sha256 1faef530…) — `image` (RGB in [0, 1]) and `mask` (1 = hole) — into one
// `input` with a `Split`, and divides its output, which is RGB in [0, 255] with the known pixels composited back, by
// 255. Checked against the source graph on random input: 4e-5 apart. Packed rather than given a two-input session seam
// of its own, so it runs through `run_graph` as Porto's RGB + trimap does.
//
// The weights are labelled Apache-2.0 and were trained on Places365, whose terms are non-commercial research: the
// reason the family is the fork's alone.

use crate::models::precision::Precision;
use crate::providers::profile::EpProfile;

/// The side of the square the graph was exported at, and the only one it accepts.
pub(crate) const SIDE: u32 = 512;

/// How many planes the graph reads: the three colour planes in `[0, 1]`, then the hole, 1 where it is to be filled.
pub(crate) const PLANES_IN: usize = 4;

/// How many planes the graph returns: the filled photograph, RGB in `[0, 1]`.
pub(crate) const PLANES_OUT: usize = 3;

/// The execution-provider tuning declared for this model: the CPU under `Auto`, and the provider defaults otherwise.
pub(crate) fn profile(_precision: Precision) -> EpProfile {
    // Measured on an M4 Pro (24 GB) with this crate's ONNX Runtime, release build, a 512 crop of the bonsai (the
    // `live` check) and the 21-megapixel street photograph (the CLI, `/usr/bin/time -l`):
    //
    //   CPU      first run in a process 1.6 s (0.5 s of it opening the session), then 1.07 s; peak 2.8 GB
    //   Core ML  first ever 6.5 s (the compile, then cached in `engines/`), first in a process 2.1 s, then 0.40 s;
    //            peak 7.2 GB
    //
    // Core ML takes about half of Big-LaMa's 7917 nodes, in 585 partitions (its FFT convolutions stay on the CPU), and
    // its output agrees with the CPU's to a level on 16 pixels of 19 million. So the accelerator is faster once warm but
    // costs four and a half gigabytes more at its peak, beside whatever else a GUI session holds resident, and a
    // one-shot CLI run is slower on it. The CPU is the default under `Auto`; a provider asked for by name is honoured.
    // Not measured on CUDA, TensorRT or WebGPU.
    EpProfile { cpu_on_auto: true, ..EpProfile::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::precision::FloatPrecision;
    use crate::models::remove::RemoveVariant;

    #[test]
    fn lima_asks_for_the_cpu_under_auto_through_its_variant() {
        let profile = RemoveVariant::Lima(FloatPrecision::Fp32).profile();

        assert!(profile.cpu_on_auto);
        assert_eq!(
            profile,
            EpProfile { cpu_on_auto: true, ..EpProfile::default() },
            "Lima declared more than the CPU"
        );
    }
}
