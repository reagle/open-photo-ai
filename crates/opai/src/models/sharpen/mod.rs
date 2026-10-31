//! The sharpen family: three variants, each one graph per precision, run at a per-run strength.

#[cfg(test)]
mod live;
pub(crate) mod moscow;
pub(crate) mod novgorod;
pub(crate) mod petersburg;
pub(crate) mod variant;

use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};
use super::simple::single_artifact;
use super::strength::Strength;

use crate::models::filter;
use crate::pipeline::Backend;
use crate::pipeline::Shared;
use crate::providers::profile::EpProfile;
use imaging::TileGeometry;
use imaging::tensor::Normalisation;

pub(crate) use variant::ALL;
pub use variant::SharpenVariant;

// `[0, 1]` is the reference's `standardize: false`, stated by this family for itself rather than borrowed from
// denoise's: the two agree because each of the reference's `process.go` files says so, not because one follows the
// other. Pinned as a literal and asserted below, because feeding a graph the wrong range is not an error a runtime
// reports: it is a worse photograph.
/// The range this family's graphs read and write: `[0, 1]`.
const RANGE: Normalisation = Normalisation::Unit;

/// One sharpen run: which model, and how much of its output to apply.
///
/// This value **is** the operation's identity, as [`super::upscale::Upscale`]'s is. Nothing here owns a native
/// resource: it is a plain `Copy` value describing *what* to run, which is what lets a front end hold and persist one
/// independently of any session built from it.
///
/// Build this; keep it as an [`Operation::Sharpen`] — see
/// [`Operation`] for which of the two to reach for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sharpen {
    /// The model and its precision.
    variant: SharpenVariant,
    /// How much of the model's output this run applies, and part of the operation's identity: two values of it are
    /// two requests.
    strength: Strength,
}

impl Sharpen {
    /// The operation running `variant` at `strength`.
    pub const fn new(variant: SharpenVariant, strength: Strength) -> Self {
        Self { variant, strength }
    }

    /// Moscow at `precision`, applying `strength` of its output.
    ///
    /// Returns the carrier rather than `Self`, so a caller collects operations of several families into one chain
    /// with no per-element wrapping; the typed value stays reachable by matching it. Published at FP32 and FP16.
    pub fn moscow(precision: FloatPrecision, strength: Strength) -> Operation {
        Operation::Sharpen(Self::new(SharpenVariant::Moscow(precision), strength))
    }

    /// Petersburg at `precision`, applying `strength` of its output. Published at FP32 and FP16; see [`Sharpen::moscow`].
    pub fn stpetersburg(precision: FloatPrecision, strength: Strength) -> Operation {
        Operation::Sharpen(Self::new(SharpenVariant::Petersburg(precision), strength))
    }

