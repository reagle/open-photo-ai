//! The application's own menu bar, on macOS.
//!
//! Without one, Tauri installs `Menu::default`, whose File > Close Window takes Command-W - and closing the only
//! window ends the application, the open images and the models loaded with it. This menu has no Close Window: its
//! Command-W is Close Image, and the window is closed by its own button.
//!
//! **Rust owns the menu, the window owns what each item does.** Every action here is frontend state - the open
//! files, the picker, the settings dialog - so an item of this application's own is forwarded to the window as
//! [`EVENT`] naming it, and the window's `useAppActions` acts on it exactly as it acts on the same keys on Windows and
//! Linux, which have no menu bar. The predefined items (Hide, Quit, the Edit items and so on) are AppKit's and need
//! nothing from the window.
//!
//! The menu is built in English and relabelled by [`configure_menu`] with the window's catalogue, which is also how
//! `open_images` gets the picker's wording: the backend has no translations of its own.

use std::collections::HashMap;

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, IsMenuItem, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::command::{Traceparent, command_span, traced_sync};

/// The event an item of this application's own is forwarded to the window as, with the item's id as its payload.
///
/// Written once more in `frontend/hooks/useAppActions.ts`.
pub(crate) const EVENT: &str = "app-menu";

/// An item the window acts on: its id, which is also the action's name in `frontend/lib/shortcuts.ts`'s
/// `APP_ACTIONS` and the key of its label, its English label, and its key.
struct Action {
    id: &'static str,
    label: &'static str,
    accelerator: &'static str,
}

/// The items of this application's own, in the order the window names them.
///
/// The keys are fixed: the Keyboard settings page lists them read-only and refuses them for anything else. An item
/// holds one key, the first `appShortcuts` lists for its action; Export's second, Shift-Command-E, is matched by the
/// window itself.
const ACTIONS: [Action; 5] = [
    Action { id: "open", label: "Open…", accelerator: "CmdOrCtrl+O" },
    Action { id: "export", label: "Export…", accelerator: "CmdOrCtrl+S" },
    Action { id: "settings", label: "Settings…", accelerator: "CmdOrCtrl+," },
    Action { id: "closeImage", label: "Close Image", accelerator: "CmdOrCtrl+W" },
    Action { id: "closeAll", label: "Close All Images", accelerator: "CmdOrCtrl+Shift+W" },
];

/// The two items that act on open images, and so are unavailable with none open.
const NEED_FILES: [&str; 2] = ["closeImage", "closeAll"];

/// The item available only while the window's Export button is - while at least one image is picked.
const NEEDS_EXPORT: &str = "export";

/// What the window says the items that follow it may do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Availability {
    /// At least one image is open: Close Image and Close All Images.
    has_files: bool,
    /// The sidebar's Export button is available: Export.
    can_export: bool,
}

/// AppKit's own items, each of which does its work without the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum System {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Fullscreen,
    Minimize,
    Zoom,
    BringAllToFront,
}

impl System {
    /// The key the window's catalogue labels it under, and its English label until it does.
    ///
    /// `None` for the three whose English label AppKit composes with the application's name, or chooses itself:
    /// those are left to it until the window sends one.
    fn label(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::About => ("about", None),
            Self::Services => ("services", Some("Services")),
            Self::Hide => ("hide", None),
            Self::HideOthers => ("hideOthers", Some("Hide Others")),
            Self::ShowAll => ("showAll", Some("Show All")),
            Self::Quit => ("quit", None),
            Self::Undo => ("undo", Some("Undo")),
            Self::Redo => ("redo", Some("Redo")),
            Self::Cut => ("cut", Some("Cut")),
            Self::Copy => ("copy", Some("Copy")),
            Self::Paste => ("paste", Some("Paste")),
            Self::SelectAll => ("selectAll", Some("Select All")),
            Self::Fullscreen => ("fullscreen", Some("Enter Full Screen")),
            Self::Minimize => ("minimize", Some("Minimize")),
            Self::Zoom => ("zoom", Some("Zoom")),
            Self::BringAllToFront => ("bringAllToFront", Some("Bring All to Front")),
        }
    }
}

