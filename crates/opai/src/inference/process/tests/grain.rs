//! A grain at the end of a chain: matched against the chain's input, or against what its last denoise made, whether
//! that denoise was computed just now or served from the store.

use super::*;
use crate::models::{Denoise, Grain, GrainAmount, GrainMode, Sharpen};
use imaging::grain::{measured, normals};

/// A flat grey photograph with sensor-like noise, at 8 bits as a decoded JPEG is.
fn grainy(width: u32, height: u32) -> Arc<DynamicImage> {
    Arc::new(DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        let [shared, ..] = normals(u64::from(x), u64::from(y), 5);
        image::Rgb([(0.45 + 0.02 * shared).clamp(0.0, 1.0); 3].map(|value| (value * 255.0).round() as u8))
    })))
}

fn smoothing() -> Fake {
    Fake { smooths: true, ..Fake::new() }
}

fn denoise() -> Operation {
    Denoise::stockholm(FloatPrecision::Fp32, Strength::clamped(1.0))
}

fn sharpen() -> Operation {
    Sharpen::moscow(FloatPrecision::Fp32, Strength::clamped(1.0))
}

fn grain() -> Operation {
    Grain::sensor(GrainAmount::clamped(1.0), GrainMode::Match)
}

/// The green residual of the band the photograph's grey falls in.
fn residual(picture: &DynamicImage) -> f32 {
    measured(picture).expect("measured").sigma[3][1]
}

#[tokio::test]
async fn a_grain_after_a_step_that_smoothed_puts_the_photographs_own_back() {
    let source = grainy(256, 192);
    let backend = smoothing();

    let smoothed = run_with(&backend, Arc::clone(&source), &[sharpen()]).await.unwrap();
    let grained = run_with(&backend, Arc::clone(&source), &[sharpen(), grain()]).await.unwrap();

    assert!(residual(&smoothed) < 0.1 * residual(&source), "the fake did not smooth");
    let (got, had) = (residual(&grained), residual(&source));
    assert!((got - had).abs() / had < 0.15, "the grain put back {got} of the photograph's {had}");
}

#[tokio::test]
async fn a_grain_over_the_photograph_alone_adds_nothing() {
    let source = grainy(256, 192);
    let grained = run_with(&Fake::new(), Arc::clone(&source), &[grain()]).await.unwrap();

    assert_eq!(grained.as_bytes(), source.as_bytes(), "the photograph was given grain it already had");
}

#[tokio::test]
async fn a_grain_after_a_denoise_matches_what_the_denoise_left_and_puts_nothing_back() {
    // The denoise was asked for less grain, so matching the photograph it was handed would undo it.
    let source = grainy(256, 192);
    let backend = smoothing();

    let denoised = run_with(&backend, Arc::clone(&source), &[denoise()]).await.unwrap();
    let grained = run_with(&backend, Arc::clone(&source), &[denoise(), sharpen(), grain()]).await.unwrap();

    assert_eq!(grained.as_bytes(), denoised.as_bytes(), "the grain undid the denoise");
}

#[tokio::test]
async fn a_grain_after_a_stored_denoise_matches_the_denoise_it_was_served() {
    // Once with the denoise just before the grain, whose stored result is the grain's own input, and once with a
    // step between them, whose stored denoise is decoded for the grain alone.
    for chain in [vec![denoise()], vec![denoise(), sharpen()]] {
        let backend = smoothing();
        let cache = store();
        let source = Picture::new("/pictures/grainy.jpg", grainy(256, 192), "0123456789abcdef");

        let stored = cached_run(&backend, Some(&cache), &source, &chain, OutputDepth::Eight).await.unwrap();
        let ran = backend.log().ran.len();
        settled();

        let grained =
            cached_run(&backend, Some(&cache), &source, &[chain.clone(), vec![grain()]].concat(), OutputDepth::Eight)
                .await
                .unwrap();

        assert_eq!(backend.log().ran.len(), ran, "{chain:?} ran again for the grain after it");
        assert_eq!(
            grained.pixels().as_bytes(),
            stored.pixels().as_bytes(),
            "{chain:?}: the grain undid the denoise"
        );
    }
}

