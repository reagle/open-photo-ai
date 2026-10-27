//! Presenting a photograph of any size and aspect to a graph of one fixed size, and bringing its answer back to the
//! photograph's aspect.

// **Stretched, not letterboxed.** The photograph is resized to fill the graph's frame on both axes, so a 3:2 photograph
// is squeezed vertically by a sixth into the 5:4 landscape graph. Measured on three photographs (5:4, 3:2 and a 3:4
// portrait) against the dynamic graph run at each one's own aspect: the shape of the depth agrees equally well either
// way (correlation of log depth 0.998 or better), but its scale does not — the median distance is 1.2% off stretched
// against 14.3% letterboxed with replicated edges on the 3:2 photograph, and 3.2% against 7.6% on the portrait — and
// the focal length is closer too. The model reads the scale off what it recognises, and a band of smeared edge pixels
// is something it recognises badly. Stretching also leaves no padding to cut away.
//
// Plain unit RGB, CHW, and no ImageNet normalisation: the graph normalises its own input.

use std::borrow::Cow;

use image::imageops::FilterType::{Lanczos3, Triangle};
use image::{DynamicImage, ImageBuffer, Luma};

use super::Orientation;

use crate::error::InferenceError;

/// How many times the graph's frame a large photograph is first box-averaged to, before the Lanczos3 pass.
const PREFILTER_FACTOR: u32 = 3;

/// The graph's input tensor for `source`: the photograph resized to fill `orientation`'s frame, unit RGB, planar CHW.
///
/// # Errors
///
/// [`InferenceError::Untileable`] where `source` has no area.
pub(super) fn stretched(source: &DynamicImage, orientation: Orientation) -> Result<Vec<f32>, InferenceError> {
    let (width, height) = (source.width(), source.height());

    if width == 0 || height == 0 {
        return Err(InferenceError::Untileable { width, height });
    }

    let (graph_width, graph_height) = orientation.size();

    // Lanczos3 over a box prefilter to three times the frame, as detection's letterbox resamples, and for a reason of
    // this model's own: its distances follow the sharpness of what it is shown. On a test photograph, a triangle filter
    // from twice the frame put the subject 2.3% farther than one Lanczos3 pass over the full photograph did; this
    // prefilter left it where that pass put it, to a hundredth of a percent.
    let prefiltered = if width > PREFILTER_FACTOR * graph_width && height > PREFILTER_FACTOR * graph_height {
        Cow::Owned(source.thumbnail_exact(PREFILTER_FACTOR * graph_width, PREFILTER_FACTOR * graph_height))
    } else {
        Cow::Borrowed(source)
    };
    let resized = prefiltered.resize_exact(graph_width, graph_height, Lanczos3).into_rgb32f();

    let plane = (graph_width * graph_height) as usize;
    let mut tensor = vec![0.0_f32; 3 * plane];

    for (index, pixel) in resized.pixels().enumerate() {
        for channel in 0..3 {
            tensor[channel * plane + index] = pixel[channel].clamp(0.0, 1.0);
        }
    }

    Ok(tensor)
}

/// The size of the map a `width` by `height` photograph gets: its own aspect, its long side the graph's long side, or
/// the photograph's where that is shorter, so a small photograph is not given a map larger than itself.
pub(super) fn map_size(width: u32, height: u32, orientation: Orientation) -> (u32, u32) {
    let (graph_width, graph_height) = orientation.size();
    let long = graph_width.max(graph_height).min(width.max(height)).max(1);

    // Rounded, and at least one pixel, so a photograph of extreme aspect still has a short side.
    let short = |numerator: u32, denominator: u32| {
        ((f64::from(long) * f64::from(numerator) / f64::from(denominator)).round() as u32).clamp(1, long)
    };

    if width >= height { (long, short(height, width)) } else { (short(width, height), long) }
}

