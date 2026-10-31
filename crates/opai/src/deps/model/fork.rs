//! Fork-only, not for upstream: installing a model this fork exports itself and publishes nowhere.
//!
//! The fork's own models are exported locally and placed in `models/<artifact>/` by the fork's install scripts, which
//! also add each file's name, size and SHA-256 to `models/.fork-listing.json`. No published listing names them, so
//! [`super::install`] would refuse every one as unpublished. For an artifact of a model in [`EXPORTED`], this reads
//! that local listing instead: the files it names for the artifact, every one in place at its listed size, are used as
//! they are, through `rust-sak`'s installer under [`Trust::LocalFiles`], so a replaced file still empties the engines
//! compiled from the old one. Nothing is transferred, the published listing is not read, and a model the project
//! publishes is never looked up here, so the local listing can add a model but never replace a published one's files.

use std::path::{Path, PathBuf};

use rust_sak::ai::{self, Files, Folder, Outcome, Select, Trust};
use serde::Deserialize;

use crate::config;
use crate::deps::model::paths::{MODELS_DIR, engine_dir, model_dir};
use crate::error::InitError;
use crate::models::build::ParameterValues;
use crate::models::{ArtifactId, FamilyEntry};

/// The local listing of fork-exported model files, in the models directory.
pub(crate) const FORK_LISTING: &str = ".fork-listing.json";

/// The codenames of the models this fork exports itself: Kazan, Lisbon, Porto, Bilbao and Lima.
pub(crate) const EXPORTED: [&str; 5] = ["kazan", "lisbon", "porto", "bilbao", "lima"];

/// One file the local listing names. Its hash is the install script's record and is not read here.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Placed {
    /// The file's name in the model's directory, as in `rm_lima_fp32.onnx`.
    pub(crate) name: String,
    /// Its size in bytes.
    pub(crate) size: u64,
}

/// Whether `id` is an artifact of a model this fork exports itself: its codename, the second part of its name, is in
/// [`EXPORTED`].
pub(crate) fn exported(id: &ArtifactId) -> bool {
    id.as_str().split('_').nth(1).is_some_and(|codename| EXPORTED.contains(&codename))
}

/// The files the local listing in `app_dir` names for `id`: every one whose name starts with `<id>.onnx`, so a graph
/// and its weights blob both. None where the listing is absent or unreadable.
pub(crate) fn listed(app_dir: &Path, id: &ArtifactId) -> Vec<Placed> {
    let path = app_dir.join(MODELS_DIR).join(FORK_LISTING);
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let files: Vec<Placed> = match serde_json::from_slice(&bytes) {
        Ok(files) => files,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "the fork's local model listing could not be read");
            return Vec::new();
        }
    };

    let prefix = format!("{id}.onnx");
    files.into_iter().filter(|file| file.name.starts_with(&prefix)).collect()
}

/// Whether `listed` names at least one file and every one is in `id`'s directory under `app_dir` at its listed size.
pub(crate) fn placed(app_dir: &Path, id: &ArtifactId, listed: &[Placed]) -> bool {
    let dir = app_dir.join(model_dir(id));

    !listed.is_empty()
        && listed.iter().all(|file| {
            std::fs::metadata(dir.join(&file.name)).is_ok_and(|meta| meta.is_file() && meta.len() == file.size)
        })
}

/// Uses the placed files `listed` of `id` as they are and returns their directory.
///
/// # Errors
///
/// [`InitError::UnlistedModel`] where a file went missing between the check and the install, and [`InitError::Io`] or
/// [`InitError::Fs`] for the directories around them.
pub(crate) async fn install(
    folder: &Folder,
    app_dir: &Path,
    name: &str,
    id: &ArtifactId,
    listed: &[Placed],
) -> Result<PathBuf, InitError> {
    let dir = config::sub_dir(app_dir, name, &model_dir(id))?;
    let files = Files {
        name: id.as_str().to_owned().into(),
        folder: folder.clone(),
        select: Select::Names(listed.iter().map(|file| file.name.clone()).collect()),
        dir: dir.clone(),
        derived: vec![app_dir.join(engine_dir(id))],
        trust: Trust::LocalFiles,
    };

    match ai::install_files(&files, None, |_| {}).await {
        Ok(Outcome::Trusted) => {
            // `debug`, unlike the `warn` a trusted published model writes: placing these by hand is the only way they
            // are ever installed.
            tracing::debug!(dependency = id.as_str(), path = %dir.display(), "a fork-exported model is used as placed");
            Ok(dir)
        }
        Ok(_) | Err(_) => Err(InitError::UnlistedModel { artifact: id.as_str().to_owned() }),
    }
}

