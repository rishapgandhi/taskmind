//! Windows: foreground window title and process name, idle from GetLastInputInfo.

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::{BOOL, PWSTR};

use super::{ActivitySource, WindowInfo, is_meeting_candidate};

pub struct Source;

impl Source {
    pub fn new() -> Self {
        Self
    }
}

pub fn request_permission() {}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: hwnd may be stale; these calls then return 0 and we return "".
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

/// Executable name without extension, e.g. "chrome", "Teams", "Code".
fn process_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    // SAFETY: out-pointer to a local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    // SAFETY: handle closed below; buffer and size are consistent.
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if ok.is_err() {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the &mut Vec passed by `meeting_candidates`, alive during EnumWindows.
    let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    // SAFETY: plain query.
    if unsafe { IsWindowVisible(hwnd) }.as_bool() && out.len() < 500 {
        out.push(hwnd);
    }
    BOOL(1)
}

impl ActivitySource for Source {
    fn active(&mut self) -> Option<WindowInfo> {
        // SAFETY: plain query.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return None;
        }
        Some(WindowInfo {
            app_name: process_name(hwnd),
            window_title: window_title(hwnd),
        })
    }

    fn meeting_candidates(&mut self) -> Vec<WindowInfo> {
        let mut hwnds: Vec<HWND> = Vec::new();
        // SAFETY: callback only touches `hwnds` through lparam during this call.
        let _ =
            unsafe { EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut Vec<HWND> as isize)) };
        hwnds
            .into_iter()
            .filter_map(|h| {
                let title = window_title(h);
                if title.is_empty() {
                    return None;
                }
                let app = process_name(h);
                is_meeting_candidate(&app).then_some(WindowInfo {
                    app_name: app,
                    window_title: title,
                })
            })
            .collect()
    }

    fn idle_seconds(&mut self) -> Option<u64> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: cbSize set as required.
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        // SAFETY: plain query. Both are 32-bit tick counts, so wrapping_sub handles the 49-day wrap.
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
    }

    fn problem(&mut self) -> Option<String> {
        None
    }
}
