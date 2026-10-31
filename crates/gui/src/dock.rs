//! The batch on the Dock tile (macOS) or the taskbar button (Windows): a progress bar while the window pages through
//! the open photographs by itself, and on macOS the count of photographs left as the tile's badge.
//!
//! The window says where the batch is - `frontend/hooks/useDockProgress.ts` sends one [`DockProgress`] each time the
//! answer changes, and none when there is no batch to show - and this turns it into the two calls Tauri has for it.

// A command of this application's own rather than the window API's `setProgressBar` and `setBadgeCount` behind
// `core:window:allow-set-progress-bar` and `core:window:allow-set-badge-count`, for the reason
// `capabilities/default.json` gives for the opener plugin: the window says what the batch is doing, never which calls
// to make, and the command is traced like every other request the window sends.
//
// What each platform draws, as Tauri 2.11 (tao 0.35) does it:
//
// - macOS: tao draws its own bar along the bottom of the tile, in the system blue while running and the system yellow
//   while paused, so a paused batch keeps its bar rather than losing it. The bar is app-wide, not the window's. The
//   badge is the tile's red label; `None` takes it away.
// - Windows: the taskbar button's own bar, green while running and yellow while paused. Badges are not supported there
//   (Tauri points at overlay icons instead), so the count is not sent.
// - Linux: only under a desktop with libunity, which draws paused as running.

use serde::Deserialize;
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{Runtime, WebviewWindow};

use crate::command::{Traceparent, command_span, traced_sync};

/// Whether the batch is going or waiting on the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DockState {
    Running,
    Paused,
}

/// Where the batch is, as the window reports it. Crosses the boundary as `{ state, percent, left }`, which
/// `frontend/ipc/dock.ts` writes a second time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) struct DockProgress {
    state: DockState,
    /// How much of the batch is through, 0 to 100. More is read as 100.
    percent: u8,
    /// How many photographs are left: the badge.
    left: u32,
}

/// The bar and the badge for `progress`: none of either with no batch to show.
fn shown(progress: Option<DockProgress>) -> (ProgressBarState, Option<i64>) {
    let Some(DockProgress { state, percent, left }) = progress else {
        return (ProgressBarState { status: Some(ProgressBarStatus::None), progress: None }, None);
    };

    let status = match state {
        DockState::Running => ProgressBarStatus::Normal,
        DockState::Paused => ProgressBarStatus::Paused,
    };

    // A badge of 0 would be taken away by Tauri anyway; `None` says so.
    (
        ProgressBarState { status: Some(status), progress: Some(u64::from(percent.min(100))) },
        (left > 0).then_some(i64::from(left)),
    )
}

/// Draws `progress` on the window's Dock tile or taskbar button, or clears both.
///
/// Refusals are logged and dropped: the tile is a mirror of what the window already says, and a batch is not worth
/// failing for the want of one.
#[tauri::command]
pub(crate) fn dock_progress(window: WebviewWindow, traceparent: Traceparent, progress: Option<DockProgress>) {
    traced_sync(command_span!("dock_progress", traceparent), || draw(&window, progress));
}

fn draw<R: Runtime>(window: &WebviewWindow<R>, progress: Option<DockProgress>) {
    let (bar, badge) = shown(progress);

    if let Err(error) = window.set_progress_bar(bar) {
        tracing::debug!(%error, "the batch's progress could not be drawn on the Dock tile or taskbar button");
    }

    // Not on Windows, where Tauri does not support a badge count.
    #[cfg(not(target_os = "windows"))]
    if let Err(error) = window.set_badge_count(badge) {
        tracing::debug!(%error, "the count of photographs left could not be drawn on the Dock tile");
    }
    #[cfg(target_os = "windows")]
    let _ = badge;
}

#[cfg(test)]
mod tests {
    use tauri::{
        WebviewWindowBuilder,
        test::{mock_builder, mock_context, noop_assets},
    };

    use super::*;

    fn progress(state: DockState, percent: u8, left: u32) -> Option<DockProgress> {
        Some(DockProgress { state, percent, left })
    }

    #[test]
    fn a_running_batch_draws_a_normal_bar_and_the_count_left() {
        let (bar, badge) = shown(progress(DockState::Running, 16, 21));

        assert!(matches!(bar.status, Some(ProgressBarStatus::Normal)));
        assert_eq!(bar.progress, Some(16));
        assert_eq!(badge, Some(21));
    }

    #[test]
    fn a_paused_batch_keeps_its_bar_in_the_paused_state() {
        let (bar, badge) = shown(progress(DockState::Paused, 40, 3));

        assert!(matches!(bar.status, Some(ProgressBarStatus::Paused)));
        assert_eq!(bar.progress, Some(40));
        assert_eq!(badge, Some(3));
    }

    #[test]
    fn no_batch_clears_the_bar_and_the_badge() {
        let (bar, badge) = shown(None);

        assert!(matches!(bar.status, Some(ProgressBarStatus::None)));
        assert_eq!(badge, None);
    }

    #[test]
    fn nothing_left_draws_no_badge_and_too_much_progress_is_a_full_bar() {
        let (bar, badge) = shown(progress(DockState::Running, 250, 0));

        assert_eq!(bar.progress, Some(100));
        assert_eq!(badge, None);
    }

    #[test]
    fn the_progress_crosses_the_boundary_as_the_window_writes_it() {
        let read: Option<DockProgress> = serde_json::from_str(r#"{ "state": "paused", "percent": 50, "left": 2 }"#)
            .expect("the window's shape reads");

        assert_eq!(read, progress(DockState::Paused, 50, 2));
        assert_eq!(serde_json::from_str::<Option<DockProgress>>("null").expect("none reads"), None);
    }

    /// `MockRuntime`'s window methods succeed and do nothing, so this pins only that drawing and clearing against a
    /// real window handle neither panics nor needs anything else in place.
    #[test]
    fn drawing_and_clearing_run_against_a_window() {
        let app = mock_builder().build(mock_context(noop_assets())).expect("the mock app should build");
        let window = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("the mock window should build");

        draw(&window, progress(DockState::Running, 10, 9));
        draw(&window, None);
    }
}
