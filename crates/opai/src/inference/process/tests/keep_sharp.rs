//! A mask under a lens blur: the blend before the blur, the mask kept in focus, and what each of the two uses costs.

use super::*;
use crate::mask::{Base, Mask, Plane};
use crate::models::{
    Bias, Denoise, FNumber, Fraction, LensBlur, LensBlurEdges, LightAdjustment, LightAdjustmentVariant, Strength,
    Subject,
};

/// The landscape depth graph.
const LANDSCAPE: &str = "dp_lisbon_landscape_fp16";

/// A whole-image operation that changes every pixel.
fn brighten() -> Operation {
    Operation::LightAdjustment(LightAdjustment::new(
        LightAdjustmentVariant::Paris(FloatPrecision::Fp16),
        Bias::new(0.5).unwrap(),
    ))
}

fn denoise() -> Operation {
    Denoise::stockholm(FloatPrecision::Fp32, Strength::new(0.5).expect("in range"))
}

/// A lens blur at f/1 focused near the left edge of [`checkered`], where the fake depth is nearest: the right half,
/// farthest, is blurred the most.
fn blur(edges: LensBlurEdges) -> Operation {
    let Operation::LensBlur(blur) =
        LensBlur::lisbon(FNumber::new(1.0).expect("in range"), Fraction::new(0.1).expect("in range"), Fraction::CENTRE)
    else {
        unreachable!("Lisbon builds a lens blur");
    };

    Operation::LensBlur(blur.with_edges(edges))
}

/// The lens blur tests' checkerboard, which the fake depth puts two to five metres away, left to right.
fn checkered() -> Picture {
    let pixels = image::RgbImage::from_fn(960, 640, |x, y| {
        if (x / 4 + y / 4) % 2 == 0 { image::Rgb([230, 200, 40]) } else { image::Rgb([20, 40, 120]) }
    });

    Picture::new("/pictures/holiday.jpg", DynamicImage::ImageRgb8(pixels), "cafebabecafebabe")
}

/// A mask white on the right of column `split` of the checkerboard, used as `uses` say: (limits, keeps sharp).
fn right_of(split: u32, (limits_enhancements, keeps_sharp): (bool, bool)) -> Mask {
    let values = (0..960 * 640).map(|index| if index % 960 < split { 0.0 } else { 1.0 }).collect();

    Mask {
        base: Some(Base::new(Plane::from_vec(960, 640, values).unwrap())),
        limits_enhancements,
        keeps_sharp,
        ..Mask::default()
    }
}

/// `chain` over the checkerboard, through `cache` where there is one, limited and kept in focus by `mask` as it says.
async fn masked(backend: &Fake, cache: Option<&Memo>, chain: &[Operation], mask: Option<Mask>) -> Picture {
    let options = ProcessOptions { mask: mask.map(Arc::new), ..Default::default() };
    let picture = process(backend, cache, &checkered(), chain, Some(options))
        .await
        .expect("the chain runs")
        .picture;
    if cache.is_some() {
        settled();
    }

    picture
}

/// The mean difference, in levels, between `one` and `other` over `columns` of every eighth row.
fn mean_difference(one: &image::RgbImage, other: &image::RgbImage, columns: std::ops::Range<u32>) -> f64 {
    let pairs: Vec<(u8, u8)> = (0..640)
        .step_by(8)
        .flat_map(|y| columns.clone().map(move |x| (x, y)))
        .flat_map(|(x, y)| one.get_pixel(x, y).0.into_iter().zip(other.get_pixel(x, y).0))
        .collect();

    pairs.iter().map(|(one, other)| f64::from(one.abs_diff(*other))).sum::<f64>() / pairs.len() as f64
}

/// Whether `report` is of a step of `family`.
fn of_family(report: &InferenceProgress, family: Family) -> bool {
    matches!(&*report.subject, Subject::Enhancement(operation) if operation.family() == family)
}

/// How many times the depth graph ran.
fn estimates(backend: &Fake) -> usize {
    backend.log().ran.iter().filter(|artifact| *artifact == LANDSCAPE).count()
}