/// `metres`, a map at the graph's size, resampled to `size`: the photograph's aspect, unstretched.
///
/// Resampled in disparity, the quantity a blur is linear in, so a pixel the resampling mixes across an edge between a
/// subject and its background takes a distance between the two that is weighted as the blur would weight it. The
/// triangle filter has no negative lobe, so every result stays within the distances it was made from.
pub(super) fn unstretched(metres: Vec<f32>, from: (u32, u32), size: (u32, u32)) -> Vec<f32> {
    if from == size {
        return metres;
    }

    // Normalised to the nearest distance's disparity before the resize and scaled back after it, because `image`
    // clamps a float sample to 0..=1 as it resamples: anything nearer than a metre would otherwise come back at one
    // metre exactly.
    let nearest = metres.iter().copied().fold(f32::INFINITY, f32::min);
    let disparity: Vec<f32> = metres.iter().map(|distance| nearest / distance).collect();
    let plane: ImageBuffer<Luma<f32>, Vec<f32>> =
        ImageBuffer::from_raw(from.0, from.1, disparity).expect("a map's plane fits its size");

    image::imageops::resize(&plane, size.0, size.1, Triangle)
        .into_raw()
        .into_iter()
        .map(|fraction| nearest / fraction)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use image::{Rgb, RgbImage};

    #[test]
    fn a_photograph_fills_its_orientations_frame_whatever_its_aspect() {
        for (width, height, orientation) in [
            (3000, 2000, Orientation::Landscape),
            (2000, 3000, Orientation::Portrait),
            (500, 500, Orientation::Landscape),
        ] {
            let source = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 128, 0])));
            let tensor = stretched(&source, orientation).expect("a photograph with area");
            let (graph_width, graph_height) = orientation.size();
            let plane = (graph_width * graph_height) as usize;

            assert_eq!(tensor.len(), 3 * plane);

            // No padding anywhere: every corner of every plane is the photograph's own colour, in unit RGB.
            for (channel, expected) in [1.0, 128.0 / 255.0, 0.0].into_iter().enumerate() {
                for index in [0, graph_width as usize - 1, plane - graph_width as usize, plane - 1] {
                    let value = tensor[channel * plane + index];
                    assert!((value - expected).abs() < 1e-3, "channel {channel} at {index} is {value}");
                }
            }
        }
    }

    #[test]
    fn the_map_takes_the_photographs_aspect_at_the_graphs_long_side() {
        assert_eq!(map_size(6000, 4000, Orientation::Landscape), (1280, 853));
        assert_eq!(map_size(5000, 4000, Orientation::Landscape), (1280, 1024));
        assert_eq!(map_size(3064, 4080, Orientation::Portrait), (961, 1280));
        assert_eq!(map_size(4000, 4000, Orientation::Landscape), (1280, 1280));
        assert_eq!(map_size(40_000, 2, Orientation::Landscape), (1280, 1));
    }

    #[test]
    fn a_photograph_smaller_than_the_graph_gets_a_map_no_larger_than_itself() {
        assert_eq!(map_size(640, 480, Orientation::Landscape), (640, 480));
        assert_eq!(map_size(480, 640, Orientation::Portrait), (480, 640));
        assert_eq!(map_size(800, 800, Orientation::Landscape), (800, 800));
        assert_eq!(map_size(1, 1, Orientation::Landscape), (1, 1));
        assert_eq!(map_size(1500, 1000, Orientation::Landscape), (1280, 853));
    }

    #[test]
    fn unstretching_keeps_every_distance_within_the_ones_it_was_made_from() {
        // A step from a quarter of a metre to ten, which the resampling has to blend across without overshooting
        // either — and nearer than a metre, which a resampler clamping disparity to 0..=1 would lose.
        let from = (64, 48);
        let metres: Vec<f32> = (0..64 * 48).map(|i| if (i / 64) < 24 { 0.25 } else { 10.0 }).collect();

        let resampled = unstretched(metres.clone(), from, (64, 37));

        assert_eq!(resampled.len(), 64 * 37);
        assert!(resampled.iter().all(|distance| (0.2499..=10.001).contains(distance)), "{resampled:?}");
        assert!((resampled[0] - 0.25).abs() < 1e-6, "the near half's corner moved: {}", resampled[0]);
        assert!(
            (resampled[64 * 37 - 1] - 10.0).abs() < 1e-4,
            "the far half's corner moved: {}",
            resampled[64 * 37 - 1]
        );

        assert_eq!(unstretched(metres.clone(), from, from), metres, "a map already at its size was resampled");
    }

    #[test]
    fn an_image_with_no_area_is_refused() {
        let empty = DynamicImage::ImageRgb8(RgbImage::new(0, 10));

        assert!(matches!(
            stretched(&empty, Orientation::Landscape),
            Err(InferenceError::Untileable { width: 0, height: 10 })
        ));
    }
}
