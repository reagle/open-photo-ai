//! A run limited to a mask: what it blends, what it runs, and what it is called.

use super::*;
use crate::mask::{Base, Mask, Plane, Stroke};
use crate::models::{
    Bias, Denoise, DenoiseVariant, LightAdjustment, LightAdjustmentVariant, Sharpen, SharpenVariant, Strength,
};

/// A whole-image operation whose result differs from the photograph everywhere, so which side of a blend a pixel came
/// from is a different number rather than the same one.
fn brighten() -> Operation {
    Operation::LightAdjustment(LightAdjustment::new(
        LightAdjustmentVariant::Paris(FloatPrecision::Fp16),
        Bias::new(0.5).unwrap(),
    ))
}

/// Denoise and then sharpen, the chain the GUI's brush is most often painted over.
fn denoise_and_sharpen() -> [Operation; 2] {
    let strength = Strength::new(0.5).unwrap();

    [
        Operation::Denoise(Denoise::new(DenoiseVariant::Stockholm(FloatPrecision::Fp32), strength)),
        Operation::Sharpen(Sharpen::new(SharpenVariant::Moscow(FloatPrecision::Fp32), strength)),
    ]
}

/// A mask whose base is `left` on the left `split` columns of a `width` x `height` plane and `right` on the others: a
/// hard edge.
fn halves(width: u32, height: u32, split: u32, (left, right): (f32, f32)) -> Mask {
    let values = (0..width * height).map(|index| if index % width < split { left } else { right }).collect();

    Mask { base: Some(Base::new(Plane::from_vec(width, height, values).unwrap())), ..Mask::default() }
}

/// `chain` over `source` through `backend`, limited to `mask` where there is one.
async fn masked_run(
    backend: &Fake,
    cache: Option<&Memo>,
    source: &Picture,
    chain: &[Operation],
    mask: Option<Mask>,
) -> Picture {
    let options = ProcessOptions { mask: mask.map(Arc::new), ..Default::default() };

    process(backend, cache, source, chain, Some(options)).await.unwrap().picture
}

#[tokio::test]
async fn a_black_mask_returns_the_photograph() {
    let backend = Fake::new();
    let source = picture(120, 80);

    let unmasked = masked_run(&backend, None, &source, &[brighten()], None).await;
    assert_ne!(
        unmasked.pixels().as_bytes(),
        source.pixels().as_bytes(),
        "the operation changed nothing to mask"
    );

    let masked = masked_run(&backend, None, &source, &[brighten()], Some(halves(120, 80, 0, (1.0, 0.0)))).await;

    assert_eq!(
        masked.pixels().as_bytes(),
        source.pixels().as_bytes(),
        "a black mask let the enhancement through"
    );
}

#[tokio::test]
async fn a_white_mask_returns_the_enhancement_under_its_own_identity() {
    let backend = Fake::new();
    let source = picture(120, 80);

    let unmasked = masked_run(&backend, None, &source, &[brighten()], None).await;
    let masked = masked_run(&backend, None, &source, &[brighten()], Some(halves(120, 80, 120, (1.0, 0.0)))).await;

    assert_eq!(masked.pixels().as_bytes(), unmasked.pixels().as_bytes(), "a white mask changed the enhancement");
    // The same pixels, but a mask that is white today is painted on tomorrow: a result is named for what it was asked.
    assert_ne!(masked.identity(), unmasked.identity());
}

#[tokio::test]
async fn half_a_mask_is_the_enhancement_on_one_side_and_the_photograph_on_the_other() {
    let backend = Fake::new();
    let source = picture(120, 80);
    let original = source.pixels().to_rgb8();

    let unmasked = masked_run(&backend, None, &source, &[brighten()], None).await.pixels().to_rgb8();
    let masked = masked_run(&backend, None, &source, &[brighten()], Some(halves(120, 80, 60, (1.0, 0.0))))
        .await
        .pixels()
        .to_rgb8();

    for (x, y, pixel) in masked.enumerate_pixels() {
        if x < 58 {
            assert_eq!(pixel, unmasked.get_pixel(x, y), "({x}, {y}) on the white side is not the enhancement");
        } else if x >= 62 {
            assert_eq!(pixel, original.get_pixel(x, y), "({x}, {y}) on the black side is not the photograph");
        }
    }
}