    /// Novgorod at `precision`, applying `strength` of its output. Published at FP32 and FP16; see [`Sharpen::moscow`].
    pub fn novgorod(precision: FloatPrecision, strength: Strength) -> Operation {
        Operation::Sharpen(Self::new(SharpenVariant::Novgorod(precision), strength))
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(self) -> SharpenVariant {
        self.variant
    }

    /// How much of the model's output this run applies, at the value it was quantized to.
    pub const fn strength(self) -> Strength {
        self.strength
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning measured for the model this runs, at the precision it runs at.
    pub(crate) fn profile(self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Moscow (FP32)`, without the strength.
    pub fn display_name(self) -> String {
        // The strength does not select the weights, so a name fixed at whichever value happened to load them first
        // would be wrong from the second run onwards.
        self.to_string()
    }

    /// What this run needs beyond the operation itself.
    pub const fn params(self) -> SharpenParams {
        SharpenParams { strength: self.strength }
    }

    /// The pipeline this operation runs — the **family's contract seam**, where a variant becomes a run.
    pub(crate) fn pipeline<B: Backend>(self) -> Shared<B> {
        // One arm, because the three variants differ in nothing that reaches `filter` but their weights, their guard,
        // their anchor and their profile — not the tiling, not the normalisation, not what the strength means. No
        // `Result`: there is no state here to refuse.
        //
        // Built from what `params` publishes rather than from this type's own fields, so a pipeline cannot be
        // constructed from something the library did not say a run needs. The guard and the anchor come from the
        // variant; see `SharpenVariant::guard` and `SharpenVariant::anchor`.
        filter::pipeline::<B>(
            self.display_name(),
            self.artifact(),
            self.profile(),
            RANGE,
            self.params().strength,
            self.variant.guard(),
            self.variant.anchor(),
            TileGeometry::DEFAULT,
        )
    }

    /// The artifact serving this operation.
    pub fn artifact(self) -> ArtifactId {
        single_artifact(Family::Sharpen, self.variant.codename(), self.precision())
    }

    /// The artifacts that must be on disk before this operation can run.
    ///
    /// Always exactly one: every sharpen variant is published as a single graph.
    pub fn required_artifacts(self) -> Vec<ArtifactId> {
        // A `Vec` to match `Upscale::required_artifacts`, so that whoever downloads what an operation needs asks every
        // family the same question.
        vec![self.artifact()]
    }

    /// The key the run cache stores this operation's result under.
    ///
    /// Carries the strength, which the tag has to spell itself because it is written out by hand rather than derived
    /// from the identity: two strengths are one model and two images, so a tag that left it out would serve the
    /// second run the first one's picture. Hyphen-separated, so it cannot be mistaken for an underscore-separated
    /// artifact name. Ends in the filter's [`FILTER_VERSION`](filter::FILTER_VERSION), so an image an older build drew
    /// is not served to a newer one.
    pub fn cache_tag(self) -> String {
        // Written deliberately rather than derived from the serialized form, so that renaming a field cannot silently
        // invalidate every cached image.
        format!(
            "{}-{}-{}-t{}-v{}",
            Family::Sharpen.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.strength,
            filter::FILTER_VERSION
        )
    }
}

/// What one sharpen run takes, in the shape its pipeline wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharpenParams {
    // A typed value rather than a map of strings to anything, for the reason `UpscaleParams` gives.
    /// How much of the model's output this run applies.
    pub strength: Strength,
}

impl std::fmt::Display for Sharpen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

#[cfg(test)]
mod tests {
    use super::variant::tests::every_variant;
    use super::*;
    use crate::models::precision::FloatPrecision;

    fn strength(value: f64) -> Strength {
        Strength::new(value).expect("the test supplied a strength in range")
    }

    crate::models::test_support::strength_family_tests! {
        Sharpen, SharpenVariant { Moscow, Petersburg, Novgorod },
        params: SharpenParams,
        every_variant: every_variant,
    }

    #[test]
    fn the_graph_is_fed_the_unit_range_rather_than_the_signed_one() {
        // The literal, pinned rather than left to be read off a call, for the reason `RANGE` gives.
        assert_eq!(RANGE, Normalisation::Unit, "the family's graphs were switched to the signed range");
    }

    #[test]
    fn every_variant_names_the_artifact_it_is_published_as() {
        // Transcribed from the reference implementation's `models/ids_test.go`, which pins these as literals because
        // they key the on-disk cache and name the downloaded files.
        let expected = [
            (SharpenVariant::Moscow(FloatPrecision::Fp32), "sh_moscow_fp32"),
            (SharpenVariant::Moscow(FloatPrecision::Fp16), "sh_moscow_fp16"),
            (SharpenVariant::Petersburg(FloatPrecision::Fp32), "sh_petersburg_fp32"),
            (SharpenVariant::Petersburg(FloatPrecision::Fp16), "sh_petersburg_fp16"),
            (SharpenVariant::Novgorod(FloatPrecision::Fp32), "sh_novgorod_fp32"),
            (SharpenVariant::Novgorod(FloatPrecision::Fp16), "sh_novgorod_fp16"),
        ];

        for (variant, name) in expected {
            assert_eq!(Sharpen::new(variant, strength(1.0)).artifact().as_str(), name);
        }
    }

    #[test]
    fn the_cache_tag_is_the_written_one_rather_than_whatever_serialization_produces() {
        // Pinned here so that renaming a field is a failing test rather than a silent invalidation of every cached
        // image on every user's disk.
        assert_eq!(
            Sharpen::new(SharpenVariant::Moscow(FloatPrecision::Fp32), strength(0.5)).cache_tag(),
            "sh-moscow-fp32-t0.5-v4"
        );
        assert_eq!(
            Sharpen::new(SharpenVariant::Novgorod(FloatPrecision::Fp16), strength(1.0)).cache_tag(),
            "sh-novgorod-fp16-t1-v4"
        );
    }

    #[test]
    fn the_cache_tag_carries_the_filter_version_so_an_older_builds_image_is_not_served() {
        for operation in every_variant().into_iter().map(|variant| Sharpen::new(variant, strength(1.0))) {
            assert!(
                operation.cache_tag().ends_with(&format!("-v{}", filter::FILTER_VERSION)),
                "{} carries no filter version",
                operation.cache_tag()
            );
        }
    }

    #[test]
    fn a_display_name_is_the_label_and_the_precision_and_carries_no_strength() {
        for value in [0.0, 0.5, 2.5] {
            let operation = Sharpen::new(SharpenVariant::Moscow(FloatPrecision::Fp32), strength(value));

            assert_eq!(operation.display_name(), "Moscow (FP32)");
            assert_eq!(operation.to_string(), operation.display_name());
        }

        assert_eq!(
            Sharpen::new(SharpenVariant::Novgorod(FloatPrecision::Fp16), strength(1.0)).display_name(),
            "Novgorod (FP16)"
        );
    }

    #[test]
    fn the_serialized_form_names_its_variant_rather_than_positioning_it() {
        // Tagged by name so that adding a family or a variant later cannot invalidate anything already persisted.
        let json = serde_json::to_string(&Sharpen::new(SharpenVariant::Moscow(FloatPrecision::Fp32), strength(0.5)))
            .expect("an operation serializes");

        assert_eq!(json, r#"{"variant":{"codename":"moscow","precision":"fp32"},"strength":0.5}"#);
    }

    #[test]
    fn a_serialized_operation_pairing_a_variant_with_int8_is_refused() {
        let json = r#"{"variant":{"codename":"moscow","precision":"int8"},"strength":0.5}"#;

        assert!(
            serde_json::from_str::<Sharpen>(json).is_err(),
            "deserialization produced a sharpen at a precision it is not published in"
        );
    }

    #[test]
    fn a_serialized_operation_carrying_an_out_of_range_strength_is_refused() {
        let json = r#"{"variant":{"codename":"moscow","precision":"fp32"},"strength":4.0}"#;

        assert!(
            serde_json::from_str::<Sharpen>(json).is_err(),
            "deserialization produced an operation whose strength could not have been constructed"
        );
    }

    // What only this family knows, run through its own seam against the shared fake: which of its models are guarded.
    // `filter`'s own suite proves the mechanism once; these prove the seam hands it to the right models.

    /// The pipeline for `variant` at a strength of 1, built through the family's own seam.
    fn sharpening(variant: SharpenVariant) -> Shared<crate::pipeline::test_support::Fake> {
        Sharpen::new(variant, strength(1.0)).pipeline()
    }

    #[test]
    fn a_tile_petersburg_diverges_on_keeps_the_photographs_own_pixels() {
        // Fails if the seam stops handing `SharpenVariant::guard` to the pipeline: the first tile would then be the
        // model's, with one white pixel in it.
        use crate::pipeline::test_support::{PAIR, original, session};
        use imaging::ChannelDepth;
        use imaging::tensor::Sampler;
        use imaging::test_support::photograph;

        let source = photograph(PAIR.0, PAIR.1);
        let sampler = Sampler::new(&source);

        for precision in FloatPrecision::ALL {
            let handle = session(Some((0, 4.0)));
            let produced = sharpening(SharpenVariant::Petersburg(precision))
                .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
                .expect("a sharpen over a photograph");
            let produced = produced.as_rgb8().expect("eight-bit");

            for (x, y) in [(0, 0), (10, 150), (200, 30)] {
                assert_eq!(produced.get_pixel(x, y).0, original::<u8>(&sampler, x, y), "{precision:?} ({x}, {y})");
            }
        }
    }

    #[test]
    fn moscow_and_novgorod_pass_an_exploding_tile_through() {
        // The same backend that trips Petersburg's guard, and nothing is kept: the exploded value is the model's, and
        // reaches the photograph as white. The fake writes it into the middle of the green plane: row 128, column 0.
        use crate::pipeline::test_support::{PAIR, session};
        use imaging::ChannelDepth;
        use imaging::test_support::photograph;

        let source = photograph(PAIR.0, PAIR.1);

        for variant in [SharpenVariant::Moscow(FloatPrecision::Fp32), SharpenVariant::Novgorod(FloatPrecision::Fp16)] {
            let handle = session(Some((0, 4.0)));
            let produced = sharpening(variant)
                .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
                .expect("a sharpen over a photograph");
            let produced = produced.as_rgb8().expect("eight-bit");

            assert_eq!(
                produced.get_pixel(0, 128).0[1],
                u8::MAX,
                "{variant:?} kept the tile rather than using the model's"
            );
        }
    }

    #[test]
    fn every_variant_runs_exactly_the_filter_its_own_guard_and_anchor_describe() {
        // Bit for bit against the shared pipeline built by hand. Moscow and Novgorod against the radius-16 anchor the
        // family gave every variant before each model declared its own, so moving the declaration changed neither of
        // them; Petersburg against its own radius, and its result differs from what 16 made, so the comparison can
        // tell the two apart.
        use crate::models::filter::Anchor;
        use crate::pipeline::test_support::{Fake, PAIR, session};
        use imaging::ChannelDepth;
        use imaging::test_support::photograph;

        let source = photograph(PAIR.0, PAIR.1);
        let through = |pipeline: Shared<Fake>| {
            pipeline
                .run(&source, std::slice::from_ref(&session(None)), ChannelDepth::Eight, None, &|| false)
                .expect("a sharpen over a photograph")
                .into_rgb8()
        };
        let by_hand = |variant: SharpenVariant, anchor: Anchor| {
            let operation = Sharpen::new(variant, strength(1.0));

            through(filter::pipeline::<Fake>(
                operation.display_name(),
                operation.artifact(),
                operation.profile(),
                RANGE,
                strength(1.0),
                variant.guard(),
                Some(anchor),
                TileGeometry::DEFAULT,
            ))
        };

        for variant in every_variant() {
            let produced = through(sharpening(variant));
            let before = by_hand(variant, Anchor { radius: 16 });

            if let SharpenVariant::Petersburg(_) = variant {
                assert!(produced == by_hand(variant, petersburg::ANCHOR), "{variant:?} is not its own anchor's run");
                assert!(produced != before, "{variant:?} came out as the radius of 16 makes it");
            } else {
                assert!(produced == before, "{variant:?} moved from the run it made before");
            }
        }
    }

    #[test]
    fn every_variant_is_held_to_the_photographs_local_brightness() {
        // Fails if the seam stops handing the anchor to the pipeline. The fake halves every value, which over a flat
        // photograph is a change with nothing in it finer than the whole tile: held to its input's local brightness,
        // it is no change at all. Without the anchor every pixel would come out at half its level.
        use crate::pipeline::test_support::{PAIR, session};
        use image::{DynamicImage, ImageBuffer, Rgb};
        use imaging::ChannelDepth;

        let colour = [200, 120, 40];
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(PAIR.0, PAIR.1, Rgb(colour)));

        for variant in every_variant() {
            let handle = session(None);
            let produced = sharpening(variant)
                .run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
                .expect("a sharpen over a photograph");
            let produced = produced.as_rgb8().expect("eight-bit");

            for (x, y, pixel) in produced.enumerate_pixels() {
                assert!(
                    pixel.0.iter().zip(colour).all(|(got, want)| got.abs_diff(want) <= 1),
                    "{variant:?} ({x}, {y}) came out at {:?} rather than the photograph's {colour:?}",
                    pixel.0
                );
            }
        }
    }
}
