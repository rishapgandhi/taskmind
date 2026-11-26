//! Linux: GNOME Wayland through the Window Calls extension (D-Bus, as v1), otherwise X11.
//! Idle from Mutter IdleMonitor on GNOME, XScreenSaver on X11.

use serde::Deserialize;
use x11rb::connection::Connection as _;
use x11rb::protocol::screensaver::ConnectionExt as _;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

use super::{ActivitySource, WindowInfo, is_meeting_candidate};

const EXTENSION_HINT: &str = "On GNOME Wayland, window titles need the \"Window Calls\" extension (extensions.gnome.org/extension/4724/window-calls/). Install it, then log out and back in.";

pub fn request_permission() {}

pub struct Source {
    gnome: Option<zbus::blocking::Connection>,
    x11: Option<X11>,
    gnome_wayland: bool,
}

impl Source {
    pub fn new() -> Self {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let gnome = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_uppercase().contains("GNOME"))
            .unwrap_or(false);
        let gnome_wayland = wayland && gnome;
        let dbus = if gnome {
            zbus::blocking::Connection::session()
                .map_err(|e| tracing::warn!(%e, "no D-Bus session"))
                .ok()
        } else {
            None
        };
        // On Wayland, X11 only sees XWayland apps, so it is a fallback for non-GNOME desktops.
        let x11 = if gnome_wayland { None } else { X11::connect() };
        Self {
            gnome: dbus,
            x11,
            gnome_wayland,
        }
    }

    fn gnome_windows(&self) -> Option<Vec<GnomeWindow>> {
        let conn = self.gnome.as_ref()?;
        let reply = conn
            .call_method(
                Some("org.gnome.Shell"),
                "/org/gnome/Shell/Extensions/Windows",
                Some("org.gnome.Shell.Extensions.Windows"),
                "List",
                &(),
            )
            .ok()?;
        let json: String = reply.body().deserialize().ok()?;
        serde_json::from_str(&json).ok()
    }

    fn gnome_title(&self, id: u64) -> Option<String> {
        let conn = self.gnome.as_ref()?;
        let reply = conn
            .call_method(
                Some("org.gnome.Shell"),
                "/org/gnome/Shell/Extensions/Windows",
                Some("org.gnome.Shell.Extensions.Windows"),
                "GetTitle",
                &(id as u32),
            )
            .ok()?;
        reply.body().deserialize().ok()
    }

    fn gnome_idle(&self) -> Option<u64> {
        let conn = self.gnome.as_ref()?;
        let reply = conn
            .call_method(
                Some("org.gnome.Mutter.IdleMonitor"),
                "/org/gnome/Mutter/IdleMonitor/Core",
                Some("org.gnome.Mutter.IdleMonitor"),
                "GetIdletime",
                &(),
            )
            .ok()?;
        let ms: u64 = reply.body().deserialize().ok()?;
        Some(ms / 1000)
    }
}

#[derive(Deserialize)]
struct GnomeWindow {
    id: u64,
    #[serde(default)]
    wm_class: Option<String>,
    #[serde(default)]
    focus: bool,
    #[serde(default)]
    title: Option<String>,
}

impl ActivitySource for Source {
    fn active(&mut self) -> Option<WindowInfo> {
        if let Some(windows) = self.gnome_windows() {
            let w = windows.into_iter().find(|w| w.focus)?;
            let title = w
                .title
                .clone()
                .or_else(|| self.gnome_title(w.id))
                .unwrap_or_default();
            return Some(WindowInfo {
                app_name: w.wm_class.unwrap_or_default(),
                window_title: title,
            });
        }
        self.x11.as_ref()?.active()
    }