#[tokio::test]
async fn a_masked_upscale_shows_a_lanczos_enlargement_where_the_mask_is_black() {
    let backend = Fake::new();
    let source = picture(120, 80);

    let unmasked = masked_run(&backend, None, &source, &[kyoto(2.0)], None).await.pixels().to_rgb8();
    let masked = masked_run(&backend, None, &source, &[kyoto(2.0)], Some(halves(120, 80, 60, (1.0, 0.0)))).await;
    let enlarged = source.pixels().resize_exact(240, 160, FilterType::Lanczos3).to_rgb8();

    assert_eq!(masked.dimensions(), (240, 160));
    assert_ne!(
        enlarged, unmasked,
        "the fake upscale is itself a Lanczos enlargement, so the two sides cannot be told apart"
    );

    let masked = masked.pixels().to_rgb8();
    for (x, y, pixel) in masked.enumerate_pixels() {
        // The mask is read bilinearly at twice its size, so its edge is a pixel or two wider than it was.
        if x < 116 {
            assert_eq!(pixel, unmasked.get_pixel(x, y), "({x}, {y}) on the white side is not the upscale");
        } else if x >= 124 {
            assert_eq!(pixel, enlarged.get_pixel(x, y), "({x}, {y}) on the black side is not the enlargement");
        }
    }
}

#[tokio::test]
async fn one_more_stroke_runs_no_model_and_names_a_new_result() {
    let backend = Fake::new();
    let cache = store();
    let source = picture(120, 80);
    let chain = denoise_and_sharpen();

    let first = Mask { strokes: vec![Stroke::new(false, 0.1, 0.5, vec![0.2, 0.2, 0.6, 0.7])], ..Mask::default() };
    let mut second = first.clone();
    second.strokes.push(Stroke::new(false, 0.05, 0.0, vec![0.8, 0.3]));

    let before = masked_run(&backend, Some(&cache), &source, &chain, Some(first)).await;
    settled();
    let acquired = backend.log().acquired.len();
    let ran = backend.log().ran.len();
    assert!(ran > 0, "the first run ran nothing");

    let after = masked_run(&backend, Some(&cache), &source, &chain, Some(second)).await;

    assert_eq!(backend.log().acquired.len(), acquired, "a mask edit opened a model");
    assert_eq!(backend.log().ran.len(), ran, "a mask edit ran a model");
    assert_ne!(after.identity(), before.identity(), "two masks named one result");

    // And the unmasked chain is served from the same steps, under the identity it had before masks existed.
    let unmasked = masked_run(&backend, Some(&cache), &source, &chain, None).await;
    assert_eq!(backend.log().ran.len(), ran, "the unmasked chain was not served the masked runs' steps");
    assert_eq!(unmasked.identity(), identity_after(source.identity(), &chain, ChannelDepth::Eight));
}

#[tokio::test]
async fn an_empty_mask_is_no_mask_at_all() {
    let backend = Fake::new();
    let source = picture(120, 80);

    let unmasked = masked_run(&backend, None, &source, &[brighten()], None).await;

    for empty in [Mask::default(), Mask { inverted: true, ..Mask::default() }] {
        let masked = masked_run(&backend, None, &source, &[brighten()], Some(empty)).await;

        assert_eq!(masked.identity(), unmasked.identity());
        assert_eq!(masked.pixels().as_bytes(), unmasked.pixels().as_bytes());
    }
}

