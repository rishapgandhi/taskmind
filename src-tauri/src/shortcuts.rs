//! Global keyboard shortcuts.
//!
//! Defaults use Cmd/Ctrl + Shift + Alt so they do not steal common app shortcuts
//! (Ctrl+Shift+R reloads browsers, Ctrl+Shift+T reopens tabs, Ctrl+Alt+T opens a terminal on Ubuntu).
//! Making them user-editable is still open in docs/PLAN.md (M0).

use serde::Serialize;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::recorder::{self, Action};
use crate::windows;

#[cfg(target_os = "macos")]
const PRIMARY: Modifiers = Modifiers::SUPER;
#[cfg(not(target_os = "macos"))]
const PRIMARY: Modifiers = Modifiers::CONTROL;

#[cfg(target_os = "macos")]
const PREFIX: &str = "Cmd+Shift+Option";
#[cfg(not(target_os = "macos"))]
const PREFIX: &str = "Ctrl+Shift+Alt";

fn toggle_recording() -> Shortcut {
    Shortcut::new(
        Some(PRIMARY | Modifiers::SHIFT | Modifiers::ALT),
        Code::KeyR,
    )
}

fn show_app() -> Shortcut {
    Shortcut::new(
        Some(PRIMARY | Modifiers::SHIFT | Modifiers::ALT),
        Code::KeyT,
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    toggle_recording: String,
    show_app: String,
}

pub fn labels() -> Labels {
    Labels {
        toggle_recording: format!("{PREFIX}+R"),
        show_app: format!("{PREFIX}+T"),
    }
}

pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if shortcut == &toggle_recording() {
                if let Err(err) = recorder::dispatch(app, Action::Toggle) {
                    tracing::warn!(%err, "shortcut record toggle failed");
                }
            } else if shortcut == &show_app() {
                windows::show_main(app);
            }
        })
        .build()
}

/// Register defaults. Failure is not fatal: GNOME Wayland and some setups block global shortcuts,
/// and the tray and widget still work.
pub fn register(app: &AppHandle) {
    for shortcut in [toggle_recording(), show_app()] {
        if let Err(err) = app.global_shortcut().register(shortcut) {
            tracing::warn!(%err, ?shortcut, "could not register global shortcut");
        }
    }
}
