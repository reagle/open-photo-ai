//! Drawing a mask for the window: the weight a run will use, as a picture.
//!
//! The brush is the window's, and so are the strokes and their undo; what a set of strokes *weighs* is `opai`'s, and
//! the window never rasterises one for itself, so the tint it shows over the photograph is the weight the run will
//! blend with and not a second reading of the same strokes.

use opai::Mask;
use serde::Serialize;
use tauri::ipc::Response;

use crate::command::{CommandError, Ended, Traceparent, command_span, traced};
use crate::task::spawn_blocking;

/// The longest edge of a picture [`mask_preview`] draws. A larger one is drawn at this size and the window stretches
/// it: the tint is a soft shape over a photograph, and a bigger picture of it costs an encode for nothing visible.
const LONGEST_EDGE: u32 = 2048;

/// Why a mask could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum MaskError {
    /// The window asked for a picture with no pixels in it.
    #[error("a mask was asked for at {width}x{height}, which has no pixels")]
    NoSize {
        /// The width asked for.
        width: u32,
        /// The height asked for.
        height: u32,
    },

    /// The picture could not be made.
    #[error("the mask could not be drawn: {message}")]
    Draw {
        /// The library's own sentence, or the reason the work did not finish, untranslated.
        message: String,
    },
}

impl CommandError for MaskError {
    fn ended(&self) -> Ended<'_> {
        match self {
            // Neither is a failure `opai` saw: the first never reaches it, and the second is a task that did not end.
            Self::NoSize { .. } | Self::Draw { .. } => Ended::Failed { error: self, recorded: false },
        }
    }
}

/// The size `width` x `height` is drawn at: itself, or the same shape with its longer edge at [`LONGEST_EDGE`].
fn capped(width: u32, height: u32) -> (u32, u32) {
    let longer = width.max(height);
    if longer <= LONGEST_EDGE {
        return (width, height);
    }

    let scale = f64::from(LONGEST_EDGE) / f64::from(longer);
    let edge = |length: u32| ((f64::from(length) * scale).round() as u32).max(1);

    (edge(width), edge(height))
}

/// `mask`, rasterised at `width` x `height` (or the cap) and encoded as a PNG.
///
/// **The weight is the picture's grey and its alpha both.** The window draws the tint through a CSS `mask-image`, which
/// reads an image's alpha unless told to read its luminance — and the luminance form is one more thing a webview has to
/// support. With the weight in both, the picture is a grey weight image to anything that opens it and a mask to a
/// browser that takes the default.
fn preview(mask: &Mask, width: u32, height: u32) -> Result<Vec<u8>, MaskError> {
    if width == 0 || height == 0 {
        return Err(MaskError::NoSize { width, height });
    }

    let (width, height) = capped(width, height);
    let weight = mask.rasterised(width, height);

    let grey: Vec<u8> = weight
        .as_slice()
        .iter()
        .flat_map(|value| {
            let level = (value.clamp(0.0, 1.0) * 255.0).round() as u8;

            [level, level]
        })
        .collect();
    let pixels = image::GrayAlphaImage::from_raw(width, height, grey)
        .map(image::DynamicImage::ImageLumaA8)
        .ok_or_else(|| MaskError::Draw { message: "the weight does not fill the picture".to_string() })?;

    opai::image::encode_blocking(&pixels, &opai::image::Metadata::default(), opai::ImageFormat::Png, None)
        .map_err(|error| MaskError::Draw { message: error.to_string() })
}

// The command's name is written once more, in `frontend/ipc/mask.ts`.
/// Draw the weight `mask` gives a `width` x `height` picture, as a PNG, for the window to tint the preview with.
///
/// `mask` is the window's strokes in the wire's own spelling, over the framed photograph, so the picture is the weight
/// a run over the same mask will blend with — at the size asked for, brought down to [`LONGEST_EDGE`] where it is
/// larger. The picture is greyscale with the weight also in its alpha channel; see [`preview`]. Answers **the PNG's
/// bytes**, as a binary response rather than a list of numbers.
///
/// Holds no state and touches no run: a mask painted while a run is working is drawn all the same.
///
/// # Errors
///
/// [`MaskError`], for a picture of no size.
#[tauri::command]
pub(crate) async fn mask_preview(
    mask: Mask,
    width: u32,
    height: u32,
    traceparent: Traceparent,
) -> Result<Response, MaskError> {
    traced(command_span!("mask_preview", traceparent), async move {
        let bytes = spawn_blocking(move || preview(&mask, width, height))
            .await
            .map_err(|error| MaskError::Draw { message: error.to_string() })??;

        Ok(Response::new(bytes))
    })
    .await
}