#[tokio::test]
async fn a_mask_over_an_empty_chain_returns_the_photograph_as_it_was() {
    // Applying nothing derives nothing, through a mask or not.
    let backend = Fake::new();
    let source = picture(120, 80);

    let masked = masked_run(&backend, None, &source, &[], Some(halves(120, 80, 60, (1.0, 0.0)))).await;

    assert_eq!(masked.identity(), source.identity());
    assert_eq!(masked.pixels().as_bytes(), source.pixels().as_bytes());
}

#[tokio::test]
async fn a_sixteen_bit_masked_run_stays_sixteen_bits() {
    let backend = Fake::new();
    let source = picture(120, 80);
    let options = ProcessOptions {
        depth: OutputDepth::Sixteen,
        mask: Some(Arc::new(halves(120, 80, 60, (1.0, 0.0)))),
        ..Default::default()
    };

    let masked = process(&backend, None, &source, &[brighten()], Some(options)).await.unwrap().picture;

    assert!(matches!(masked.pixels(), DynamicImage::ImageRgb16(_)), "{:?}", masked.pixels());
    // The black side is the 8-bit photograph carried to 16 bits exactly.
    let DynamicImage::ImageRgb16(pixels) = masked.pixels() else { unreachable!() };
    let original = source.pixels().to_rgb16();
    assert_eq!(pixels.get_pixel(100, 40), original.get_pixel(100, 40));
}

#[test]
fn an_upscale_whose_sides_round_apart_is_still_the_photographs_shape() {
    // A portrait at a fractional scale: 21 x 52 at 2.7 is 56.7 x 140.4, rounded each side to 57 x 140. Their ratio is a
    // pixel and a tenth off the photograph's in height, which a guard of one pixel on the height alone refused.
    let photograph = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(21, 52, image::Rgb([30_u8; 3])));
    let result = Arc::new(DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(57, 140, image::Rgb([200_u8; 3]))));
    let white = halves(21, 52, 21, (1.0, 0.0));

    let blended = blend(&photograph, result, &white, ChannelDepth::Eight).unwrap();

    assert_eq!((blended.width(), blended.height()), (57, 140));
    assert_eq!(blended.to_rgb8().get_pixel(10, 70).0, [200; 3]);
}

#[test]
fn a_result_of_another_shape_is_an_error_rather_than_unmasked_pixels() {
    let photograph = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(21, 52, image::Rgb([30_u8; 3])));
    let result = Arc::new(DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(57, 100, image::Rgb([200_u8; 3]))));

    let refused = blend(&photograph, result, &halves(21, 52, 10, (1.0, 0.0)), ChannelDepth::Eight).unwrap_err();

    assert!(
        matches!(
            refused,
            InferenceError::MaskShape { width: 21, height: 52, result_width: 57, result_height: 100 }
        ),
        "{refused:?}"
    );
}

#[test]
fn an_upscale_enlarges_a_transparent_photograph_from_its_flattening() {
    // Colour stored under a fully transparent pixel is black once flattened, so two photographs that differ only there
    // enlarge to one picture; Lanczos over the straight alpha would carry it into the pixels beside.
    let flat = |hidden: u8| {
        DynamicImage::ImageRgba8(image::ImageBuffer::from_fn(8, 8, |x, _| {
            if x < 4 { image::Rgba([hidden, hidden, hidden, 0]) } else { image::Rgba([90, 90, 90, 255]) }
        }))
    };
    let result = || Arc::new(DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(16, 16, image::Rgb([200_u8; 3]))));
    let black = halves(8, 8, 0, (1.0, 0.0));

    let (a, b) = (flat(0), flat(255));
    let (a, b) = (
        blend(&a, result(), &black, ChannelDepth::Eight).unwrap(),
        blend(&b, result(), &black, ChannelDepth::Eight).unwrap(),
    );

    assert_eq!(a.as_bytes(), b.as_bytes(), "colour under a transparent pixel reached the enlargement");
}