/// One line of a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// An item of this application's own, by its id in [`ACTIONS`].
    Action(&'static str),
    System(System),
    Separator,
}

/// One menu in the bar: the key of its title, its English title, the id AppKit recognises it by (the Window and Help
/// menus, which AppKit fills in itself), and its lines.
struct Section {
    key: &'static str,
    title: &'static str,
    id: Option<&'static str>,
    entries: &'static [Entry],
}

/// The whole bar, as data, so what it holds is tested without building a native menu.
///
/// The first section is the application's menu, titled by the application's name whatever it is given.
const SECTIONS: &[Section] = &[
    Section {
        key: "app",
        title: "Open Photo AI",
        id: None,
        entries: &[
            Entry::System(System::About),
            Entry::Separator,
            Entry::Action("settings"),
            Entry::Separator,
            Entry::System(System::Services),
            Entry::Separator,
            Entry::System(System::Hide),
            Entry::System(System::HideOthers),
            Entry::System(System::ShowAll),
            Entry::Separator,
            Entry::System(System::Quit),
        ],
    },
    Section {
        key: "file",
        title: "File",
        id: None,
        entries: &[
            Entry::Action("open"),
            Entry::Separator,
            Entry::Action("export"),
            Entry::Separator,
            Entry::Action("closeImage"),
            Entry::Action("closeAll"),
        ],
    },
    // The text fields - an export's name, a folder - need these: without an Edit menu Command-C, -V and -A do
    // nothing in a WebKit view on macOS.
    Section {
        key: "edit",
        title: "Edit",
        id: None,
        entries: &[
            Entry::System(System::Undo),
            Entry::System(System::Redo),
            Entry::Separator,
            Entry::System(System::Cut),
            Entry::System(System::Copy),
            Entry::System(System::Paste),
            Entry::System(System::SelectAll),
        ],
    },
    Section { key: "view", title: "View", id: None, entries: &[Entry::System(System::Fullscreen)] },
    Section {
        key: "window",
        title: "Window",
        id: Some(WINDOW_SUBMENU_ID),
        entries: &[
            Entry::System(System::Minimize),
            Entry::System(System::Zoom),
            Entry::Separator,
            Entry::System(System::BringAllToFront),
        ],
    },
    Section { key: "help", title: "Help", id: Some(HELP_SUBMENU_ID), entries: &[] },
];

/// The action a menu item names, when it is one the window acts on; `None` for AppKit's own and for anything else.
fn action_for(id: &str) -> Option<&'static str> {
    ACTIONS.iter().map(|action| action.id).find(|&action| action == id)
}

/// The menu's items that the window relabels, by the key of their label.
///
/// Held as application state because AppKit's own items carry no id of ours to find them by again.
pub(crate) struct AppMenu<R: Runtime> {
    labelled: Vec<(&'static str, MenuItemKind<R>)>,
}

impl<R: Runtime> AppMenu<R> {
    /// Applies the window's labels, by key, and makes the two close items available only with images open and
    /// Export only while the window could export.
    ///
    /// A failure is logged and the rest are still applied - a label left in English is not worth a broken menu.
    fn configure(&self, labels: &HashMap<String, String>, now: Availability) {
        for (key, item) in &self.labelled {
            let (text, enabled) = plan(key, labels, now);

            if let Some(text) = text
                && let Err(error) = set_text(item, text)
            {
                tracing::warn!(%error, key, "could not relabel a menu item");
            }

            if let Some(enabled) = enabled
                && let Some(item) = item.as_menuitem()
                && let Err(error) = item.set_enabled(enabled)
            {
                tracing::warn!(%error, key, "could not enable or disable a menu item");
            }
        }
    }
}

/// What configuring does to the item labelled under `key`: the label it takes, if `labels` has one, and whether it
/// is available, for the two items that follow the open images and for Export.
///
/// Asked only for the keys the menu holds, so a key in `labels` the menu does not hold is ignored; an item missing
/// from `labels` keeps what it says.
// Apart from `AppMenu::configure` because the native items cannot be built in a test: AppKit's menus are made on the
// main thread only, and a test runs on another.
fn plan<'a>(key: &str, labels: &'a HashMap<String, String>, now: Availability) -> (Option<&'a str>, Option<bool>) {
    let text = labels.get(key).map(String::as_str);
    let enabled = if NEED_FILES.contains(&key) {
        Some(now.has_files)
    } else if key == NEEDS_EXPORT {
        Some(now.can_export)
    } else {
        None
    };

    (text, enabled)
}

