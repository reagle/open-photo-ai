//! The layered depth-of-field render: a photograph in linear light blurred as a lens would have blurred it, given how
//! large a disc each pixel should be.
//!
//! The first caller is the lens blur (`models::lens_blur`), which computes the radius per pixel from a depth map and a
//! lens; nothing here knows about either. A radius is signed: negative behind the plane of focus, positive in front of
//! it, zero on it. The sign orders the depth layers, which is what lets a sharp subject keep its edge against a
//! blurred background and a blurred foreground spread over a sharp subject.
//!
//! **No FFT.** A disc is, row by row, a horizontal run, so a running sum along each row convolves a layer with it at a
//! cost of a few reads per row of the disc, and a layer wider than [`Defocus::max_direct_radius`] is convolved at a
//! reduced resolution, so no layer costs more than a disc of that radius does. At 24 megapixels the four transforms a
//! layer would need cost more than that, and an FFT crate would be this crate's third dependency.

// The algorithm, in the order `render` runs it (the series design's D10 and D11):
//
// 1. The radius is scaled by `1 - keep_sharp` (the masking seam), and the radii it spans are split into layer radii,
//    one of them exactly 0: about `layers` evenly spaced, or geometrically from a first radius by a ratio.
// 2. Each pixel's weight is shared between the two layers its radius falls between, linearly, so the blur grows
//    smoothly with the radius rather than in steps. Under half a pixel the whole weight is on the layer at 0, so a
//    pixel that close to focus comes back exactly as it went in.
// 3. Far to near, each layer's premultiplied colour and coverage are convolved with an anti-aliased disc of the layer's
//    radius and composited "over" what is behind it. The layer at 0 is not convolved.
// 4. A matted subject, if there is one, is one more layer, its colour times its alpha, composited before the first
//    layer nearer than its own radius: over the background, which the caller has filled in behind it, and under
//    anything nearer. One layer rather than its pixels spread over the depth layers by their own radii: a subject's
//    depth wobbles, and spread over many layers its blurred coverage leaves gaps the filled background shows through.
// 5. The result is the accumulated colour divided by the accumulated coverage (a normalised convolution), so a layer's
//    blur never pulls in colour from a layer in front of it, and nearer, sharper layers cover what is behind them.
//
// Rejected: a gather-only variable-radius blur, which cannot let a near, blurred subject spread over a sharp background
// nor stop a sharp subject bleeding into a blurred one; separable or hexagonal box approximations, whose highlights are
// polygons rather than discs.

use std::borrow::Cow;

use crate::bilinear::Axis;
use crate::plane::Plane;
use crate::rows::for_each_row;

/// How the render divides the work: how many depth layers, and up to what radius a layer is convolved at full
/// resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Defocus {
    /// About how many layers the radii are split into, with [`Spacing::Even`]. More is smoother grading between depths
    /// and a proportionally slower render; there is one more than this at most, because one layer is placed exactly at
    /// 0.
    pub layers: usize,
    /// The largest radius, in pixels, a layer is convolved with at full resolution, and at least 1. A layer beyond it
    /// is reduced by `ceil(radius / max_direct_radius)` on each axis, convolved there and enlarged back, which softens
    /// its discs' rims by about that many pixels: at most a twelfth of the radius at the default.
    pub max_direct_radius: f32,
    /// How the layer radii are spread over the radii the photograph spans.
    pub spacing: Spacing,
}

impl Defocus {
    /// Fourteen evenly spaced layers, and full resolution up to 12 px.
    pub const DEFAULT: Self = Self { layers: 14, max_direct_radius: 12.0, spacing: Spacing::Even };
}

impl Default for Defocus {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// How the layer radii are spread over the radii a photograph spans. Either way one layer is exactly at 0 and the
/// outermost are exactly at the extreme radii.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spacing {
    /// About [`Defocus::layers`] radii, evenly spaced on each side of 0 and at least a pixel apart.
    Even,
    /// Radii 0, then `first`, `first * ratio`, `first * ratio^2` and so on, on each side of 0: dense near focus, where
    /// a pixel of radius is a visible difference, and sparse far from it, where it is not. [`Defocus::layers`] is not
    /// read. Evenly spaced layers over a wide range leave nothing near focus rendered at its own radius (14 layers
    /// over -400 to +150 px are 40 px apart), so a subject a pixel off focus is a cross-fade of sharp and a 40 px
    /// blur.
    Geometric {
        /// The smallest radius after 0, in pixels; at least half a pixel (the in-focus dead zone), since a layer
        /// closer to 0 than that would be a layer of nothing.
        first: f32,
        /// How much larger each radius is than the one before it, at least 1.05; and no radius is less than a pixel
        /// larger than the one before it, so near focus the layers are a pixel apart however close to 1 the ratio is.
        ratio: f32,
    },
}

/// A matted subject: composited over the rest of the photograph as one layer, at one radius.
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    /// The subject's own colour, linear, three interleaved values a pixel at the photograph's size, with the background
    /// its edge pixels were mixed with taken out ([`fusion`](crate::fusion)).
    pub colour: &'a [f32],
    /// How much of each pixel the subject covers, in `[0, 1]`, at any size: it is read bilinearly, as a keep-sharp
    /// weight is.
    pub alpha: &'a Plane,
    /// The one signed radius the whole subject is blurred by, in pixels; within half a pixel of 0 it is sharp.
    pub radius: f32,
}

/// Under this radius, in pixels, a pixel is in focus and is not blurred at all.
const IN_FOCUS: f32 = 0.5;

/// The closest two layer radii are placed. A disc of radius 4.3 and one of 4.6 differ by less than a pixel at the rim,
/// and a layer that close to its neighbour is a full pass over its box for nothing.
const MIN_SPACING: f32 = 1.0;

/// The smallest ratio a geometric spacing may have, which bounds the layers: at 24 megapixels, whose diagonal is the
/// largest radius, 1.05 is some 140 layers on each side of 0, each a pass over its box, where a ratio of 1.001 would ask
/// for thousands. Radii 5% apart are already finer than the eye can tell.
const MIN_RATIO: f32 = 1.05;

/// A layer's weight below which a pixel is left out of it.
const NEGLIGIBLE: f32 = 1e-4;

/// `linear`, three interleaved values a pixel over `width` x `height`, blurred by an anti-aliased disc of `radius`'s
/// value at each pixel, in layers: see the module documentation.
///
/// `radius` is signed and in pixels, at the photograph's size. `keep_sharp`, at any size, is a weight in `[0, 1]` that
/// scales the radius by one minus itself: 1 keeps a pixel in focus, and a weight with no area is no weight. A radius
/// that is not finite is taken as 0, and one beyond the photograph's diagonal as the diagonal: a disc that size
/// already covers the photograph from any pixel. `linear` should be finite; a value that is not is read as 0, so one
/// bad sample cannot poison the running sums along its row.
///
/// With a `subject`, `linear` and `radius` are the photograph as if the subject were not there, filled in behind it
/// ([`fill`](crate::fill)), and the subject is composited "over" it as one layer of its colour times its alpha,
/// convolved by the disc of its own radius, in its place in the far-to-near order: over every layer at or behind that
/// radius and under every nearer one. A pixel it covers by `a` is `a` of its colour and `1 - a` of what is behind it,
/// blurred. A subject within half a pixel of focus is in front of the in-focus layer. `keep_sharp` scales the radius
/// of what is behind, never the subject's; an alpha with no area is no subject.
///
/// Returns `None` when `cancelled` answers true, which it is asked before each layer and before the subject.
///
/// # Panics
///
/// Panics where `linear` or a subject's colour is not three values for every pixel, `radius` is not `width` x
/// `height`, `settings.max_direct_radius` is under 1 or not a number, or a geometric spacing's `first` is under half a
/// pixel or its `ratio` under 1.05.
#[expect(
    clippy::too_many_arguments,
    reason = "the photograph, its radius and the two optional seams into it are each their own thing; a struct of them \
              would be built inline at the one call site"
)]
pub fn defocus(
    linear: &[f32],
    width: u32,
    height: u32,
    radius: &Plane,
    keep_sharp: Option<&Plane>,
    subject: Option<Subject<'_>>,
    settings: Defocus,
    cancelled: &dyn Fn() -> bool,
) -> Option<Vec<f32>> {
    let (columns, rows) = (width as usize, height as usize);

    assert_eq!(
        linear.len(),
        3 * columns * rows,
        "the photograph is not three values for each of {width}x{height}"
    );
    assert_eq!(
        (radius.width(), radius.height()),
        (width, height),
        "the radius plane is not the photograph's size"
    );

    // Scaled into a resample of the weight, which is the one full-resolution plane the seam costs, and only when there
    // is a weight at all.
    let scaled = keep_sharp
        .filter(|weight| weight.width() > 0 && weight.height() > 0)
        .map(|weight| keep_sharp_scaled(radius, weight));

    let image = Image { linear, radius: scaled.as_ref().unwrap_or(radius).as_slice(), width: columns, height: rows };

    let subject = subject
        .filter(|subject| subject.alpha.width() > 0 && subject.alpha.height() > 0)
        .map(|subject| {
            assert_eq!(
                subject.colour.len(),
                3 * columns * rows,
                "the subject's colour is not three values for each of {width}x{height}"
            );

            Matted::new(subject, width, height)
        });

    render(&image, subject.as_ref(), settings, cancelled, Workers::Many)
}

/// `radius` scaled by one minus `weight`, which is resampled to `radius`'s size and clamped to `[0, 1]`.
///
/// `weight` must have an area.
fn keep_sharp_scaled(radius: &Plane, weight: &Plane) -> Plane {
    let mut scaled = weight.resized(radius.width(), radius.height());

    for (value, radius) in scaled.as_mut_slice().iter_mut().zip(radius.as_slice()) {
        *value = radius * (1.0 - value.clamp(0.0, 1.0));
    }

    scaled
}

