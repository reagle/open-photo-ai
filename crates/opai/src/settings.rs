//! The settings one front end saves for another to follow: today, what the GUI publishes for the command line.
//!
//! The GUI keeps its preferences in its own webview storage, which no other process can read. Whenever they change, it
//! mirrors the few the command line also has a say in — the processor, the families Autopilot may not suggest, and
//! each family's chosen model and precision — into [`FILE`] in this library's configuration directory, through
//! [`write_at`]. The command line reads it back with [`read_at`] and decides for itself how it ranks against its own
//! flags. The GUI never reads the file: its own storage stays the only source of what it does.
//!
//! The format is versioned ([`VERSION`]) and read leniently: an unknown key is skipped, an entry naming something this
//! library does not publish is dropped on its own with a warning, and only a file that cannot be read at all, or that
//! declares another version, is ignored as a whole. Reading never fails a caller.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::models::{Family, Precision, catalogue};
use crate::providers::ExecutionProvider;

/// The file's name inside the configuration directory.
pub const FILE: &str = "gui-settings.json";

/// The version of the format [`write_at`] writes and [`read_at`] accepts. Raised only for a change of meaning; a key
/// added later is skipped by an older reader without one.
pub const VERSION: u64 = 1;

/// One family's chosen model, resolved against the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelChoice {
    /// The family it was chosen for.
    pub family: Family,
    /// The model's codename, as the catalogue spells it (`tokyo`, `saopaulo`).
    pub codename: &'static str,
    /// The precision it was chosen at.
    pub precision: Precision,
    /// Where that precision sits among the model's published ones: `0` is its higher-fidelity build (HD), `1` its
    /// lighter one (SD). The tier a front end names, read positionally as the GUI's chooser reads it.
    pub position: usize,
}

impl ModelChoice {
    /// The published model `codename` at `precision` in `family`, or `None` where the catalogue publishes no such
    /// pairing.
    pub fn resolve(family: Family, codename: &str, precision: &str) -> Option<Self> {
        let entry = catalogue().iter().find(|entry| entry.family == family)?;
        let variant = entry.variants.iter().find(|variant| variant.codename == codename)?;
        let position = variant.precisions.iter().position(|published| published.as_str() == precision)?;

        Some(Self { family, codename: variant.codename, precision: variant.precisions[position], position })
    }

    /// The model a stored `<codename>_<precision>` value names in `family`, or `None` where it names nothing published.
    ///
    /// Matched against every published `<codename>_<precision>` rather than split on `_`, so a codename containing one
    /// cannot be misread.
    pub fn from_value(family: Family, value: &str) -> Option<Self> {
        let entry = catalogue().iter().find(|entry| entry.family == family)?;

        entry.variants.iter().find_map(|variant| {
            let position = variant
                .precisions
                .iter()
                .position(|precision| value == format!("{}_{}", variant.codename, precision.as_str()))?;

            Some(Self { family, codename: variant.codename, precision: variant.precisions[position], position })
        })
    }
}

/// What a front end saved for another to follow. Every part is optional: what is absent is left to the reader's own
/// default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedSettings {
    /// The processor chosen, where one was.
    pub processor: Option<ExecutionProvider>,
    /// The families Autopilot may not suggest, in the library's apply order.
    pub autopilot_excluded: Vec<Family>,
    /// The model chosen for each family a choice was made for, in the library's apply order.
    pub models: Vec<ModelChoice>,
}

impl SharedSettings {
    /// The settings the GUI's store holds, in its own spellings: the processor (`coreml`), the excluded families
    /// (`color_balance`), and each family's stored `<codename>_<precision>`.
    ///
    /// Anything that names nothing this library publishes is dropped with a warning, which is the reading the GUI
    /// itself gives it: an unknown processor is `auto`, and an unknown model is the family's default.
    pub fn from_gui<'a>(
        processor: &str,
        excluded: impl IntoIterator<Item = &'a str>,
        models: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let processor = processor.parse().inspect_err(|error| tracing::warn!(%error, "not publishing")).ok();

        let excluded: Vec<Family> = excluded.into_iter().filter_map(family).collect();
        let mut chosen: Vec<ModelChoice> = models
            .into_iter()
            .filter_map(|(name, value)| {
                let choice = family(name).and_then(|family| ModelChoice::from_value(family, value));
                if choice.is_none() {
                    tracing::warn!(family = name, model = value, "not publishing a model the library does not publish");
                }
                choice
            })
            .collect();
        chosen.sort_by_key(|choice| choice.family.applied_at());

