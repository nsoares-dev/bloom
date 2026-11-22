//! Which of Bloom's windows take the mouse, and when the island and dock
//! reveal from a screen edge. macOS has no window regions, so every Bloom
//! window stays click-through except while the cursor is over what it draws
//! (the rectangles each webview reports through `set_window_region`).

use crate::types::IntRect;

/// Height of the strip along the top and bottom screen edges that reveals the
/// island and dock, in points (same as Windows).
pub const EDGE_BAND: f64 = 8.0;
/// Default dwell before an edge reveal, in milliseconds (same as Windows).
pub const EDGE_DWELL_MS: i64 = 200;
/// Extra reach beside the island along the top edge, so moving along the top
/// toward it doesn't flicker click-through (same as Windows).
const TOP_SPAN_PAD: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// A window's top-left corner on screen and the parts of it that draw, in the
/// window's own coordinates. All values are in points.
#[derive(Clone, Debug, Default)]
pub struct WindowRegion {
    pub origin: (f64, f64),
    pub rects: Vec<IntRect>,
}

impl WindowRegion {
    fn screen_rects(&self) -> impl Iterator<Item = Rect> + '_ {
        self.rects
            .iter()
            .filter(|r| r.width > 0 && r.height > 0)
            .map(|r| Rect {
                x: self.origin.0 + r.x as f64,
                y: self.origin.1 + r.y as f64,
                width: r.width as f64,
                height: r.height as f64,
            })
    }

    fn contains(&self, point: (f64, f64)) -> bool {
        self.screen_rects().any(|r| r.contains(point))
    }

    /// Horizontal span of everything the window draws.
    fn span(&self) -> Option<(f64, f64)> {
        self.screen_rects().fold(None, |span, r| {
            let (left, right) = span.unwrap_or((r.x, r.x + r.width));
            Some((left.min(r.x), right.max(r.x + r.width)))
        })
    }
}

/// Everything one decision looks at.
#[derive(Clone, Debug)]
pub struct Frame {
    pub cursor: (f64, f64),
    pub screen: Rect,
    pub main: WindowRegion,
    pub dock: WindowRegion,
    pub overlay: WindowRegion,
    /// The island's webview says the pointer is over it.
    pub notch_hovered: bool,
    /// The dock's webview says the pointer is over it.
    pub dock_hovered: bool,
    pub now_ms: i64,
    pub top_dwell_ms: i64,
    pub bottom_dwell_ms: i64,
}

/// Dwell progress along the top and bottom edges, carried between frames.
#[derive(Clone, Debug, Default)]
pub struct Edges {
    top_enter_ms: Option<i64>,
    top_armed: bool,
    bottom_enter_ms: Option<i64>,
    bottom_armed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decision {
    pub main_interactive: bool,
    pub dock_interactive: bool,
    pub overlay_interactive: bool,
    pub notch_edge_hover: bool,
    pub dock_edge_hover: bool,
}

/// Arms after the cursor has rested in a band for `dwell_ms`; leaving resets.
fn dwell(
    in_band: bool,
    enter: &mut Option<i64>,
    armed: &mut bool,
    now: i64,
    dwell_ms: i64,
) -> bool {
    if !in_band {
        *enter = None;
        *armed = false;
        return false;
    }
    if !*armed {
        let since = *enter.get_or_insert(now);
        *armed = dwell_ms <= 0 || now - since >= dwell_ms;
    }
    *armed
}

pub fn decide(edges: &mut Edges, frame: &Frame) -> Decision {
    let (x, y) = frame.cursor;
    let screen = frame.screen;
    let on_screen = x >= screen.x && x < screen.x + screen.width;
    let in_top = on_screen && y >= screen.y && y <= screen.y + EDGE_BAND;
    let bottom = screen.y + screen.height;
    let in_bottom = on_screen && y >= bottom - EDGE_BAND && y <= bottom;

    let top_armed = dwell(
        in_top,
        &mut edges.top_enter_ms,
        &mut edges.top_armed,
        frame.now_ms,
        frame.top_dwell_ms,
    );
    let bottom_armed = dwell(
        in_bottom,
        &mut edges.bottom_enter_ms,
        &mut edges.bottom_armed,
        frame.now_ms,
        frame.bottom_dwell_ms,
    );

    let notch_edge_hover = top_armed || frame.notch_hovered;
    let dock_edge_hover = bottom_armed || frame.dock_hovered;
    let near_island = frame
        .main
        .span()
        .is_some_and(|(left, right)| x >= left - TOP_SPAN_PAD && x <= right + TOP_SPAN_PAD);

    Decision {
        main_interactive: frame.main.contains(frame.cursor)
            || (notch_edge_hover && in_top && near_island),
        dock_interactive: frame.dock.contains(frame.cursor) || (dock_edge_hover && in_bottom),
        overlay_interactive: frame.overlay.contains(frame.cursor),
        notch_edge_hover,
        dock_edge_hover,
    }
}


// ---- Runtime: shared state fed by the webviews, and the cursor worker ----

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub static NOTCH_HOVERED: AtomicBool = AtomicBool::new(false);
pub static DOCK_HOVERED: AtomicBool = AtomicBool::new(false);

struct Layout {
    screen: Rect,
    /// Window label → top-left corner on screen, in points.
    origins: HashMap<String, (f64, f64)>,
    /// Window label → rectangles it draws, in its own coordinates.
    regions: HashMap<String, Vec<IntRect>>,
}

static LAYOUT: Mutex<Option<Layout>> = Mutex::new(None);

/// Records where a window sits (called when windows are placed).
pub fn set_window_origin(label: &str, screen: Rect, origin: (f64, f64)) {
    if let Ok(mut layout) = LAYOUT.lock() {
        let layout = layout.get_or_insert_with(|| Layout {
            screen,
            origins: HashMap::new(),
            regions: HashMap::new(),
        });
        layout.screen = screen;
        layout.origins.insert(label.to_string(), origin);
    }
}

/// Records what a window draws (the `set_window_region` command).
pub fn set_window_rects(label: &str, rects: Vec<IntRect>) {
    if let Ok(mut layout) = LAYOUT.lock() {
        if let Some(layout) = layout.as_mut() {
            layout.regions.insert(label.to_string(), rects);
        }
    }
}

fn region(layout: &Layout, label: &str) -> WindowRegion {
    WindowRegion {
        origin: layout.origins.get(label).copied().unwrap_or_default(),
        rects: layout.regions.get(label).cloned().unwrap_or_default(),
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: *const std::ffi::c_void);
}

/// The cursor in points from the main screen's top-left corner. Needs no
/// permission, unlike a global event monitor.
fn cursor_position() -> Option<(f64, f64)> {
    // SAFETY: a null-source event is a snapshot of the current state; it is
    // released right after reading its location.
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Some((point.x, point.y))
    }
}