/// The photograph and its radius, as the render reads them.
struct Image<'a> {
    /// Three interleaved linear values a pixel.
    linear: &'a [f32],
    /// The signed radius a pixel, already scaled by any keep-sharp weight.
    radius: &'a [f32],
    width: usize,
    height: usize,
}

/// How a pass's rows are run.
#[derive(Debug, Clone, Copy)]
enum Workers {
    /// Across the available cores, through [`for_each_row`].
    Many,
    /// One after another on the calling thread: the serial result the parallel one must equal bit for bit.
    #[cfg(test)]
    One,
}

impl Workers {
    /// `row` over each of `dst`'s `height` rows of `row_len` elements.
    fn each_row<T: Send>(self, dst: &mut [T], row_len: usize, height: usize, row: impl Fn(usize, &mut [T]) + Sync) {
        if row_len * height == 0 {
            return;
        }

        match self {
            Self::Many => for_each_row(dst, row_len, height, row),
            #[cfg(test)]
            Self::One => {
                for (y, out) in dst.chunks_mut(row_len).enumerate().take(height) {
                    row(y, out);
                }
            }
        }
    }
}

/// The whole render, once the radius is final.
fn render(
    image: &Image<'_>,
    subject: Option<&Matted<'_>>,
    settings: Defocus,
    cancelled: &dyn Fn() -> bool,
    workers: Workers,
) -> Option<Vec<f32>> {
    assert!(
        settings.max_direct_radius >= 1.0,
        "max_direct_radius is {}, not at least 1",
        settings.max_direct_radius
    );

    // The layers are bounded by the diagonal, so no disc, and no reduction factor, is larger than the photograph is.
    let bound = (image.width as f32).hypot(image.height as f32).ceil();
    let (lo, hi) = image.radius.iter().fold((0.0_f32, 0.0_f32), |(lo, hi), radius| {
        let radius = finite(*radius).clamp(-bound, bound);
        (lo.min(radius), hi.max(radius))
    });

    // Where the subject goes: before the first layer nearer than its radius. One within the in-focus dead zone is on
    // the plane of focus, and in front of the layer there: what is behind the subject reaches it only through the
    // fill, and nothing equally in focus should cover it.
    let mut subject = subject.map(|subject| {
        let radius = finite(subject.radius).clamp(-bound, bound);
        (subject, if radius.abs() < IN_FOCUS { 0.0 } else { radius })
    });

    // A layer at the subject's radius, so that no background pixel shares its weight between a layer behind the subject
    // and one in front of it: it would show over the subject though it is behind it.
    let mut layers = Layers::new(lo, hi, settings);
    if let Some((_, radius)) = subject {
        layers.insert(radius);
    }
    let extents = layers.extents(image, workers);

    // Premultiplied colour and coverage, four interleaved values a pixel: what every layer is composited into.
    let mut accumulated = vec![0.0_f32; 4 * image.width * image.height];

    // Ascending, which is far to near: the most negative radius is the farthest behind the plane of focus.
    for (index, extent) in extents.iter().enumerate() {
        if let Some((matted, radius)) = subject.take_if(|(_, radius)| layers.centres[index] > *radius) {
            if cancelled() {
                return None;
            }

            matted.composite(image, radius, settings, workers, &mut accumulated);
        }

        if cancelled() {
            return None;
        }

        let Some(extent) = *extent else { continue };
        let source = Slice { image, layers: &layers, index };

        Layer { image, source, workers }.composite(layers.centres[index].abs(), extent, settings, &mut accumulated);
    }

    // A subject nearer than every layer goes over all of them.
    if let Some((matted, radius)) = subject {
        if cancelled() {
            return None;
        }

        matted.composite(image, radius, settings, workers, &mut accumulated);
    }

    let mut out = vec![0.0_f32; 3 * image.width * image.height];

    workers.each_row(&mut out, 3 * image.width, image.height, |y, out| {
        let row = &accumulated[4 * image.width * y..][..4 * image.width];
        let original = &image.linear[3 * image.width * y..][..3 * image.width];

        let pixels = out.as_chunks_mut::<3>().0.iter_mut().zip(row.as_chunks::<4>().0);

        for ((rgb, sum), original) in pixels.zip(original.as_chunks::<3>().0) {
            // Unreachable in practice — every pixel's own weights sum to one and a disc includes its centre — but a
            // pixel no layer covered keeps the photograph rather than a division by nothing.
            if sum[3] > 1e-6 {
                let inverse = 1.0 / sum[3];
                *rgb = [sum[0] * inverse, sum[1] * inverse, sum[2] * inverse];
            } else {
                *rgb = original.map(finite);
            }
        }
    });

    Some(out)
}

/// A radius as the layering reads it, or a sample as the layers do: anything that is not a finite number is 0, which is
/// in focus for a radius.
#[inline]
fn finite(radius: f32) -> f32 {
    if radius.is_finite() { radius } else { 0.0 }
}

/// The layer radii, ascending, one of them exactly 0.
struct Layers {
    centres: Vec<f32>,
}

impl Layers {
    /// The layer radii over `[lo, hi]` widened to include 0, spaced as `settings` say.
    fn new(lo: f32, hi: f32, settings: Defocus) -> Self {
        match settings.spacing {
            Spacing::Even => Self::spanning(lo, hi, settings.layers),
            Spacing::Geometric { first, ratio } => Self::geometric(lo, hi, first, ratio),
        }
    }

    /// Radii 0, `first`, `first * ratio` and so on out to the extreme radius on each side, which is a layer of its own,
    /// each at least [`MIN_SPACING`] beyond the one before.
    fn geometric(lo: f32, hi: f32, first: f32, ratio: f32) -> Self {
        assert!(
            (IN_FOCUS..=f32::MAX).contains(&first) && (MIN_RATIO..=f32::MAX).contains(&ratio),
            "a geometric spacing from {first} px by {ratio}, rather than from at least {IN_FOCUS} px by {MIN_RATIO}"
        );

        let side = |extreme: f32| {
            let mut radii = Vec::new();

            if extreme < IN_FOCUS {
                return radii;
            }

            let mut radius = first;
            while radius < extreme {
                radii.push(radius);
                radius = (radius * ratio).max(radius + MIN_SPACING);
            }

            // The extreme radius ends the side exactly, as with even spacing. It replaces the last of the sequence
            // where it is within the square root of the ratio of it, or within `MIN_SPACING`, so no two layers are
            // much closer or farther apart than the ratio says.
            match radii.last_mut() {
                Some(last) if extreme / *last < ratio.sqrt() || extreme - *last < MIN_SPACING => *last = extreme,
                _ => radii.push(extreme),
            }

            radii
        };

        let centres = side(-lo.min(0.0))
            .into_iter()
            .rev()
            .map(|radius| -radius)
            .chain([0.0])
            .chain(side(hi.max(0.0)))
            .collect();

        Self { centres }
    }

    /// About `count` radii evenly spaced over `[lo, hi]` widened to include 0, with one exactly at 0 and none closer
    /// together than [`MIN_SPACING`].
    fn spanning(lo: f32, hi: f32, count: usize) -> Self {
        let (lo, hi) = (lo.min(0.0), hi.max(0.0));
        let spacing = ((hi - lo) / (count.max(2) - 1) as f32).max(MIN_SPACING);

        // Each side of 0 divided evenly on its own, so the ends land exactly on the extreme radii and the middle
        // exactly on 0: a layer at 0.3 px instead would blur the whole in-focus subject, a little.
        let behind = (-lo / spacing).ceil() as usize;
        let front = (hi / spacing).ceil() as usize;

        let centres = (0..behind)
            .map(|step| lo * (behind - step) as f32 / behind as f32)
            .chain([0.0])
            .chain((1..=front).map(|step| hi * step as f32 / front as f32))
            .collect();

        Self { centres }
    }

    /// A layer at exactly `radius`, where there is none and layers lie on both sides of it, so that the tent a pixel
    /// shares between two layers never straddles `radius`: a pixel at or behind it is wholly on layers at or behind it.
    /// A pixel nearer than it still fades in over the interval above it. Outside the layers' span no tent crosses it.
    fn insert(&mut self, radius: f32) {
        let at = self.centres.partition_point(|centre| *centre < radius);

        if at > 0 && at < self.centres.len() && self.centres[at] != radius {
            self.centres.insert(at, radius);
        }
    }

    /// The layer `radius` falls at or above, and how far towards the next one it is: its weight is `1 - t` on that
    /// layer and `t` on the next.
    fn split(&self, radius: f32) -> (usize, f32) {
        let radius = finite(radius);
        let above = self.centres.partition_point(|centre| *centre <= radius);

        if above == 0 {
            return (0, 0.0);
        }

        if above == self.centres.len() {
            return (above - 1, 0.0);
        }

        let (near, far) = (self.centres[above - 1], self.centres[above]);

        // The interval that touches 0 starts at half a pixel rather than at 0 (the in-focus dead zone), so a pixel
        // within it is wholly on the layer at 0 and the blur still grows continuously from there.
        let start = if near == 0.0 { IN_FOCUS.min(far) } else { near };
        let end = if far == 0.0 { (-IN_FOCUS).max(near) } else { far };

        let t = if end > start {
            ((radius - start) / (end - start)).clamp(0.0, 1.0)
        } else {
            f32::from(radius >= end)
        };

        (above - 1, t)
    }

