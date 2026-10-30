//! What was done to a photograph to make a picture's pixels: how its frame moved, and which enhancements ran.
//!
//! A record kept beside the pixels, as the metadata is, for the one reader that needs it: the export, which rewrites
//! the fields that state facts about pixels (where the subject is, how many pixels a millimetre of the sensor is) and
//! records what was rendered. Nothing here touches a pixel, and nothing in a picture's identity depends on it: the
//! identity already folds in the framing and the operations this records.

// Kept on `Picture` rather than handed to the export beside it, so that it travels with the pixels the way the profile
// does: a crop in the GUI, an enhancement in `opai`, a cached result — each derives the next picture, and the record
// of what was done cannot be forgotten by a front end that never heard of it.

use crate::models::Operation;

/// A plane transform, `x' = a·x + b·y + c`, `y' = d·x + e·y + f`, in pixel coordinates (`0,0` the top-left corner of
/// the top-left pixel, `y` down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    /// The transform that moves nothing.
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 0.0, e: 1.0, f: 0.0 };

    /// `x' = a·x + b·y + c`, `y' = d·x + e·y + f`.
    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }

    /// A move by `(x, y)`.
    pub const fn translation(x: f64, y: f64) -> Self {
        Self::new(1.0, 0.0, x, 0.0, 1.0, y)
    }

    /// A scale by `x` across and `y` down, about the origin.
    pub const fn scale(x: f64, y: f64) -> Self {
        Self::new(x, 0.0, 0.0, 0.0, y, 0.0)
    }

    /// A turn by `degrees`, positive clockwise on screen (`y` down), about `(cx, cy)`, which lands on `(to_x, to_y)`:
    /// the turn of [`rust_sak::image::rotate`], whose canvas is the turned bounding box with the centres matched.
    pub fn turn(degrees: f64, (cx, cy): (f64, f64), (to_x, to_y): (f64, f64)) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self::new(cos, -sin, to_x - cos * cx + sin * cy, sin, cos, to_y - sin * cx - cos * cy)
    }

    /// This transform, then `next`.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.b * self.d,
            b: next.a * self.b + next.b * self.e,
            c: next.a * self.c + next.b * self.f + next.c,
            d: next.d * self.a + next.e * self.d,
            e: next.d * self.b + next.e * self.e,
            f: next.d * self.c + next.e * self.f + next.f,
        }
    }

    /// Where `(x, y)` lands.
    pub fn apply(self, (x, y): (f64, f64)) -> (f64, f64) {
        (self.a * x + self.b * y + self.c, self.d * x + self.e * y + self.f)
    }

    /// The linear part, `[[a, b], [d, e]]`: what a length or a size goes through, the move left out.
    pub fn linear(self) -> [[f64; 2]; 2] {
        [[self.a, self.b], [self.d, self.e]]
    }

    /// How many times longer a step along `x` and a step along `y` come out: the scale a pixel pitch goes through.
    pub fn stretch(self) -> (f64, f64) {
        (self.a.hypot(self.d), self.b.hypot(self.e))
    }

    /// Whether this only scales, by positive factors about the origin: no cut, no turn, no flip.
    pub fn is_scale(self) -> bool {
        let near = |value: f64| value.abs() < 1e-6;
        self.a > 0.0 && self.e > 0.0 && near(self.b) && near(self.c) && near(self.d) && near(self.f)
    }

    /// Whether this moves nothing, to within a millionth of a pixel.
    pub fn is_identity(self) -> bool {
        let near = |value: f64, to: f64| (value - to).abs() < 1e-6;
        near(self.a, 1.0)
            && near(self.b, 0.0)
            && near(self.c, 0.0)
            && near(self.d, 0.0)
            && near(self.e, 1.0)
            && near(self.f, 0.0)
    }
}

/// Where the source's pixels went: the size of the source as decoded (turned upright), and how a point of it lands on
/// a picture's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The source's size as decoded, before any framing or enhancement.
    pub source: (u32, u32),
    /// A point of the source to a point of the picture.
    pub to_pixels: Affine,
}

/// What was done to a picture's source to make its pixels. [`Picture::edits`](super::Picture::edits).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Edits {
    /// How the frame moved, or `None` where the pixels are still the source's frame at the source's size.
    frame: Option<Frame>,
    /// The enhancements run, in the order they ran.
    operations: Vec<Operation>,
    /// Whether a mask limited them to part of the photograph.
    masked: bool,
}

