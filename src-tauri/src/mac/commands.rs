//! macOS versions of the commands the interface calls, with the same names,
//! parameters and results as on Windows (Tauri matches arguments by name, so
//! unused parameters keep their names). Commands not ported yet answer with
//! an empty result when they only read, and with an explicit error when they
//! would act, so nothing pretends to have worked.
#![allow(unused_variables)]

use crate::types::{AppInfo, AudioSessionInfo, IntRect, VolumeChangeEvent, WifiStatus};
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Window};

/// Logs a command that has no macOS implementation yet and returns the error
/// a caller sees.
fn unsupported(name: &str) -> String {
    crate::diagnostics::record("unsupported", name);
    format!("not yet implemented on macOS: {name}")
}

/// The error for a Windows feature macOS has no equivalent of (the interface
/// hides those controls on a Mac).
fn not_on_macos(name: &str) -> String {
    format!("unsupported on macOS: {name}")
}

/// Serializes writes of pinned_apps.json from the dock.
static PINNED_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn pinned_apps_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("pinned_apps.json"))
}

#[tauri::command]
pub fn save_pinned_apps(app: AppHandle, apps: Vec<AppInfo>) -> Result<(), String> {
    let path = pinned_apps_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _guard = PINNED_FILE_LOCK.lock().map_err(|e| e.to_string())?;
    if path.exists() {
        let old = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Vec<AppInfo>>(&old)
            .map_err(|e| format!("Pinned apps file is invalid; refusing to overwrite it: {e}"))?;
    }
    let content = serde_json::to_string(&apps).map_err(|e| e.to_string())?;
    crate::settings_store::atomic_write_file(&path, content.as_bytes())
}