fn dwell_setting(app: &AppHandle, key: &str) -> i64 {
    crate::settings_store::get_setting_str(app, key)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(EDGE_DWELL_MS)
        .clamp(0, 2000)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Polls the cursor 30 times a second and applies `decide`: click-through per
/// window, and the edge-hover events the island and dock listen for. Only
/// changes reach the windows and webviews.
pub fn spawn_worker(app: AppHandle) {
    std::thread::Builder::new()
        .name("bloom-cursor-worker".into())
        .spawn(move || {
            let mut edges = Edges::default();
            let mut last: Option<Decision> = None;
            while !crate::state::SHUTTING_DOWN.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(32));
                let Some(cursor) = cursor_position() else {
                    continue;
                };
                let frame = {
                    let Ok(layout) = LAYOUT.lock() else { continue };
                    let Some(layout) = layout.as_ref() else {
                        continue;
                    };
                    Frame {
                        cursor,
                        screen: layout.screen,
                        main: region(layout, "main"),
                        dock: region(layout, "dock"),
                        overlay: region(layout, "overlay"),
                        notch_hovered: NOTCH_HOVERED.load(Ordering::Relaxed),
                        dock_hovered: DOCK_HOVERED.load(Ordering::Relaxed),
                        now_ms: now_ms(),
                        top_dwell_ms: dwell_setting(&app, "bloom-notch-edge-delay"),
                        bottom_dwell_ms: dwell_setting(&app, "bloom-dock-edge-delay"),
                    }
                };
                let decision = decide(&mut edges, &frame);
                apply(&app, last, decision);
                last = Some(decision);
            }
        })
        .expect("failed to start the cursor worker");
}

