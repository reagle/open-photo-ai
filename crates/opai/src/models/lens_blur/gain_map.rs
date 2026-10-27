//! The lens blur's gain map: the HDR rendition rebuilt from the SDR base and the gain map, both blurred by the same
//! radii, and a gain map encoding their ratio under the parameters the source's was encoded with.
//!
//! Pure maths at the gain map's resolution; [`render`](super::render) decides when it runs and on what.

// Per channel `c`, with `g` a stored value in `0..=1` (the series design's H1):
//
//   log2 G = min_c + (max_c - min_c) * g^(1 / gamma_c)
//   HDR_c  = (SDR_c + k_sdr_c) * G_c - k_hdr_c                  (SDR linear: the sRGB transfer decoded)
//   G'_c   = (HDR'_c + k_hdr_c) / (SDR'_c + k_sdr_c)            (after both are blurred alike)
//   g'     = ((clamp(log2 G'_c, min_c, max_c) - min_c) / (max_c - min_c))^gamma_c
//
// The render is a normalised convolution, so a blurred ratio is the mean of the ratios under the disc weighted by
// `SDR + k_sdr`: it stays inside min..max but for rounding, and where nothing is blurred `G' = G` but for rounding,
// wherever `SDR + k_sdr` is positive. A black pixel under zero offsets has no ratio (0 / 0) and is stored as the
// minimum gain, whatever it held. A
// single-channel gain map applies one gain to all three channels, so the two renditions are reduced to luminance
// (BT.709 weights, which sum to 1, so `Y(HDR) = (Y(SDR) + k_sdr) * G - k_hdr`) and the ratio of the two luminances is
// the one gain; both are blurred in one render, as two channels of one image.

use std::borrow::Cow;

use image::{DynamicImage, GrayImage, RgbImage};
use imaging::defocus::{Defocus, defocus};
use imaging::plane::Plane;

use crate::image::GainMapParams;

/// BT.709 luminance weights on linear RGB.
const LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// One channel's parameters, as the arithmetic reads them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Channel {
    min: f32,
    max: f32,
    gamma: f32,
    offset_sdr: f32,
    offset_hdr: f32,
}

impl Channel {
    /// Channel `c` of `params`.
    pub(crate) fn of(params: &GainMapParams, c: usize) -> Self {
        Self {
            min: params.min[c],
            max: params.max[c],
            gamma: params.gamma[c],
            offset_sdr: params.offset_sdr[c],
            offset_hdr: params.offset_hdr[c],
        }
    }

    /// Whether every stored value stands for the same gain, so the stored values carry nothing and are kept as is.
    fn is_flat(&self) -> bool {
        self.max - self.min <= f32::EPSILON
    }

    /// The gain a stored value `g` in `0..=1` stands for.
    pub(crate) fn gain(&self, g: f32) -> f32 {
        (self.min + (self.max - self.min) * g.clamp(0.0, 1.0).powf(self.gamma.recip())).exp2()
    }

    /// The stored value in `0..=1` that stands for `gain`, clamped to the gains this channel can encode.
    pub(crate) fn encoded(&self, gain: f32) -> f32 {
        if self.is_flat() {
            return 0.0;
        }
        let stops = if gain > 0.0 && gain.is_finite() { gain.log2() } else { self.min };
        ((stops.clamp(self.min, self.max) - self.min) / (self.max - self.min)).powf(self.gamma)
    }

    /// The HDR rendition of a linear SDR value under `gain`.
    pub(crate) fn hdr(&self, sdr: f32, gain: f32) -> f32 {
        (sdr + self.offset_sdr) * gain - self.offset_hdr
    }

    /// The gain that takes a linear SDR value to an HDR one.
    pub(crate) fn ratio(&self, hdr: f32, sdr: f32) -> f32 {
        (hdr + self.offset_hdr) / (sdr + self.offset_sdr)
    }

    /// The gain of every 8-bit stored value.
    fn gains(&self) -> [f32; 256] {
        std::array::from_fn(|value| self.gain(f32::from(u8::try_from(value).expect("a byte")) / 255.0))
    }
}

/// An encoded value in `0..=1` as the byte stored.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded and clamped to a byte's range"
)]
fn byte(value: f32) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