#[tokio::test]
async fn a_grain_after_a_stored_step_that_smoothed_still_matches_the_photograph() {
    let backend = smoothing();
    let cache = store();
    let source = Picture::new("/pictures/grainy.jpg", grainy(256, 192), "0123456789abcdef");

    cached_run(&backend, Some(&cache), &source, &[sharpen()], OutputDepth::Eight).await.unwrap();
    settled();
    let grained = cached_run(&backend, Some(&cache), &source, &[sharpen(), grain()], OutputDepth::Eight)
        .await
        .unwrap();
    let computed = run_with(&smoothing(), grainy(256, 192), &[sharpen(), grain()]).await.unwrap();

    assert_eq!(grained.pixels().as_bytes(), computed.as_bytes(), "a stored step changed what the grain matched");
}

#[tokio::test]
async fn a_grain_draws_the_same_pixels_on_every_run_and_opens_no_session() {
    let source = grainy(200, 120);
    let backend = smoothing();

    let one = run_with(&backend, Arc::clone(&source), &[sharpen(), grain()]).await.unwrap();
    let acquired = backend.log().acquired.clone();
    let two = run_with(&backend, Arc::clone(&source), &[sharpen(), grain()]).await.unwrap();

    assert_eq!(one.as_bytes(), two.as_bytes());
    assert_eq!(acquired, vec!["sh_moscow_fp32"], "the grain opened a session");
}

/// A mask whose left half is white and right half black, over a `width` x `height` plane.
fn left_half(width: u32, height: u32) -> crate::mask::Mask {
    use crate::mask::{Base, Mask, Plane};

    let values = (0..width * height).map(|index| if index % width < width / 2 { 1.0 } else { 0.0 }).collect();
    Mask { base: Some(Base::new(Plane::from_vec(width, height, values).unwrap())), ..Mask::default() }
}

/// `chain` over a grainy photograph, limited to the left half.
async fn limited(chain: &[Operation]) -> DynamicImage {
    let source = Picture::new("/pictures/grainy.jpg", grainy(256, 192), "0123456789abcdef");
    let options = ProcessOptions { mask: Some(Arc::new(left_half(256, 192))), ..Default::default() };

    process(&smoothing(), None, &source, chain, Some(options))
        .await
        .unwrap()
        .picture
        .pixels()
        .clone()
}

#[tokio::test]
async fn a_grain_after_a_limiting_mask_puts_the_noise_back_only_where_the_mask_let_a_smoothing_through() {
    // The grain runs over the blend rather than being limited by it: the left half was smoothed and gets the
    // photograph's grain back, the right half is the photograph with its own and gets next to nothing.
    let source = grainy(256, 192);
    let smoothed = limited(&[sharpen()]).await;
    let grained = limited(&[sharpen(), grain()]).await;
    let half = |picture: &DynamicImage, x: u32| residual(&picture.crop_imm(x, 0, 128, 192));

    assert!(half(&smoothed, 0) < 0.1 * half(&source, 0), "the left half was not smoothed");
    let (got, had) = (half(&grained, 0), half(&source, 0));
    assert!((got - had).abs() / had < 0.15, "the smoothed half was given {got} of the photograph's {had}");
    let (got, had) = (half(&grained, 128), half(&source, 128));
    assert!(got / had < 1.1, "the photograph's own half went from {had} to {got}");
}

#[tokio::test]
async fn a_grain_after_a_limiting_mask_matches_the_denoise_before_the_blend() {
    // The head hands the tail what its denoise made, so the grain does not put back what the denoise took away inside
    // the mask; outside it is the photograph, which has its own.
    let denoised = limited(&[denoise()]).await;
    let grained = limited(&[denoise(), grain()]).await;

    assert_eq!(grained.as_bytes(), denoised.as_bytes(), "the grain undid the masked denoise");
}

#[tokio::test]
async fn a_stored_denoise_that_will_not_decode_is_run_again_for_the_grain_after_it() {
    let backend = smoothing();
    let cache = store();
    let source = Picture::new("/pictures/grainy.jpg", grainy(256, 192), "0123456789abcdef");
    let head = [denoise(), sharpen()];

    let expected = cached_run(&backend, Some(&cache), &source, &head, OutputDepth::Eight).await.unwrap();
    settled();
    let denoised = identity_after(source.identity(), &head[..1], ChannelDepth::Eight);
    cache.set_bytes(&denoised, b"\x89PNG\r\n\x1a\n and then nothing", ENTRY_TTL).unwrap();
    let ran = backend.log().ran.len();

    let grained = cached_run(&backend, Some(&cache), &source, &[denoise(), sharpen(), grain()], OutputDepth::Eight)
        .await
        .unwrap();

    assert!(backend.log().ran.len() > ran, "the denoise was not run again");
    assert_eq!(grained.pixels().as_bytes(), expected.pixels().as_bytes(), "the grain undid the denoise");
}
