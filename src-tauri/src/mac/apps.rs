//! Apps for Bloom's dock on macOS: running apps and their windows, icons,
//! installed apps, and opening or focusing them.
//!
//! A dock entry's `hwnd` is a CGWindowID here, the number the window server
//! gives each window; the app owning it is found from the window list.

use crate::types::AppInfo;
use base64::Engine;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_app_kit::{
    NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBitmapImageFileType,
    NSBitmapImageRep, NSRunningApplication, NSWorkspace,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ---- Window list ----

/// One on-screen, normal-level window.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowEntry {
    pub id: u32,
    pub pid: i32,
    pub title: String,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut std::ffi::c_void;
}
const ON_SCREEN_ONLY: u32 = 1 << 0;
const EXCLUDE_DESKTOP: u32 = 1 << 4;

fn number(dict: &NSDictionary<NSString, objc2::runtime::AnyObject>, key: &str) -> Option<i64> {
    let value = dict.objectForKey(&NSString::from_str(key))?;
    value.downcast::<NSNumber>().ok().map(|n| n.longLongValue())
}

/// Normal windows (layer 0) on screen, front to back. Titles are empty unless
/// Bloom has the Screen Recording permission.
pub fn windows() -> Vec<WindowEntry> {
    // SAFETY: the call returns a retained CFArray of CFDictionary, toll-free
    // bridged to NSArray<NSDictionary>; from_raw takes over that reference.
    let list: Option<Retained<NSArray<NSDictionary<NSString, objc2::runtime::AnyObject>>>> = unsafe {
        Retained::from_raw(CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0).cast())
    };
    let Some(list) = list else { return Vec::new() };
    let own_pid = std::process::id() as i64;
    list.iter()
        .filter_map(|dict| {
            let pid = number(&dict, "kCGWindowOwnerPID")?;
            if number(&dict, "kCGWindowLayer")? != 0 || pid == own_pid {
                return None;
            }
            let title = dict
                .objectForKey(&NSString::from_str("kCGWindowName"))
                .and_then(|v| v.downcast::<NSString>().ok())
                .map(|s| s.to_string())
                .unwrap_or_default();
            Some(WindowEntry { id: number(&dict, "kCGWindowNumber")? as u32, pid: pid as i32, title })
        })
        .collect()
}

fn owner_of(window_id: isize) -> Option<i32> {
    windows().into_iter().find(|w| w.id as isize == window_id).map(|w| w.pid)
}

// ---- Running apps ----

fn path_of(app: &NSRunningApplication) -> Option<String> {
    app.bundleURL()?.path().map(|p| p.to_string())
}

/// Regular apps (the ones with a Dock icon), each with its windows, in the
/// shape the dock expects. Apps without windows are included, as in the macOS
/// Dock.
pub fn running_apps() -> Vec<AppInfo> {
    let windows = windows();
    let own_pid = std::process::id() as i32;
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|app| {
            app.activationPolicy() == NSApplicationActivationPolicy::Regular
                && app.processIdentifier() != own_pid
        })
        .filter_map(|app| {
            let path = path_of(&app)?;
            let pid = app.processIdentifier();
            let all: Vec<(isize, String)> = windows
                .iter()
                .filter(|w| w.pid == pid)
                .map(|w| (w.id as isize, w.title.clone()))
                .collect();
            Some(AppInfo {
                name: app
                    .localizedName()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| app_name_from_path(&path)),
                executable: app.executableURL().and_then(|u| u.path()).map(|p| p.to_string()),
                hwnd: all.first().map(|(id, _)| *id),
                all_hwnds: (!all.is_empty()).then_some(all),
                is_running: true,
                icon: None,
                path,
            })
        })
        .collect()
}

/// A cheap summary of running apps and windows, compared to notice changes.
pub fn fingerprint() -> Vec<(i32, u32)> {
    let mut ids: Vec<(i32, u32)> = windows().into_iter().map(|w| (w.pid, w.id)).collect();
    ids.extend(
        NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
            .map(|a| (a.processIdentifier(), 0)),
    );
    ids.sort_unstable();
    ids
}

/// Brings the app owning `window_id` to the front, that window on top when
/// Bloom may use Accessibility.
pub fn focus_window(window_id: isize) -> bool {
    let Some(pid) = owner_of(window_id) else { return false };
    let _ = super::ax::raise(pid, window_id as u32);
    activate_pid(pid)
}

/// Closes one window by pressing its close button.
pub fn close_window(window_id: isize) -> Result<(), String> {
    let pid = owner_of(window_id).ok_or("window not found")?;
    super::ax::close(pid, window_id as u32)
}

pub fn activate_pid(pid: i32) -> bool {
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows))
}

// ---- Opening apps ----

/// Opens a path or URL, or an app by name.
pub fn open(target: &str, new_instance: bool) -> Result<(), String> {
    let mut command = std::process::Command::new("/usr/bin/open");
    if new_instance {
        command.arg("-n");
    }
    // Paths and URLs (x-apple.systempreferences:…) open as they are; a bare
    // name is an app name.
    if !target.starts_with('/') && !target.contains(':') {
        command.arg("-a");
    }
    let status = command.arg(target).status().map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("could not open {target}"))
}

// ---- Installed apps ----

