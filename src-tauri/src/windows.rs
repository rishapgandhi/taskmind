//! Main window and floating recorder widget helpers.

use tauri::{AppHandle, Manager, PhysicalPosition};

pub const MAIN: &str = "main";
pub const WIDGET: &str = "widget";

const WIDGET_WIDTH: f64 = 300.0;
const MARGIN: f64 = 24.0;
const TOP_OFFSET: f64 = 48.0;

pub fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("main window missing");
        return;
    };
    let result = window
        .show()
        .and_then(|_| window.unminimize())
        .and_then(|_| window.set_focus());
    if let Err(err) = result {
        tracing::warn!(%err, "could not show main window");
    }
}

pub fn show_widget(app: &AppHandle) {
    if let Some(widget) = app.get_webview_window(WIDGET)
        && let Err(err) = widget.show()
    {
        tracing::warn!(%err, "could not show widget");
    }
}

pub fn toggle_widget(app: &AppHandle) {
    let Some(widget) = app.get_webview_window(WIDGET) else {
        return;
    };
    let result = match widget.is_visible() {
        Ok(true) => widget.hide(),
        _ => widget.show(),
    };
    if let Err(err) = result {
        tracing::warn!(%err, "could not toggle widget");
    }
}

/// Put the widget in the top-right corner of the monitor it opens on.
pub fn place_widget(app: &AppHandle) {
    let Some(widget) = app.get_webview_window(WIDGET) else {
        return;
    };
    let monitor = match widget
        .current_monitor()
        .or_else(|_| widget.primary_monitor())
    {
        Ok(Some(m)) => m,
        _ => return,
    };
    let scale = monitor.scale_factor();
    let size = monitor.size();
    let origin = monitor.position();
    let x = f64::from(origin.x) + f64::from(size.width) - (WIDGET_WIDTH + MARGIN) * scale;
    let y = f64::from(origin.y) + TOP_OFFSET * scale;
    if let Err(err) = widget.set_position(PhysicalPosition::new(x, y)) {
        tracing::warn!(%err, "could not place widget");
    }
}
