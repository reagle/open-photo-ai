//! The lens a photograph was taken with, as far as its EXIF says: what the Lens Blur panel bounds its f-number slider
//! by.
//!
//! The command holds no logic of its own. The resolution is `opai`'s ([`opai::own_lens`]), so the window and the
//! command line agree on a photograph's own aperture. It reads the file's EXIF and nothing else — never the depth
//! model — so it answers at once, and it is asked for an open photograph by identity, as the `opai://` scheme is.
//!
//! **As the photograph is framed.** A crop is a smaller sensor behind the same lens, so its equivalent f-number is the
//! frame's times how many times the crop's diagonal goes into the file's. The window sends the framing it draws, and
//! `opai` scales by it exactly as a run does, so the slider ends where the run's own aperture is.

use opai::{LensOrigin, OwnLens};
use serde::Serialize;
use tauri::State;

use crate::command::{Answer, CommandError, Ended, Traceparent, command_span, traced};
use crate::images::{Crop, Opened};
use crate::task::spawn_blocking;

/// A photograph's own lens, as the window is told it.
///
/// `own_f_number` is the 35 mm full-frame-equivalent f-number the photograph already has, which is where the slider
/// ends: an f-number at or above it adds no blur. `focal_35mm` is `None` where only the depth model could say, and
/// the window does not show it then. `crop_factor` is the picture's as framed, which the window divides an equivalent
/// f-number by to read the real aperture beside it; `None` where the EXIF cannot measure one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LensInfo {
    /// The photograph's own equivalent f-number, or the f/8 assumed where its EXIF cannot say.
    pub(crate) own_f_number: f32,
    /// The equivalent focal length in millimetres, where the EXIF gives it.
    pub(crate) focal_35mm: Option<f32>,
    /// The crop factor of the photograph as it is framed, where the EXIF gives one to measure.
    pub(crate) crop_factor: Option<f32>,
    /// Whether the numbers were read (`exif`) or assumed (`assumed`). `estimated` needs the depth model and is never
    /// answered here.
    pub(crate) origin: LensOrigin,
}

impl From<OwnLens> for LensInfo {
    fn from(own: OwnLens) -> Self {
        Self {
            own_f_number: own.own_f_number,
            focal_35mm: own.focal_35mm,
            crop_factor: own.crop_factor,
            origin: own.origin,
        }
    }
}

impl Answer for LensInfo {}

/// Why the photograph's lens could not be asked for.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum LensError {
    /// The source names an identity this application never admitted — a bug in the interface.
    #[error("no image with identity `{identity}` has been opened")]
    UnknownSource {
        /// What was asked for.
        identity: String,
    },

    /// The blocking thread that reads the file did not come back: the runtime is going down.
    #[error("the photograph's lens could not be read: {message}")]
    Read {
        /// The runtime's own sentence, untranslated.
        message: String,
    },
}

impl CommandError for LensError {
    fn ended(&self) -> Ended<'_> {
        // Neither is recorded by `opai`, which was never asked anything: the wrapper records both.
        Ended::Failed { error: self, recorded: false }
    }
}

/// The lens of an open photograph, from its EXIF, as the user has framed it.
///
/// `crop` is how the user has framed the photograph, or absent for the whole of it; only its size matters here.
///
/// # Errors
///
/// [`LensError`]: an identity the application never opened, and a runtime that is shutting down. A photograph with no
/// EXIF is not one — it answers the assumed f/8, which is what a run assumes.
#[tauri::command]
pub(crate) async fn lens_info(
    source: String,
    crop: Option<Crop>,
    opened: State<'_, Opened>,
    traceparent: Traceparent,
) -> Result<LensInfo, LensError> {
    traced(command_span!("lens_info", traceparent), async move { lens_of(&opened, source, crop).await }).await
}