    fn meeting_candidates(&mut self) -> Vec<WindowInfo> {
        if let Some(windows) = self.gnome_windows() {
            return windows
                .into_iter()
                .filter(|w| is_meeting_candidate(w.wm_class.as_deref().unwrap_or_default()))
                .take(30)
                .filter_map(|w| {
                    let title = w.title.clone().or_else(|| self.gnome_title(w.id))?;
                    Some(WindowInfo {
                        app_name: w.wm_class.unwrap_or_default(),
                        window_title: title,
                    })
                })
                .collect();
        }
        self.x11.as_ref().map(X11::all_windows).unwrap_or_default()
    }

    fn idle_seconds(&mut self) -> Option<u64> {
        self.gnome_idle()
            .or_else(|| self.x11.as_ref()?.idle_seconds())
    }

    fn problem(&mut self) -> Option<String> {
        if self.gnome_wayland && self.gnome_windows().is_none() {
            return Some(EXTENSION_HINT.to_string());
        }
        if self.x11.is_none() && self.gnome.is_none() {
            return Some("This desktop does not expose the active window (supported: GNOME, or any X11 session).".into());
        }
        None
    }
}

struct X11 {
    conn: RustConnection,
    root: Window,
    net_active_window: Atom,
    net_wm_name: Atom,
    net_client_list: Atom,
    utf8_string: Atom,
}

impl X11 {
    fn connect() -> Option<Self> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = |name: &[u8]| -> Option<Atom> {
            Some(conn.intern_atom(false, name).ok()?.reply().ok()?.atom)
        };
        let net_active_window = atom(b"_NET_ACTIVE_WINDOW")?;
        let net_wm_name = atom(b"_NET_WM_NAME")?;
        let net_client_list = atom(b"_NET_CLIENT_LIST")?;
        let utf8_string = atom(b"UTF8_STRING")?;
        Some(Self {
            conn,
            root,
            net_active_window,
            net_wm_name,
            net_client_list,
            utf8_string,
        })
    }

    fn window_prop(
        &self,
        window: Window,
        prop: Atom,
        ty: impl Into<Atom>,
        len: u32,
    ) -> Option<Vec<u8>> {
        let reply = self
            .conn
            .get_property(false, window, prop, ty, 0, len)
            .ok()?
            .reply()
            .ok()?;
        Some(reply.value)
    }

    fn info(&self, window: Window) -> WindowInfo {
        let title = self
            .window_prop(window, self.net_wm_name, self.utf8_string, 1024)
            .filter(|v| !v.is_empty())
            .or_else(|| self.window_prop(window, AtomEnum::WM_NAME.into(), AtomEnum::STRING, 1024))
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_default();
        // WM_CLASS is "instance\0Class\0"; the class is the friendlier name.
        let class = self
            .window_prop(window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 256)
            .map(|v| {
                let parts: Vec<&[u8]> = v.split(|b| *b == 0).filter(|p| !p.is_empty()).collect();
                String::from_utf8_lossy(parts.last().copied().unwrap_or_default()).into_owned()
            })
            .unwrap_or_default();
        WindowInfo {
            app_name: class,
            window_title: title,
        }
    }

    fn active(&self) -> Option<WindowInfo> {
        let value = self.window_prop(self.root, self.net_active_window, AtomEnum::WINDOW, 1)?;
        let id = u32::from_ne_bytes(value.get(..4)?.try_into().ok()?);
        (id != 0).then(|| self.info(id))
    }

    fn all_windows(&self) -> Vec<WindowInfo> {
        let Some(value) = self.window_prop(self.root, self.net_client_list, AtomEnum::WINDOW, 1024)
        else {
            return Vec::new();
        };
        value
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_ne_bytes(*c))
            .map(|w| self.info(w))
            .filter(|w| is_meeting_candidate(&w.app_name) && !w.window_title.is_empty())
            .collect()
    }

    fn idle_seconds(&self) -> Option<u64> {
        let info = self
            .conn
            .screensaver_query_info(self.root)
            .ok()?
            .reply()
            .ok()?;
        Some(u64::from(info.ms_since_user_input) / 1000)
    }
}
