//! Commands that read and write settings.json. Shared by every platform.

use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Manager, Window};

#[tauri::command(async)]
pub fn save_setting(app: AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    // Async command workers must emit in commit order: a later save must not
    // broadcast before an earlier one and leave windows on an older value.
    static SAVE_COMMAND_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _save_guard = SAVE_COMMAND_LOCK.lock().map_err(|e| e.to_string())?;
    let path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let settings =
        crate::settings_store::save_setting_file(&path, key.clone(), value).map_err(|error| {
            crate::diagnostics::record("settings_save_failed", &format!("key={key} error={error}"));
            error
        })?;

    // Broadcast so all windows sync — emit the key as-is (bloom-prefixed)
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "key": &key, "value": &settings[&key] }),
    );

    if key == "bloom-scale" {
        #[cfg(windows)]
        crate::commands::re_register_appbars(&app, &settings);
    }
    Ok(())
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<HashMap<String, serde_json::Value>, String> {
    let path = match app.path().app_config_dir() {
        Ok(p) => p.join("settings.json"),
        Err(_) => return Ok(HashMap::new()),
    };
    crate::settings_store::read_settings_file(&path)
}

/// Whether settings.json exists and parses, which load_settings can't tell
/// apart from a missing file since both load as an empty map.
#[tauri::command]
pub fn settings_file_readable(app: AppHandle) -> bool {
    app.path().app_config_dir().ok().is_some_and(|dir| {
        let path = dir.join("settings.json");
        path.exists() && crate::settings_store::read_settings_file(&path).is_ok()
    })
}