/// What [`lens_info`] does once the managed state has been taken off the window's request.
async fn lens_of(opened: &Opened, source: String, crop: Option<Crop>) -> Result<LensInfo, LensError> {
    let path = opened.resolve(&source).ok_or(LensError::UnknownSource { identity: source })?;
    let framed = crop.map(|crop| (crop.width(), crop.height()));

    let own = spawn_blocking(move || opai::own_lens(&path, framed))
        .await
        .map_err(|error| LensError::Read { message: error.to_string() })?;

    tracing::debug!(?own, "the lens of a photograph was read");

    Ok(own.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::admitted;

    /// The file `bytes` is, admitted, and the identity it was admitted under. Not decoded by anything here.
    fn photograph(dir: &tempfile::TempDir, opened: &Opened, bytes: &[u8]) -> String {
        let path = dir.path().join("lens.jpg");
        std::fs::write(&path, bytes).expect("writable");
        tauri::async_runtime::block_on(crate::images::files::describe_and_admit(vec![path.clone()], opened))
            .expect("the runtime is alive");

        opai::image::identity_blocking(&path).expect("a written file has an identity")
    }

    /// The smallest JPEG a reader finds a camera's lens in: the start marker, an EXIF segment holding a little-endian
    /// TIFF, the end marker. The TIFF's first directory holds an orientation and the pointer to the EXIF directory,
    /// which holds the f-number, the focal length and its 35 mm equivalent, as a camera writes them.
    ///
    /// Written by hand rather than with an EXIF writer, so this crate takes no dependency for a fixture: the reader's
    /// own field-level tests are `opai`'s.
    fn jpeg_with_lens(focal: u32, focal_35mm: u16, f_number: u32) -> Vec<u8> {
        /// A twelve-byte directory entry: tag, type (3 short, 4 long, 5 rational), a count of one, and its value or
        /// offset.
        fn entry(tag: u16, kind: u16, value: u32) -> Vec<u8> {
            [tag.to_le_bytes().as_slice(), &kind.to_le_bytes(), &1_u32.to_le_bytes(), &value.to_le_bytes()].concat()
        }

        // The header; the first directory at byte 8 (a count, two entries, no next directory), ending at 38; the EXIF
        // directory at 38 (a count, three entries, no next), ending at 80; and the two rationals, at 80 and 88.
        let mut tiff = [b"II*\0".as_slice(), &8_u32.to_le_bytes(), &2_u16.to_le_bytes()].concat();
        tiff.extend(entry(0x0112, 3, 1));
        tiff.extend(entry(0x8769, 4, 38));
        tiff.extend(0_u32.to_le_bytes());
        tiff.extend(3_u16.to_le_bytes());
        tiff.extend(entry(0x829D, 5, 80));
        tiff.extend(entry(0x920A, 5, 88));
        tiff.extend(entry(0xA405, 3, u32::from(focal_35mm)));
        tiff.extend(0_u32.to_le_bytes());
        for whole in [f_number, focal] {
            tiff.extend(whole.to_le_bytes());
            tiff.extend(1_u32.to_le_bytes());
        }

        let payload = [b"Exif\0\0".as_slice(), &tiff].concat();
        let length = u16::try_from(payload.len() + 2).expect("a small segment");

        [&[0xFF, 0xD8, 0xFF, 0xE1][..], &length.to_be_bytes(), &payload, &[0xFF, 0xD9]].concat()
    }

    #[test]
    fn a_photograph_with_a_lens_answers_it_in_full_frame_terms() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let opened = Opened::default();
        // The a6700 at 24 mm f/4, 36 mm equivalent: crop 1.5, so f/6.
        let identity = photograph(&dir, &opened, &jpeg_with_lens(24, 36, 4));

        let info = tauri::async_runtime::block_on(lens_of(&opened, identity, None)).expect("an admitted photograph");

        assert_eq!(
            info,
            LensInfo { own_f_number: 6.0, focal_35mm: Some(36.0), crop_factor: Some(1.5), origin: LensOrigin::Exif }
        );
        assert_eq!(
            serde_json::to_value(info).expect("serializes"),
            serde_json::json!({ "ownFNumber": 6.0, "focal35mm": 36.0, "cropFactor": 1.5, "origin": "exif" })
        );
    }

    #[test]
    fn a_photograph_with_no_exif_answers_the_assumed_f8() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let opened = Opened::default();
        let identity = admitted(&dir, &opened);

        let info = tauri::async_runtime::block_on(lens_of(&opened, identity, None)).expect("an admitted photograph");

        assert_eq!(
            info,
            LensInfo { own_f_number: 8.0, focal_35mm: None, crop_factor: None, origin: LensOrigin::Assumed }
        );
        assert_eq!(
            serde_json::to_value(info).expect("serializes"),
            serde_json::json!({ "ownFNumber": 8.0, "focal35mm": null, "cropFactor": null, "origin": "assumed" })
        );
    }

    #[test]
    fn a_cropped_photograph_answers_its_lens_as_framed() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let opened = Opened::default();
        // The fixture is 8x6 pixels with no EXIF: an assumed f/8 of the whole frame.
        let identity = admitted(&dir, &opened);
        let crop = Crop::new(0, 0, 4, 3, 0, false, false);

        let whole = tauri::async_runtime::block_on(lens_of(&opened, identity.clone(), None)).expect("admitted");
        let cut = tauri::async_runtime::block_on(lens_of(&opened, identity, crop)).expect("admitted");

        // Half the diagonal is a sensor half the size: the assumed aperture is twice the f-number across it.
        assert_eq!((whole.own_f_number, cut.own_f_number), (8.0, 16.0));
        assert_eq!(cut.origin, LensOrigin::Assumed);
    }

    #[test]
    fn an_identity_never_admitted_is_refused() {
        let opened = Opened::default();

        let error = tauri::async_runtime::block_on(lens_of(&opened, "0123456789abcdef".to_string(), None))
            .expect_err("nothing was admitted");

        assert_eq!(error, LensError::UnknownSource { identity: "0123456789abcdef".to_string() });
        assert_eq!(
            serde_json::to_value(&error).expect("serializes"),
            serde_json::json!({ "kind": "unknownSource", "identity": "0123456789abcdef" })
        );
    }
}