    /// Layer `index`'s weight for a pixel at `radius`, with a negligible one left out.
    #[inline]
    fn weight(&self, index: usize, radius: f32) -> f32 {
        let (layer, t) = self.split(radius);

        let weight = if layer == index {
            1.0 - t
        } else if layer + 1 == index {
            t
        } else {
            0.0
        };

        if weight < NEGLIGIBLE { 0.0 } else { weight }
    }

    /// Each layer's extent: the smallest rectangle holding every pixel it has a weight on, or `None` where it has
    /// none.
    fn extents(&self, image: &Image<'_>, workers: Workers) -> Vec<Option<Extent>> {
        let count = self.centres.len();

        // Per row and per layer, the first and last column with a weight, in parallel; then folded over the rows.
        let mut spans = vec![[u32::MAX, 0]; count * image.height];

        workers.each_row(&mut spans, count, image.height, |y, spans| {
            for (x, radius) in image.radius[image.width * y..][..image.width].iter().enumerate() {
                let (layer, t) = self.split(*radius);

                for (index, weight) in [(layer, 1.0 - t), (layer + 1, t)] {
                    if index < count && weight >= NEGLIGIBLE {
                        let span = &mut spans[index];
                        *span = [span[0].min(x as u32), span[1].max(x as u32)];
                    }
                }
            }
        });

        (0..count)
            .map(|index| {
                let mut extent: Option<Extent> = None;

                for (y, span) in spans.iter().skip(index).step_by(count).enumerate() {
                    if span[0] > span[1] {
                        continue;
                    }

                    let (x0, x1) = (span[0] as usize, span[1] as usize + 1);
                    extent = Some(match extent {
                        None => Extent { x0, x1, y0: y, y1: y + 1 },
                        Some(e) => Extent { x0: e.x0.min(x0), x1: e.x1.max(x1), y0: e.y0, y1: y + 1 },
                    });
                }

                extent
            })
            .collect()
    }
}

/// A rectangle of the photograph, as half-open column and row ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Extent {
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

impl Extent {
    /// The rectangle grown by `pad` on every side and clipped to the photograph.
    fn padded(self, pad: usize, image: &Image<'_>) -> Self {
        Self {
            x0: self.x0.saturating_sub(pad),
            x1: (self.x1 + pad).min(image.width),
            y0: self.y0.saturating_sub(pad),
            y1: (self.y1 + pad).min(image.height),
        }
    }

    fn width(self) -> usize {
        self.x1 - self.x0
    }

    fn height(self) -> usize {
        self.y1 - self.y0
    }
}

/// What a layer is cut from: its premultiplied colour and coverage at each pixel of the photograph.
trait Source: Sync {
    fn at(&self, x: usize, y: usize) -> [f32; 4];
}

/// One of the depth layers: the photograph, weighted by the layer's share of each pixel's radius.
struct Slice<'a> {
    image: &'a Image<'a>,
    layers: &'a Layers,
    index: usize,
}

impl Source for Slice<'_> {
    #[inline]
    fn at(&self, x: usize, y: usize) -> [f32; 4] {
        let pixel = y * self.image.width + x;
        let weight = self.layers.weight(self.index, self.image.radius[pixel]);
        let rgb = &self.image.linear[3 * pixel..][..3];

        // Read through `finite`: NaN times a weight of 0 is still NaN, and one in a window poisons its whole row.
        [finite(rgb[0]) * weight, finite(rgb[1]) * weight, finite(rgb[2]) * weight, weight]
    }
}

/// A [`Subject`] as the render reads it: its alpha at the photograph's size.
struct Matted<'a> {
    colour: &'a [f32],
    alpha: Cow<'a, Plane>,
    radius: f32,
}

impl<'a> Matted<'a> {
    /// `subject` over a `width` x `height` photograph; its alpha must have an area.
    fn new(subject: Subject<'a>, width: u32, height: u32) -> Self {
        let alpha = if (subject.alpha.width(), subject.alpha.height()) == (width, height) {
            Cow::Borrowed(subject.alpha)
        } else {
            Cow::Owned(subject.alpha.resized(width, height))
        };

        Self { colour: subject.colour, alpha, radius: subject.radius }
    }

    /// The subject convolved by the disc of `radius` and composited into `accumulated`.
    fn composite(&self, image: &Image<'_>, radius: f32, settings: Defocus, workers: Workers, accumulated: &mut [f32]) {
        if let Some(extent) = self.extent(image, workers) {
            Layer { image, source: self, workers }.composite(radius.abs(), extent, settings, accumulated);
        }
    }

    /// The smallest rectangle holding every pixel the subject covers, or `None` where it covers none.
    fn extent(&self, image: &Image<'_>, workers: Workers) -> Option<Extent> {
        let mut spans = vec![[u32::MAX, 0]; image.height];

        workers.each_row(&mut spans, 1, image.height, |y, span| {
            let row = &self.alpha.as_slice()[image.width * y..][..image.width];

            if let (Some(first), Some(last)) =
                (row.iter().position(|alpha| covers(*alpha)), row.iter().rposition(|alpha| covers(*alpha)))
            {
                span[0] = [first as u32, last as u32];
            }
        });

        spans.iter().enumerate().filter(|(_, span)| span[0] <= span[1]).fold(None, |extent, (y, span)| {
            let (x0, x1) = (span[0] as usize, span[1] as usize + 1);

            Some(match extent {
                None => Extent { x0, x1, y0: y, y1: y + 1 },
                Some(e) => Extent { x0: e.x0.min(x0), x1: e.x1.max(x1), y0: e.y0, y1: y + 1 },
            })
        })
    }
}

impl Source for &Matted<'_> {
    #[inline]
    fn at(&self, x: usize, y: usize) -> [f32; 4] {
        let pixel = y * self.alpha.width() as usize + x;
        let alpha = self.alpha.as_slice()[pixel];
        let alpha = if covers(alpha) { alpha.min(1.0) } else { 0.0 };
        let rgb = &self.colour[3 * pixel..][..3];

        [finite(rgb[0]) * alpha, finite(rgb[1]) * alpha, finite(rgb[2]) * alpha, alpha]
    }
}

/// Whether a subject's alpha covers its pixel at all: one that is negligible, or not a number, does not.
#[inline]
fn covers(alpha: f32) -> bool {
    alpha >= NEGLIGIBLE
}

/// One layer of the render, and the photograph it is cut from.
struct Layer<'a, S> {
    image: &'a Image<'a>,
    source: S,
    workers: Workers,
}

impl<S: Source> Layer<'_, S> {
    /// The layer's premultiplied colour and coverage at `(x, y)`.
    #[inline]
    fn at(&self, x: usize, y: usize) -> [f32; 4] {
        self.source.at(x, y)
    }

    /// The layer convolved with the disc of `radius`, the way that radius calls for, and composited.
    fn composite(&self, radius: f32, extent: Extent, settings: Defocus, accumulated: &mut [f32]) {
        if radius < IN_FOCUS {
            self.composite_sharp(extent.padded(0, self.image), accumulated);
        } else if radius <= settings.max_direct_radius {
            self.composite_direct(radius, extent, accumulated);
        } else {
            let factor = (radius / settings.max_direct_radius).ceil() as usize;
            self.composite_reduced(radius, factor, extent, accumulated);
        }
    }

    /// The rows of `accumulated` that `region` covers, and the length of one of them.
    fn rows_of<'b>(&self, region: Extent, accumulated: &'b mut [f32]) -> (usize, &'b mut [f32]) {
        let row_len = 4 * self.image.width;
        (row_len, &mut accumulated[row_len * region.y0..row_len * region.y1])
    }

    /// The in-focus layer, composited as it is.
    fn composite_sharp(&self, region: Extent, accumulated: &mut [f32]) {
        let (row_len, rows) = self.rows_of(region, accumulated);

        self.workers.each_row(rows, row_len, region.height(), |row, out| {
            for x in region.x0..region.x1 {
                over(&mut out[4 * x..4 * x + 4], self.at(x, region.y0 + row));
            }
        });
    }

    /// A layer convolved at full resolution and composited.
    fn composite_direct(&self, radius: f32, extent: Extent, accumulated: &mut [f32]) {
        let disc = Disc::new(radius);
        let region = extent.padded(disc.reach + 1, self.image);
        let (width, pad) = (region.width(), disc.reach + 1);

        // Padded by the disc's reach on both sides of every row, so the convolution's inner loops never clamp a column.
        let stride = width + 2 * pad;
        let mut layer = vec![0.0_f32; 4 * stride * region.height()];

        self.workers.each_row(&mut layer, 4 * stride, region.height(), |row, out| {
            for x in 0..width {
                out[4 * (pad + x)..][..4].copy_from_slice(&self.at(region.x0 + x, region.y0 + row));
            }

            replicate_edges(out, width, pad);
        });

        let (row_len, rows) = self.rows_of(region, accumulated);

        self.workers.each_row(rows, row_len, region.height(), |row, out| {
            let mut sum = vec![0.0_f32; 4 * width];
            disc.convolve_row(&layer, stride, pad, row, &mut sum);

            for (x, sum) in sum.as_chunks::<4>().0.iter().enumerate() {
                let at = 4 * (region.x0 + x);
                over(&mut out[at..at + 4], sum.map(|value| value * disc.scale));
            }
        });
    }

    /// A layer reduced by `factor`, convolved at `radius / factor`, enlarged back and composited.
    fn composite_reduced(&self, radius: f32, factor: usize, extent: Extent, accumulated: &mut [f32]) {
        let disc = Disc::new(radius / factor as f32);
        // The enlargement spreads a reduced pixel over two of them, so the region reaches a further `factor` pixels
        // beyond the disc on each side.
        let region = extent.padded((disc.reach + 2) * factor, self.image);
        let (width, height) = (region.width().div_ceil(factor), region.height().div_ceil(factor));
        let pad = disc.reach + 1;
        let stride = width + 2 * pad;

        // Each block's mean, so the layer's light and coverage are kept rather than resampled: a reduced layer holds
        // exactly the total the full one did, up to the blocks cut short at the region's right and bottom edges.
        let mut reduced = vec![0.0_f32; 4 * stride * height];

        self.workers.each_row(&mut reduced, 4 * stride, height, |row, out| {
            let ys = region.y0 + row * factor..(region.y0 + (row + 1) * factor).min(region.y1);

            for x in 0..width {
                let xs = region.x0 + x * factor..(region.x0 + (x + 1) * factor).min(region.x1);
                let mut sum = [0.0_f32; 4];

                for y in ys.clone() {
                    for column in xs.clone() {
                        let value = self.at(column, y);
                        sum = std::array::from_fn(|channel| sum[channel] + value[channel]);
                    }
                }

                let count = (ys.len() * xs.len()) as f32;
                out[4 * (pad + x)..][..4].copy_from_slice(&sum.map(|value| value / count));
            }

            replicate_edges(out, width, pad);
        });

        let mut blurred = vec![0.0_f32; 4 * width * height];

        self.workers.each_row(&mut blurred, 4 * width, height, |row, out| {
            disc.convolve_row(&reduced, stride, pad, row, out);

            for value in out.iter_mut() {
                *value *= disc.scale;
            }
        });

        drop(reduced);

        // The enlargement maps the reduced grid back at exactly `factor`, reading only as far as the region goes, for
        // the reason `guided::upsampled` gives for the same construction.
        let across = Axis::new(width as u32, (width * factor) as u32);
        let down = Axis::new(height as u32, (height * factor) as u32);
        let (row_len, rows) = self.rows_of(region, accumulated);

        self.workers.each_row(rows, row_len, region.height(), |row, out| {
            let tap = down.tap(row);
            let (top, bottom) =
                (&blurred[4 * width * tap.lo..][..4 * width], &blurred[4 * width * tap.hi..][..4 * width]);

            for x in 0..region.width() {
                let column = across.tap(x);
                let value = std::array::from_fn(|channel| {
                    let (lo, hi) = (4 * column.lo + channel, 4 * column.hi + channel);
                    let upper = top[lo] * (1.0 - column.frac) + top[hi] * column.frac;
                    let lower = bottom[lo] * (1.0 - column.frac) + bottom[hi] * column.frac;

                    upper * (1.0 - tap.frac) + lower * tap.frac
                });

                let at = 4 * (region.x0 + x);
                over(&mut out[at..at + 4], value);
            }
        });
    }
}

