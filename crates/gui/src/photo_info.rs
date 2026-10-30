//! What the information card over the preview says about a photograph as it was taken: camera, lens, exposure, when,
//! whether where, its size, colour profile and HDR, and what an earlier Open Photo AI export rendered into it.
//!
//! The command holds no logic of its own: the EXIF and the XMP are read by `opai` ([`opai::image::read_capture`]), the
//! size and the profile by its probe. It reads the file and never decodes it, so it answers at once, and it is asked
//! for an open photograph by identity, as [`lens_info`](crate::lens::lens_info) is.
//!
//! **As shot, never as rendered.** What a crop, an upscale or a lens blur in the stack makes of these numbers is the
//! window's to work out from the stack it holds; this answers the file.

use opai::image::{Capture, Hdr};
use serde::Serialize;
use tauri::State;

use crate::command::{Answer, CommandError, Ended, Traceparent, command_span, traced};
use crate::images::Opened;
use crate::task::spawn_blocking;

/// A photograph as it was taken, as the window is told it. Every field is `null` (or `false`, or empty) where the
/// file does not say.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhotoInfo {
    pub(crate) make: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) lens_model: Option<String>,
    /// The focal length in millimetres, and its 35 mm equivalent, as the EXIF records them.
    pub(crate) focal_mm: Option<f32>,
    pub(crate) focal_35mm: Option<f32>,
    /// The f-number the lens was set to.
    pub(crate) f_number: Option<f32>,
    /// The exposure time, in seconds.
    pub(crate) exposure_seconds: Option<f64>,
    pub(crate) iso: Option<u32>,
    /// When it was taken, `YYYY-MM-DD HH:MM:SS`, and the offset then, `+02:00`.
    pub(crate) taken: Option<String>,
    pub(crate) taken_offset: Option<String>,
    /// The file's own size in pixels, from its header.
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    /// The colour profile's own name (`Display P3`), where the file declares one with a name.
    pub(crate) color_profile: Option<String>,
    /// How the file declares HDR: `gainMap`, `pq` or `hlg` (`other` for a kind added later).
    pub(crate) hdr: Option<&'static str>,
    /// Whether the file records where it was taken. Never the place.
    pub(crate) located: bool,
    /// The f-number an earlier Open Photo AI lens blur simulated, and the enhancements that export recorded.
    pub(crate) rendered_f_number: Option<f32>,
    pub(crate) rendered: Vec<String>,
}

impl PhotoInfo {
    fn of(capture: Capture, size: Option<(u32, u32)>, color_profile: Option<String>) -> Self {
        Self {
            make: capture.make,
            model: capture.model,
            lens_model: capture.lens_model,
            focal_mm: capture.focal_mm,
            focal_35mm: capture.focal_35mm,
            f_number: capture.f_number,
            exposure_seconds: capture.exposure_s,
            iso: capture.iso,
            taken: capture.taken,
            taken_offset: capture.taken_offset,
            width: size.map(|(width, _)| width),
            height: size.map(|(_, height)| height),
            color_profile,
            // As the export's loss warnings spell it, so the window words both alike.
            hdr: capture.hdr.map(|hdr| match hdr {
                Hdr::GainMap => "gainMap",
                Hdr::Pq => "pq",
                Hdr::Hlg => "hlg",
                _ => "other",
            }),
            located: capture.located,
            rendered_f_number: capture.rendered_f_number,
            rendered: capture.rendered,
        }
    }
}

impl Answer for PhotoInfo {}

/// Why a photograph's information could not be asked for.
#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum PhotoInfoError {
    /// The source names an identity this application never admitted — a bug in the interface.
    #[error("no image with identity `{identity}` has been opened")]
    UnknownSource {
        /// What was asked for.
        identity: String,
    },

    /// The blocking thread that reads the file did not come back: the runtime is going down.
    #[error("the photograph's information could not be read: {message}")]
    Read {
        /// The runtime's own sentence, untranslated.
        message: String,
    },
}

impl CommandError for PhotoInfoError {
    fn ended(&self) -> Ended<'_> {
        // Neither is recorded by `opai`, which was never asked anything: the wrapper records both.
        Ended::Failed { error: self, recorded: false }
    }
}

/// What the file of an open photograph says about how it was taken.
///
/// # Errors
///
/// [`PhotoInfoError`]: an identity the application never opened, and a runtime that is shutting down. A photograph with
/// no EXIF is not one: it answers what its header says, and nothing else.
#[tauri::command]
pub(crate) async fn photo_info(
    source: String,
    opened: State<'_, Opened>,
    traceparent: Traceparent,
) -> Result<PhotoInfo, PhotoInfoError> {
    traced(command_span!("photo_info", traceparent), async move { info_of(&opened, source).await }).await
}

/// What [`photo_info`] does once the managed state has been taken off the window's request.
async fn info_of(opened: &Opened, source: String) -> Result<PhotoInfo, PhotoInfoError> {
    let path = opened.resolve(&source).ok_or(PhotoInfoError::UnknownSource { identity: source })?;

    let info = spawn_blocking(move || {
        let capture = opai::image::read_capture(&path);
        // A RAW is described by what it develops to, which reads the whole file; its profile is the developer's.
        let (size, profile) = if opai::image::is_raw(&path) {
            (opai::image::probe_raw_blocking(&path).ok().map(|info| (info.width, info.height)), None)
        } else {
            match opai::image::probe_blocking(&path) {
                Ok(info) => (Some((info.width, info.height)), info.color_profile),
                Err(_) => (None, None),
            }
        };
        PhotoInfo::of(capture, size, profile)
    })
    .await
    .map_err(|error| PhotoInfoError::Read { message: error.to_string() })?;

    tracing::debug!(?info, "the information of a photograph was read");

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::admitted;

    #[test]
    fn a_photograph_with_no_exif_answers_its_size_and_nothing_it_does_not_record() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let opened = Opened::default();
        let identity = admitted(&dir, &opened);

        let info = tauri::async_runtime::block_on(info_of(&opened, identity)).expect("an admitted photograph");

        assert_eq!((info.width, info.height), (Some(8), Some(6)));
        assert_eq!(info.make, None);
        assert!(!info.located);
        assert_eq!(
            serde_json::to_value(&info).expect("serializes"),
            serde_json::json!({
                "make": null, "model": null, "lensModel": null, "focalMm": null, "focal35mm": null, "fNumber": null,
                "exposureSeconds": null, "iso": null, "taken": null, "takenOffset": null, "width": 8, "height": 6,
                "colorProfile": null, "hdr": null, "located": false, "renderedFNumber": null, "rendered": [],
            })
        );
    }

    #[test]
    fn an_identity_never_opened_is_refused_by_name() {
        let opened = Opened::default();
        let refused = tauri::async_runtime::block_on(info_of(&opened, "feedface".into())).expect_err("unknown");
        assert_eq!(refused, PhotoInfoError::UnknownSource { identity: "feedface".into() });
    }
}