#[tauri::command]
pub async fn load_pinned_apps(app: AppHandle) -> Vec<AppInfo> {
    pinned_apps_path(&app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) {
    if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let window = match app.get_webview_window("settings") {
        Some(window) => Ok(window),
        None => app
            .config()
            .app
            .windows
            .iter()
            .find(|config| config.label == "settings")
            .ok_or_else(|| "settings window missing from tauri.conf.json".to_string())
            .and_then(|config| {
                tauri::WebviewWindowBuilder::from_config(&app, config)
                    .and_then(|builder| builder.build())
                    .map_err(|e| e.to_string())
            }),
    };
    match window {
        Ok(window) => {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        Err(error) => crate::diagnostics::record("settings_open_failed", &error),
    }
}

#[tauri::command]
pub async fn close_settings_window(app: AppHandle) -> Result<(), String> {
    match app.get_webview_window("settings") {
        Some(window) => window.destroy().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn quit_bloom(handle: AppHandle) {
    crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    restore_native_dock(&handle);
    handle.exit(0);
}

#[tauri::command]
pub async fn restart_bloom(handle: AppHandle) {
    crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
    restore_native_dock(&handle);
    handle.restart();
}

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub async fn shelf_prune(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|p| std::path::Path::new(p).exists())
        .collect()
}

/// The main screen's notch, measured at startup (macOS only; the interface
/// asks for it only on a Mac).
pub static NOTCH: std::sync::OnceLock<Option<super::window::Notch>> = std::sync::OnceLock::new();

#[tauri::command]
pub fn get_notch() -> Option<super::window::Notch> {
    NOTCH.get().copied().flatten()
}

static INSTALLED_APPS: tokio::sync::Mutex<Option<Vec<AppInfo>>> = tokio::sync::Mutex::const_new(None);

/// Apps in the Applications folders, scanned once and kept until refreshed.
async fn installed_apps() -> Vec<AppInfo> {
    let mut cache = INSTALLED_APPS.lock().await;
    if cache.is_none() {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        let apps = tauri::async_runtime::spawn_blocking(move || {
            super::apps::scan_apps(&super::apps::app_folders(&home))
        })
        .await
        .unwrap_or_default();
        *cache = Some(apps);
    }
    cache.clone().unwrap_or_default()
}

fn clear_installed_apps() {
    if let Ok(mut cache) = INSTALLED_APPS.try_lock() {
        *cache = None;
    }
}

fn dock_enabled(app: &AppHandle) -> bool {
    crate::settings_store::get_setting_str(app, "bloom-dock-enabled").as_deref() != Some("false")
}

fn config_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok()
}

/// Shows Bloom's dock and hides the macOS Dock behind it, unless the dock is
/// turned off in settings.
pub fn show_dock(app: &AppHandle) {
    if !dock_enabled(app) {
        hide_dock(app);
        return;
    }
    if let Some(window) = app.get_webview_window("dock") {
        let _ = window.show();
    }
    if let Some(dir) = config_dir(app) {
        if let Err(error) = super::native_dock::hide(&dir) {
            crate::diagnostics::record("native_dock_hide", &error);
        }
    }
}

/// Hides Bloom's dock and gives the macOS Dock back.
pub fn hide_dock(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("dock") {
        let _ = window.hide();
    }
    restore_native_dock(app);
}

/// Puts the user's macOS Dock settings back if Bloom changed them.
pub fn restore_native_dock(app: &AppHandle) {
    if let Some(dir) = config_dir(app) {
        if let Err(error) = super::native_dock::restore(&dir) {
            crate::diagnostics::record("native_dock_restore", &error);
        }
    }
}

/// Sends a transport command to the player the island shows, off the main thread.
fn media_command(command: &str) {
    let Some(player) = super::media::current_player() else { return };
    let command = command.to_string();
    std::thread::spawn(move || {
        if let Err(error) = super::media::control(player, &command) {
            crate::diagnostics::record("media_command", &error);
        }
    });
}

/// Opens a System Settings pane by its extension id.
fn open_settings_pane(pane: &str) -> Result<(), String> {
    super::apps::open(&format!("x-apple.systempreferences:{pane}"), false)
}

// ---- Not ported yet ----

#[tauri::command]
pub fn agent_alert_status(app: AppHandle) -> Result<serde_json::Value, String> {
    Err(unsupported("agent_alert_status"))
}

#[tauri::command]
pub async fn configure_agent_alert(
    app: AppHandle,
    agent: String,
    install: bool,
) -> Result<(), String> {
    Err(unsupported("configure_agent_alert"))
}

#[tauri::command]
pub fn test_agent_alert(
    app: AppHandle,
    agent: String,
    kind: String,
    activity: Option<String>,
) -> Result<(), String> {
    Err(unsupported("test_agent_alert"))
}

#[tauri::command]
pub fn agent_alerts_quiet() -> bool {
    false
}

#[tauri::command]
pub fn hide_native_osd() {
    // macOS has no supported way to hide its volume/brightness HUD.
}

#[tauri::command]
pub fn open_wifi_settings() {
    let _ = open_settings_pane("com.apple.wifi-settings-extension");
}

#[tauri::command]
pub fn open_sound_settings() {
    let _ = open_settings_pane("com.apple.Sound-Settings.extension");
}

#[tauri::command]
pub fn open_notification_center() {
    let _ = open_settings_pane("com.apple.Notifications-Settings.extension");
}

#[tauri::command]
pub fn open_system_tray() {
    // macOS has no notification-area overflow to open.
}

#[tauri::command]
pub fn set_ignore_cursor_events(window: Window, ignore: bool) {
    let _ = window.set_ignore_cursor_events(ignore);
}

#[tauri::command]
pub fn set_window_height(window: Window, height: f64) {
    if let (Ok(scale), Ok(size)) = (window.scale_factor(), window.inner_size()) {
        let _ = window.set_size(tauri::LogicalSize::new(size.width as f64 / scale, height));
    }
}

#[tauri::command]
pub fn resize_settings_window(app: AppHandle, width: f64, height: f64) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
    }
}

#[tauri::command]
pub fn set_splash_fullscreen(app: AppHandle, fullscreen: bool) {
    crate::state::OVERLAY_IN_SPLASH.store(fullscreen, std::sync::atomic::Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = if fullscreen { window.show() } else { window.hide() };
    }
}

#[tauri::command]
pub fn sync_overlay_position(app: AppHandle) {
    // Windows are placed once at startup and on display changes.
}

#[tauri::command]
pub fn media_play_pause() {
    media_command("playpause");
}

#[tauri::command]
pub fn media_next() {
    media_command("next track");
}

#[tauri::command]
pub fn media_previous() {
    media_command("previous track");
}

#[tauri::command]
pub fn media_seek(position_ms: f64) {
    media_command(&format!("set player position to {}", (position_ms / 1000.0).max(0.0)));
}

#[tauri::command]
pub fn open_media_source_app() {
    if let Some(player) = super::media::current_player() {
        let _ = super::apps::open(player.app_name(), false);
    }
}

#[tauri::command]
pub async fn init_dock(app: AppHandle, mode: String) {
    show_dock(&app);
}

#[tauri::command]
pub async fn toggle_dock(app: AppHandle, enable: bool) {
    if enable {
        show_dock(&app);
    } else {
        hide_dock(&app);
    }
}

#[tauri::command]
pub async fn change_dock_mode(app: AppHandle, mode: String) {
    // macOS keeps its dock in smart mode: nothing can reserve screen space.
}

