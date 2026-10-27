//! sRGB to linear light and back: the transfer curve as `f32` functions, and a whole photograph decoded into linear
//! RGB and encoded back to the channel depth it came at.
//!
//! The first caller is the lens blur (`models::lens_blur`), which blurs in linear light so that a bright point spreads
//! into a bright disc, as a lens spreads it, rather than into the grey smear an average of encoded values gives.

// Not `models::colorization::lab`'s curve, which is `f64`, crate-private to `opai` and tuned for Lab's own fast
// paths. This one is the primitive a render needs at full resolution: decoded through a table, since every sample a
// photograph can hold is one of 65,536, and encoded through a table of thresholds at eight bits, since every result is
// one of 256.

use std::sync::LazyLock;

use image::{DynamicImage, ImageBuffer, Rgb};

use crate::ChannelDepth;
use crate::rows::for_each_row;
use crate::tensor::{Channel, Sampler};

/// An sRGB-encoded channel in `[0, 1]` as the linear light it stands for.
#[inline]
pub fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

/// A linear-light channel as its sRGB encoding. Unclamped: [`encoded`] clamps, because a render's result can stray a
/// rounding error past either end and a channel cannot.
#[inline]
pub fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 }
}

/// The linear light of every sample [`Sampler::rgb`] can return.
static FROM_SIXTEEN: LazyLock<Box<[f32]>> =
    LazyLock::new(|| (0..=u16::MAX).map(|sample| srgb_to_linear(sample.to_unit())).collect());

// Read out of the sixteen-bit table rather than computed from `v / 255`, so that an eight-bit pixel decodes to exactly
// the value it would through the sampler as `v * 257`: an `Rgb8` photograph and the same photograph as `Rgba8` with
// every pixel opaque cannot come out a rounding error apart.
/// The linear light of every eight-bit value, for the borrowed `Rgb8` buffer most photographs arrive as.
static FROM_EIGHT: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let table = &*FROM_SIXTEEN;
    std::array::from_fn(|value| table[value * 257])
});

// Each boundary is found by bisecting the `f32` bit pattern rather than the value, as `lab::THRESHOLDS` does it: for
// positive floats the bit order is the value order, so the search lands on the exact float at which the encoded byte
// changes, and the lookup below is the direct expression bit for bit rather than within a level.
/// `TO_EIGHT[k]` is the smallest linear value that encodes to byte `k + 1`.
static TO_EIGHT: LazyLock<[f32; 255]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let step = index + 1;
        let (mut lo, mut hi) = (0.0_f32.to_bits(), 1.0_f32.to_bits());

        while lo < hi {
            let mid = lo + (hi - lo) / 2;

            if usize::from(u8::from_unit(linear_to_srgb(f32::from_bits(mid)))) >= step {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }

        f32::from_bits(lo)
    })
});

/// A linear value encoded to the byte it rounds to: eight comparisons in place of a `powf`.
///
/// Equal to `u8::from_unit(linear_to_srgb(value.clamp(0.0, 1.0)))` for every `value`, `NaN` included (which is 0).
#[inline]
fn encode_eight(value: f32) -> u8 {
    TO_EIGHT.partition_point(|threshold| *threshold <= value) as u8
}

/// A linear value encoded to the sixteen-bit channel it rounds to.
#[inline]
fn encode_sixteen(value: f32) -> u16 {
    u16::from_unit(linear_to_srgb(value.clamp(0.0, 1.0)))
}

/// `image` as linear RGB, three interleaved `f32`s a pixel, and the channel depth it carried.
///
/// Any variant: greyscale is spread over three channels, and alpha is composited against black, as every pipeline in
/// this crate reads a photograph through [`Sampler`]. The depth is sixteen for anything carrying more than eight bits a
/// channel, the float variants included, and eight otherwise.
pub fn decoded_linear(image: &DynamicImage) -> (Vec<f32>, ChannelDepth) {
    // The same classification as `opai`'s `inference::depth`, answered from the pixel layout rather than from a match
    // on the variants, which `#[non_exhaustive]` would leave open.
    let color = image.color();
    let depth = if color.bits_per_pixel() / u16::from(color.channel_count()) > 8 {
        ChannelDepth::Sixteen
    } else {
        ChannelDepth::Eight
    };

    let (width, height) = (image.width() as usize, image.height() as usize);
    let mut linear = vec![0.0; 3 * width * height];

    if linear.is_empty() {
        return (linear, depth);
    }

    if let DynamicImage::ImageRgb8(buffer) = image {
        let (raw, table) = (buffer.as_raw(), &*FROM_EIGHT);

        for_each_row(&mut linear, 3 * width, height, |y, out| {
            for (value, byte) in out.iter_mut().zip(&raw[3 * width * y..][..3 * width]) {
                *value = table[usize::from(*byte)];
            }
        });

        return (linear, depth);
    }

    let (sampler, table) = (Sampler::new(image), &*FROM_SIXTEEN);

    for_each_row(&mut linear, 3 * width, height, |y, out| {
        let mut pixels = vec![[0_u16; 3]; width];
        sampler.row(y as u32, &mut pixels);

        for (value, sample) in out.iter_mut().zip(pixels.iter().flatten()) {
            *value = table[usize::from(*sample)];
        }
    });

    (linear, depth)
}

/// Linear RGB, three interleaved `f32`s a pixel, encoded to sRGB at `depth`, every channel clamped to `[0, 1]` first.
///
/// # Panics
///
/// Panics where `linear` is not three values for every pixel of `width` x `height`.
pub fn encoded(linear: &[f32], width: u32, height: u32, depth: ChannelDepth) -> DynamicImage {
    let (columns, rows) = (width as usize, height as usize);
    assert_eq!(
        linear.len(),
        3 * columns * rows,
        "the linear image is not three values for each of {width}x{height}"
    );

    match depth {
        ChannelDepth::Eight => DynamicImage::ImageRgb8(encode_with(linear, width, height, encode_eight)),
        ChannelDepth::Sixteen => DynamicImage::ImageRgb16(encode_with(linear, width, height, encode_sixteen)),
    }
}