fn apply(app: &AppHandle, last: Option<Decision>, now: Decision) {
    let changed = |f: fn(&Decision) -> bool| last.map(|l| f(&l)) != Some(f(&now));
    for (label, interactive) in [
        (
            "main",
            (|d: &Decision| d.main_interactive) as fn(&Decision) -> bool,
        ),
        ("dock", |d: &Decision| d.dock_interactive),
        ("overlay", |d: &Decision| d.overlay_interactive),
    ] {
        // Setup owns the overlay's input while it runs.
        if label == "overlay" && crate::state::ONBOARDING_ACTIVE.load(Ordering::Relaxed) {
            continue;
        }
        if changed(interactive) {
            if let Some(window) = app.get_webview_window(label) {
                let _ = window.set_ignore_cursor_events(!interactive(&now));
            }
        }
    }
    if changed(|d| d.notch_edge_hover) {
        let _ = app.emit("notch-edge-hover", now.notch_edge_hover);
    }
    if changed(|d| d.dock_edge_hover) {
        let _ = app.emit("dock-edge-hover", now.dock_edge_hover);
    }
    // A click-through webview never sees the pointer leave; tell the island.
    if !now.main_interactive && NOTCH_HOVERED.swap(false, Ordering::Relaxed) {
        let _ = app.emit("notch-pointer-left", ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> IntRect {
        IntRect {
            x,
            y,
            width,
            height,
        }
    }

    /// A 1470x956 screen with the island drawing a 200x40 pill at the top
    /// center and the dock window (600 tall) drawing a 500x70 bar at its bottom.
    fn frame(cursor: (f64, f64), now_ms: i64) -> Frame {
        Frame {
            cursor,
            screen: Rect {
                x: 0.0,
                y: 0.0,
                width: 1470.0,
                height: 956.0,
            },
            main: WindowRegion {
                origin: (0.0, 0.0),
                rects: vec![rect(635, 0, 200, 40)],
            },
            dock: WindowRegion {
                origin: (0.0, 356.0),
                rects: vec![rect(485, 520, 500, 70)],
            },
            overlay: WindowRegion::default(),
            notch_hovered: false,
            dock_hovered: false,
            now_ms,
            top_dwell_ms: EDGE_DWELL_MS,
            bottom_dwell_ms: EDGE_DWELL_MS,
        }
    }

    #[test]
    fn clicks_pass_through_where_nothing_is_drawn() {
        let decision = decide(&mut Edges::default(), &frame((300.0, 500.0), 0));
        assert_eq!(decision, Decision::default());
    }

    #[test]
    fn a_window_takes_the_mouse_over_what_it_draws() {
        let decision = decide(&mut Edges::default(), &frame((700.0, 20.0), 0));
        assert!(decision.main_interactive);
        assert!(!decision.dock_interactive);

        // Dock rect sits at y 356+520 = 876 on screen.
        let decision = decide(&mut Edges::default(), &frame((700.0, 900.0), 0));
        assert!(decision.dock_interactive);
        assert!(!decision.main_interactive);
    }

    #[test]
    fn the_top_edge_reveals_the_island_only_after_the_dwell() {
        let mut edges = Edges::default();
        assert!(!decide(&mut edges, &frame((100.0, 2.0), 1000)).notch_edge_hover);
        assert!(!decide(&mut edges, &frame((100.0, 2.0), 1199)).notch_edge_hover);
        assert!(decide(&mut edges, &frame((100.0, 2.0), 1200)).notch_edge_hover);
    }

    #[test]
    fn leaving_the_edge_resets_the_dwell() {
        let mut edges = Edges::default();
        decide(&mut edges, &frame((100.0, 2.0), 1000));
        decide(&mut edges, &frame((100.0, 300.0), 1100));
        assert!(!decide(&mut edges, &frame((100.0, 2.0), 1250)).notch_edge_hover);
        assert!(decide(&mut edges, &frame((100.0, 2.0), 1450)).notch_edge_hover);
    }

    #[test]
    fn a_zero_delay_reveals_at_once() {
        let mut f = frame((100.0, 2.0), 0);
        f.top_dwell_ms = 0;
        assert!(decide(&mut Edges::default(), &f).notch_edge_hover);
    }

    #[test]
    fn the_bottom_edge_reveals_the_dock_and_takes_its_clicks() {
        let mut edges = Edges::default();
        decide(&mut edges, &frame((100.0, 954.0), 0));
        let decision = decide(&mut edges, &frame((100.0, 954.0), 250));
        assert!(decision.dock_edge_hover);
        assert!(decision.dock_interactive);
    }

    #[test]
    fn the_island_keeps_its_hover_while_its_webview_reports_one() {
        let mut f = frame((700.0, 60.0), 0);
        f.notch_hovered = true;
        let decision = decide(&mut Edges::default(), &f);
        assert!(decision.notch_edge_hover);
        // Below the pill and off the top edge, clicks still pass through.
        assert!(!decision.main_interactive);
    }

    #[test]
    fn the_top_edge_near_the_island_takes_clicks_once_armed() {
        let mut edges = Edges::default();
        decide(&mut edges, &frame((600.0, 1.0), 0));
        let decision = decide(&mut edges, &frame((600.0, 1.0), 250));
        assert!(decision.main_interactive);
        // Far from the island the edge reveals it but never blocks clicks.
        let mut edges = Edges::default();
        decide(&mut edges, &frame((50.0, 1.0), 0));
        let decision = decide(&mut edges, &frame((50.0, 1.0), 250));
        assert!(decision.notch_edge_hover && !decision.main_interactive);
    }
}
