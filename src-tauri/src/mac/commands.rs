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
    handle.exit(0);
}

#[tauri::command]
pub async fn restart_bloom(handle: AppHandle) {
    crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
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
    unsupported("open_wifi_settings");
}

#[tauri::command]
pub fn open_sound_settings() {
    unsupported("open_sound_settings");
}

#[tauri::command]
pub fn open_notification_center() {
    unsupported("open_notification_center");
}

#[tauri::command]
pub fn open_system_tray() {
    unsupported("open_system_tray");
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
    unsupported("resize_settings_window");
}

#[tauri::command]
pub fn set_splash_fullscreen(app: AppHandle, fullscreen: bool) {
    unsupported("set_splash_fullscreen");
}

#[tauri::command]
pub fn get_onboarding_completed(app: AppHandle) -> Result<bool, String> {
    Err(unsupported("get_onboarding_completed"))
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle, window: Window) -> Result<(), String> {
    Err(unsupported("complete_onboarding"))
}

#[tauri::command]
pub fn open_onboarding(app: AppHandle) -> Result<(), String> {
    Err(unsupported("open_onboarding"))
}

#[tauri::command]
pub fn set_onboarding_active(app: AppHandle, window: Window, active: bool) -> Result<(), String> {
    Err(unsupported("set_onboarding_active"))
}

#[tauri::command]
pub fn sync_overlay_position(app: AppHandle) {
    // Windows are placed once at startup and on display changes.
}

#[tauri::command]
pub fn media_play_pause() {
    unsupported("media_play_pause");
}

#[tauri::command]
pub fn media_next() {
    unsupported("media_next");
}

#[tauri::command]
pub fn media_previous() {
    unsupported("media_previous");
}

#[tauri::command]
pub fn media_seek(position_ms: f64) {
    unsupported("media_seek");
}

#[tauri::command]
pub fn open_media_source_app() {
    unsupported("open_media_source_app");
}

#[tauri::command]
pub async fn init_dock(app: AppHandle, mode: String) {
    unsupported("init_dock");
}

#[tauri::command]
pub async fn toggle_dock(app: AppHandle, enable: bool) {
    unsupported("toggle_dock");
}

#[tauri::command]
pub async fn change_dock_mode(app: AppHandle, mode: String) {
    unsupported("change_dock_mode");
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
    unsupported("open_app");
}

#[tauri::command]
pub async fn launch_new_instance(app_path: String, app_name: Option<String>) {
    unsupported("launch_new_instance");
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
    unsupported("set_dock_popup_active");
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
    unsupported("setup_shelf_drop");
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
    Err(unsupported("shelf_open"))
}

#[tauri::command]
pub async fn get_active_windows(app: AppHandle) -> Vec<AppInfo> {
    Default::default()
}

#[tauri::command]
pub async fn get_tray_apps() -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub async fn show_tray_context_menu(tray_id: String) -> Result<(), String> {
    Err(unsupported("show_tray_context_menu"))
}

#[tauri::command]
pub async fn activate_tray_icon(tray_id: String) -> Result<(), String> {
    Err(unsupported("activate_tray_icon"))
}

#[tauri::command]
pub async fn open_system_action(action: serde_json::Value) -> Result<(), String> {
    Err(unsupported("open_system_action"))
}

#[tauri::command]
pub async fn get_app_icon(
    app: AppHandle,
    path: String,
    name: Option<String>,
    hwnd: Option<isize>,
) -> Result<Option<String>, String> {
    Err(unsupported("get_app_icon"))
}

#[tauri::command]
pub async fn get_installed_apps() -> Vec<AppInfo> {
    Default::default()
}

#[tauri::command]
pub fn refresh_installed_apps() {
    unsupported("refresh_installed_apps");
}

#[tauri::command]
pub async fn clear_icon_cache(app: AppHandle) -> Result<(), String> {
    Err(unsupported("clear_icon_cache"))
}

#[tauri::command]
pub async fn set_custom_icon(
    app: AppHandle,
    cache_key: String,
    icon_data: String,
) -> Result<String, String> {
    Err(unsupported("set_custom_icon"))
}