/// Export and import both pick their file in a native dialog opened from
/// Rust, so the path a webview can influence never reaches the filesystem
/// commands. A script injection in any Bloom window would otherwise turn
/// these into arbitrary file read/write as the user.
#[tauri::command]
pub async fn export_settings_to_file(
    app: AppHandle,
    window: Window,
) -> Result<Option<String>, String> {
    if window.label() != "settings" {
        return Err("Settings export is only available from the Settings window".into());
    }
    let source = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if !source.exists() {
        return Err("No settings file found".into());
    }
    let content = std::fs::read_to_string(&source).map_err(|e| e.to_string())?;
    // Validate it's valid JSON before offering it for export.
    crate::settings_store::read_settings_file(&source)?;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        app.dialog()
            .file()
            .set_parent(&window)
            .set_title("Export Bloom Settings")
            .set_file_name("bloom-settings.json")
            .add_filter("JSON", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(file_path) = picked else {
        return Ok(None);
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn import_settings_from_file(
    app: AppHandle,
    window: Window,
) -> Result<Option<String>, String> {
    if window.label() != "settings" {
        return Err("Settings import is only available from the Settings window".into());
    }
    let picked = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        move || {
            use tauri_plugin_dialog::DialogExt;
            app.dialog()
                .file()
                .set_parent(&window)
                .set_title("Import Bloom Settings")
                .add_filter("JSON", &["json"])
                .blocking_pick_file()
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(file_path) = picked else {
        return Ok(None);
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;
    let file = std::fs::File::open(&path).map_err(|e| format!("Failed to open file: {}", e))?;
    let size = file
        .metadata()
        .map_err(|e| format!("Failed to inspect file: {}", e))?
        .len();
    if size > MAX_SETTINGS_IMPORT_BYTES {
        return Err("Settings file exceeds the 1 MiB import limit".into());
    }
    let content = read_settings_import(file)?;
    apply_imported_settings(&app, &content)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

const MAX_SETTINGS_IMPORT_BYTES: u64 = 1024 * 1024;

fn read_settings_import(reader: impl std::io::Read) -> Result<String, String> {
    use std::io::Read;

    // Bound the read as well as checking metadata, in case the file grows.
    let mut bytes = Vec::new();
    reader
        .take(MAX_SETTINGS_IMPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    if bytes.len() as u64 > MAX_SETTINGS_IMPORT_BYTES {
        return Err("Settings file exceeds the 1 MiB import limit".into());
    }
    String::from_utf8(bytes).map_err(|e| format!("Settings file is not valid UTF-8: {}", e))
}

fn apply_imported_settings(app: &AppHandle, settings: &str) -> Result<(), String> {
    let imported: HashMap<String, serde_json::Value> =
        serde_json::from_str(settings.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("Invalid JSON: {}", e))?;

    let path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _guard = crate::settings_store::SETTINGS_FILE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let previous = crate::settings_store::read_settings_file(&path)?;
    crate::settings_store::write_settings_file(&path, &imported)?;
    crate::settings_store::replace_settings_cache(imported.clone());
    drop(_guard);

    // Broadcast changes for every imported key so all windows re-sync
    for (key, value) in &imported {
        let _ = app.emit(
            "settings-changed",
            serde_json::json!({ "key": key, "value": value }),
        );
    }

    // Keys the imported file doesn't have must leave each window's localStorage
    // mirror too, or they come back from it on the next start. Same sentinels
    // as reset_settings are kept.
    let keep = ["bloom-first-run", "bloom-app-version"];
    for key in previous.keys().filter(|key| {
        key.starts_with("bloom-") && !keep.contains(&key.as_str()) && !imported.contains_key(*key)
    }) {
        let _ = app.emit(
            "settings-external-changed",
            serde_json::json!({ "key": key, "value": null }),
        );
    }

    if imported.get("bloom-scale").is_some() {
        #[cfg(windows)]
        crate::commands::re_register_appbars(app, &imported);
    }

    Ok(())
}

#[tauri::command]
pub fn reset_settings(app: AppHandle) -> Result<(), String> {
    let path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Keep non-Bloom keys (the file is user-editable) and the first-run/version
    // sentinels (lifecycle markers, not preferences — keeping them avoids
    // replaying the splash after a reset). Everything else falls back to
    // defaults in both the backend and the frontends.
    let keep = ["bloom-first-run", "bloom-app-version"];
    let _guard = crate::settings_store::SETTINGS_FILE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let mut settings = crate::settings_store::read_settings_file(&path)?;
    let removed: Vec<String> = settings
        .keys()
        .filter(|key| key.starts_with("bloom-") && !keep.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in &removed {
        settings.remove(key);
    }

    crate::settings_store::write_settings_file(&path, &settings)?;
    crate::settings_store::replace_settings_cache(settings);
    drop(_guard);

    // Mirror the external-edit watcher so every open window drops the removed
    // keys from localStorage. The caller restarts Bloom right after, but this
    // keeps the reset correct even if a window lingers.
    for key in removed {
        let _ = app.emit(
            "settings-external-changed",
            serde_json::json!({ "key": key, "value": null }),
        );
    }
    Ok(())
}

#[cfg(test)]
mod settings_import_tests {
    use super::{read_settings_import, MAX_SETTINGS_IMPORT_BYTES};
    use std::io::{Cursor, Read};

    #[test]
    fn accepts_utf8_settings_at_the_size_limit() {
        let mut content = String::from("{\"bloom-weather-city\":\"Montréal\"}");
        content.extend(std::iter::repeat_n(
            ' ',
            MAX_SETTINGS_IMPORT_BYTES as usize - content.len(),
        ));
        assert_eq!(read_settings_import(content.as_bytes()).unwrap(), content);
    }

    #[test]
    fn rejects_oversized_input_without_reading_the_rest() {
        let mut reader = Cursor::new(vec![b' '; MAX_SETTINGS_IMPORT_BYTES as usize + 100]);
        let error = read_settings_import(&mut reader).unwrap_err();
        assert!(error.contains("1 MiB"));
        assert_eq!(reader.position(), MAX_SETTINGS_IMPORT_BYTES + 1);
    }

    #[test]
    fn rejects_invalid_utf8_and_read_errors() {
        assert!(read_settings_import(&[0xff][..])
            .unwrap_err()
            .contains("UTF-8"));
        struct FailedReader;
        impl Read for FailedReader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("read failed"))
            }
        }
        assert!(read_settings_import(FailedReader)
            .unwrap_err()
            .contains("read failed"));
    }
}
