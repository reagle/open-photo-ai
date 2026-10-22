//! What each queued file will lose, written as its chosen format, before anything is written.
//!
//! The rule is `opai::export`'s ([`predict_blocking`]); the format each file is written as is the one
//! [`ExportNaming::destination_for`] gives it, which is what [`export_with`](super::run::export_with) writes.

use std::path::Path;

use opai::export::{Loss, Named, predict_blocking};
use serde::Serialize;

use super::names::ExportNaming;

/// What one queued file will lose, as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ExportLosses {
    /// The source's path, as it was asked for.
    pub(crate) path: String,
    /// What writing it as its format will lose against it; empty when nothing will be lost.
    pub(crate) losses: Vec<Loss>,
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
                return ExportLosses { path, losses: Vec::new() };
            }
            let (_, Named { format, .. }) = naming.destination_for(Path::new(&path));
            let losses = predict_blocking(&path, format.into()).unwrap_or_else(|error| {
                tracing::debug!(%error, "the losses of a queued file could not be predicted");
                Vec::new()
            });

            ExportLosses { path, losses }
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

    #[test]
    fn an_unreadable_file_answers_no_loss() {
        let answered = losses(vec!["/nowhere/missing.png".into()], &naming(FormatChoice::Preserve), |_| true);

        assert_eq!(answered, [ExportLosses { path: "/nowhere/missing.png".into(), losses: Vec::new() }]);
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