#[tauri::command]
pub async fn remove_custom_icon(
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> Result<(), String> {
    Err(unsupported("remove_custom_icon"))
}

#[tauri::command]
pub async fn get_custom_icons(app: AppHandle) -> Result<HashMap<String, String>, String> {
    Err(unsupported("get_custom_icons"))
}

#[tauri::command]
pub async fn set_menu_open(open: bool, rect: Option<IntRect>) {
    // Menus draw inside the dock's window region, which already takes clicks.
}

#[tauri::command]
pub async fn focus_window(app: AppHandle, hwnd: isize) -> bool {
    false
}

#[tauri::command]
pub async fn focus_app_windows(app: AppHandle, hwnds: Vec<isize>) {
    unsupported("focus_app_windows");
}

#[tauri::command]
pub async fn close_window(hwnd: isize) {
    unsupported("close_window");
}

#[tauri::command]
pub fn get_volume() -> f32 {
    0.0
}

#[tauri::command]
pub async fn get_volume_state() -> Result<VolumeChangeEvent, String> {
    Err(unsupported("get_volume_state"))
}

#[tauri::command]
pub fn get_brightness() -> u32 {
    0
}

#[tauri::command]
pub fn set_volume(volume: f32) {
    unsupported("set_volume");
}

#[tauri::command]
pub async fn get_audio_sessions() -> Result<Vec<AudioSessionInfo>, String> {
    Err(unsupported("get_audio_sessions"))
}

#[tauri::command]
pub async fn set_app_volume(pid: u32, volume: f32) -> Result<(), String> {
    Err(unsupported("set_app_volume"))
}

#[tauri::command]
pub async fn set_app_mute(pid: u32, muted: bool) -> Result<(), String> {
    Err(unsupported("set_app_mute"))
}

#[tauri::command]
pub fn set_volume_mixer_rect(x: f64, y: f64, width: f64, height: f64) {
    unsupported("set_volume_mixer_rect");
}

#[tauri::command]
pub fn clear_volume_mixer_rect() {
    unsupported("clear_volume_mixer_rect");
}

#[tauri::command]
pub async fn capture_window_thumbnail(
    hwnd: isize,
    max_width: u32,
    max_height: u32,
) -> Result<Option<(String, i64)>, String> {
    Err(unsupported("capture_window_thumbnail"))
}

#[tauri::command]
pub async fn get_wifi_status() -> Result<WifiStatus, String> {
    Err(unsupported("get_wifi_status"))
}

#[tauri::command]
pub async fn set_wifi_state(enabled: bool) -> Result<(), String> {
    Err(unsupported("set_wifi_state"))
}

#[tauri::command]
pub async fn get_bluetooth_state() -> Result<bool, String> {
    Err(unsupported("get_bluetooth_state"))
}

#[tauri::command]
pub async fn set_bluetooth_state(enabled: bool) -> Result<(), String> {
    Err(unsupported("set_bluetooth_state"))
}

#[tauri::command]
pub fn open_bluetooth_settings() {
    unsupported("open_bluetooth_settings");
}

#[tauri::command]
pub fn open_airplane_mode_settings() {
    unsupported("open_airplane_mode_settings");
}

#[tauri::command]
pub fn set_brightness(app: AppHandle, brightness: u32) {
    unsupported("set_brightness");
}

#[tauri::command]
pub async fn get_battery_saver_state() -> Result<bool, String> {
    Err(unsupported("get_battery_saver_state"))
}

#[tauri::command]
pub fn open_battery_saver_settings() {
    unsupported("open_battery_saver_settings");
}

#[tauri::command]
pub async fn get_battery_details() -> Result<serde_json::Value, String> {
    Err(unsupported("get_battery_details"))
}

#[tauri::command]
pub fn get_system_accent_color() -> Result<String, String> {
    Err(unsupported("get_system_accent_color"))
}

#[tauri::command]
pub fn get_cpu_usage() -> Result<u32, String> {
    Err(unsupported("get_cpu_usage"))
}

#[tauri::command]
pub fn get_ram_usage() -> Result<f32, String> {
    Err(unsupported("get_ram_usage"))
}

#[tauri::command]
pub fn get_disk_space() -> Result<u64, String> {
    Err(unsupported("get_disk_space"))
}

#[tauri::command]
pub fn get_network_speed() -> Result<(u64, u64), String> {
    Err(unsupported("get_network_speed"))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle, force: bool) -> Result<serde_json::Value, String> {
    Err(unsupported("check_for_updates"))
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    Err(unsupported("install_update"))
}

#[tauri::command]
pub fn get_update_state(app: AppHandle) -> serde_json::Value {
    serde_json::json!({ "available": false })
}

#[tauri::command]
pub async fn get_dock_extras(app: AppHandle) -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub async fn open_dock_extra(kind: String, path: String) -> Result<(), String> {
    Err(unsupported("open_dock_extra"))
}

#[tauri::command]
pub async fn eject_dock_extra(path: String) -> Result<(), String> {
    Err(unsupported("eject_dock_extra"))
}

#[tauri::command]
pub async fn get_dock_drives() -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub async fn wifi_list_networks() -> Result<serde_json::Value, String> {
    Err(unsupported("wifi_list_networks"))
}

#[tauri::command]
pub async fn wifi_scan() -> Result<(), String> {
    Err(unsupported("wifi_scan"))
}

#[tauri::command]
pub async fn wifi_connect(ssid: String, password: Option<String>) -> Result<(), String> {
    Err(unsupported("wifi_connect"))
}

#[tauri::command]
pub async fn wifi_disconnect() -> Result<(), String> {
    Err(unsupported("wifi_disconnect"))
}

#[tauri::command]
pub async fn wifi_forget(ssid: String) -> Result<(), String> {
    Err(unsupported("wifi_forget"))
}

#[tauri::command]
pub fn wifi_open_settings(page: String) -> Result<(), String> {
    Err(unsupported("wifi_open_settings"))
}

#[tauri::command]
pub async fn bluetooth_list_devices() -> Result<Vec<serde_json::Value>, String> {
    Err(unsupported("bluetooth_list_devices"))
}

#[tauri::command]
pub async fn bluetooth_set_connected(id: String, connected: bool) -> Result<(), String> {
    Err(unsupported("bluetooth_set_connected"))
}

#[tauri::command]
pub async fn bluetooth_discovery(active: bool) -> Result<(), String> {
    Err(unsupported("bluetooth_discovery"))
}

#[tauri::command]
pub async fn bluetooth_pair(id: String) -> Result<(), String> {
    Err(unsupported("bluetooth_pair"))
}

#[tauri::command]
pub fn bluetooth_pairing_respond(accept: bool, pin: Option<String>) {
    unsupported("bluetooth_pairing_respond");
}

#[tauri::command]
pub async fn bluetooth_unpair(id: String) -> Result<(), String> {
    Err(unsupported("bluetooth_unpair"))
}

#[tauri::command]
pub async fn bluetooth_refresh_buds() -> Result<(), String> {
    Err(unsupported("bluetooth_refresh_buds"))
}

#[tauri::command]
pub fn usb_list_devices() -> Vec<serde_json::Value> {
    Default::default()
}

#[tauri::command]
pub fn jack_list_devices() -> Vec<serde_json::Value> {
    Default::default()
}