/// `layer` composited over `behind`, both premultiplied colour and coverage.
#[inline]
fn over(behind: &mut [f32], layer: [f32; 4]) {
    // The layer's coverage is capped at one because a convolved coverage can overshoot it by a rounding, and a
    // negative remainder would subtract what is behind.
    let keep = 1.0 - layer[3].min(1.0);

    for (behind, layer) in behind.iter_mut().zip(layer) {
        *behind = layer + *behind * keep;
    }
}

/// Copies a padded row's first and last real pixels outwards over its `pad` pixels on each side: the edge of the
/// photograph extended, so a blur near it neither darkens nor lets what is behind show through.
fn replicate_edges(row: &mut [f32], width: usize, pad: usize) {
    let (first, last) = (4 * pad, 4 * (pad + width - 1));

    for offset in 0..pad {
        row.copy_within(first..first + 4, 4 * offset);
        row.copy_within(last..last + 4, 4 * (pad + width + offset));
    }
}

/// An anti-aliased disc, as the runs and rim taps each of its rows is made of.
///
/// The kernel is `clamp(radius + 0.5 - hypot(dx, dy), 0, 1)`: one inside, zero outside, and the fraction of the pixel
/// the rim crosses in between.
struct Disc {
    rows: Vec<DiscRow>,
    /// The largest offset, on either axis, with a weight.
    reach: usize,
    /// One over the kernel's total weight.
    scale: f32,
}

/// One row of a [`Disc`].
struct DiscRow {
    /// The row's offset from the centre.
    dy: isize,
    /// The half-width of the run of whole pixels across the middle of the row, if it has any.
    run: Option<usize>,
    /// The fractional pixels beyond the run, by their distance from the centre column, weighted on both sides of it
    /// (or once, for a distance of 0 on a row with no run).
    rim: Vec<(usize, f32)>,
}

impl Disc {
    fn new(radius: f32) -> Self {
        // Every pixel the rim crosses is a tap of its own, rather than one fractional end per row. Near the top and
        // bottom of the disc the rim runs almost along the row, crossing several pixels each, and folding those into
        // one end weight would square the disc's top off by up to half a pixel; at the radii convolved here (12 px
        // and under) that is several percent of the disc. It costs about the circumference in extra reads a pixel,
        // which is under half again the runs' own.
        let reach = ((radius + 0.5).ceil() as usize).saturating_sub(1);
        let mut rows = Vec::with_capacity(2 * reach + 1);
        let mut total = 0.0_f64;

        for dy in -(reach as isize)..=reach as isize {
            let weights: Vec<f32> = (0..=reach)
                .map(|dx| (radius + 0.5 - ((dx * dx) as f32 + (dy * dy) as f32).sqrt()).clamp(0.0, 1.0))
                .collect();

            let run = weights.iter().position(|weight| *weight < 1.0).unwrap_or(weights.len()).checked_sub(1);
            let first_rim = run.map_or(0, |half| half + 1);

            let rim: Vec<(usize, f32)> =
                (first_rim..=reach).filter(|dx| weights[*dx] > 0.0).map(|dx| (dx, weights[dx])).collect();

            let row_total = run.map_or(0.0, |half| (2 * half + 1) as f64)
                + rim
                    .iter()
                    .map(|(dx, weight)| f64::from(*weight) * if *dx == 0 { 1.0 } else { 2.0 })
                    .sum::<f64>();

            if row_total > 0.0 {
                total += row_total;
                rows.push(DiscRow { dy, run, rim });
            }
        }

        Self { rows, reach, scale: (1.0 / total) as f32 }
    }