/// `map` (8-bit grayscale or RGB, `width` x `height`) rewritten for `sdr`, the linear SDR rendition at the same size,
/// blurred by `radius` (and `keep_sharp`, at any size), under `params`: the gain map of the blurred photograph.
///
/// `blurred_sdr` is `sdr` already rendered by the same radii, where the caller has it — the photograph's own render,
/// for a gain map of its size — which spares a three-channel map one of its two renders. A single-channel map renders
/// both luminances in one render either way, and does not read it.
///
/// Returns `None` when `cancelled` answers true, which the render asks before each layer.
///
/// # Panics
///
/// Panics where `sdr` or `blurred_sdr` is not three values a pixel of `map`, or `radius` is not `map`'s size.
pub(crate) fn rewritten(
    map: &DynamicImage,
    sdr: Vec<f32>,
    blurred_sdr: Option<&[f32]>,
    params: &GainMapParams,
    radius: &Plane,
    keep_sharp: Option<&Plane>,
    cancelled: &dyn Fn() -> bool,
) -> Option<DynamicImage> {
    let (width, height) = (map.width(), map.height());
    let render = |linear: &[f32]| defocus(linear, width, height, radius, keep_sharp, None, Defocus::DEFAULT, cancelled);

    match map {
        DynamicImage::ImageLuma8(stored) => {
            let channel = Channel::of(params, 0);
            if channel.is_flat() {
                return Some(map.clone());
            }
            let gains = channel.gains();
            // Two renditions as two channels of one image: the render treats every channel alike and independently.
            let mut both = Vec::with_capacity(sdr.len());
            for (rgb, g) in sdr.as_chunks::<3>().0.iter().zip(stored.as_raw()) {
                let luminance = rgb.iter().zip(LUMINANCE).map(|(value, weight)| value * weight).sum::<f32>();
                both.extend([luminance, channel.hdr(luminance, gains[usize::from(*g)]), 0.0]);
            }
            let blurred = render(&both)?;
            let pixels = blurred
                .as_chunks::<3>()
                .0
                .iter()
                .map(|pair| byte(channel.encoded(channel.ratio(pair[1], pair[0]))));
            Some(DynamicImage::ImageLuma8(
                GrayImage::from_vec(width, height, pixels.collect()).expect("one a pixel"),
            ))
        }
        _ => {
            let stored = map.to_rgb8();
            let channels: [Channel; 3] = std::array::from_fn(|c| Channel::of(params, c));
            let gains = channels.map(|channel| channel.gains());
            let blurred_sdr = match blurred_sdr {
                Some(blurred) => Cow::Borrowed(blurred),
                None => Cow::Owned(render(&sdr)?),
            };
            assert_eq!(blurred_sdr.len(), sdr.len(), "the SDR rendition blurred at its own size");
            let hdr: Vec<f32> = sdr
                .iter()
                .zip(stored.as_raw())
                .enumerate()
                .map(|(i, (value, g))| channels[i % 3].hdr(*value, gains[i % 3][usize::from(*g)]))
                .collect();
            drop(sdr);
            let blurred_hdr = render(&hdr)?;
            drop(hdr);
            let pixels = blurred_hdr
                .iter()
                .zip(blurred_sdr.iter())
                .zip(stored.as_raw())
                .enumerate()
                .map(|(i, ((hdr, sdr), g))| {
                    let channel = channels[i % 3];
                    if channel.is_flat() { *g } else { byte(channel.encoded(channel.ratio(*hdr, *sdr))) }
                })
                .collect();
            Some(DynamicImage::ImageRgb8(RgbImage::from_vec(width, height, pixels).expect("three a pixel")))
        }
    }
}