#[tauri::command]
pub async fn change_notch_mode(app: AppHandle, mode: String) {
    // The island reads its mode from settings; macOS reserves no space for it.
}

#[tauri::command]
pub async fn sync_appbar(app: AppHandle) {
    // macOS has no AppBars; nothing reserves screen space.
}

#[tauri::command]
pub async fn open_app(app: AppHandle, app_name: String) {
    let target = match app_name.as_str() {
        // The Windows Start button opens Launchpad's successor, the Apps view.
        "start" => "/System/Applications/Apps.app".to_string(),
        name if name.starts_with("ms-settings:") => "/System/Applications/System Settings.app".to_string(),
        _ => app_name,
    };
    if let Err(error) = super::apps::open(&target, false) {
        crate::diagnostics::record("open_app", &error);
    }
}

#[tauri::command]
pub async fn launch_new_instance(app_path: String, app_name: Option<String>) {
    if let Err(error) = super::apps::open(&app_path, true) {
        crate::diagnostics::record("launch_new_instance", &error);
    }
}

#[tauri::command]
pub async fn update_dock_rect(rect: IntRect, overlap_rect: Option<IntRect>) {
    // The dock's drawn area arrives through set_window_region.
}

#[tauri::command]
pub async fn update_notch_rect(rect: IntRect) {
    // The island's drawn area arrives through set_window_region.
}

#[tauri::command]
pub fn set_window_region(window: Window, rects: Vec<IntRect>) {
    super::interaction::set_window_rects(window.label(), rects);
}

