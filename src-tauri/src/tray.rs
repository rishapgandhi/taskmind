//! System tray icon and menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::recorder::{self, Action, Snapshot, Status};
use crate::windows;

const TRAY_ID: &str = "main";

pub struct TrayState {
    record: MenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open TaskMind", true, None::<&str>)?;
    let widget = MenuItem::with_id(app, "widget", "Show / Hide Recorder", true, None::<&str>)?;
    let record = MenuItem::with_id(app, "record", "Start Recording", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit TaskMind", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &widget, &record, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("TaskMind")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => windows::show_main(app),
            "widget" => windows::toggle_widget(app),
            "record" => {
                if let Err(err) = recorder::dispatch(app, Action::Toggle) {
                    tracing::warn!(%err, "tray record toggle failed");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.manage(TrayState { record });
    Ok(())
}

/// Keep tray text in line with the recorder state.
pub fn sync(app: &AppHandle, snapshot: &Snapshot) {
    let (label, tooltip) = match snapshot.status {
        Status::Idle => ("Start Recording", "TaskMind"),
        Status::Recording => ("Stop Recording", "TaskMind: recording"),
        Status::Paused => ("Stop Recording", "TaskMind: paused"),
    };
    if let Some(state) = app.try_state::<TrayState>()
        && let Err(err) = state.record.set_text(label)
    {
        tracing::warn!(%err, "failed to update tray menu");
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID)
        && let Err(err) = tray.set_tooltip(Some(tooltip))
    {
        tracing::warn!(%err, "failed to update tray tooltip");
    }
}