/// `values`, `channels` interleaved a pixel over `width` x `height`, averaged over the area each pixel of a
/// `to_width` x `to_height` grid covers: an exact box reduction for any ratio, fractional overlaps weighted by their
/// share.
///
/// # Panics
///
/// Panics where `values` is not `channels` values a pixel, or the target is larger than the source on an axis.
pub(crate) fn box_reduced(
    values: &[f32],
    channels: usize,
    (width, height): (u32, u32),
    (to_width, to_height): (u32, u32),
) -> Vec<f32> {
    assert_eq!(values.len(), channels * width as usize * height as usize, "not {channels} values a pixel");
    assert!(to_width <= width && to_height <= height, "a box reduction does not enlarge");

    let (columns, rows) = (spans(width, to_width), spans(height, to_height));
    let stride = channels * width as usize;

    // Across first, into one row of the target's width for each source row, then down.
    let mut across = vec![0.0_f32; channels * to_width as usize * height as usize];
    for (source, out) in values.chunks_exact(stride).zip(across.chunks_exact_mut(channels * to_width as usize)) {
        for (pixel, taps) in out.chunks_exact_mut(channels).zip(&columns) {
            for &(x, weight) in taps {
                for (value, sample) in pixel.iter_mut().zip(&source[channels * x..channels * (x + 1)]) {
                    *value += weight * sample;
                }
            }
        }
    }

    let row = channels * to_width as usize;
    let mut reduced = vec![0.0_f32; row * to_height as usize];
    for (out, taps) in reduced.chunks_exact_mut(row).zip(&rows) {
        for &(y, weight) in taps {
            for (value, sample) in out.iter_mut().zip(&across[row * y..row * (y + 1)]) {
                *value += weight * sample;
            }
        }
    }
    reduced
}

/// `values`, one a pixel over `width` x `height`, brought to a `to_width` x `to_height` grid by keeping, for each cell,
/// the value of largest magnitude among the pixels it covers: how the signed radius plane is brought to a smaller gain
/// map. An average would mix radii across a focus edge — a disc behind with none in focus, or a radius behind with one
/// in front of opposite sign — into a blur that neither side has.
///
/// # Panics
///
/// Panics where `values` is not one value a pixel, or the target is larger than the source on an axis.
pub(crate) fn largest_reduced(
    values: &[f32],
    (width, height): (u32, u32),
    (to_width, to_height): (u32, u32),
) -> Vec<f32> {
    assert_eq!(values.len(), width as usize * height as usize, "not one value a pixel");
    assert!(to_width <= width && to_height <= height, "a reduction does not enlarge");

    let largest = |values: &mut dyn Iterator<Item = f32>| {
        values.fold(0.0_f32, |kept, value| if value.abs() > kept.abs() { value } else { kept })
    };
    let (columns, rows) = (spans(width, to_width), spans(height, to_height));
    let to_width = to_width as usize;

    // Across first, into one row of the target's width for each source row, then down, as `box_reduced` does.
    let mut across = vec![0.0_f32; to_width * height as usize];
    for (source, out) in values.chunks_exact(width as usize).zip(across.chunks_exact_mut(to_width)) {
        for (value, taps) in out.iter_mut().zip(&columns) {
            *value = largest(&mut taps.iter().map(|&(x, _)| source[x]));
        }
    }

    let mut reduced = vec![0.0_f32; to_width * to_height as usize];
    for (out, taps) in reduced.chunks_exact_mut(to_width).zip(&rows) {
        for (x, value) in out.iter_mut().enumerate() {
            *value = largest(&mut taps.iter().map(|&(y, _)| across[y * to_width + x]));
        }
    }
    reduced
}