#[tauri::command]
pub async fn set_dock_hovered(hovered: bool) {
    super::interaction::DOCK_HOVERED.store(hovered, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
pub fn set_dock_popup_active(app: AppHandle, window: Window, active: bool) {
    // Windows closes its Start menu here; macOS has nothing in the way.
}

#[tauri::command]
pub async fn set_notch_hovered(app: AppHandle, hovered: bool) {
    super::interaction::NOTCH_HOVERED.store(hovered, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
pub async fn set_notch_visible(visible: bool) {
    // Click-through follows what the island draws, so visibility needs no state.
}

#[tauri::command]
pub fn setup_shelf_drop(app: AppHandle) {
    // Tauri's own drop target works on macOS.
}

#[tauri::command]
pub async fn shelf_thumbnail(path: String) -> serde_json::Value {
    serde_json::json!({ "thumb": null, "text": null, "folder": false })
}

#[tauri::command]
pub fn shelf_drag_icon() -> Option<String> {
    Default::default()
}

#[tauri::command]
pub async fn shelf_open(path: String) -> Result<(), String> {
    super::apps::open(&path, false)
}

#[tauri::command]
pub async fn get_active_windows(app: AppHandle) -> Vec<AppInfo> {
    tauri::async_runtime::spawn_blocking(super::apps::running_apps)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn get_tray_apps() -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub async fn show_tray_context_menu(tray_id: String) -> Result<(), String> {
    Err(not_on_macos("show_tray_context_menu"))
}

#[tauri::command]
pub async fn activate_tray_icon(tray_id: String) -> Result<(), String> {
    Err(not_on_macos("activate_tray_icon"))
}

#[tauri::command]
pub async fn open_system_action(action: serde_json::Value) -> Result<(), String> {
    let action = action.as_str().unwrap_or_default().to_string();
    let args = super::apps::system_action_target(&action)
        .ok_or_else(|| format!("unsupported on macOS: {action}"))?;
    std::process::Command::new("/usr/bin/open")
        .args(args)
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| s.success().then_some(()).ok_or_else(|| format!("could not open {action}")))
}

#[tauri::command]
pub async fn get_app_icon(
    app: AppHandle,
    path: String,
    name: Option<String>,
    hwnd: Option<isize>,
) -> Result<Option<String>, String> {
    // An icon the user picked for this app wins over the app's own.
    let key = crate::custom_icons::get_cache_key(&path, name.as_deref());
    if let Ok(dir) = crate::custom_icons::get_custom_icons_dir(&app) {
        let file = dir.join(format!("{}.png", crate::custom_icons::sanitize_filename(&key)));
        if let Ok(png) = std::fs::read(&file) {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(png);
            return Ok(Some(format!("data:image/png;base64,{b64}")));
        }
    }
    tauri::async_runtime::spawn_blocking(move || Ok(super::apps::icon(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_installed_apps() -> Vec<AppInfo> {
    installed_apps().await
}

#[tauri::command]
pub fn refresh_installed_apps() {
    clear_installed_apps();
}

#[tauri::command]
pub async fn clear_icon_cache(app: AppHandle) -> Result<(), String> {
    super::apps::clear_icon_cache();
    Ok(())
}

#[tauri::command]
pub async fn set_menu_open(open: bool, rect: Option<IntRect>) {
    // Menus draw inside the dock's window region, which already takes clicks.
}

#[tauri::command]
pub async fn focus_window(app: AppHandle, hwnd: isize) -> bool {
    tauri::async_runtime::spawn_blocking(move || super::apps::focus_window(hwnd))
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn focus_app_windows(app: AppHandle, hwnds: Vec<isize>) {
    if let Some(first) = hwnds.first().copied() {
        let _ = tauri::async_runtime::spawn_blocking(move || super::apps::focus_window(first)).await;
    }
}

#[tauri::command]
pub async fn close_window(hwnd: isize) {
    let result = tauri::async_runtime::spawn_blocking(move || super::apps::close_window(hwnd)).await;
    if let Ok(Err(error)) = result {
        crate::diagnostics::record("close_window", &error);
    }
}

#[tauri::command]
pub fn get_volume() -> f32 {
    super::hardware::volume().unwrap_or(0.0)
}

#[tauri::command]
pub async fn get_volume_state() -> Result<VolumeChangeEvent, String> {
    Ok(VolumeChangeEvent { volume: super::hardware::volume()?, is_muted: super::hardware::muted()? })
}

#[tauri::command]
pub fn get_brightness() -> u32 {
    super::hardware::brightness().unwrap_or(0)
}

#[tauri::command]
pub fn set_volume(volume: f32) {
    if let Err(error) = super::hardware::set_volume(volume) {
        crate::diagnostics::record("set_volume", &error);
    }
}

#[tauri::command]
pub async fn get_audio_sessions() -> Result<Vec<AudioSessionInfo>, String> {
    // macOS has no per-app volume; the mixer stays empty.
    Ok(Vec::new())
}

#[tauri::command]
pub async fn set_app_volume(pid: u32, volume: f32) -> Result<(), String> {
    Err(not_on_macos("set_app_volume"))
}

#[tauri::command]
pub async fn set_app_mute(pid: u32, muted: bool) -> Result<(), String> {
    Err(not_on_macos("set_app_mute"))
}

#[tauri::command]
pub fn set_volume_mixer_rect(x: f64, y: f64, width: f64, height: f64) {
    // No per-app mixer on macOS.
}

#[tauri::command]
pub fn clear_volume_mixer_rect() {
    // No per-app mixer on macOS.
}

#[tauri::command]
pub async fn capture_window_thumbnail(
    hwnd: isize,
    max_width: u32,
    max_height: u32,
) -> Result<Option<(String, i64)>, String> {
    // Window previews need ScreenCaptureKit; the dock shows titles meanwhile.
    Ok(None)
}

#[tauri::command]
pub async fn get_wifi_status() -> Result<WifiStatus, String> {
    tauri::async_runtime::spawn_blocking(super::wifi::status).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_wifi_state(enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || super::wifi::set_power(enabled)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_bluetooth_state() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(super::hardware::bluetooth_powered).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_bluetooth_state(enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || super::hardware::set_bluetooth_powered(enabled)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_bluetooth_settings() {
    let _ = open_settings_pane("com.apple.BluetoothSettings");
}

#[tauri::command]
pub fn open_airplane_mode_settings() {
    // macOS has no airplane mode; the interface hides its tile on a Mac.
}

#[tauri::command]
pub fn set_brightness(app: AppHandle, brightness: u32) {
    if let Err(error) = super::hardware::set_brightness(brightness) {
        crate::diagnostics::record("set_brightness", &error);
    }
}

#[tauri::command]
pub async fn get_battery_saver_state() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(super::system::low_power).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_battery_saver_settings() {
    let _ = open_settings_pane("com.apple.Battery-Settings.extension");
}

#[tauri::command]
pub async fn get_battery_details() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(super::system::battery)
        .await
        .map_err(|e| e.to_string())
        .and_then(|d| serde_json::to_value(d).map_err(|e| e.to_string()))
}

#[tauri::command]
pub fn get_system_accent_color() -> Result<String, String> {
    Ok(super::system::accent_color())
}

#[tauri::command]
pub fn get_cpu_usage() -> Result<u32, String> {
    Ok(super::metrics::cpu_percent())
}

#[tauri::command]
pub fn get_ram_usage() -> Result<f32, String> {
    Ok(super::metrics::ram_percent())
}

#[tauri::command]
pub fn get_disk_space() -> Result<u64, String> {
    super::metrics::free_disk_gb()
}

#[tauri::command]
pub fn get_network_speed() -> Result<(u64, u64), String> {
    Ok(super::metrics::network_bytes_per_second())
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle, force: bool) -> Result<serde_json::Value, String> {
    // Updates install Windows packages; the Mac build updates from git.
    Ok(serde_json::json!({ "available": false }))
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    Err(not_on_macos("install_update"))
}

#[tauri::command]
pub fn get_update_state(app: AppHandle) -> serde_json::Value {
    serde_json::json!({ "available": false })
}

#[tauri::command]
pub async fn get_dock_extras(app: AppHandle) -> Vec<serde_json::Value> {
    let setting = |key: &str, default: &str| {
        crate::settings_store::get_setting_str(&app, key).unwrap_or_else(|| default.into())
    };
    let position = setting("bloom-dock-extras-position", "off");
    if position != "left" && position != "right" {
        return Vec::new();
    }
    let drives = setting("bloom-dock-extras-drives", "true") != "false";
    let bin = setting("bloom-dock-extras-recycle-bin", "true") != "false";
    let folders = setting("bloom-dock-extras-folders", "downloads");
    let custom = setting("bloom-dock-extras-custom-folders", "[]");
    tauri::async_runtime::spawn_blocking(move || {
        use super::extras;
        let home = extras::home();
        let mut all = Vec::new();
        if drives {
            all.extend(extras::volumes(std::path::Path::new("/Volumes")));
        }
        all.extend(extras::known_folders(&home, &folders));
        all.extend(extras::custom_folders(&custom));
        if bin {
            all.push(extras::trash(&home));
        }
        for extra in &mut all {
            extra.icon = super::apps::icon(&extra.path);
        }
        all.into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn open_dock_extra(kind: String, path: String) -> Result<(), String> {
    super::apps::open(&path, false)
}

#[tauri::command]
pub async fn eject_dock_extra(path: String) -> Result<(), String> {
    let status = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("/usr/sbin/diskutil").args(["eject", &path]).status()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| "the volume could not be ejected".to_string())
}

#[tauri::command]
pub async fn get_dock_drives() -> Vec<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(|| {
        super::extras::volumes(std::path::Path::new("/Volumes"))
            .into_iter()
            .map(|mut e| {
                e.icon = super::apps::icon(&e.path);
                e
            })
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn wifi_list_networks() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(super::wifi::list)
        .await
        .map_err(|e| e.to_string())?
        .and_then(|list| serde_json::to_value(list).map_err(|e| e.to_string()))
}

#[tauri::command]
pub async fn wifi_scan() -> Result<(), String> {
    // wifi_list_networks scans each time it is called.
    Ok(())
}

#[tauri::command]
pub async fn wifi_connect(ssid: String, password: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || super::wifi::connect(&ssid, password.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn wifi_disconnect() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(super::wifi::disconnect).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn wifi_forget(ssid: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || super::wifi::forget(&ssid)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn wifi_open_settings(page: String) -> Result<(), String> {
    open_settings_pane("com.apple.wifi-settings-extension")
}

#[tauri::command]
pub async fn bluetooth_list_devices() -> Result<Vec<serde_json::Value>, String> {
    tauri::async_runtime::spawn_blocking(super::bluetooth::devices)
        .await
        .map_err(|e| e.to_string())?
        .map(|list| list.into_iter().filter_map(|d| serde_json::to_value(d).ok()).collect())
}

#[tauri::command]
pub async fn bluetooth_set_connected(id: String, connected: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || super::bluetooth::set_connected(&id, connected))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn bluetooth_discovery(active: bool) -> Result<(), String> {
    Err(not_on_macos("bluetooth_discovery: pair new devices in System Settings"))
}

#[tauri::command]
pub async fn bluetooth_pair(id: String) -> Result<(), String> {
    Err(not_on_macos("bluetooth_pair: pair new devices in System Settings"))
}

#[tauri::command]
pub fn bluetooth_pairing_respond(accept: bool, pin: Option<String>) {
    // Pairing happens in System Settings on macOS.
}

#[tauri::command]
pub async fn bluetooth_unpair(id: String) -> Result<(), String> {
    Err(not_on_macos("bluetooth_unpair: remove devices in System Settings"))
}

#[tauri::command]
pub async fn bluetooth_refresh_buds() -> Result<(), String> {
    // Earbud levels come with every bluetooth_list_devices.
    Ok(())
}

#[tauri::command]
pub fn usb_list_devices() -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub fn jack_list_devices() -> Vec<serde_json::Value> {
    Default::default()
}