        Self { processor, autopilot_excluded: in_apply_order(&excluded), models: chosen }
    }

    /// Whether Autopilot may not suggest `family`.
    pub fn excludes(&self, family: Family) -> bool {
        self.autopilot_excluded.contains(&family)
    }

    /// The model chosen for `family`, where one was.
    pub fn model(&self, family: Family) -> Option<&ModelChoice> {
        self.models.iter().find(|choice| choice.family == family)
    }

    /// The file's JSON.
    fn to_json(&self) -> Value {
        let models: Map<String, Value> = self
            .models
            .iter()
            .map(|choice| {
                (name(choice.family), json!({ "model": choice.codename, "precision": choice.precision.as_str() }))
            })
            .collect();

        json!({
            "version": VERSION,
            "processor": self.processor.map(ExecutionProvider::as_str),
            "autopilot_excluded": self.autopilot_excluded.iter().copied().map(name).collect::<Vec<_>>(),
            "models": models,
        })
    }
}

/// What [`read_at`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// No file: nothing was saved.
    Absent,
    /// A file that was not read, and why. Already recorded as a warning.
    Ignored(String),
    /// The settings read. An entry that could not be resolved was dropped and recorded as a warning.
    Read(SharedSettings),
}

/// Where the file is, under the configuration directory of the application `name`. Creates nothing.
///
/// # Errors
///
/// Where the platform has no configuration directory, or `name` is not a usable directory name.
pub fn path(name: &str) -> Result<PathBuf, rust_sak::fs::FsError> {
    rust_sak::fs::user_config_dir(name, FILE)
}

/// Writes `settings` to `path`, answering whether the file changed.
///
/// Written through a temporary file beside it and renamed over it, so a reader sees the old settings or the new ones and
/// never a torn file. A write that would change nothing is skipped. The directory is created if missing.
///
/// # Errors
///
/// Where the directory or the file could not be written.
pub fn write_at(path: &Path, settings: &SharedSettings) -> Result<bool, std::io::Error> {
    let bytes = serde_json::to_vec_pretty(&settings.to_json())?;

    if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
        return Ok(false);
    }

    let directory = path.parent().ok_or_else(|| std::io::Error::other("the settings path has no directory"))?;
    std::fs::create_dir_all(directory)?;

    let mut temporary = rust_sak::fs::mk_temp_file_in(directory, ".gui-settings-").map_err(std::io::Error::other)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    temporary.sync_all()?;
    temporary.into_inner().persist(path).map_err(|error| error.error)?;

    Ok(true)
}

/// The settings saved at `path`. Never fails: see [`Loaded`].
pub fn read_at(path: &Path) -> Loaded {
    let ignored = |reason: String| {
        tracing::warn!(path = %path.display(), "ignoring the GUI's settings: {reason}");
        Loaded::Ignored(reason)
    };

    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Loaded::Absent,
        Err(error) => return ignored(format!("it could not be read: {error}")),
    };

    let object = match serde_json::from_slice::<Value>(&raw) {
        Ok(Value::Object(object)) => object,
        Ok(_) => return ignored("it is not a JSON object".to_owned()),
        Err(error) => return ignored(format!("it is not valid JSON: {error}")),
    };

    match object.get("version").and_then(Value::as_u64) {
        Some(VERSION) => {}
        Some(other) => return ignored(format!("it is version {other}, and this build reads version {VERSION}")),
        None => return ignored("it declares no version".to_owned()),
    }

    Loaded::Read(SharedSettings {
        processor: read_processor(object.get("processor")),
        autopilot_excluded: read_excluded(object.get("autopilot_excluded")),
        models: read_models(object.get("models")),
    })
}

/// The `processor` entry, or `None` where it is absent, `null` or names no provider.
fn read_processor(value: Option<&Value>) -> Option<ExecutionProvider> {
    match value? {
        Value::Null => None,
        Value::String(text) => text
            .parse()
            .inspect_err(|error| tracing::warn!(%error, "ignoring the GUI's saved processor"))
            .ok(),
        other => {
            tracing::warn!(processor = %other, "ignoring the GUI's saved processor: it is not a name");
            None
        }
    }
}

