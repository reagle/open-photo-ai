//! What each queued file will lose, written as its chosen format, before anything is written.
//!
//! The rule is `opai::export`'s ([`predict_export_blocking`]); the format each file is written as, and whether it keeps
//! a gain map, are what [`ExportNaming::destination_for`] gives it, which is what
//! [`export_with`](super::run::export_with) writes.

use std::path::Path;

use opai::export::{Loss, Named, Prediction, predict_export_blocking};
use serde::Serialize;

use super::names::ExportNaming;

/// What one queued file will lose, as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportLosses {
    /// The source's path, as it was asked for.
    pub(crate) path: String,
    /// What writing it as its format will lose against it; empty when nothing will be lost.
    pub(crate) losses: Vec<Loss>,
    /// Whether the source declares an HDR gain map, so a row names it an Ultra HDR JPEG; `false` for a file not read.
    pub(crate) gain_map: bool,
}

/// What each of `paths` will lose under `naming`, in the order asked. A file that cannot be read answers no loss: its
/// export will report its own failure, and a warning about it here would only say the same thing first.
///
/// Only a path `admitted` says this application opened is read; any other answers no loss without being touched, so
/// the window cannot use this to read headers of files the user never chose.
pub(crate) fn losses(paths: Vec<String>, naming: &ExportNaming, admitted: impl Fn(&Path) -> bool) -> Vec<ExportLosses> {
    paths
        .into_iter()
        .map(|path| {
            if !admitted(Path::new(&path)) {
                tracing::debug!("the losses of a file this application never opened were asked for");
                return ExportLosses { path, losses: Vec::new(), gain_map: false };
            }
            let (_, Named { format, gain_map, .. }) = naming.destination_for(Path::new(&path));
            let Prediction { losses, gain_map } = predict_export_blocking(&path, format.into(), gain_map)
                .unwrap_or_else(|error| {
                    tracing::debug!(%error, "the losses of a queued file could not be predicted");
                    Prediction { losses: Vec::new(), gain_map: false }
                });

            ExportLosses { path, losses, gain_map }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use opai::export::{FormatChoice, Naming};

    use super::*;

    fn naming(format: FormatChoice) -> ExportNaming {
        ExportNaming { naming: Naming { prefix: String::new(), suffix: "-opai".into(), format }, location: None }
    }

    /// A 16-bit PNG and an 8-bit JPEG in `dir`, without a profile.
    fn queue(dir: &Path) -> Vec<String> {
        let colour = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(32, 24, |x, y| {
            image::Rgb([(x * 8) as u8, (y * 10) as u8, 120])
        }));
        let deep = dir.join("deep.png");
        image::DynamicImage::ImageRgb16(colour.to_rgb16()).save(&deep).expect("written");
        let plain = dir.join("plain.jpg");
        colour.save(&plain).expect("written");
        [deep, plain].iter().map(|path| path.display().to_string()).collect()
    }

    #[test]
    fn each_file_answers_what_its_format_will_lose_in_the_order_asked() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let paths = queue(dir.path());

        let answered = losses(paths.clone(), &naming(FormatChoice::Format(opai::export::ExportFormat::Jpeg)), |_| true);

        assert_eq!(answered.iter().map(|row| row.path.clone()).collect::<Vec<_>>(), paths);
        assert_eq!(
            answered[0].losses.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["16-bit → 8-bit in JPEG"]
        );
        assert_eq!(answered[1].losses, []);
    }

    #[test]
    fn preserve_writes_each_file_as_itself_and_loses_nothing_here() {
        let dir = tempfile::tempdir().expect("a temporary directory");

        let answered = losses(queue(dir.path()), &naming(FormatChoice::Preserve), |_| true);

        assert!(answered.iter().all(|row| row.losses.is_empty()), "{answered:?}");
    }

    /// A gain-map JPEG in `dir`, as `photo_info`'s test writes one: an ISO 21496-1 payload on the map and the primary.
    fn ultra_hdr(dir: &Path) -> String {
        let encoded = |size: (u32, u32), metadata: &rust_sak::image::Metadata| {
            let pixels =
                image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(size.0, size.1, image::Rgb([90; 3])));
            let mut bytes = Vec::new();
            let quality = Some(rust_sak::image::EncodeOptions::Jpeg { quality: 90 });
            rust_sak::image::encode_writer_with(
                &pixels,
                &mut bytes,
                rust_sak::image::ImageFormat::Jpeg,
                quality,
                metadata,
            )
            .expect("encodable");
            bytes
        };
        let mut own = rust_sak::image::Metadata::default();
        own.iso_gain_map = Some(vec![0; 4]);
        let mut primary = rust_sak::image::Metadata::default();
        primary.iso_gain_map = Some(vec![0; 4]);
        primary.gain_map = Some(encoded((4, 3), &own));

        let path = dir.join("hdr.jpg");
        std::fs::write(&path, encoded((16, 12), &primary)).expect("written");
        path.display().to_string()
    }

    #[test]
    fn a_gain_map_jpeg_is_named_by_its_gain_map_and_loses_it_only_as_plain_jpeg_by_choice() {
        use opai::export::{ExportFormat, HdrCause};
        use opai::image::Hdr;

        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = ultra_hdr(dir.path());
        let ask = |format| {
            let [answer] = <[ExportLosses; 1]>::try_from(losses(vec![path.clone()], &naming(format), |_| true))
                .expect("one asked, one answered");
            answer
        };

        for keeping in [FormatChoice::UltraHdr, FormatChoice::Preserve] {
            let answer = ask(keeping);
            assert_eq!((answer.losses, answer.gain_map), (Vec::new(), true), "{keeping}");
        }
        let plain = ask(FormatChoice::Format(ExportFormat::Jpeg));
        assert!(plain.gain_map);
        assert_eq!(
            plain.losses,
            [Loss::Hdr { hdr: Hdr::GainMap, format: opai::ImageFormat::Jpeg, cause: HdrCause::Chosen }]
        );
        assert_eq!(
            serde_json::to_value(&plain).expect("serializes")["gainMap"],
            serde_json::json!(true),
            "the window reads it as gainMap"
        );

        // An SDR JPEG is not named by one, whatever the choice.
        let queue = queue(dir.path());
        let sdr = losses(vec![queue[1].clone()], &naming(FormatChoice::UltraHdr), |_| true);
        assert_eq!((sdr[0].losses.as_slice(), sdr[0].gain_map), (&[][..], false));
    }

    #[test]
    fn an_unreadable_file_answers_no_loss() {
        let answered = losses(vec!["/nowhere/missing.png".into()], &naming(FormatChoice::Preserve), |_| true);

        assert_eq!(
            answered,
            [ExportLosses { path: "/nowhere/missing.png".into(), losses: Vec::new(), gain_map: false }]
        );
    }

    #[test]
    fn a_file_this_application_never_opened_is_not_read() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let paths = queue(dir.path());
        let jpeg = naming(FormatChoice::Format(opai::export::ExportFormat::Jpeg));

        // The 16-bit PNG would lose depth as JPEG, but nothing admitted it.
        let answered = losses(paths.clone(), &jpeg, |path| path != Path::new(&paths[0]));

        assert_eq!(answered[0].losses, []);
    }
}