/// [`encoded`]'s loop at one channel type.
fn encode_with<T: Channel + Send>(
    linear: &[f32],
    width: u32,
    height: u32,
    encode: fn(f32) -> T,
) -> ImageBuffer<Rgb<T>, Vec<T>>
where
    Rgb<T>: image::Pixel<Subpixel = T>,
{
    let row_len = 3 * width as usize;
    let mut raw = vec![T::from_unit(0.0); row_len * height as usize];

    if !raw.is_empty() {
        for_each_row(&mut raw, row_len, height as usize, |y, out| {
            for (channel, value) in out.iter_mut().zip(&linear[row_len * y..][..row_len]) {
                *channel = encode(*value);
            }
        });
    }

    ImageBuffer::from_raw(width, height, raw).expect("three channels for every pixel of every row")
}

#[cfg(test)]
mod tests {
    use image::Rgba;

    use super::*;

    #[test]
    fn the_curve_passes_through_its_known_points() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert_eq!(srgb_to_linear(1.0), 1.0);
        assert!((srgb_to_linear(0.5) - 0.214_041_14).abs() < 1e-6, "mid-grey is {}", srgb_to_linear(0.5));

        // The joint between the linear segment and the power law, where the two halves meet to within a rounding of
        // each other: the published constants make the curve continuous there but not exactly.
        assert!((srgb_to_linear(0.04045) - 0.003_130_8).abs() < 1e-6);
        assert!((linear_to_srgb(0.003_130_8) - 0.04045).abs() < 1e-5);

        assert_eq!(linear_to_srgb(0.0), 0.0);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
        assert!((linear_to_srgb(0.214_041_14) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn every_eight_bit_value_comes_back_exactly() {
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(256, 2, |x, y| {
            Rgb([x as u8, 255 - x as u8, (x as u8).wrapping_mul(7).wrapping_add(y as u8)])
        }));

        let (linear, depth) = decoded_linear(&source);
        assert_eq!(depth, ChannelDepth::Eight);

        let back = encoded(&linear, 256, 2, depth);
        assert_eq!(back.as_rgb8().unwrap().as_raw(), source.as_rgb8().unwrap().as_raw());
    }

    #[test]
    fn sampled_sixteen_bit_values_come_back_within_one_code() {
        // A prime stride through the whole range, so the samples land on no pattern of the curve's own.
        let samples: Vec<u16> = (0..=u16::MAX).step_by(97).collect();
        let width = samples.len() as u32;
        let source = DynamicImage::ImageRgb16(ImageBuffer::from_fn(width, 1, |x, _| {
            let value = samples[x as usize];
            Rgb([value, u16::MAX - value, value / 3])
        }));

        let (linear, depth) = decoded_linear(&source);
        assert_eq!(depth, ChannelDepth::Sixteen);

        let back = encoded(&linear, width, 1, depth);
        for (got, want) in back.as_rgb16().unwrap().as_raw().iter().zip(source.as_rgb16().unwrap().as_raw()) {
            assert!(got.abs_diff(*want) <= 1, "{want} came back as {got}");
        }
    }

    #[test]
    fn the_eight_bit_thresholds_are_the_direct_encode_bit_for_bit() {
        // A dense sweep of the unit range and a little either side of it, plus every threshold and its neighbours,
        // which is where a search that had stopped one float short would show.
        let mut values: Vec<f32> = (-1000..=101_000).map(|step| step as f32 / 100_000.0).collect();
        for threshold in TO_EIGHT.iter() {
            values.extend([
                f32::from_bits(threshold.to_bits() - 1),
                *threshold,
                f32::from_bits(threshold.to_bits() + 1),
            ]);
        }

        for value in values {
            let direct = u8::from_unit(linear_to_srgb(value.clamp(0.0, 1.0)));
            assert_eq!(encode_eight(value), direct, "{value} encoded to a different byte");
        }

        assert_eq!(encode_eight(f32::NAN), 0);
    }

    #[test]
    fn alpha_is_composited_against_black_and_an_opaque_pixel_decodes_as_without_it() {
        let source = DynamicImage::ImageRgba8(ImageBuffer::from_fn(3, 1, |x, _| match x {
            0 => Rgba([200, 100, 50, 255]),
            1 => Rgba([200, 100, 50, 0]),
            _ => Rgba([255, 255, 255, 128]),
        }));
        let opaque = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(1, 1, Rgb([200, 100, 50])));

        let (linear, depth) = decoded_linear(&source);
        let (reference, _) = decoded_linear(&opaque);

        assert_eq!(depth, ChannelDepth::Eight);
        assert_eq!(
            linear[..3],
            reference[..],
            "an opaque pixel decoded differently from the same pixel without alpha"
        );
        assert_eq!(linear[3..6], [0.0; 3], "a transparent pixel was not composited against black");
        assert!(
            linear[6] > 0.2 && linear[6] < 0.3,
            "half-transparent white is {} rather than about mid-grey",
            linear[6]
        );
    }

    #[test]
    fn an_image_with_no_area_decodes_and_encodes_to_nothing() {
        let (linear, _) = decoded_linear(&DynamicImage::ImageRgb8(ImageBuffer::new(0, 5)));

        assert!(linear.is_empty());
        assert_eq!(encoded(&linear, 0, 5, ChannelDepth::Eight).width(), 0);
    }
}