/// The `autopilot_excluded` entry: every family it names that the library applies, in apply order.
fn read_excluded(value: Option<&Value>) -> Vec<Family> {
    let Some(value) = value else { return Vec::new() };
    let Some(entries) = value.as_array() else {
        tracing::warn!("ignoring the GUI's Autopilot exclusions: they are not a list");
        return Vec::new();
    };

    let families: Vec<Family> = entries
        .iter()
        .filter_map(|entry| {
            let found = entry.as_str().and_then(family);
            if found.is_none() {
                tracing::warn!(%entry, "ignoring an Autopilot exclusion that names no enhancement");
            }
            found
        })
        .collect();

    in_apply_order(&families)
}

/// The `models` entry: each family's model that resolves against the catalogue, in apply order.
fn read_models(value: Option<&Value>) -> Vec<ModelChoice> {
    let Some(value) = value else { return Vec::new() };
    let Some(entries) = value.as_object() else {
        tracing::warn!("ignoring the GUI's saved models: they are not an object");
        return Vec::new();
    };

    let mut models: Vec<ModelChoice> = entries
        .iter()
        .filter_map(|(key, entry)| {
            let model = entry.get("model").and_then(Value::as_str);
            let precision = entry.get("precision").and_then(Value::as_str);
            let found = family(key)
                .zip(model.zip(precision))
                .and_then(|(family, (model, precision))| ModelChoice::resolve(family, model, precision));
            if found.is_none() {
                tracing::warn!(family = %key, %entry, "ignoring a saved model the library does not publish");
            }
            found
        })
        .collect();
    models.sort_by_key(|choice| choice.family.applied_at());

    models
}

/// The applied family `text` names in the library's spelling (`color_balance`), or `None`. Detection is never applied,
/// so it is never one.
fn family(text: &str) -> Option<Family> {
    serde_json::from_value::<Family>(Value::String(text.to_owned()))
        .ok()
        .filter(|family| family.applied_at().is_some())
}

/// `family` in the library's spelling.
fn name(family: Family) -> String {
    match serde_json::to_value(family) {
        Ok(Value::String(text)) => text,
        _ => unreachable!("a family serializes as its name"),
    }
}