    /// Row `y` of `layer` convolved with the disc into `out`, unscaled.
    ///
    /// `layer` is rows of `stride` pixels, four values each, padded by `pad` pixels on both sides; `out` is one
    /// unpadded row of four values a pixel, and is added to. Rows above the first and below the last are the first and
    /// last.
    fn convolve_row(&self, layer: &[f32], stride: usize, pad: usize, y: usize, out: &mut [f32]) {
        let width = out.len() / 4;
        let height = layer.len() / (4 * stride);

        for row in &self.rows {
            let source = y.saturating_add_signed(row.dy).min(height - 1);
            let line = &layer[4 * stride * source..][..4 * stride];

            if let Some(half) = row.run {
                // A running sum in `f64` along the row: it slides one pixel at a time, adding what enters and taking
                // away what leaves, and in `f32` the rounding of a bright stretch would linger into a dark one after
                // it at a level the dark one cannot hide.
                let mut window = [0.0_f64; 4];

                for pixel in line[4 * (pad - half)..4 * (pad + half + 1)].as_chunks::<4>().0 {
                    window = std::array::from_fn(|channel| window[channel] + f64::from(pixel[channel]));
                }

                for x in 0..width {
                    if x > 0 {
                        let (enter, leave) = (4 * (pad + x + half), 4 * (pad + x - half - 1));
                        window = std::array::from_fn(|channel| {
                            window[channel] + f64::from(line[enter + channel]) - f64::from(line[leave + channel])
                        });
                    }

                    for channel in 0..4 {
                        out[4 * x + channel] += window[channel] as f32;
                    }
                }
            }

            for &(dx, weight) in &row.rim {
                if dx == 0 {
                    for (value, pixel) in out.iter_mut().zip(&line[4 * pad..4 * (pad + width)]) {
                        *value += weight * pixel;
                    }
                } else {
                    let (left, right) = (&line[4 * (pad - dx)..], &line[4 * (pad + dx)..]);

                    for ((value, left), right) in out.iter_mut().zip(left).zip(right) {
                        *value += weight * (left + right);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageBuffer, Rgb};

    use super::*;
    use crate::ChannelDepth;
    use crate::linear::{decoded_linear, encoded};

    /// A reproducible stream of values in `[0, 1)`.
    fn noise(count: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;

        (0..count)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32
            })
            .collect()
    }

    /// Renders with nothing cancelling it and no keep-sharp weight.
    fn rendered(linear: &[f32], width: u32, height: u32, radius: &Plane) -> Vec<f32> {
        defocus(linear, width, height, radius, None, None, Defocus::default(), &|| false).unwrap()
    }

    /// The anti-aliased disc's own weight at an offset, written out independently of [`Disc`].
    fn kernel(radius: f32, dx: isize, dy: isize) -> f32 {
        (radius + 0.5 - ((dx * dx + dy * dy) as f32).sqrt()).clamp(0.0, 1.0)
    }

    #[test]
    fn a_radius_of_zero_everywhere_returns_the_photograph() {
        let (width, height) = (61_u32, 47_u32);
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x * 7 + y * 3) as u8, (x * 2 + y * 11) as u8, (x + y) as u8])
        }));

        let (linear, depth) = decoded_linear(&source);
        let out = rendered(&linear, width, height, &Plane::new(width, height, 0.0));

        assert_eq!(out, linear, "nothing was to be blurred, and something changed");
        assert_eq!(
            encoded(&out, width, height, depth).as_rgb8().unwrap().as_raw(),
            source.as_rgb8().unwrap().as_raw()
        );
    }

    #[test]
    fn radii_under_half_a_pixel_change_nothing() {
        let (width, height) = (40_u32, 30_u32);
        let linear = noise(3 * 40 * 30, 7);
        let radius = Plane::from_vec(width, height, noise(40 * 30, 8).iter().map(|v| v * 0.98 - 0.49).collect());

        assert_eq!(rendered(&linear, width, height, &radius.unwrap()), linear);
    }

    #[test]
    fn a_point_of_light_becomes_an_even_disc_holding_the_same_light() {
        let (side, centre) = (61_u32, 30_isize);
        let mut linear = vec![0.0_f32; 3 * 61 * 61];
        linear[3 * (30 * 61 + 30)..][..3].copy_from_slice(&[1.0, 0.5, 0.25]);

        let out = rendered(&linear, side, side, &Plane::new(side, side, 10.0));

        let total: f32 = out.as_chunks::<3>().0.iter().map(|rgb| rgb[0]).sum();
        assert!((total - 1.0).abs() < 1e-4, "the disc holds {total} of the point's light");

        let inside = out[3 * (30 * 61 + 30)];
        for y in 0..61_isize {
            for x in 0..61_isize {
                let (dx, dy) = (x - centre, y - centre);
                let red = out[3 * (y as usize * 61 + x as usize)];
                let distance = ((dx * dx + dy * dy) as f32).sqrt();

                // Even inside, the anti-aliased fraction across the rim, and nothing beyond it; and the colour is the
                // point's own in every pixel it reached.
                let want = inside * kernel(10.0, dx, dy);
                assert!((red - want).abs() < 1e-6, "({x}, {y}) at {distance:.2} px is {red} rather than {want}");

                let rgb = &out[3 * (y as usize * 61 + x as usize)..][..3];
                assert!((rgb[1] - red / 2.0).abs() < 1e-6 && (rgb[2] - red / 4.0).abs() < 1e-6, "({x}, {y})'s colour");
            }
        }
    }

    #[test]
    fn the_runs_match_a_brute_force_disc() {
        let (width, height) = (37_usize, 29_usize);
        let layer = noise(4 * width * height, 11);

        for radius in [0.5_f32, 0.8, 1.0, 1.5, 2.3, 3.0, 4.7, 7.5, 9.99, 12.0] {
            let disc = Disc::new(radius);
            let pad = disc.reach + 1;
            let stride = width + 2 * pad;

            let mut padded = vec![0.0_f32; 4 * stride * height];
            for (y, row) in padded.chunks_exact_mut(4 * stride).enumerate() {
                row[4 * pad..4 * (pad + width)].copy_from_slice(&layer[4 * width * y..][..4 * width]);
                replicate_edges(row, width, pad);
            }

            let reach = disc.reach as isize;
            let total: f32 =
                (-reach..=reach).flat_map(|dy| (-reach..=reach).map(move |dx| kernel(radius, dx, dy))).sum();

            for y in 0..height {
                let mut out = vec![0.0_f32; 4 * width];
                disc.convolve_row(&padded, stride, pad, y, &mut out);

                for x in 0..width {
                    for channel in 0..4 {
                        let mut want = 0.0;
                        for dy in -reach..=reach {
                            for dx in -reach..=reach {
                                let sy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                                let sx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                                want += kernel(radius, dx, dy) * layer[4 * (sy * width + sx) + channel];
                            }
                        }

                        let got = out[4 * x + channel] * disc.scale;
                        assert!(
                            (got - want / total).abs() < 1e-4,
                            "radius {radius} at ({x}, {y}): {got} against the brute force's {}",
                            want / total
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_reduced_layer_keeps_its_light() {
        // 40 px is past the direct path's 12, so the layer is reduced by four, convolved at 10 and enlarged back.
        let side = 241_u32;
        let mut linear = vec![0.0_f32; 3 * 241 * 241];
        for y in 116..125 {
            for x in 113..122 {
                linear[3 * (y * 241 + x)..][..3].fill(1.0);
            }
        }

        let out = rendered(&linear, side, side, &Plane::new(side, side, 40.0));
        let (before, after): (f32, f32) = (linear.iter().sum(), out.iter().sum());

        assert!((after / before - 1.0).abs() < 0.01, "{before} of light went in and {after} came out");

        // And it was spread as a disc of about the radius, not kept or lost: nothing near the centre is the square's
        // full brightness, and nothing beyond the disc and the enlargement's spread is lit.
        assert!(out[3 * (120 * 241 + 117)] < 0.1);
        assert_eq!(out[3 * (120 * 241 + 117 + 50)], 0.0);
    }

    #[test]
    fn a_sharp_square_in_front_of_a_blurred_background_keeps_its_edge_and_its_colour() {
        let side = 120_u32;
        let inside = |x: u32, y: u32| (40..80).contains(&x) && (40..80).contains(&y);

        let linear: Vec<f32> = (0..side * side)
            .flat_map(|index| if inside(index % side, index / side) { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] })
            .collect();
        let radius = Plane::from_vec(
            side,
            side,
            (0..side * side).map(|i| if inside(i % side, i / side) { 0.0 } else { -20.0 }).collect(),
        );

        let out = rendered(&linear, side, side, &radius.unwrap());

        for y in 0..side {
            for x in 0..side {
                let rgb = &out[3 * (y * side + x) as usize..][..3];
                let distance = (40_i32 - x as i32).max(x as i32 - 79).max(40 - y as i32).max(y as i32 - 79);

                if inside(x, y) {
                    assert_eq!(rgb, [1.0, 0.0, 0.0], "the square changed at ({x}, {y})");
                } else if distance > 1 {
                    assert!(rgb[0] <= 0.02 * (rgb[0] + rgb[1] + rgb[2]), "({x}, {y}) took the square's red: {rgb:?}");
                }

                // The blurred background next to the square has the square's coverage missing from it, which the
                // normalisation has to put back: without it, green here would fall off with the square's share of the
                // disc instead of staying at the background's own.
                if (2..=20).contains(&distance) {
                    assert!((rgb[1] - 1.0).abs() < 1e-3, "({x}, {y}) at {distance} px lost its green: {rgb:?}");
                }
            }
        }
    }

    #[test]
    fn a_keep_sharp_weight_holds_its_region_in_focus() {
        let (width, height) = (200_u32, 90_u32);
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([((x * 7 + y * 3) % 256) as u8, ((x * 2 + y * 11) % 251) as u8, ((x + y) % 241) as u8])
        }));
        let (linear, depth) = decoded_linear(&source);

        // At a quarter of the photograph's size, which is what "any size" has to survive: a weight of 1 over the
        // left half.
        let weight = Plane::from_vec(50, 23, (0..50 * 23).map(|i| if i % 50 < 25 { 1.0 } else { 0.0 }).collect());
        let out = defocus(
            &linear,
            width,
            height,
            &Plane::new(width, height, 15.0),
            Some(&weight.unwrap()),
            None,
            Defocus::default(),
            &|| false,
        )
        .unwrap();

        let (before, after) = (source.to_rgb8(), encoded(&out, width, height, depth).to_rgb8());

        for y in 0..height {
            for x in 0..width {
                if x < 100 - 15 {
                    assert_eq!(after.get_pixel(x, y), before.get_pixel(x, y), "({x}, {y}) was blurred");
                }
            }
        }

        let changed = (115..width).filter(|x| after.get_pixel(*x, 45) != before.get_pixel(*x, 45)).count();
        assert!(changed > 80, "only {changed} of the right half's pixels were blurred");
    }

    #[test]
    fn one_worker_and_many_give_the_same_render() {
        // Tall enough that `for_each_row` splits it across cores on any machine with two of them, with radii on both
        // sides of focus, past the direct path's limit, and through a keep-sharp weight.
        let (width, height) = (90_u32, 400_u32);
        let linear = noise(3 * 90 * 400, 21);
        let radius: Vec<f32> = noise(90 * 400, 22)
            .iter()
            .enumerate()
            .map(|(index, jitter)| (index / 90) as f32 / 400.0 * 45.0 - 30.0 + 4.0 * jitter)
            .collect();
        let radius = Plane::from_vec(width, height, radius).unwrap();
        let weight = Plane::from_vec(3, 4, noise(12, 23)).unwrap();

        let scaled = keep_sharp_scaled(&radius, &weight);
        let image = Image { linear: &linear, radius: scaled.as_slice(), width: 90, height: 400 };
        let one = render(&image, None, Defocus::default(), &|| false, Workers::One).unwrap();
        let many = render(&image, None, Defocus::default(), &|| false, Workers::Many).unwrap();

        assert!(one.iter().zip(&many).all(|(a, b)| a.to_bits() == b.to_bits()), "the split changed the render");

        let public =
            defocus(&linear, width, height, &radius, Some(&weight), None, Defocus::default(), &|| false).unwrap();
        assert_eq!(public, many);

        // And with a subject between two layers and past the direct path, its alpha at a size of its own, under
        // geometric spacing.
        let colour = noise(3 * 90 * 400, 24);
        let alpha = Plane::from_vec(9, 20, noise(180, 25)).unwrap();
        let subject = Subject { colour: &colour, alpha: &alpha, radius: -17.0 };
        let settings = Defocus { spacing: Spacing::Geometric { first: 1.0, ratio: 1.4 }, ..Defocus::DEFAULT };

        let matted = Matted::new(subject, width, height);
        let one = render(&image, Some(&matted), settings, &|| false, Workers::One).unwrap();
        let many = render(&image, Some(&matted), settings, &|| false, Workers::Many).unwrap();

        assert!(one.iter().zip(&many).all(|(a, b)| a.to_bits() == b.to_bits()), "the split changed the subject");

        let public = defocus(&linear, width, height, &radius, Some(&weight), Some(subject), settings, &|| false);
        assert_eq!(public.unwrap(), many);
    }

    #[test]
    fn radii_beyond_any_photograph_are_bounded_by_it() {
        let (width, height) = (24_u32, 18_u32);
        let linear = noise(3 * 24 * 18, 41);

        // Past what `ceil(radius / max_direct_radius)` or the layers' count can take, and not numbers at all.
        for huge in [1e30_f32, f32::MAX, f32::INFINITY, f32::NAN] {
            let radius = Plane::from_vec(
                width,
                height,
                (0..24 * 18)
                    .map(|i| {
                        if i % 3 == 0 {
                            huge
                        } else if i % 3 == 1 {
                            -huge
                        } else {
                            0.0
                        }
                    })
                    .collect(),
            )
            .unwrap();

            let out = rendered(&linear, width, height, &radius);

            assert!(
                out.iter().all(|value| value.is_finite() && (-1e-6..=1.0 + 1e-6).contains(value)),
                "a radius of {huge} gave a value outside the photograph's range"
            );
        }
    }

    #[test]
    #[should_panic(expected = "max_direct_radius")]
    fn a_direct_radius_under_one_is_refused() {
        let settings = Defocus { max_direct_radius: 0.0, ..Defocus::DEFAULT };

        let _ = defocus(&[0.5; 3 * 4], 2, 2, &Plane::new(2, 2, 5.0), None, None, settings, &|| false);
    }

    #[test]
    #[should_panic(expected = "max_direct_radius")]
    fn a_direct_radius_that_is_not_a_number_is_refused() {
        let settings = Defocus { max_direct_radius: f32::NAN, ..Defocus::DEFAULT };

        let _ = defocus(&[0.5; 3 * 4], 2, 2, &Plane::new(2, 2, 5.0), None, None, settings, &|| false);
    }

    #[test]
    fn a_keep_sharp_weight_with_no_area_is_no_weight() {
        let (width, height) = (30_u32, 20_u32);
        let linear = noise(3 * 30 * 20, 51);
        let radius = Plane::new(width, height, 6.0);

        let want = rendered(&linear, width, height, &radius);

        for (w, h) in [(0, 0), (0, 5), (5, 0)] {
            let weight = Plane::new(w, h, 1.0);
            let out = defocus(&linear, width, height, &radius, Some(&weight), None, Defocus::default(), &|| false);

            assert_eq!(out.as_deref(), Some(want.as_slice()), "a {w}x{h} weight changed the render");
        }
    }

    #[test]
    fn a_sample_that_is_not_finite_is_read_as_zero_and_poisons_nothing() {
        let (width, height) = (40_u32, 12_u32);
        let radius = Plane::new(width, height, 4.0);

        let mut dark = noise(3 * 40 * 12, 61);
        let mut poisoned = dark.clone();

        for (index, bad) in [
            (3 * (5 * 40 + 7), f32::NAN),
            (3 * (5 * 40 + 20) + 1, f32::INFINITY),
            (3 * (9 * 40), f32::NEG_INFINITY),
        ] {
            poisoned[index] = bad;
            dark[index] = 0.0;
        }

        let out = rendered(&poisoned, width, height, &radius);

        assert!(out.iter().all(|value| value.is_finite()), "a sample that was not finite reached the render");
        assert_eq!(out, rendered(&dark, width, height, &radius), "it was not read as a zero");
    }

    #[test]
    fn a_cancelled_render_stops_between_layers_and_returns_nothing() {
        let (width, height) = (50_u32, 40_u32);
        let linear = noise(3 * 50 * 40, 31);
        let radius = Plane::from_vec(width, height, (0..50 * 40).map(|i| (i % 50) as f32 - 30.0).collect()).unwrap();

        let asked = std::cell::Cell::new(0);
        let stop_after_three = || {
            asked.set(asked.get() + 1);
            asked.get() > 3
        };

        assert!(defocus(&linear, width, height, &radius, None, None, Defocus::default(), &stop_after_three).is_none());
        assert_eq!(asked.get(), 4, "the render did not stop at the first layer after it was cancelled");
    }

    #[test]
    fn the_layers_span_the_radii_with_one_at_zero_and_weights_that_sum_to_one() {
        let layers = Layers::spanning(-60.0, 20.0, 14);

        assert_eq!(layers.centres.first(), Some(&-60.0));
        assert_eq!(layers.centres.last(), Some(&20.0));
        assert!(layers.centres.contains(&0.0), "no layer at the plane of focus: {:?}", layers.centres);
        assert!(layers.centres.len() <= 15, "{} layers for 14", layers.centres.len());
        assert!(layers.centres.windows(2).all(|pair| pair[1] > pair[0]), "not ascending: {:?}", layers.centres);

        for step in -650..=250 {
            let radius = step as f32 / 10.0;
            let sum: f32 = (0..layers.centres.len()).map(|index| layers.weight(index, radius)).sum();
            assert!((sum - 1.0).abs() < 2.0 * NEGLIGIBLE, "the weights at {radius} sum to {sum}");
        }

        // The dead zone: within half a pixel of focus, the whole weight is on the layer at 0.
        let zero = layers.centres.iter().position(|centre| *centre == 0.0).unwrap();
        for radius in [-0.49, -0.2, 0.0, 0.3, 0.5] {
            assert_eq!(layers.weight(zero, radius), 1.0, "a pixel at {radius} px is not wholly in focus");
        }

        // One sign only, and none at all: still a layer at 0, and the ends still exact.
        assert_eq!(Layers::spanning(0.0, 0.0, 14).centres, [0.0]);
        let front = Layers::spanning(10.0, 10.0, 14).centres;
        assert_eq!((front[0], *front.last().unwrap()), (0.0, 10.0));
    }

    /// A hash of every bit of a render, so a render that changes anywhere changes it.
    fn fingerprint(values: &[f32]) -> u64 {
        values.iter().fold(0xcbf2_9ce4_8422_2325, |hash, value| {
            (hash ^ u64::from(value.to_bits())).wrapping_mul(0x0100_0000_01b3)
        })
    }

    #[test]
    fn with_no_subject_and_the_default_settings_the_render_is_exactly_what_it_was() {
        // Radii from -40 to +23 px with jitter, so the render goes through the reduced, direct and sharp paths, with
        // and without a keep-sharp weight. The fingerprints are of the render before the subject and the spacing were
        // added (`add-lens-blur-4-gui`), in debug and release alike.
        let (width, height) = (173_u32, 131_u32);
        let linear = noise(3 * 173 * 131, 71);
        let radius: Vec<f32> = noise(173 * 131, 72)
            .iter()
            .enumerate()
            .map(|(index, jitter)| (index % 173) as f32 / 173.0 * 60.0 - 40.0 + 3.0 * jitter)
            .collect();
        let radius = Plane::from_vec(width, height, radius).unwrap();

        assert_eq!(fingerprint(&rendered(&linear, width, height, &radius)), 0xe6cf_4b33_3c07_38fc);

        let weight = Plane::from_vec(5, 4, noise(20, 73)).unwrap();
        let kept = defocus(&linear, width, height, &radius, Some(&weight), None, Defocus::default(), &|| false);
        assert_eq!(fingerprint(&kept.unwrap()), 0x9ef5_1499_f958_d977);
    }

    #[test]
    fn the_default_spacing_is_the_even_one_it_always_was() {
        assert_eq!(Defocus::DEFAULT.spacing, Spacing::Even);

        let layers = Layers::new(-60.0, 20.0, Defocus::DEFAULT);
        let want = [-60.0, -54.0, -48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, 0.0, 5.0, 10.0, 15.0, 20.0];
        assert_eq!(layers.centres, want);

        for (lo, hi) in [(-401.0, 154.0), (-30.3, 5.8), (0.0, 0.0), (-0.2, 7.0), (3.0, 90.0)] {
            assert_eq!(Layers::new(lo, hi, Defocus::DEFAULT).centres, Layers::spanning(lo, hi, 14).centres);
        }
    }

    #[test]
    fn geometric_layers_keep_every_radius_near_focus_within_a_pixel_of_a_layer() {
        let layers = Layers::geometric(-100.0, 40.0, 1.0, 1.4);
        let centres = &layers.centres;

        assert_eq!((centres[0], *centres.last().unwrap()), (-100.0, 40.0), "the ends are not the extreme radii");
        assert!(centres.contains(&0.0) && centres.contains(&1.0) && centres.contains(&-1.0), "{centres:?}");
        assert!(centres.windows(2).all(|pair| pair[1] > pair[0]), "not ascending: {centres:?}");

        // Each side grows by the ratio, or a pixel where that is less (the first steps), and the last step by no more
        // than the ratio times its square root.
        for pair in centres.windows(2).filter(|pair| pair[0] != 0.0 && pair[1] != 0.0) {
            let (near, far) = (pair[0].abs().min(pair[1].abs()), pair[0].abs().max(pair[1].abs()));
            let step = far / near;

            assert!(far - near > MIN_SPACING - 1e-4, "{pair:?} in {centres:?}");
            assert!(step > 1.4_f32.sqrt() - 1e-4 && step <= (1.4 * 1.4_f32.sqrt()).max((near + 1.0) / near) + 1e-4);
        }

        // The spec's scenario: a pixel at 2 px is rendered from layers no more than 1 px from its radius.
        for radius in [2.0_f32, -2.0, 1.5, 2.5] {
            for (index, centre) in centres.iter().enumerate() {
                if layers.weight(index, radius) > 0.0 {
                    assert!((centre - radius).abs() <= 1.0, "a pixel at {radius} px is rendered from {centre} px");
                }
            }
        }

        // Against even spacing over the same range, whose nearest layers to 2 px are 0 and 10.
        let even = Layers::spanning(-100.0, 40.0, 14);
        assert!(even.centres.iter().all(|centre| *centre == 0.0 || centre.abs() >= 10.0), "{:?}", even.centres);
    }

    #[test]
    fn geometric_layers_on_one_side_of_focus_start_or_end_at_zero() {
        let front = Layers::geometric(5.0, 30.0, 1.0, 1.4).centres;
        assert_eq!((front[0], *front.last().unwrap()), (0.0, 30.0));
        assert!(front.iter().all(|centre| *centre >= 0.0), "{front:?}");

        let behind = Layers::geometric(-30.0, -5.0, 1.0, 1.4).centres;
        assert_eq!((behind[0], *behind.last().unwrap()), (-30.0, 0.0));

        // Under half a pixel there is nothing to blur, and the one layer is at 0.
        assert_eq!(Layers::geometric(-0.3, 0.4, 1.0, 1.4).centres, [0.0]);
        assert_eq!(Layers::geometric(0.0, 0.7, 1.0, 1.4).centres, [0.0, 0.7]);
    }

    #[test]
    fn geometric_layers_are_a_pixel_apart_however_fine_the_ratio_and_never_absurdly_many() {
        // A ratio of 1.05 is 0.05 px a step at 1 px: the floor makes it a pixel.
        let layers = Layers::geometric(-100.0, 40.0, 0.5, MIN_RATIO);
        let centres = &layers.centres;

        assert_eq!((centres[0], *centres.last().unwrap()), (-100.0, 40.0));
        assert!(centres.windows(2).all(|pair| pair[1] > pair[0]), "not ascending: {centres:?}");
        for pair in centres.windows(2).filter(|pair| pair[0].abs() >= 1.0 && pair[1].abs() >= 1.0) {
            assert!((pair[1] - pair[0]).abs() >= MIN_SPACING - 1e-4, "{pair:?} in {centres:?}");
        }

        // The most layers there can be: a 24 megapixel photograph's diagonal on both sides, at the finest ratio and
        // from the nearest first radius.
        let count = Layers::geometric(-7211.0, 7211.0, 0.5, MIN_RATIO).centres.len();
        assert!((270..=300).contains(&count), "{count} layers for the finest geometric spacing over a diagonal");
    }

    #[test]
    #[should_panic(expected = "geometric spacing")]
    fn a_geometric_ratio_of_one_is_refused() {
        let settings = Defocus { spacing: Spacing::Geometric { first: 1.0, ratio: 1.0 }, ..Defocus::DEFAULT };

        let _ = defocus(&[0.5; 3 * 4], 2, 2, &Plane::new(2, 2, 5.0), None, None, settings, &|| false);
    }

    #[test]
    #[should_panic(expected = "geometric spacing")]
    fn a_geometric_ratio_under_the_floor_is_refused() {
        let _ = Layers::geometric(-10.0, 10.0, 1.0, 1.04);
    }

    #[test]
    #[should_panic(expected = "geometric spacing")]
    fn a_geometric_first_radius_inside_the_in_focus_zone_is_refused() {
        let _ = Layers::geometric(-10.0, 10.0, 0.4, 1.4);
    }

    /// Renders `linear` at `radius` with a subject, nothing cancelling it.
    fn with_subject(
        linear: &[f32],
        width: u32,
        height: u32,
        radius: &Plane,
        subject: Subject<'_>,
        settings: Defocus,
    ) -> Vec<f32> {
        defocus(linear, width, height, radius, None, Some(subject), settings, &|| false).unwrap()
    }

    #[test]
    fn a_half_covered_edge_is_half_the_subject_and_half_the_blurred_background() {
        // The spec's scenario: a sharp red subject whose alpha is 1, then 0.5 along its edge, then 0, over a
        // background blurred by 20 px.
        let (width, height) = (140_u32, 50_u32);
        let background = noise(3 * 140 * 50, 81);
        let radius = Plane::new(width, height, -20.0);
        let blurred = rendered(&background, width, height, &radius);

        let coverage = |x: u32| match x {
            0..60 => 1.0,
            60..64 => 0.5,
            _ => 0.0,
        };
        let alpha = Plane::from_vec(width, height, (0..140 * 50).map(|index| coverage(index % 140)).collect()).unwrap();
        let colour: Vec<f32> = (0..140 * 50).flat_map(|_| [1.0, 0.0, 0.0]).collect();

        let subject = Subject { colour: &colour, alpha: &alpha, radius: 0.0 };
        let out = with_subject(&background, width, height, &radius, subject, Defocus::default());

        for (index, (rgb, behind)) in out.as_chunks::<3>().0.iter().zip(blurred.as_chunks::<3>().0).enumerate() {
            let a = coverage(index as u32 % 140);

            if a == 0.0 {
                assert_eq!(rgb, behind, "the subject reached {index}, which it does not cover");
            } else {
                for channel in 0..3 {
                    let want = a * [1.0, 0.0, 0.0][channel] + (1.0 - a) * behind[channel];
                    assert!(
                        (rgb[channel] - want).abs() < 1e-4,
                        "{index}'s channel {channel}: {} for {want}",
                        rgb[channel]
                    );
                }
            }
        }
    }

    #[test]
    fn a_nearer_layer_covers_the_subject() {
        // A red subject over everything, sharp, and a blue square in front of it, blurred by 3 px.
        let (width, height) = (80_u32, 60_u32);
        let near = |x: u32, y: u32| (20..60).contains(&x) && (20..40).contains(&y);

        let linear: Vec<f32> = (0..80 * 60)
            .flat_map(|index| if near(index % 80, index / 80) { [0.0, 0.0, 1.0] } else { [0.0; 3] })
            .collect();
        let radius = Plane::from_vec(
            width,
            height,
            (0..80 * 60).map(|i| if near(i % 80, i / 80) { 3.0 } else { -10.0 }).collect(),
        );
        let colour: Vec<f32> = (0..80 * 60).flat_map(|_| [1.0, 0.0, 0.0]).collect();
        let alpha = Plane::new(width, height, 1.0);

        let subject = Subject { colour: &colour, alpha: &alpha, radius: 0.0 };
        let out = with_subject(&linear, width, height, &radius.unwrap(), subject, Defocus::default());

        for y in 0..height {
            for x in 0..width {
                let rgb = &out[3 * (y * width + x) as usize..][..3];
                let inside = (24..56).contains(&x) && (24..36).contains(&y);
                let outside = !(16..64).contains(&x) || !(16..44).contains(&y);

                if inside {
                    assert!(
                        (rgb[2] - 1.0).abs() < 1e-5 && rgb[0].abs() < 1e-5,
                        "({x}, {y}) shows the subject: {rgb:?}"
                    );
                } else if outside {
                    assert!((rgb[0] - 1.0).abs() < 1e-5 && rgb[2].abs() < 1e-5, "({x}, {y}) lost the subject: {rgb:?}");
                }
            }
        }
    }

    #[test]
    fn a_subject_between_two_layers_is_over_the_farther_and_under_the_nearer() {
        // Layers at -4, -2, -1 and 0 (geometric from 1 by 2), and a red subject at -3 over four bands: green at -4 and
        // yellow at -3.5 (behind it, and the latter between the layers at -4 and -2, which is the layer in front of
        // the subject), white at -3 (as far back as it) and blue at -2 (in front of it).
        let (width, height) = (120_u32, 30_u32);
        let band = |x: u32| (x / 30) as usize;

        let linear: Vec<f32> = (0..120 * 30)
            .flat_map(|index| [[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 0.0, 1.0]][band(index % 120)])
            .collect();
        let radius = (0..120 * 30).map(|i| [-4.0, -3.5, -3.0, -2.0][band(i % 120)]).collect();
        let radius = Plane::from_vec(width, height, radius).unwrap();
        let colour: Vec<f32> = (0..120 * 30).flat_map(|_| [1.0, 0.0, 0.0]).collect();
        let alpha = Plane::new(width, height, 1.0);

        let settings = Defocus { spacing: Spacing::Geometric { first: 1.0, ratio: 2.0 }, ..Defocus::DEFAULT };
        assert_eq!(Layers::geometric(-4.0, 0.0, 1.0, 2.0).centres, [-4.0, -2.0, -1.0, 0.0]);

        let subject = Subject { colour: &colour, alpha: &alpha, radius: -3.0 };
        let out = with_subject(&linear, width, height, &radius, subject, settings);

        let (red, blue) = ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        for (x, want) in [(15_u32, red), (45, red), (75, red), (105, blue)] {
            for y in 0..height {
                let rgb = &out[3 * (y * width + x) as usize..][..3];
                assert!(
                    rgb.iter().zip(want).all(|(got, want)| (got - want).abs() < 1e-5),
                    "({x}, {y}) is {rgb:?}, not {want:?}"
                );
            }
        }
    }

    #[test]
    fn a_layer_at_the_subjects_radius_is_added_once_and_only_where_a_tent_would_cross_it() {
        let layers = Layers::geometric(-100.0, 40.0, 1.0, 1.4);

        let mut inserted = Layers { centres: layers.centres.clone() };
        inserted.insert(-17.3);
        assert_eq!(inserted.centres.len(), layers.centres.len() + 1);
        assert_eq!(inserted.centres.iter().filter(|centre| **centre == -17.3).count(), 1);
        let centres = &inserted.centres;
        assert!(centres.windows(2).all(|pair| pair[1] > pair[0]), "not ascending: {centres:?}");

        // No pixel behind the subject has any weight on a layer in front of it, and none in front of it on a layer
        // behind the one at it.
        let at = inserted.centres.iter().position(|centre| *centre == -17.3).unwrap();
        for step in -1000..=-173 {
            let radius = step as f32 / 10.0;
            for index in at + 1..inserted.centres.len() {
                assert_eq!(inserted.weight(index, radius), 0.0, "a pixel at {radius} px is on layer {index}");
            }
        }
        for step in -172..=400 {
            let radius = step as f32 / 10.0;
            for index in 0..at {
                assert_eq!(inserted.weight(index, radius), 0.0, "a pixel at {radius} px is on layer {index}");
            }
        }

        // Already a layer, or outside them, or at the dead zone's 0: nothing to add.
        for radius in [layers.centres[3], 0.0, -100.0, 40.0, -250.0, 90.0] {
            let mut same = Layers { centres: layers.centres.clone() };
            same.insert(radius);
            assert_eq!(same.centres, layers.centres, "a layer was added at {radius}");
        }
    }

    #[test]
    fn a_subject_between_geometric_layers_is_never_shown_through_by_what_is_behind_it() {
        // A background at every radius from -60 to -18 px, so most pixels fall between two layers, and a red subject
        // over all of it at -17.3 px, which is in no layer's place but the one the subject adds.
        let (width, height) = (200_u32, 40_u32);
        let linear = noise(3 * 200 * 40, 91);
        let radius =
            Plane::from_vec(width, height, (0..200 * 40).map(|i| -60.0 + 42.0 * (i % 200) as f32 / 199.0).collect());
        let colour: Vec<f32> = (0..200 * 40).flat_map(|_| [1.0, 0.0, 0.0]).collect();
        let alpha = Plane::new(width, height, 1.0);
        let settings = Defocus { spacing: Spacing::Geometric { first: 1.0, ratio: 1.4 }, ..Defocus::DEFAULT };

        let subject = Subject { colour: &colour, alpha: &alpha, radius: -17.3 };
        let out = with_subject(&linear, width, height, &radius.unwrap(), subject, settings);

        for (index, rgb) in out.as_chunks::<3>().0.iter().enumerate() {
            assert!(
                (rgb[0] - 1.0).abs() < 1e-5 && rgb[1].abs() < 1e-5 && rgb[2].abs() < 1e-5,
                "({}, {}) shows what is behind the subject: {rgb:?}",
                index % 200,
                index / 200
            );
        }
    }

    #[test]
    fn a_blurred_subject_keeps_its_light() {
        // A small square of light in front of a black scene in focus, blurred through the direct path and the reduced
        // one.
        let side = 141_u32;
        let linear = vec![0.0_f32; 3 * 141 * 141];
        let radius = Plane::new(side, side, 0.0);
        let colour: Vec<f32> = (0..141 * 141).flat_map(|_| [1.0, 0.5, 0.25]).collect();
        let alpha = Plane::from_vec(
            side,
            side,
            (0..141 * 141)
                .map(|i| f32::from((66..75).contains(&(i % 141)) && (66..75).contains(&(i / 141))))
                .collect(),
        )
        .unwrap();

        for blur in [10.0_f32, 30.0] {
            let subject = Subject { colour: &colour, alpha: &alpha, radius: blur };
            let out = with_subject(&linear, side, side, &radius, subject, Defocus::default());

            let total: f32 = out.as_chunks::<3>().0.iter().map(|rgb| rgb[0]).sum();
            assert!((total / 81.0 - 1.0).abs() < 0.01, "{total} of 81 came out at {blur} px");
            assert!(out[3 * (70 * 141 + 70)] < 0.5, "the subject was not spread at {blur} px");

            let rgb = &out[3 * (70 * 141 + 70)..][..3];
            assert!(
                (rgb[1] - rgb[0] / 2.0).abs() < 1e-5 && (rgb[2] - rgb[0] / 4.0).abs() < 1e-5,
                "the colour changed"
            );
        }

        // And behind the scene, which is all in focus and so in front of it, nothing of it shows.
        let subject = Subject { colour: &colour, alpha: &alpha, radius: -10.0 };
        assert_eq!(with_subject(&linear, side, side, &radius, subject, Defocus::default()), linear);
    }

    #[test]
    #[ignore = "a timing, run with `cargo test -p imaging --release -- --ignored defocus_timing --nocapture`"]
    fn defocus_timing() {
        for (width, height) in [(6000_u32, 4000_u32), (1600, 1200)] {
            let (columns, rows) = (width as usize, height as usize);
            let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
                Rgb([((x * 7 + y * 3) % 256) as u8, ((x * 2 + y * 11) % 251) as u8, ((x + y) % 241) as u8])
            }));

            // Radii from -60 to +20 px along a diagonal, so every layer's box is wide and tall: a pessimistic case,
            // since the cost of a layer is its box.
            let radius: Vec<f32> = (0..columns * rows)
                .map(|index| {
                    let (x, y) = ((index % columns) as f32 / width as f32, (index / columns) as f32 / height as f32);
                    -60.0 + 80.0 * (0.6 * x + 0.4 * y)
                })
                .collect();
            let radius = Plane::from_vec(width, height, radius).unwrap();

            let start = std::time::Instant::now();
            let (linear, depth) = decoded_linear(&source);
            let decoded = start.elapsed();

            let start = std::time::Instant::now();
            let out = rendered(&linear, width, height, &radius);
            let render = start.elapsed();

            let start = std::time::Instant::now();
            let _ = encoded(&out, width, height, depth);
            let encode = start.elapsed();

            println!(
                "{width}x{height}: decode {:.3} s, render {:.3} s, encode {:.3} s",
                decoded.as_secs_f64(),
                render.as_secs_f64(),
                encode.as_secs_f64()
            );
        }

        // A matted subject at 24 megapixels: a soft-edged ellipse over the middle of the frame, the background filled
        // in behind it (colour and radius), its colour separated by blur fusion, and the render with it under geometric
        // spacing over -400 to +150 px (a 181 mm lens at f/1.4).
        let (width, height) = (6000_u32, 4000_u32);
        let (columns, rows) = (width as usize, height as usize);
        let source = DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([((x * 7 + y * 3) % 256) as u8, ((x * 2 + y * 11) % 251) as u8, ((x + y) % 241) as u8])
        }));
        let (linear, _) = decoded_linear(&source);

        let radius: Vec<f32> = (0..columns * rows)
            .map(|index| {
                let (x, y) = ((index % columns) as f32 / width as f32, (index / columns) as f32 / height as f32);
                -400.0 + 550.0 * (0.6 * x + 0.4 * y)
            })
            .collect();
        let radius = Plane::from_vec(width, height, radius).unwrap();

        let alpha: Vec<f32> = (0..columns * rows)
            .map(|index| {
                let (x, y) = ((index % columns) as f32 - 3000.0, (index / columns) as f32 - 2000.0);
                ((1.0 - (x / 1500.0).hypot(y / 1300.0)) * 1300.0 / 40.0).clamp(0.0, 1.0)
            })
            .collect();
        let alpha = Plane::from_vec(width, height, alpha).unwrap();
        let behind = Plane::from_vec(width, height, alpha.as_slice().iter().map(|a| f32::from(*a <= 0.004)).collect());
        let behind = behind.unwrap();

        let start = std::time::Instant::now();
        let background = crate::fill::filled(&linear, 3, width, height, &behind);
        let radius = Plane::from_vec(width, height, crate::fill::filled(radius.as_slice(), 1, width, height, &behind));
        let radius = radius.unwrap();
        let fill = start.elapsed();

        let start = std::time::Instant::now();
        let colour = crate::fusion::foreground(&linear, &alpha, width, height, &[90, 6]);
        let fusion = start.elapsed();

        let settings = Defocus { spacing: Spacing::Geometric { first: 1.0, ratio: 1.4 }, ..Defocus::DEFAULT };
        let subject = Subject { colour: &colour, alpha: &alpha, radius: 0.0 };

        let start = std::time::Instant::now();
        let _ = defocus(&background, width, height, &radius, None, Some(subject), settings, &|| false);
        let matted = start.elapsed();

        let start = std::time::Instant::now();
        let _ = defocus(&background, width, height, &radius, None, None, settings, &|| false);
        let geometric = start.elapsed();

        let start = std::time::Instant::now();
        let _ = defocus(&background, width, height, &radius, None, None, Defocus::DEFAULT, &|| false);
        let even = start.elapsed();

        println!(
            "{width}x{height} matted: fill {:.3} s, fusion {:.3} s, render with the subject {:.3} s ({} layers); \
             without it {:.3} s, and with {} even layers {:.3} s",
            fill.as_secs_f64(),
            fusion.as_secs_f64(),
            matted.as_secs_f64(),
            Layers::new(-400.0, 150.0, settings).centres.len(),
            geometric.as_secs_f64(),
            Layers::new(-400.0, 150.0, Defocus::DEFAULT).centres.len(),
            even.as_secs_f64()
        );
    }

    #[test]
    fn the_depth_round_trip_is_the_one_the_render_starts_and_ends_with() {
        // The render takes and returns linear light at the photograph's own depth; a sixteen-bit photograph through
        // a render that changed nothing comes back within a code.
        let source = DynamicImage::ImageRgb16(ImageBuffer::from_fn(33, 21, |x, y| {
            Rgb([(x * 1999) as u16, (y * 3001) as u16, ((x + y) * 1201) as u16])
        }));
        let (linear, depth) = decoded_linear(&source);
        let out = rendered(&linear, 33, 21, &Plane::new(33, 21, 0.2));

        assert_eq!(depth, ChannelDepth::Sixteen);
        let back = encoded(&out, 33, 21, depth);
        for (got, want) in back.as_rgb16().unwrap().as_raw().iter().zip(source.as_rgb16().unwrap().as_raw()) {
            assert!(got.abs_diff(*want) <= 1, "{want} came back as {got}");
        }
    }
}
