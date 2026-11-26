//! macOS: frontmost app from NSWorkspace, window titles through the Accessibility API
//! (needs the user to allow TaskMind in Privacy & Security → Accessibility), idle from CoreGraphics.

use std::ffi::c_void;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFGetTypeID, CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringGetTypeID, CFStringRef};
use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

use super::{ActivitySource, WindowInfo, is_meeting_candidate};

type AXUIElementRef = *const c_void;
const AX_SUCCESS: i32 = 0;
/// kCGEventSourceStateHIDSystemState
const HID_SYSTEM_STATE: i32 = 1;
/// kCGAnyInputEventType
const ANY_INPUT_EVENT: u32 = u32::MAX;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
}

pub struct Source;

impl Source {
    pub fn new() -> Self {
        Self
    }
}

pub fn request_permission() {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let options =
        CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
    // SAFETY: valid CFDictionary for the call's duration.
    unsafe {
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef());
    }
}

fn trusted() -> bool {
    // SAFETY: no arguments, no side effects.
    unsafe { AXIsProcessTrusted() }
}

/// Copy an attribute; returns an owned CFType (released on drop) or None.
fn copy_attr(element: AXUIElementRef, name: &'static str) -> Option<CFType> {
    let attr = CFString::from_static_string(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: element is a live AXUIElement, attr a valid CFString, value an out-pointer.
    let err =
        unsafe { AXUIElementCopyAttributeValue(element, attr.as_concrete_TypeRef(), &mut value) };
    if err != AX_SUCCESS || value.is_null() {
        return None;
    }
    // SAFETY: "Copy" rule: we own the returned reference.
    Some(unsafe { CFType::wrap_under_create_rule(value) })
}

fn as_string(value: &CFType) -> Option<String> {
    // SAFETY: type check before the cast.
    unsafe {
        if CFGetTypeID(value.as_CFTypeRef()) != CFStringGetTypeID() {
            return None;
        }
        Some(CFString::wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef).to_string())
    }
}

struct AxApp(AXUIElementRef);

impl AxApp {
    fn new(pid: i32) -> Option<Self> {
        // SAFETY: plain constructor; returns NULL on failure.
        let el = unsafe { AXUIElementCreateApplication(pid) };
        (!el.is_null()).then_some(Self(el))
    }

    fn focused_title(&self) -> Option<String> {
        let window = copy_attr(self.0, "AXFocusedWindow")?;
        copy_attr(window.as_CFTypeRef(), "AXTitle").and_then(|t| as_string(&t))
    }

    fn window_titles(&self) -> Vec<String> {
        let Some(windows) = copy_attr(self.0, "AXWindows") else {
            return Vec::new();
        };
        // SAFETY: AXWindows is documented to be a CFArray of AXUIElements.
        let array: CFArray<CFType> =
            unsafe { CFArray::wrap_under_get_rule(windows.as_CFTypeRef() as CFArrayRef) };
        array
            .iter()
            .take(20)
            .filter_map(|w| copy_attr(w.as_CFTypeRef(), "AXTitle").and_then(|t| as_string(&t)))
            .filter(|t| !t.is_empty())
            .collect()
    }
}

impl Drop for AxApp {
    fn drop(&mut self) {
        // SAFETY: created by AXUIElementCreateApplication ("Create" rule).
        unsafe { CFRelease(self.0) };
    }
}

impl ActivitySource for Source {
    fn active(&mut self) -> Option<WindowInfo> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let app_name = app
            .localizedName()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let window_title = if trusted() {
            AxApp::new(app.processIdentifier())
                .and_then(|a| a.focused_title())
                .unwrap_or_default()
        } else {
            String::new()
        };
        Some(WindowInfo {
            app_name,
            window_title,
        })
    }

    fn meeting_candidates(&mut self) -> Vec<WindowInfo> {
        if !trusted() {
            return Vec::new();
        }
        let workspace = NSWorkspace::sharedWorkspace();
        let mut out = Vec::new();
        for app in workspace.runningApplications().iter() {
            if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
                continue;
            }
            let name = app
                .localizedName()
                .map(|n| n.to_string())
                .unwrap_or_default();
            if !is_meeting_candidate(&name) {
                continue;
            }
            if let Some(ax) = AxApp::new(app.processIdentifier()) {
                out.extend(ax.window_titles().into_iter().map(|t| WindowInfo {
                    app_name: name.clone(),
                    window_title: t,
                }));
            }
        }
        out
    }

    fn idle_seconds(&mut self) -> Option<u64> {
        // SAFETY: pure query, no pointers.
        let secs =
            unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) };
        (secs.is_finite() && secs >= 0.0).then_some(secs as u64)
    }

    fn problem(&mut self) -> Option<String> {
        (!trusted()).then(|| {
            "Window titles need Accessibility access: System Settings → Privacy & Security → Accessibility → allow TaskMind. Until then only app names are recorded.".to_string()
        })
    }
}
