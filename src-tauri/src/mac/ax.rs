//! Per-window actions through the Accessibility API: raising one window and
//! pressing its close button. Needs the Accessibility permission; without it
//! every call reports that instead of failing silently.

use objc2_foundation::NSString;
use std::ffi::c_void;

type AXUIElementRef = *const c_void;
type CFTypeRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFTypeRef) -> i32;
    fn _AXUIElementGetWindow(element: AXUIElementRef, window: *mut u32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: CFTypeRef);
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
}

const NOT_TRUSTED: &str = "Bloom needs the Accessibility permission for this";

pub fn trusted() -> bool {
    // SAFETY: no arguments; reads this process's trust state.
    unsafe { AXIsProcessTrusted() }
}

fn cf(s: &NSString) -> CFTypeRef {
    // NSString is toll-free bridged to CFString.
    (s as *const NSString).cast()
}

/// Runs `f` on the accessibility element of window `window_id` of `pid`.
fn with_window<T>(pid: i32, window_id: u32, f: impl FnOnce(AXUIElementRef) -> T) -> Result<T, String> {
    if !trusted() {
        return Err(NOT_TRUSTED.into());
    }
    // SAFETY: every element and array obtained here is released before
    // returning; `window` is borrowed from `windows` while it is alive.
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return Err("no such app".into());
        }
        let mut windows: CFTypeRef = std::ptr::null();
        let attribute = NSString::from_str("AXWindows");
        let status = AXUIElementCopyAttributeValue(app, cf(&attribute), &mut windows);
        CFRelease(app);
        if status != 0 || windows.is_null() {
            return Err(format!("could not list the app's windows ({status})"));
        }
        let mut result = Err("window not found".to_string());
        for i in 0..CFArrayGetCount(windows) {
            let window = CFArrayGetValueAtIndex(windows, i);
            let mut id = 0u32;
            if _AXUIElementGetWindow(window, &mut id) == 0 && id == window_id {
                result = Ok(f(window));
                break;
            }
        }
        CFRelease(windows);
        result
    }
}

/// Brings one window above its app's other windows.
pub fn raise(pid: i32, window_id: u32) -> Result<(), String> {
    with_window(pid, window_id, |window| {
        let action = NSString::from_str("AXRaise");
        // SAFETY: `window` is valid for the duration of the closure.
        unsafe { AXUIElementPerformAction(window, cf(&action)) }
    })
    .and_then(|status| if status == 0 { Ok(()) } else { Err(format!("raise failed ({status})")) })
}

/// Presses the window's close button, as a click on it would.
pub fn close(pid: i32, window_id: u32) -> Result<(), String> {
    with_window(pid, window_id, |window| unsafe {
        // SAFETY: `window` is valid for the duration of the closure; the
        // button copied out of it is released after pressing.
        let mut button: CFTypeRef = std::ptr::null();
        let attribute = NSString::from_str("AXCloseButton");
        if AXUIElementCopyAttributeValue(window, cf(&attribute), &mut button) != 0 || button.is_null() {
            return -1;
        }
        let press = NSString::from_str("AXPress");
        let status = AXUIElementPerformAction(button, cf(&press));
        CFRelease(button);
        status
    })
    .and_then(|status| if status == 0 { Ok(()) } else { Err(format!("close failed ({status})")) })
}
