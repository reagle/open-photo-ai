//! A chain holding a removal: where it runs, what an empty one is, what the store keeps, the limiting mask over it, and
//! the faces it takes out.

use super::*;
use crate::mask::{Base, Mask, Plane, Stroke};
use crate::models::face::tests::face_at;
use crate::models::{
    Bias, FaceRecovery, Fidelity, LightAdjustment, LightAdjustmentVariant, Remove, Sharpen, SharpenVariant, Strength,
};

/// The artifact the removal opens.
const LIMA: &str = "rm_lima_fp32";

/// A removal of a dot at `(x, y)`, in fractions of the photograph.
fn removing(x: f32, y: f32) -> Operation {
    Remove::lima(Arc::new(Mask { strokes: vec![Stroke::new(false, 0.05, 0.0, vec![x, y])], ..Mask::default() }))
}

/// A removal with nothing painted.
fn removing_nothing() -> Operation {
    Remove::lima(Arc::new(Mask::default()))
}

/// A sharpen at `strength`.
fn sharpen(strength: f64) -> Operation {
    Operation::Sharpen(Sharpen::new(SharpenVariant::Moscow(FloatPrecision::Fp32), Strength::new(strength).unwrap()))
}

/// A whole-image operation that changes every pixel.
fn brighten() -> Operation {
    Operation::LightAdjustment(LightAdjustment::new(
        LightAdjustmentVariant::Paris(FloatPrecision::Fp16),
        Bias::new(0.5).unwrap(),
    ))
}

/// A limiting mask white on the right `width - split` columns of a `width` x `height` plane.
fn right_of(width: u32, height: u32, split: u32) -> Arc<Mask> {
    let values = (0..width * height).map(|index| if index % width < split { 0.0 } else { 1.0 }).collect();

    Arc::new(Mask { base: Some(Base::new(Plane::from_vec(width, height, values).unwrap())), ..Mask::default() })
}

#[tokio::test]
async fn a_removal_runs_first_whatever_order_it_was_listed_in() {
    let backend = Fake::new();

    process(&backend, None, &picture(120, 80), &[sharpen(0.5), removing(0.5, 0.5)], None)
        .await
        .unwrap();

    let ran = backend.log().ran.clone();
    assert_eq!(ran.first().map(String::as_str), Some(LIMA), "{ran:?}");
    assert!(ran.iter().skip(1).all(|artifact| artifact != LIMA), "{ran:?}");
}

#[tokio::test]
async fn a_removal_of_nothing_is_left_out_installs_nothing_and_names_the_result_as_the_chain_without_it() {
    let backend = Fake::new();
    let source = picture(120, 80);

    let with = process(&backend, None, &source, &[removing_nothing(), sharpen(0.5)], None)
        .await
        .unwrap()
        .picture;
    assert!(
        !backend.log().acquired.iter().any(|artifact| artifact == LIMA),
        "an empty removal opened its model"
    );

    let without = process(&backend, None, &source, &[sharpen(0.5)], None).await.unwrap().picture;
    assert_eq!(with.identity(), without.identity());
    assert_eq!(with.pixels().as_bytes(), without.pixels().as_bytes());

    // And a chain of nothing but an empty removal is the photograph, under its own identity.
    let alone = process(&backend, None, &source, &[removing_nothing()], None).await.unwrap().picture;
    assert_eq!(alone.identity(), source.identity());
}

#[tokio::test]
async fn a_removal_changes_its_region_and_nothing_far_from_it() {
    let backend = Fake::new();
    let source = picture(200, 150);

    let removed = process(&backend, None, &source, &[removing(0.25, 0.5)], None).await.unwrap().picture;
    let (before, after) = (source.pixels().to_rgb8(), removed.pixels().to_rgb8());

    // A dot of 0.05 of 200 is 10 px across the centre (50, 75): the centre is the fill, the far corner untouched.
    assert_ne!(before.get_pixel(50, 75), after.get_pixel(50, 75), "the removal left its region as it was");
    assert_eq!(before.get_pixel(190, 10), after.get_pixel(190, 10));
    assert_ne!(removed.identity(), source.identity());
}