pub fn app_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Folders scanned for apps, the user's own first.
pub fn app_folders(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Applications"),
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ]
}

/// Every `.app` directly inside `folders` or one folder below (vendor folders
/// such as /Applications/Microsoft Office), sorted by name, without
/// duplicates.
pub fn scan_apps(folders: &[PathBuf]) -> Vec<AppInfo> {
    let mut found: Vec<PathBuf> = Vec::new();
    let is_app = |p: &Path| p.extension().is_some_and(|e| e == "app");
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_app(&path) {
                found.push(path);
            } else if path.is_dir() {
                if let Ok(inner) = std::fs::read_dir(&path) {
                    found.extend(inner.flatten().map(|e| e.path()).filter(|p| is_app(p)));
                }
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut apps: Vec<AppInfo> = found
        .into_iter()
        .filter(|p| seen.insert(p.file_name().map(|n| n.to_owned())))
        .map(|p| {
            let path = p.to_string_lossy().into_owned();
            AppInfo {
                name: app_name_from_path(&path),
                path,
                icon: None,
                is_running: false,
                hwnd: None,
                executable: None,
                all_hwnds: None,
            }
        })
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

// ---- Icons ----

static ICON_CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// The Finder icon of `path` as a 128-point PNG data URI, cached per path.
pub fn icon(path: &str) -> Option<String> {
    if let Some(hit) = ICON_CACHE.lock().ok()?.get_or_insert_with(HashMap::new).get(path) {
        return Some(hit.clone());
    }
    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    image.setSize(NSSize::new(128.0, 128.0));
    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(128.0, 128.0));
    // SAFETY: `rect` outlives the call; no context or hints are passed.
    let cg = unsafe { image.CGImageForProposedRect_context_hints(&mut rect, None, None) }?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
    // SAFETY: an empty properties dictionary is valid for PNG.
    let png = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    let uri = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png.to_vec())
    );
    ICON_CACHE.lock().ok()?.get_or_insert_with(HashMap::new).insert(path.to_string(), uri.clone());
    Some(uri)
}

/// Drops one cached icon, so a removed custom icon gives way to the app's own.
pub fn forget_icon(path: &str) {
    if let Ok(mut cache) = ICON_CACHE.lock() {
        if let Some(cache) = cache.as_mut() {
            cache.remove(path);
        }
    }
}

pub fn clear_icon_cache() {
    if let Ok(mut cache) = ICON_CACHE.lock() {
        *cache = None;
    }
}

// ---- System actions ----

/// What a Windows system shortcut opens on macOS (`open` arguments), or None
/// when macOS has nothing like it.
pub fn system_action_target(action: &str) -> Option<&'static [&'static str]> {
    Some(match action {
        "task-manager" => &["-a", "Activity Monitor"],
        "disk-management" => &["-a", "Disk Utility"],
        "device-manager" | "computer-management" => &["-a", "System Information"],
        "settings" => &["-a", "System Settings"],
        "taskbar-settings" => &["x-apple.systempreferences:com.apple.preference.dock"],
        "installed-apps" => &["/Applications"],
        "power-options" => &["x-apple.systempreferences:com.apple.Battery-Settings.extension"],
        "network-connections" => &["x-apple.systempreferences:com.apple.Network-Settings.extension"],
        "task-view" => &["-a", "Mission Control"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bloom-apps-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_finds_apps_and_apps_one_folder_down_without_duplicates() {
        let a = temp_dir("a");
        let b = temp_dir("b");
        std::fs::create_dir_all(a.join("Zed.app")).unwrap();
        std::fs::create_dir_all(a.join("Vendor/Word.app")).unwrap();
        std::fs::create_dir_all(a.join("Vendor/Deeper/Hidden.app")).unwrap();
        std::fs::create_dir_all(a.join("notes")).unwrap();
        std::fs::create_dir_all(b.join("Zed.app")).unwrap();
        std::fs::create_dir_all(b.join("Calculator.app")).unwrap();
        let apps = scan_apps(&[a.clone(), b.clone()]);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Calculator", "Word", "Zed"]);
        // The first folder wins for duplicates (the user's own Applications).
        assert!(apps[2].path.starts_with(a.to_str().unwrap()));
        assert!(apps.iter().all(|a| !a.is_running && a.hwnd.is_none()));
    }

    #[test]
    fn app_names_drop_the_bundle_extension() {
        assert_eq!(app_name_from_path("/Applications/Visual Studio Code.app"), "Visual Studio Code");
    }

    #[test]
    fn system_actions_map_to_mac_equivalents() {
        assert_eq!(system_action_target("task-manager"), Some(&["-a", "Activity Monitor"][..]));
        assert_eq!(system_action_target("task-view"), Some(&["-a", "Mission Control"][..]));
        assert_eq!(system_action_target("unknown"), None);
    }
}

#[cfg(test)]
mod live {
    /// Talks to the real window server; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn lists_running_apps_and_encodes_an_icon() {
        let apps = super::running_apps();
        assert!(apps.iter().any(|a| a.name == "Finder"), "{apps:?}");
        let icon = super::icon("/System/Applications/Calculator.app").unwrap();
        assert!(icon.starts_with("data:image/png;base64,") && icon.len() > 1000);
        println!("{} apps, {} windows", apps.len(), super::windows().len());
    }
}