/// Every key the menu labels an item under, in the order [`build`] keeps them.
#[cfg(test)]
fn label_keys() -> impl Iterator<Item = &'static str> {
    SECTIONS.iter().flat_map(|section| {
        let entries = section.entries.iter().filter_map(|entry| match *entry {
            Entry::Action(id) => Some(id),
            Entry::System(item) => Some(item.label().0),
            Entry::Separator => None,
        });
        // The application's menu keeps its name: macOS shows the process's whatever the title says.
        entries.chain((section.key != "app").then_some(section.key))
    })
}

/// Sets the label of whichever kind of item `item` is.
fn set_text<R: Runtime>(item: &MenuItemKind<R>, text: &str) -> tauri::Result<()> {
    match item {
        MenuItemKind::MenuItem(item) => item.set_text(text),
        MenuItemKind::Submenu(item) => item.set_text(text),
        MenuItemKind::Predefined(item) => item.set_text(text),
        MenuItemKind::Check(item) => item.set_text(text),
        MenuItemKind::Icon(item) => item.set_text(text),
    }
}

/// Builds one of AppKit's items, in English.
// Only `install` builds the menu, and only macOS installs it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn system<R: Runtime>(app: &AppHandle<R>, item: System) -> tauri::Result<PredefinedMenuItem<R>> {
    let text = item.label().1;

    match item {
        System::About => {
            let info = app.package_info();
            let metadata = AboutMetadata {
                name: Some(info.name.clone()),
                version: Some(info.version.to_string()),
                ..AboutMetadata::default()
            };
            PredefinedMenuItem::about(app, text, Some(metadata))
        }
        System::Services => PredefinedMenuItem::services(app, text),
        System::Hide => PredefinedMenuItem::hide(app, text),
        System::HideOthers => PredefinedMenuItem::hide_others(app, text),
        System::ShowAll => PredefinedMenuItem::show_all(app, text),
        System::Quit => PredefinedMenuItem::quit(app, text),
        System::Undo => PredefinedMenuItem::undo(app, text),
        System::Redo => PredefinedMenuItem::redo(app, text),
        System::Cut => PredefinedMenuItem::cut(app, text),
        System::Copy => PredefinedMenuItem::copy(app, text),
        System::Paste => PredefinedMenuItem::paste(app, text),
        System::SelectAll => PredefinedMenuItem::select_all(app, text),
        System::Fullscreen => PredefinedMenuItem::fullscreen(app, text),
        System::Minimize => PredefinedMenuItem::minimize(app, text),
        System::Zoom => PredefinedMenuItem::maximize(app, text),
        System::BringAllToFront => PredefinedMenuItem::bring_all_to_front(app, text),
    }
}

/// Builds the menu bar of [`SECTIONS`], in English, with the items the window relabels.
///
/// The two close items and Export start unavailable: no image is open until the window says otherwise.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<(Menu<R>, AppMenu<R>)> {
    let mut labelled = Vec::new();
    let mut sections = Vec::new();

    for section in SECTIONS {
        let title = if section.key == "app" { app.package_info().name.as_str() } else { section.title };
        let submenu = match section.id {
            Some(id) => Submenu::with_id(app, id, title, true)?,
            None => Submenu::new(app, title, true)?,
        };

        for entry in section.entries {
            let item: MenuItemKind<R> = match *entry {
                Entry::Action(id) => {
                    let action = ACTIONS.iter().find(|action| action.id == id).expect("an entry names an action");
                    let enabled = !NEED_FILES.contains(&id) && id != NEEDS_EXPORT;
                    let item = MenuItem::with_id(app, id, action.label, enabled, Some(action.accelerator))?;
                    labelled.push((id, item.kind()));
                    item.kind()
                }
                Entry::System(item) => {
                    let built = system(app, item)?;
                    labelled.push((item.label().0, built.kind()));
                    built.kind()
                }
                Entry::Separator => PredefinedMenuItem::separator(app)?.kind(),
            };
            submenu.append(&item)?;
        }

        // The application's menu keeps its name: macOS shows the process's whatever the title says.
        if section.key != "app" {
            labelled.push((section.key, submenu.kind()));
        }
        sections.push(submenu);
    }

    let menu = Menu::new(app)?;
    for submenu in &sections {
        menu.append(submenu)?;
    }

    Ok((menu, AppMenu { labelled }))
}