#[tokio::test]
async fn a_change_after_the_removal_serves_the_removal_from_the_store_and_a_new_region_does_not() {
    let backend = Fake::new();
    let cache = store();
    let source = picture(120, 80);
    let lima_runs = |backend: &Fake| backend.log().ran.iter().filter(|artifact| *artifact == LIMA).count();

    cached_run(&backend, Some(&cache), &source, &[removing(0.5, 0.5), sharpen(0.5)], OutputDepth::Eight)
        .await
        .unwrap();
    settled();
    assert_eq!(lima_runs(&backend), 1);

    cached_run(&backend, Some(&cache), &source, &[removing(0.5, 0.5), sharpen(1.0)], OutputDepth::Eight)
        .await
        .unwrap();
    settled();
    assert_eq!(lima_runs(&backend), 1, "a sharpen strength ran the removal again");

    let moved = cached_run(&backend, Some(&cache), &source, &[removing(0.4, 0.5), sharpen(1.0)], OutputDepth::Eight)
        .await
        .unwrap();
    assert_eq!(lima_runs(&backend), 2, "another region was served the first one's removal");

    let first = cached_run(&backend, Some(&cache), &source, &[removing(0.5, 0.5), sharpen(1.0)], OutputDepth::Eight)
        .await
        .unwrap();
    assert_ne!(moved.identity(), first.identity());
}

#[tokio::test]
async fn a_limiting_mask_limits_the_enhancements_and_never_brings_back_what_was_removed() {
    // The removal is on the left, the limiting mask white on the right: the left keeps the removal and not the
    // brightening, the right has both.
    for cached in [false, true] {
        let backend = Fake::new();
        let cache = store();
        let cache = cached.then_some(&cache);
        let source = picture(200, 150);
        let chain = [removing(0.25, 0.5), brighten()];
        let options = || Some(ProcessOptions { mask: Some(right_of(200, 150, 100)), ..Default::default() });

        let removed = process(&backend, None, &source, &chain[..1], None).await.unwrap().picture.pixels().to_rgb8();
        let whole = process(&backend, None, &source, &chain, None).await.unwrap().picture.pixels().to_rgb8();

        // Twice with a store: the second run serves both steps from it, and the removal must still be decoded for
        // the blend rather than the photograph used.
        let mut limited = None;
        for _ in 0..if cached { 2 } else { 1 } {
            limited = Some(process(&backend, cache, &source, &chain, options()).await.unwrap().picture);
            settled();
        }
        let limited = limited.unwrap().pixels().to_rgb8();

        assert_eq!(limited.get_pixel(50, 75), removed.get_pixel(50, 75), "cached {cached}: the removal came back");
        assert_eq!(limited.get_pixel(10, 10), removed.get_pixel(10, 10), "cached {cached}: the left was enhanced");
        assert_eq!(limited.get_pixel(150, 75), whole.get_pixel(150, 75), "cached {cached}: the right was not");
    }
}

#[tokio::test]
async fn a_limiting_mask_over_nothing_but_removals_is_no_mask() {
    let backend = Fake::new();
    let source = picture(200, 150);
    let options = Some(ProcessOptions { mask: Some(right_of(200, 150, 100)), ..Default::default() });

    let limited = process(&backend, None, &source, &[removing(0.25, 0.5)], options).await.unwrap().picture;
    let plain = process(&backend, None, &source, &[removing(0.25, 0.5)], None).await.unwrap().picture;

    assert_eq!(limited.identity(), plain.identity());
    assert_eq!(limited.pixels().as_bytes(), plain.pixels().as_bytes());
}

#[test]
fn a_face_whose_centre_a_removal_marks_is_not_restored_and_the_others_are() {
    // A dot of 0.05 of the longer side (200 px of 4000) at a quarter across and half down.
    let taken = face_at(950.0, 1450.0, 1050.0, 1550.0);
    let kept = face_at(2900.0, 1400.0, 3100.0, 1600.0);
    let recovery = FaceRecovery::athens(FloatPrecision::Fp32, Faces::new([taken, kept]), Fidelity::MAXIMUM);
    let chain = vec![removing(0.25, 0.5), recovery.clone(), sharpen(0.5)];

    let spared = sparing_removed_faces(std::borrow::Cow::Owned(chain), 4000, 3000);

    let Operation::FaceRecovery(spared) = &spared[1] else { panic!("the recovery moved") };
    assert_eq!(spared.faces().as_slice(), [kept]);

    // Without a removal, or without a face to restore, the chain is handed back as it came.
    let alone = [recovery, sharpen(0.5)];
    assert!(matches!(
        sparing_removed_faces(std::borrow::Cow::Borrowed(&alone), 4000, 3000),
        std::borrow::Cow::Borrowed(_)
    ));
}
