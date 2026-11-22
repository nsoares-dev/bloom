//! AppKit settings for Bloom's windows: they float above the menu bar, follow
//! the user to every Space (full-screen apps included), and can sit at the
//! very top of the screen, around the notch.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use serde::Serialize;
use tauri::WebviewWindow;

/// Above the menu bar (NSMainMenuWindowLevel is 24) and the native Dock (20).
const BLOOM_WINDOW_LEVEL: isize = 25; // NSStatusWindowLevel

/// The camera housing at the top of the main screen, in points from the
/// screen's top-left corner.
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
pub struct Notch {
    pub x: f64,
    pub width: f64,
    pub height: f64,
}

fn ns_window(window: &WebviewWindow) -> Option<&NSWindow> {
    let ptr = window.ns_window().ok()?;
    // SAFETY: tao hands out the window's NSWindow, alive as long as `window`.
    unsafe { (ptr as *const NSWindow).as_ref() }
}

/// Floats the window above the menu bar on every Space and places it at
/// `y` points from the top of the main screen, `height` tall and full width.
/// Must run on the main thread.
pub fn float_and_place(window: &WebviewWindow, y: f64, height: f64) {
    let Some(mtm) = MainThreadMarker::new() else {
        crate::diagnostics::record("mac_window", "float_and_place off the main thread");
        return;
    };
    let (Some(ns_window), Some(screen)) = (ns_window(window), NSScreen::mainScreen(mtm)) else {
        return;
    };
    let frame = screen.frame();
    ns_window.setLevel(BLOOM_WINDOW_LEVEL);
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    // AppKit's origin is the bottom-left corner. setFrame:display: places the
    // window exactly, where moving it through tao keeps it under the menu bar.
    let rect = NSRect::new(
        NSPoint::new(
            frame.origin.x,
            frame.origin.y + frame.size.height - y - height,
        ),
        NSSize::new(frame.size.width, height),
    );
    ns_window.setFrame_display(rect, true);
    let placed = ns_window.frame();
    if placed != rect {
        crate::diagnostics::record(
            "mac_window",
            &format!(
                "{} asked {:?} got {:?}",
                window.label(),
                (
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height
                ),
                (
                    placed.origin.x,
                    placed.origin.y,
                    placed.size.width,
                    placed.size.height
                )
            ),
        );
    }
}

/// The main screen's notch, or None on a screen without one.
/// Must run on the main thread.
pub fn notch() -> Option<Notch> {
    let mtm = MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let height = screen.safeAreaInsets().top;
    let left = screen.auxiliaryTopLeftArea();
    let right = screen.auxiliaryTopRightArea();
    notch_between(screen.frame(), height, left, right)
}

/// The gap between the menu bar areas left and right of the camera housing.
fn notch_between(screen: NSRect, height: f64, left: NSRect, right: NSRect) -> Option<Notch> {
    if height <= 0.0 || left.size.width <= 0.0 || right.size.width <= 0.0 {
        return None;
    }
    let start = left.origin.x + left.size.width;
    let width = right.origin.x - start;
    (width > 0.0).then_some(Notch {
        x: start - screen.origin.x,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, width: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, 924.0), NSSize::new(width, 32.0))
    }

    #[test]
    fn the_notch_is_the_gap_between_the_menu_bar_areas() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1470.0, 956.0));
        assert_eq!(
            notch_between(screen, 32.0, rect(0.0, 640.0), rect(830.0, 640.0)),
            Some(Notch {
                x: 640.0,
                width: 190.0,
                height: 32.0
            })
        );
    }

    #[test]
    fn screens_without_a_notch_have_none() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1920.0, 1080.0));
        let empty = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        assert_eq!(notch_between(screen, 0.0, empty, empty), None);
    }
}