#[tokio::test]
async fn a_mask_kept_in_focus_is_the_chains_output_before_the_blur_and_the_rest_is_blurred() {
    let backend = Fake::new();
    let chain = [brighten(), blur(LensBlurEdges::Depth)];

    let brightened = masked(&backend, None, &chain[..1], None).await.pixels().to_rgb8();
    let blurred = masked(&backend, None, &chain, None).await.pixels().to_rgb8();
    let kept = masked(&backend, None, &chain, Some(right_of(480, (false, true)))).await.pixels().to_rgb8();

    // Where the weight is 1 the radius is 0: the pixel is the brightened one exactly, though it is the most blurred.
    for x in 482..960 {
        for y in (0..640).step_by(7) {
            assert_eq!(kept.get_pixel(x, y), brightened.get_pixel(x, y), "({x}, {y}) was not kept in focus");
        }
    }
    assert_ne!(
        blurred.get_pixel(900, 320),
        brightened.get_pixel(900, 320),
        "the right was not blurred to begin with"
    );

    // The left, which the mask does not cover, is brightened too, and blurred as it was but for the layers the render
    // spaces over a smaller span of radii.
    let off = mean_difference(&kept, &blurred, 150..400);
    assert!(off < 2.0, "the left is {off} levels off its blur without a mask");
}

#[tokio::test]
async fn a_limiting_mask_is_blended_before_the_blur_so_what_it_leaves_out_is_the_photograph_blurred() {
    let backend = Fake::new();
    let chain = [brighten(), blur(LensBlurEdges::Depth)];

    let brightened = masked(&backend, None, &chain[..1], None).await.pixels().to_rgb8();
    // The photograph under the same blur, keeping the same half in focus.
    let photograph_blurred =
        masked(&backend, None, &chain[1..], Some(right_of(480, (false, true)))).await.pixels().to_rgb8();
    let both = masked(&backend, None, &chain, Some(right_of(480, (true, true)))).await.pixels().to_rgb8();

    // Under the mask, enhanced and sharp; beyond it, not enhanced, and blurred as the photograph is.
    for x in (490..960).step_by(5) {
        assert_eq!(
            both.get_pixel(x, 320),
            brightened.get_pixel(x, 320),
            "({x}, 320) is not the enhancement in focus"
        );
    }
    let off = mean_difference(&both, &photograph_blurred, 150..400);
    assert!(off < 0.5, "the left is {off} levels off the photograph blurred");
    assert_ne!(both.get_pixel(300, 320), brightened.get_pixel(300, 320), "the left was enhanced");
}

#[tokio::test]
async fn a_blend_at_one_and_at_zero_hands_the_blur_the_enhancement_and_the_photograph_exactly() {
    let backend = Fake::new();
    let chain = [brighten(), blur(LensBlurEdges::Depth)];

    let whole = masked(&backend, None, &chain, None).await;
    let photograph_blurred = masked(&backend, None, &chain[1..], None).await;

    // Limiting only, so the blur's own render is the one without a mask.
    let white = masked(&backend, None, &chain, Some(right_of(0, (true, false)))).await;
    let black = masked(&backend, None, &chain, Some(right_of(960, (true, false)))).await;

    assert_eq!(white.pixels().as_bytes(), whole.pixels().as_bytes(), "a white mask changed the chain");
    assert_eq!(
        black.pixels().as_bytes(),
        photograph_blurred.pixels().as_bytes(),
        "a black mask let the enhancement in"
    );
    assert_ne!(white.identity(), whole.identity(), "a mask that is white today is painted on tomorrow");
}

