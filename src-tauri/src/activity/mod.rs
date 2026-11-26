//! Active window and idle time, per OS. Window titles only, never screenshots (principle 3).
//!
//! - macOS: NSWorkspace frontmost app + Accessibility API for the title. Idle from CoreGraphics.
//! - Windows: GetForegroundWindow / GetWindowTextW / process image name. Idle from GetLastInputInfo.
//! - Linux X11: `_NET_ACTIVE_WINDOW` via x11rb. Idle from the XScreenSaver extension.
//! - Linux GNOME Wayland: Window Calls extension over D-Bus (as v1). Idle from Mutter IdleMonitor.

mod browser;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;

pub use browser::BrowserUrls;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowInfo {
    pub app_name: String,
    pub window_title: String,
}

pub trait ActivitySource: Send {
    /// The focused window, if it can be read.
    fn active(&mut self) -> Option<WindowInfo>;
    /// Titles of open windows in likely meeting apps (for meeting detection). Never stored.
    fn meeting_candidates(&mut self) -> Vec<WindowInfo>;
    fn idle_seconds(&mut self) -> Option<u64>;
    /// Why tracking is limited (missing permission or extension), for the UI.
    fn problem(&mut self) -> Option<String>;
}

pub fn source() -> Box<dyn ActivitySource> {
    Box::new(platform::Source::new())
}

/// Ask the OS for the permission window titles need (macOS Accessibility). No-op elsewhere.
pub fn request_permission() {
    platform::request_permission();
}

/// App names (lowercase substrings) whose windows may be a meeting.
pub const MEETING_APPS: &[&str] = &[
    "zoom", "teams", "slack", "webex", "meet", "chrome", "chromium", "firefox", "edge", "msedge",
    "brave", "safari", "arc", "vivaldi", "opera", "skype", "discord", "whereby", "jitsi",
];

pub fn is_meeting_candidate(app_name: &str) -> bool {
    let app = app_name.to_lowercase();
    MEETING_APPS.iter().any(|m| app.contains(m))
}
