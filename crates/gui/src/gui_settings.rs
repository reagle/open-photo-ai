//! The preferences the command line can follow, published for it.
//!
//! The window's settings store is the only source of what the GUI does, and no other process can read it. So the window
//! mirrors the three preferences `opai` on the command line also has a say in — the processor, the families Autopilot
//! may not suggest, and each family's chosen model — into `opai::settings`'s file, once at boot and on every Save that
//! changes one of them, as it already mirrors the Analytics choice into `analytics.json`. The format, the resolution of
//! each stored model against the catalogue and the atomic write are the library's; what is here is the command.
//!
//! **Nothing here reads the file.** The GUI's behaviour is the store's, with or without it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{CommandError, Ended, Traceparent, command_span, traced};

/// What the window sends: its store's own fields, in its own spellings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Published {
    /// The processor, as the store spells it (`coreml`).
    processor: String,
    /// The families Autopilot may not suggest (`color_balance`).
    autopilot_excluded: Vec<String>,
    /// Each family's stored `<codename>_<precision>`, for the families the user chose one for.
    models: BTreeMap<String, String>,
}

impl Published {
    /// The library's reading of it, with anything it does not publish dropped.
    fn settings(&self) -> opai::settings::SharedSettings {
        opai::settings::SharedSettings::from_gui(
            &self.processor,
            self.autopilot_excluded.iter().map(String::as_str),
            self.models.iter().map(|(family, model)| (family.as_str(), model.as_str())),
        )
    }
}

/// Why the preferences could not be published. Nothing the GUI does depends on it: only the command line's copy is
/// stale.
///
/// Tagged like every other command's error, so the window reads every rejection the same way.
#[derive(Debug, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum GuiSettingsError {
    /// The file could not be written, or its location could not be determined.
    #[error("{message}")]
    SetGuiSettings {
        /// What went wrong, in full.
        message: String,
    },
}

impl CommandError for GuiSettingsError {
    fn ended(&self) -> Ended<'_> {
        match self {
            Self::SetGuiSettings { .. } => Ended::Failed { error: self, recorded: false },
        }
    }
}

// The command's name is written once more, in `frontend/ipc/guiSettings.ts`.
/// Publish the preferences the command line can follow, as the window's settings store holds them.
///
/// `async` only so the file is written off the window's thread.
///
/// # Errors
///
/// [`GuiSettingsError::SetGuiSettings`] where the file could not be written.
#[tauri::command]
pub(crate) async fn set_gui_settings(settings: Published, traceparent: Traceparent) -> Result<(), GuiSettingsError> {
    traced(command_span!("set_gui_settings", traceparent), async move {
        crate::task::spawn_blocking(move || {
            let path = opai::settings::path(opai::APP_NAME).map_err(std::io::Error::other)?;
            publish(&path, &settings)
        })
        .await
        .map_err(std::io::Error::other)
        .and_then(|written| written)
        .map_err(|error| GuiSettingsError::SetGuiSettings {
            message: format!("the settings for the command line could not be saved: {error}"),
        })
    })
    .await
}

/// Writes `published` to `path`, recording a write that changed the file.
fn publish(path: &std::path::Path, published: &Published) -> Result<(), std::io::Error> {
    if opai::settings::write_at(path, &published.settings())? {
        tracing::info!(path = %path.display(), "published the settings the command line follows");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use opai::settings::{Loaded, read_at};
    use opai::{ExecutionProvider, Family};

    use super::*;

    #[test]
    fn the_windows_payload_is_read_in_its_own_spellings() {
        let published: Published = serde_json::from_str(
            r#"{
                "processor": "coreml",
                "autopilotExcluded": ["colorization", "color_balance"],
                "models": { "restore": "osaka_int8", "denoise": "retired_fp16" }
            }"#,
        )
        .expect("the window's shape");

        let settings = published.settings();

        assert_eq!(settings.processor, Some(ExecutionProvider::CoreMl));
        assert_eq!(settings.autopilot_excluded, [Family::Colorization, Family::ColorBalance]);
        assert_eq!(settings.models.len(), 1, "a model the library does not publish is dropped");
        assert_eq!(settings.model(Family::Restore).map(|choice| choice.codename), Some("osaka"));
    }

    #[test]
    fn publishing_writes_what_the_command_line_reads() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("opai").join(opai::settings::FILE);
        let published = Published {
            processor: "cpu".to_owned(),
            autopilot_excluded: vec!["sharpen".to_owned()],
            models: BTreeMap::from([("upscale".to_owned(), "tokyo_fp16".to_owned())]),
        };

        publish(&path, &published).expect("writable");

        assert_eq!(read_at(&path), Loaded::Read(published.settings()));
    }

    #[test]
    fn the_rejection_crosses_under_its_kind() {
        let error = GuiSettingsError::SetGuiSettings { message: "disk full".to_owned() };

        assert_eq!(
            serde_json::to_value(&error).expect("serializable"),
            serde_json::json!({ "kind": "setGuiSettings", "message": "disk full" })
        );
    }
}