/// [`catalogue`](crate::catalogue) as the install in `app_dir` can serve it: each variant's
/// [`unavailable`](crate::VariantEntry::unavailable) lists the precisions at which a model it requires is one of this
/// fork's and is not [`placed`], so an install of it would be refused as [`InitError::UnavailableModel`].
///
/// A few file reads and `stat` calls per variant, with nothing requested and nothing created, so a chooser can ask it
/// before any initialization.
pub(crate) fn installable_catalogue(app_dir: &Path) -> Vec<FamilyEntry> {
    let missing = |id: &ArtifactId| exported(id) && !placed(app_dir, id, &listed(app_dir, id));
    let mut families = crate::catalogue().to_vec();

    for variant in families.iter_mut().flat_map(|family| family.variants.iter_mut()) {
        variant.unavailable = variant
            .precisions
            .iter()
            .copied()
            .filter(|&precision| {
                variant
                    .build(precision, &ParameterValues::new())
                    .is_ok_and(|subject| subject.required_artifacts().iter().any(missing))
            })
            .collect();

        if !variant.unavailable.is_empty() {
            tracing::debug!(
                codename = variant.codename,
                unavailable = ?variant.unavailable,
                "a model is not available at every precision on this install"
            );
        }
    }

    families
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::test_support::kyoto;
    use crate::models::{FloatPrecision, RemoveVariant};

    fn lima() -> ArtifactId {
        RemoveVariant::Lima(FloatPrecision::Fp32).artifact()
    }

    /// An application directory whose local listing names `files`.
    fn app_listing(files: &[(&str, u64)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let models = root.path().join(MODELS_DIR);
        std::fs::create_dir_all(&models).unwrap();
        let listing: Vec<serde_json::Value> = files
            .iter()
            .map(|(name, size)| serde_json::json!({ "name": name, "size": size, "sha256": "0".repeat(64) }))
            .collect();
        std::fs::write(models.join(FORK_LISTING), serde_json::to_vec(&listing).unwrap()).unwrap();
        root
    }

    #[test]
    fn only_the_forks_own_models_are_looked_up_locally() {
        assert!(exported(&lima()));
        assert!(!exported(&kyoto()), "a published model was looked up in the fork's listing");
    }

    #[test]
    fn the_listing_names_an_artifacts_graph_and_weights_and_nothing_sharing_a_leading_portion() {
        let root =
            app_listing(&[("rm_lima_fp32.onnx", 3), ("rm_lima_fp32.onnx.data", 4), ("rm_lima_1024_fp32.onnx", 5)]);

        let names: Vec<String> = listed(root.path(), &lima()).into_iter().map(|file| file.name).collect();
        assert_eq!(names, ["rm_lima_fp32.onnx", "rm_lima_fp32.onnx.data"]);
    }

    #[test]
    fn a_model_is_placed_only_with_every_file_at_its_listed_size() {
        let root = app_listing(&[("rm_lima_fp32.onnx", 3)]);
        let files = listed(root.path(), &lima());
        assert!(!placed(root.path(), &lima(), &files), "a model with no files was placed");

        let dir = root.path().join(model_dir(&lima()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rm_lima_fp32.onnx"), b"ab").unwrap();
        assert!(!placed(root.path(), &lima(), &files), "a file of the wrong size was placed");

        std::fs::write(dir.join("rm_lima_fp32.onnx"), b"abc").unwrap();
        assert!(placed(root.path(), &lima(), &files));
        assert!(!placed(root.path(), &lima(), &[]), "an artifact the listing does not name was placed");
    }

    #[tokio::test]
    async fn placed_files_install_as_they_are_with_nothing_transferred() {
        let root = app_listing(&[("rm_lima_fp32.onnx", 3)]);
        let app_dir = root.path().to_path_buf();
        let dir = app_dir.join(model_dir(&lima()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rm_lima_fp32.onnx"), b"abc").unwrap();

        // A folder nothing answers at: an install that reached for it would fail.
        let folder = Folder::parse("http://127.0.0.1:9/nobody/nothing/tree/main/models").unwrap();
        let files = listed(&app_dir, &lima());
        let installed = install(&folder, &app_dir, "opai-test", &lima(), &files).await.unwrap();

        assert_eq!(installed, dir);
        assert_eq!(std::fs::read(dir.join("rm_lima_fp32.onnx")).unwrap(), b"abc");
    }

    /// The precisions `codename` is unavailable at in `families`.
    fn unavailable(families: &[FamilyEntry], codename: &str) -> Vec<crate::models::Precision> {
        families
            .iter()
            .flat_map(|family| family.variants.iter())
            .find(|variant| variant.codename == codename)
            .unwrap_or_else(|| panic!("the catalogue has no {codename}"))
            .unavailable
            .clone()
    }

    #[test]
    fn a_fork_model_is_unavailable_until_it_is_placed_and_a_published_one_never_is() {
        let root = app_listing(&[("rm_lima_fp32.onnx", 3)]);
        let families = installable_catalogue(root.path());
        assert_eq!(unavailable(&families, "lima"), [crate::models::Precision::Fp32]);
        assert!(unavailable(&families, "kyoto").is_empty(), "a published model was marked unavailable");

        let dir = root.path().join(model_dir(&lima()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rm_lima_fp32.onnx"), b"abc").unwrap();
        assert!(
            unavailable(&installable_catalogue(root.path()), "lima").is_empty(),
            "a placed model was unavailable"
        );
    }
}