/// For each of `to` cells along an axis of `from` pixels, the source pixels it covers and the share of the cell each
/// covers, summing to 1.
fn spans(from: u32, to: u32) -> Vec<Vec<(usize, f32)>> {
    let step = f64::from(from) / f64::from(to);
    (0..to)
        .map(|cell| {
            let (start, end) = (f64::from(cell) * step, f64::from(cell + 1) * step);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "pixel indices of a u32 axis")]
            let (first, last) = (start.floor() as usize, (end.ceil() as usize).min(from as usize));
            (first..last)
                .filter_map(|pixel| {
                    #[allow(clippy::cast_precision_loss, reason = "a pixel index of a u32 axis")]
                    let overlap = end.min(pixel as f64 + 1.0) - start.max(pixel as f64);
                    #[allow(clippy::cast_possible_truncation, reason = "a share in 0..=1")]
                    (overlap > 1e-9).then_some((pixel, (overlap / step) as f32))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use image::{Luma, Rgb};

    use super::*;
    use crate::image::test_support::HAWK_GAIN_MAP_XMP;

    fn hawk() -> GainMapParams {
        GainMapParams::from_xmp(HAWK_GAIN_MAP_XMP).expect("readable")
    }

    #[test]
    fn every_stored_value_round_trips_for_the_hawks_parameters() {
        let params = hawk();
        for c in 0..3 {
            let channel = Channel::of(&params, c);
            for g in 0..=255_u8 {
                let gain = channel.gain(f32::from(g) / 255.0);
                assert_eq!(byte(channel.encoded(gain)), g, "channel {c}, {g}");
                // And through the HDR rendition and back, at any SDR value.
                for sdr in [0.0, 0.01, 0.4, 1.0] {
                    let ratio = channel.ratio(channel.hdr(sdr, gain), sdr);
                    assert_eq!(byte(channel.encoded(ratio)), g, "channel {c}, {g} at {sdr}");
                }
            }
        }
    }

    #[test]
    fn a_gain_outside_the_range_is_clamped_to_it() {
        let channel = Channel::of(&hawk(), 0);
        assert_eq!(byte(channel.encoded(1e6)), 255);
        assert_eq!(byte(channel.encoded(1e-6)), 0);
        assert_eq!(byte(channel.encoded(0.0)), 0);
        assert_eq!(byte(channel.encoded(f32::NAN)), 0);
    }

    /// A blur of `radius` everywhere.
    fn everywhere(width: u32, height: u32, radius: f32) -> Plane {
        Plane::new(width, height, radius)
    }

    #[test]
    fn a_uniform_picture_and_gain_give_the_same_gain_back() {
        let (width, height) = (24, 16);
        let sdr = vec![0.3_f32; 3 * 24 * 16];
        let map = DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([40, 128, 220])));

        let blurred =
            rewritten(&map, sdr, None, &hawk(), &everywhere(width, height, -5.0), None, &|| false).expect("runs");

        assert_eq!(blurred.to_rgb8(), map.to_rgb8());
    }

    #[test]
    fn nothing_blurred_keeps_every_value() {
        let (width, height) = (20, 12);
        let sdr: Vec<f32> = (0..3 * 20 * 12).map(|i| (i % 97) as f32 / 97.0).collect();
        let map = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            Rgb([(x * 12) as u8, (y * 20) as u8, ((x + y) * 7) as u8])
        }));

        let kept = rewritten(&map, sdr, None, &hawk(), &everywhere(width, height, 0.0), None, &|| false).expect("runs");

        assert_eq!(kept.to_rgb8(), map.to_rgb8());
    }

    #[test]
    fn a_single_channel_map_takes_the_ratio_of_luminances() {
        let (width, height) = (16, 16);
        // The left half red, the right half green, at one gain: the blur mixes them and the gain stays.
        let sdr: Vec<f32> = (0..width * height)
            .flat_map(|i| if i % width < 8 { [0.5, 0.0, 0.0] } else { [0.0, 0.5, 0.0] })
            .collect();
        let map = DynamicImage::ImageLuma8(GrayImage::from_pixel(width, height, Luma([150])));

        let blurred = rewritten(&map, sdr.clone(), None, &hawk(), &everywhere(width, height, -4.0), None, &|| false)
            .expect("runs");
        assert!(matches!(blurred, DynamicImage::ImageLuma8(_)));
        assert!(blurred.to_luma8().pixels().all(|pixel| pixel.0[0].abs_diff(150) <= 1));

        // A bright gain on the left only: the luminance ratio across the mixed middle lies between the two.
        let map =
            DynamicImage::ImageLuma8(GrayImage::from_fn(width, height, |x, _| Luma([if x < 8 { 250 } else { 50 }])));
        let blurred =
            rewritten(&map, sdr, None, &hawk(), &everywhere(width, height, -4.0), None, &|| false).expect("runs");
        let middle = blurred.to_luma8().get_pixel(8, 8).0[0];
        assert!(middle > 50 && middle < 250, "{middle}");
    }

    #[test]
    fn a_highlight_blooms_into_a_disc_dimmer_at_its_centre() {
        // A grey photograph with a gain-map highlight of radius 2 at the centre, blurred by 10 gain-map pixels.
        let (width, height) = (64, 64);
        let sdr = vec![0.2_f32; 3 * 64 * 64];
        let lit = |x: u32, y: u32| (x as f32 - 32.0).hypot(y as f32 - 32.0) <= 2.0;
        let map = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            if lit(x, y) { Rgb([255, 255, 255]) } else { Rgb([60, 60, 60]) }
        }));

        let blurred = rewritten(&map, sdr, None, &hawk(), &everywhere(width, height, -10.0), None, &|| false)
            .expect("runs")
            .to_rgb8();

        let at = |x: u32, y: u32| blurred.get_pixel(x, y).0[1];
        assert!(at(32, 32) < 255 && at(32, 32) > 60, "the centre: {}", at(32, 32));
        // Lifted across the disc, not beyond it.
        for distance in [0, 4, 8] {
            assert!(at(32 + distance, 32) > 64, "{distance} px out: {}", at(32 + distance, 32));
        }
        for distance in [14, 20] {
            assert!(at(32 + distance, 32) <= 61, "{distance} px out: {}", at(32 + distance, 32));
        }
    }

    #[test]
    fn a_box_reduction_averages_what_each_cell_covers() {
        // 4x2 single channel to 2x1: each cell the mean of a 2x2 block.
        let values = [1.0, 3.0, 5.0, 7.0, 1.0, 3.0, 5.0, 7.0];
        assert_eq!(box_reduced(&values, 1, (4, 2), (2, 1)), [2.0, 6.0]);

        // 3 to 2: the middle pixel shared half and half.
        let reduced = box_reduced(&[0.0, 3.0, 6.0], 1, (3, 1), (2, 1));
        assert!((reduced[0] - 1.0).abs() < 1e-6 && (reduced[1] - 5.0).abs() < 1e-6, "{reduced:?}");

        // Three channels, the same size: unchanged; and a flat plane stays flat at any ratio.
        let rgb: Vec<f32> = (0..3 * 5 * 3).map(|i| i as f32).collect();
        assert_eq!(box_reduced(&rgb, 3, (5, 3), (5, 3)), rgb);
        assert!(box_reduced(&[0.25; 3 * 37 * 23], 3, (37, 23), (9, 6)).iter().all(|v| (v - 0.25).abs() < 1e-6));
    }

    #[test]
    fn a_radius_reduction_keeps_the_largest_blur_a_cell_covers_rather_than_a_mean() {
        // 4x2 to 2x1: a cell across a focus edge (0 and -8) and one across the plane of focus (+3 and -5).
        let values = [0.0, -8.0, 3.0, 1.0, 0.0, 0.0, -5.0, 2.0];
        assert_eq!(largest_reduced(&values, (4, 2), (2, 1)), [-8.0, -5.0]);

        // 3 to 2: the middle pixel, shared, counts in both cells; the same size is unchanged.
        assert_eq!(largest_reduced(&[0.0, -6.0, 1.0], (3, 1), (2, 1)), [-6.0, -6.0]);
        let plane: Vec<f32> = (0..5 * 3).map(|i| i as f32 - 7.0).collect();
        assert_eq!(largest_reduced(&plane, (5, 3), (5, 3)), plane);
    }

    #[test]
    fn a_blurred_sdr_handed_in_gives_what_rendering_it_here_gives() {
        let (width, height) = (32, 24);
        let sdr: Vec<f32> = (0..3 * 32 * 24).map(|i| (i % 89) as f32 / 89.0).collect();
        let map = DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
            Rgb([(x * 7) as u8, (y * 9) as u8, ((x + y) * 4) as u8])
        }));
        let radius = Plane::from_vec(width, height, (0..32 * 24).map(|i| -((i % 32) as f32) / 4.0).collect())
            .expect("one a pixel");

        let blurred = defocus(&sdr, width, height, &radius, None, None, Defocus::DEFAULT, &|| false).expect("runs");
        let handed = rewritten(&map, sdr.clone(), Some(&blurred), &hawk(), &radius, None, &|| false).expect("runs");
        let here = rewritten(&map, sdr, None, &hawk(), &radius, None, &|| false).expect("runs");

        assert_eq!(handed.to_rgb8(), here.to_rgb8());
    }
}