/// `families`, without duplicates, in the library's apply order.
fn in_apply_order(families: &[Family]) -> Vec<Family> {
    Family::APPLY_ORDER.into_iter().filter(|family| families.contains(family)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("opai").join(FILE);

        (dir, path)
    }

    /// What the GUI publishes for a user who chose SD everywhere and switched two families off.
    fn joseph() -> SharedSettings {
        SharedSettings::from_gui(
            "coreml",
            ["colorization", "color_balance"],
            [("restore", "osaka_int8"), ("denoise", "stockholm_fp16"), ("color_balance", "saopaulo_fp32")],
        )
    }

    #[test]
    fn the_guis_spellings_are_resolved_against_the_catalogue() {
        let settings = joseph();

        assert_eq!(settings.processor, Some(ExecutionProvider::CoreMl));
        assert_eq!(settings.autopilot_excluded, [Family::Colorization, Family::ColorBalance]);
        assert!(settings.excludes(Family::ColorBalance) && !settings.excludes(Family::Upscale));

        let families: Vec<Family> = settings.models.iter().map(|choice| choice.family).collect();
        assert_eq!(families, [Family::Denoise, Family::ColorBalance, Family::Restore], "apply order");

        let restore = settings.model(Family::Restore).expect("chosen");
        assert_eq!((restore.codename, restore.precision, restore.position), ("osaka", Precision::Int8, 1));
        let balance = settings.model(Family::ColorBalance).expect("chosen");
        assert_eq!((balance.codename, balance.precision, balance.position), ("saopaulo", Precision::Fp32, 0));
    }

    #[test]
    fn what_the_library_does_not_publish_is_not_published() {
        let settings = SharedSettings::from_gui(
            "TensorRT",
            ["upscale", "detection", "teleport", "upscale"],
            [
                ("upscale", "lisbon_fp16"),
                ("denoise", "tokyo_fp16"),
                ("sharpen", "moscow_int8"),
                ("nope", "x_fp16"),
            ],
        );

        assert_eq!(settings.processor, Some(ExecutionProvider::TensorRt));
        assert_eq!(settings.autopilot_excluded, [Family::Upscale], "detection, junk and the duplicate are dropped");
        assert!(settings.models.is_empty(), "{:?}", settings.models);

        assert_eq!(SharedSettings::from_gui("quantum", [], []).processor, None);
    }

    #[test]
    fn a_written_file_reads_back() {
        let (_dir, path) = file();
        let settings = joseph();

        assert!(write_at(&path, &settings).expect("writable"), "the first write changes the file");
        assert_eq!(read_at(&path), Loaded::Read(settings.clone()));

        let text = std::fs::read_to_string(&path).expect("readable");
        assert!(text.contains(r#""version": 1"#), "{text}");
        assert!(text.contains(r#""processor": "CoreML""#), "{text}");
        assert!(text.contains(r#""restore": {"#) && text.contains(r#""model": "osaka""#), "{text}");

        assert_eq!(read_at(&path), Loaded::Read(settings));
    }

    #[test]
    fn an_unchanged_write_is_skipped_and_a_changed_one_is_not() {
        let (_dir, path) = file();

        assert!(write_at(&path, &joseph()).expect("writable"));
        assert!(!write_at(&path, &joseph()).expect("writable"), "an identical write changed the file");
        assert!(write_at(&path, &SharedSettings::default()).expect("writable"));
        assert_eq!(read_at(&path), Loaded::Read(SharedSettings::default()));
    }

    #[test]
    fn a_missing_file_is_absent() {
        let (_dir, path) = file();

        assert_eq!(read_at(&path), Loaded::Absent);
    }

    #[test]
    fn a_file_that_is_not_settings_is_ignored_whole() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");

        for (garbage, says) in [
            (&b"{\"version\": 1, \"proc"[..], "not valid JSON"),
            (b"", "not valid JSON"),
            (b"[1, 2]", "not a JSON object"),
            (b"{\"processor\": \"cpu\"}", "no version"),
            (b"{\"version\": 2, \"processor\": \"cpu\"}", "version 2"),
            (b"{\"version\": \"1\"}", "no version"),
        ] {
            std::fs::write(&path, garbage).expect("writable");

            match read_at(&path) {
                Loaded::Ignored(reason) => assert!(reason.contains(says), "{garbage:?}: {reason}"),
                other => panic!("{garbage:?} read as {other:?}"),
            }
        }
    }

    #[test]
    fn an_entry_that_does_not_resolve_is_dropped_and_the_rest_kept() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");
        std::fs::write(
            &path,
            r#"{
                "version": 1,
                "processor": "quantum",
                "autopilot_excluded": ["sharpen", "teleport", 7],
                "models": {
                    "upscale": { "model": "lisbon", "precision": "fp16" },
                    "denoise": { "model": "tokyo", "precision": "fp16" },
                    "light_adjustment": { "model": "lyon", "precision": "int8" },
                    "sharpen": { "model": "novgorod", "precision": "fp16", "colour": "blue" },
                    "face_recovery": "athens_fp16"
                },
                "shortcuts": { "undo": ["Mod+Z"] }
            }"#,
        )
        .expect("writable");

        let Loaded::Read(settings) = read_at(&path) else { panic!("a readable file was not read") };

        assert_eq!(settings.processor, None);
        assert_eq!(settings.autopilot_excluded, [Family::Sharpen]);
        assert_eq!(settings.models.len(), 1, "{:?}", settings.models);
        let sharpen = settings.model(Family::Sharpen).expect("kept");
        assert_eq!((sharpen.codename, sharpen.precision, sharpen.position), ("novgorod", Precision::Fp16, 1));
    }

    #[test]
    fn a_shape_that_is_wrong_for_a_field_drops_only_that_field() {
        let (_dir, path) = file();
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("creatable");
        std::fs::write(&path, r#"{"version": 1, "processor": 3, "autopilot_excluded": "sharpen", "models": []}"#)
            .expect("writable");

        assert_eq!(read_at(&path), Loaded::Read(SharedSettings::default()));
    }

    #[test]
    fn the_path_is_under_the_applications_configuration_directory() {
        let path = path("opai-settings-test").expect("a configuration directory");

        assert!(path.ends_with(Path::new("opai-settings-test").join(FILE)), "{}", path.display());
        assert!(!path.exists(), "resolving the path created something");
    }
}