#[cfg(test)]
mod tests {
    use tauri::ipc::IpcResponse;

    use super::*;

    /// A stroke straight across the middle of the picture, with a hard core.
    fn across() -> Mask {
        Mask { strokes: vec![opai::Stroke::new(false, 0.1, 0.0, vec![0.0, 0.5, 1.0, 0.5])], ..Mask::default() }
    }

    /// The picture `bytes` encode, as grey and alpha.
    fn decoded(bytes: &[u8]) -> image::GrayAlphaImage {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .expect("the answer is a PNG")
            .into_luma_alpha8()
    }

    #[test]
    fn the_picture_is_the_size_asked_for() {
        let picture = decoded(&preview(&across(), 300, 200).expect("a mask draws"));

        assert_eq!(picture.dimensions(), (300, 200));
    }

    #[test]
    fn a_painted_pixel_is_white_and_an_unpainted_one_is_clear_in_grey_and_alpha_alike() {
        let picture = decoded(&preview(&across(), 300, 200).expect("a mask draws"));

        // The stroke runs along y = 100 and is 20 pixels either side of it (a tenth of the longer side), core included.
        assert_eq!(picture.get_pixel(150, 100).0, [255, 255], "the middle of the stroke is not full weight");
        assert_eq!(picture.get_pixel(150, 5).0, [0, 0], "the corner the stroke never reached has weight");
    }

    #[test]
    fn the_soft_edge_is_between() {
        let mask =
            Mask { strokes: vec![opai::Stroke::new(false, 0.1, 1.0, vec![0.0, 0.5, 1.0, 0.5])], ..Mask::default() };
        let picture = decoded(&preview(&mask, 300, 200).expect("a mask draws"));

        let [grey, alpha] = picture.get_pixel(150, 110).0;
        assert!(grey > 0 && grey < 255, "ten pixels off a fully soft stroke's middle is {grey}");
        assert_eq!(grey, alpha, "the weight is not the same in both channels");
    }

    #[test]
    fn an_inverted_mask_is_drawn_turned_over() {
        let mask = Mask { inverted: true, ..across() };
        let picture = decoded(&preview(&mask, 300, 200).expect("a mask draws"));

        assert_eq!(picture.get_pixel(150, 100).0, [0, 0]);
        assert_eq!(picture.get_pixel(150, 5).0, [255, 255]);
    }

    #[test]
    fn a_large_picture_is_drawn_at_the_cap_in_the_same_shape() {
        let picture = decoded(&preview(&across(), 6000, 4000).expect("a mask draws"));

        assert_eq!(picture.dimensions(), (2048, 1365), "the cap changed the picture's shape, or was not applied");
        assert_eq!(capped(100, 4096), (50, 2048));
        assert_eq!(capped(2048, 2048), (2048, 2048), "a picture at the cap was shrunk");
    }

    #[test]
    fn a_picture_of_no_size_is_refused() {
        assert_eq!(preview(&across(), 0, 200), Err(MaskError::NoSize { width: 0, height: 200 }));
        assert_eq!(preview(&across(), 200, 0), Err(MaskError::NoSize { width: 200, height: 0 }));
    }

    #[test]
    fn the_command_answers_the_same_bytes() {
        let mask = across();
        let expected = preview(&mask, 64, 48).expect("a mask draws");

        let answer = tauri::async_runtime::block_on(mask_preview(mask, 64, 48, Traceparent::default()))
            .expect("the command draws it");

        // A binary response; its body is the PNG itself.
        let tauri::ipc::InvokeResponseBody::Raw(bytes) = answer.body().expect("a body") else {
            panic!("the answer was text rather than bytes");
        };
        assert_eq!(bytes, expected);
    }
}