/// Puts this application's menu in place of Tauri's default and keeps its items for [`configure_menu`].
///
/// Called from `setup` on macOS only. Windows and Linux get no menu bar: Tauri installs none there unless asked, and
/// one would sit under the window's own navbar.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let (menu, labelled) = build(app)?;
    app.set_menu(menu)?;
    app.manage(labelled);

    Ok(())
}

/// Forwards a click on an item of this application's own to the window, as [`EVENT`] naming it.
///
/// AppKit's own items reach here too, after AppKit has acted on them, and are not forwarded.
pub(crate) fn forward<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let Some(action) = action_for(event.id().as_ref()) else {
        return;
    };

    if let Err(error) = app.emit_to("main", EVENT, action) {
        tracing::warn!(%error, action, "could not forward a menu item to the window");
    }
}

// The command's name is written once more, in `frontend/ipc/menu.ts`.
/// Relabels the menu with the window's catalogue, and makes Close Image and Close All Images available only with
/// images open, and Export only while the window's Export button is.
///
/// `labels` is keyed as the window's `menu.bar` catalogue is; a key the menu does not hold is ignored. Without a menu
/// - on Windows and Linux - nothing happens.
#[tauri::command]
pub(crate) fn configure_menu<R: Runtime>(
    app: AppHandle<R>,
    labels: HashMap<String, String>,
    has_files: bool,
    can_export: bool,
    traceparent: Traceparent,
) {
    traced_sync(command_span!("configure_menu", traceparent), || {
        if let Some(menu) = app.try_state::<AppMenu<R>>() {
            menu.configure(&labels, Availability { has_files, can_export });
        }
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn entries() -> impl Iterator<Item = Entry> {
        SECTIONS.iter().flat_map(|section| section.entries.iter().copied())
    }

    /// Every action is in the menu once, with the key the window lists for it - and Command-W is Close Image's
    /// alone. The default menu's Close Window, which took it and quit the application, is not here at all: there is
    /// no `System` for it to be.
    #[test]
    fn the_menu_holds_each_action_once_with_its_key_and_nothing_else_takes_command_w() {
        let placed: Vec<&str> = entries()
            .filter_map(|entry| match entry {
                Entry::Action(id) => Some(id),
                _ => None,
            })
            .collect();
        let mut sorted = placed.clone();
        sorted.sort_unstable();
        let mut expected: Vec<&str> = ACTIONS.iter().map(|action| action.id).collect();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "each action should be placed exactly once");

        let keys: Vec<(&str, &str)> = ACTIONS.iter().map(|action| (action.id, action.accelerator)).collect();
        assert_eq!(
            keys,
            [
                ("open", "CmdOrCtrl+O"),
                ("export", "CmdOrCtrl+S"),
                ("settings", "CmdOrCtrl+,"),
                ("closeImage", "CmdOrCtrl+W"),
                ("closeAll", "CmdOrCtrl+Shift+W"),
            ]
        );
        let command_w: Vec<&str> = ACTIONS
            .iter()
            .filter(|action| action.accelerator == "CmdOrCtrl+W")
            .map(|action| action.id)
            .collect();
        assert_eq!(command_w, ["closeImage"]);
    }

    /// The bar is the one the design draws: the application's menu, File, Edit, View, Window and Help, with Quit in
    /// the first and the Edit items text fields need.
    #[test]
    fn the_bar_has_the_standard_menus() {
        let keys: Vec<&str> = SECTIONS.iter().map(|section| section.key).collect();
        assert_eq!(keys, ["app", "file", "edit", "view", "window", "help"]);

        let all: Vec<Entry> = entries().collect();
        for wanted in [System::Quit, System::Hide, System::Copy, System::Paste, System::SelectAll, System::Minimize] {
            assert!(all.contains(&Entry::System(wanted)), "{wanted:?} is missing");
        }

        assert_eq!(SECTIONS[4].id, Some(WINDOW_SUBMENU_ID));
        assert_eq!(SECTIONS[5].id, Some(HELP_SUBMENU_ID));
    }

    /// File holds Open, then Export, then the two close items - the order the design's table gives.
    #[test]
    fn file_holds_export_between_open_and_the_close_items() {
        let file = SECTIONS.iter().find(|section| section.key == "file").expect("a File menu");
        let actions: Vec<&str> = file
            .entries
            .iter()
            .filter_map(|entry| match *entry {
                Entry::Action(id) => Some(id),
                _ => None,
            })
            .collect();

        assert_eq!(actions, ["open", "export", "closeImage", "closeAll"]);
    }

    /// Every label key is distinct, so one key never relabels two items.
    #[test]
    fn every_label_key_is_distinct() {
        let mut keys: Vec<&str> = SECTIONS.iter().map(|section| section.key).collect();
        keys.extend(entries().filter_map(|entry| match entry {
            Entry::Action(id) => Some(id),
            Entry::System(item) => Some(item.label().0),
            Entry::Separator => None,
        }));

        let distinct: BTreeSet<&str> = keys.iter().copied().collect();
        assert_eq!(distinct.len(), keys.len(), "a label key is used twice: {keys:?}");
    }

    /// The labels land by key, a key the menu does not hold is ignored, the two close items follow whether any image
    /// is open, and Export whether the window could export - and no other item follows either.
    ///
    /// **What this does not prove:** that AppKit draws the label. The native items cannot be built off the main
    /// thread, so this covers the plan `AppMenu::configure` applies, and the drawing is a check by hand.
    #[test]
    fn configuring_relabels_by_key_and_disables_the_close_items_and_export_when_they_cannot_act() {
        let held: Vec<&str> = label_keys().collect();
        assert!(held.contains(&"file") && held.contains(&"open") && held.contains(&"copy"));
        assert!(!held.contains(&"noSuchItem") && !held.contains(&"app"));

        let labels = HashMap::from([
            ("file".to_string(), "Fichier".to_string()),
            ("open".to_string(), "Ouvrir…".to_string()),
            ("noSuchItem".to_string(), "Rien".to_string()),
        ]);

        let none = Availability { has_files: false, can_export: false };
        let open = Availability { has_files: true, can_export: false };
        let picked = Availability { has_files: true, can_export: true };

        assert_eq!(plan("file", &labels, none), (Some("Fichier"), None));
        assert_eq!(plan("open", &labels, none), (Some("Ouvrir…"), None));
        assert_eq!(plan("closeImage", &labels, none), (None, Some(false)), "an item with no label keeps its own");
        assert_eq!(plan("closeAll", &labels, open), (None, Some(true)));
        assert_eq!(plan("closeAll", &HashMap::new(), none), (None, Some(false)));
        assert_eq!(plan("export", &labels, open), (None, Some(false)), "open but nothing picked");
        assert_eq!(plan("export", &labels, picked), (None, Some(true)));

        let enabled: Vec<&str> = held.iter().copied().filter(|key| plan(key, &labels, picked).1.is_some()).collect();
        assert_eq!(enabled, ["export", "closeImage", "closeAll"]);
    }

    /// The window's own items are forwarded by id; AppKit's, and anything else, are not.
    #[test]
    fn only_the_windows_own_items_are_forwarded() {
        for action in &ACTIONS {
            assert_eq!(action_for(action.id), Some(action.id));
        }

        // What `PredefinedMenuItem`s are given: generated ids, never one of ours.
        for other in ["1", "quit", "close_window", "", "CloseImage"] {
            assert_eq!(action_for(other), None, "{other:?} was forwarded");
        }
    }
}