impl Edits {
    /// Nothing done: the pixels are the source's, as decoded.
    pub const NONE: Self = Self { frame: None, operations: Vec::new(), masked: false };

    /// How the frame moved, if it did.
    pub fn frame(&self) -> Option<Frame> {
        self.frame
    }

    /// The enhancements run, in order.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Whether a mask limited the enhancements to part of the photograph.
    pub fn masked(&self) -> bool {
        self.masked
    }

    /// Whether anything at all was done: the pixels are the source's own, as decoded, otherwise.
    pub fn is_empty(&self) -> bool {
        self.frame.is_none() && self.operations.is_empty()
    }

    /// These edits, then a move of the frame by `transform`, whose points are measured in pixels of a picture `before`
    /// in size: the size of the source where nothing had moved its frame yet.
    #[must_use]
    pub fn framed(&self, before: (u32, u32), transform: Affine) -> Self {
        let frame = match self.frame {
            Some(Frame { source, to_pixels }) => Frame { source, to_pixels: to_pixels.then(transform) },
            None => Frame { source: before, to_pixels: transform },
        };
        Self { frame: Some(frame), ..self.clone() }
    }

    /// These edits, then `operations`, run over pixels `before` in size into pixels `after` in size — an upscale's
    /// growth is a scale of the frame — and limited by a mask if `masked`.
    #[must_use]
    pub(crate) fn after(&self, operations: &[Operation], masked: bool, before: (u32, u32), after: (u32, u32)) -> Self {
        let mut edits = if before == after {
            self.clone()
        } else {
            let scale = |to: u32, from: u32| f64::from(to) / f64::from(from.max(1));
            self.framed(before, Affine::scale(scale(after.0, before.0), scale(after.1, before.1)))
        };
        edits.operations.extend_from_slice(operations);
        edits.masked |= masked && !operations.is_empty();
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close((x, y): (f64, f64), (to_x, to_y): (f64, f64)) -> bool {
        (x - to_x).abs() < 1e-9 && (y - to_y).abs() < 1e-9
    }

    #[test]
    fn a_quarter_turn_clockwise_takes_the_top_left_corner_to_the_top_right() {
        // A 40x20 picture turned 90° lands on a 20x40 canvas, centre on centre.
        let turn = Affine::turn(90.0, (20.0, 10.0), (10.0, 20.0));
        assert!(close(turn.apply((0.0, 0.0)), (20.0, 0.0)), "{:?}", turn.apply((0.0, 0.0)));
        assert!(close(turn.apply((40.0, 20.0)), (0.0, 40.0)), "{:?}", turn.apply((40.0, 20.0)));
    }

    #[test]
    fn transforms_compose_in_the_order_they_are_applied() {
        let moved = Affine::scale(2.0, 3.0).then(Affine::translation(-5.0, 1.0));
        assert!(close(moved.apply((10.0, 10.0)), (15.0, 31.0)));
        assert_eq!(moved.stretch(), (2.0, 3.0));
        assert!(Affine::translation(1e-9, 0.0).is_identity());
        assert!(!moved.is_identity());
    }

    #[test]
    fn nothing_done_is_empty_and_a_framing_or_an_operation_is_not() {
        let edits = Edits::default();
        assert!(edits.is_empty());

        let cut = edits.framed((100, 50), Affine::translation(-10.0, -5.0));
        assert!(!cut.is_empty());
        assert_eq!(cut.frame().map(|frame| frame.source), Some((100, 50)));
    }

    #[test]
    fn a_chain_that_grows_the_picture_scales_the_frame_after_any_cut_before_it() {
        let cut = Edits::default().framed((100, 50), Affine::translation(-10.0, -5.0));
        let upscaled = cut.after(&[], false, (40, 20), (80, 40));
        let frame = upscaled.frame().expect("a frame");

        // The source it started from is still the one measured, and its point (10, 5) is the cut's corner.
        assert_eq!(frame.source, (100, 50));
        assert!(close(frame.to_pixels.apply((10.0, 5.0)), (0.0, 0.0)));
        assert!(close(frame.to_pixels.apply((30.0, 15.0)), (40.0, 20.0)));
        // A mask over no operation limits nothing.
        assert!(!upscaled.masked());
    }
}