#[tokio::test]
async fn a_use_the_chain_gives_nothing_to_do_is_no_mask() {
    let backend = Fake::new();
    let (enhancement, blur_alone) = ([brighten()], [blur(LensBlurEdges::Depth)]);

    // Keeping in focus with no lens blur, and limiting a lens blur alone, each change nothing, identity included.
    let brightened = masked(&backend, None, &enhancement, None).await;
    let limited = masked(&backend, None, &enhancement, Some(right_of(480, (true, false)))).await;
    assert_ne!(limited.identity(), brightened.identity());

    // And the limit is switched off only under a lens blur: without one it is what the mask is for, and a mask switched
    // to focus alone limits the enhancements as one with both uses does.
    let focus_only = masked(&backend, None, &enhancement, Some(right_of(480, (false, true)))).await;
    let both = masked(&backend, None, &enhancement, Some(right_of(480, (true, true)))).await;
    assert_eq!(focus_only.identity(), limited.identity());
    assert_eq!(focus_only.pixels().as_bytes(), limited.pixels().as_bytes());
    assert_eq!(both.identity(), limited.identity());

    let plain = masked(&backend, None, &blur_alone, None).await;
    let limit_only = masked(&backend, None, &blur_alone, Some(right_of(480, (true, false)))).await;
    assert_eq!(limit_only.identity(), plain.identity());
    assert_eq!(limit_only.pixels().as_bytes(), plain.pixels().as_bytes());

    // And a mask kept in focus is a lens blur of its own, named so, and holds its half as the photograph.
    let kept = masked(&backend, None, &blur_alone, Some(right_of(480, (true, true)))).await;
    assert_ne!(kept.identity(), plain.identity());
    assert_eq!(kept.pixels().to_rgb8().get_pixel(900, 320), checkered().pixels().to_rgb8().get_pixel(900, 320));
    assert_ne!(plain.pixels().to_rgb8().get_pixel(900, 320), checkered().pixels().to_rgb8().get_pixel(900, 320));

    // Under a lens blur and another enhancement, each choice of uses names its own result.
    let chain = [brighten(), blur(LensBlurEdges::Depth)];
    let mut names = Vec::new();
    for uses in [(true, true), (true, false), (false, true)] {
        names.push(masked(&backend, None, &chain, Some(right_of(480, uses))).await.identity().to_string());
    }
    names.push(masked(&backend, None, &chain, None).await.identity().to_string());
    let mut distinct = names.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), names.len(), "{names:?}");
}

#[tokio::test]
async fn a_mask_edit_under_a_lens_blur_estimates_nothing_and_runs_no_model_again() {
    let (backend, cache) = (Fake::new(), store());
    let chain = [brighten(), blur(LensBlurEdges::Matte)];

    for uses in [(true, true), (false, true), (true, false)] {
        let first = masked(&backend, Some(&cache), &chain, Some(right_of(480, uses))).await;
        assert_eq!(estimates(&backend), 1, "{uses:?}");
        let ran = backend.log().ran.len();

        let second = masked(&backend, Some(&cache), &chain, Some(right_of(500, uses))).await;
        assert_eq!(backend.log().ran.len(), ran, "{uses:?}: a mask edit ran a model");
        assert_ne!(second.identity(), first.identity(), "{uses:?}: two masks named one result");
        assert_ne!(second.pixels().as_bytes(), first.pixels().as_bytes(), "{uses:?}: the edit changed nothing");
    }

    // The unmasked chain is still served the same steps.
    let ran = backend.log().ran.len();
    masked(&backend, Some(&cache), &chain, None).await;
    assert_eq!(backend.log().ran.len(), ran, "the unmasked chain was not served the masked runs' steps");
}

#[tokio::test]
async fn a_chain_split_at_its_lens_blur_reports_one_bar_with_the_blur_last() {
    let (backend, cache) = (Fake::new(), store());
    let (reports, options) = recording();
    let chain = [denoise(), brighten(), blur(LensBlurEdges::Depth)];
    let options = ProcessOptions { mask: Some(Arc::new(right_of(480, (true, true)))), ..options };

    process(&backend, Some(&cache), &checkered(), &chain, Some(options))
        .await
        .expect("the chain runs");
    settled();

    let reports = reports.lock().unwrap();
    assert_never_decreases(&reports, "over a head, its blend and a lens blur");
    assert_eq!(reports.last().map(|report| report.chain_fraction), Some(1.0));

    let of = |family: Family| {
        let of = |report: &&InferenceProgress| of_family(report, family);
        let fractions: Vec<f64> = reports.iter().filter(of).map(|report| report.chain_fraction).collect();
        (
            fractions.iter().copied().fold(f64::MAX, f64::min),
            fractions.iter().copied().fold(0.0, f64::max),
        )
    };
    // Each step in its third of the bar, the lens blur's after the blend in the last one.
    for (family, third) in [(Family::Denoise, 0.0), (Family::LightAdjustment, 1.0), (Family::LensBlur, 2.0)] {
        let (low, high) = of(family);
        assert!(low >= third / 3.0 - 1e-9 && high <= (third + 1.0) / 3.0 + 1e-9, "{family:?}: {low} to {high}");
    }
}
