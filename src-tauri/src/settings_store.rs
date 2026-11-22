//! The editable settings file (settings.json) and its in-memory cache. Plain
//! file I/O shared by every platform.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub static SETTINGS_CACHE: OnceLock<Mutex<HashMap<String, serde_json::Value>>> = OnceLock::new();
/// Serialize read/modify/write settings commands from all four webviews.
pub static SETTINGS_FILE_LOCK: Mutex<()> = Mutex::new(());

/// Parse the editable settings file without discarding a UTF-8 BOM. A parse
/// failure must reach the caller rather than turning into an empty map: the
/// next save would otherwise overwrite all of the user's preferences.
pub fn read_settings_file(
    path: &std::path::Path,
) -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(content.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("Invalid settings file: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
        Err(e) => Err(format!("Could not read settings file: {e}")),
    }
}

/// Write a complete file in the same directory and replace the old file only
/// after the new contents are ready. A crash or write failure leaves the old file.
pub fn atomic_write_file(path: &std::path::Path, content: &[u8]) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_file_name(format!(
        "{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bloom"),
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    if let Err(e) = std::fs::write(&temp, content) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Could not write {}: {e}", temp.display()));
    }
    // Antivirus, indexers and editors can briefly open the destination without
    // FILE_SHARE_DELETE. Keep the original intact and retry only these Windows
    // sharing/access errors; a permanent failure is still returned to Settings.
    let mut replaced = std::fs::rename(&temp, path);
    for delay_ms in [25, 50, 100, 200] {
        if !cfg!(windows)
            || !replaced
                .as_ref()
                .is_err_and(|e| matches!(e.raw_os_error(), Some(5 | 32 | 33)))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        replaced = std::fs::rename(&temp, path);
    }
    if let Err(e) = replaced {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("Could not replace {}: {e}", path.display()));
    }
    Ok(())
}

pub fn write_settings_file(
    path: &std::path::Path,
    settings: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<(), String> {
    let content = serde_json::to_vec(settings).map_err(|e| e.to_string())?;
    atomic_write_file(path, &content)
}

/// The four webviews may save at once. Keep each read/modify/write transaction
/// together so one window cannot replace another window's new setting.
pub fn save_setting_file(
    path: &std::path::Path,
    key: String,
    value: serde_json::Value,
) -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
    let _guard = SETTINGS_FILE_LOCK
        .lock()
        .map_err(|e| e.to_string())?;
    let mut settings = read_settings_file(path)?;
    settings.insert(key, value);
    write_settings_file(path, &settings)?;
    replace_settings_cache(settings.clone());
    Ok(settings)
}

/// Load settings.json into the in-memory cache. Call once at startup.
pub fn init_settings_cache(app: &tauri::AppHandle) {
    use SETTINGS_CACHE;
    use tauri::Manager;
    let cache =
        SETTINGS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(path) = app
        .path()
        .app_config_dir()
        .ok()
        .map(|p| p.join("settings.json"))
    {
        if let Ok(settings) = read_settings_file(&path) {
            if let Ok(mut cache) = cache.lock() {
                *cache = settings;
            }
        }
    }
}

/// Replace the entire settings cache (used by save_setting and the file watcher).
///
/// Uses `get_or_init` rather than `get().unwrap()`: a webview can invoke
/// `save_setting` before the setup hook has initialized the cache, and a panic
/// on the main thread inside a WebView2 callback cannot unwind, which aborts
/// the process.
pub fn replace_settings_cache(new_settings: std::collections::HashMap<String, serde_json::Value>) {
    let cache = SETTINGS_CACHE
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Ok(mut cache) = cache.lock() {
        *cache = new_settings;
    }
}

/// Read any string value from the settings cache. Returns None if the key is absent.
pub fn get_setting_str(_app: &tauri::AppHandle, key: &str) -> Option<String> {
    let cache = SETTINGS_CACHE.get()?;
    let guard = cache.lock().ok()?;
    guard.get(key)?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    /// Every test that touches the settings file or cache holds this, on
    /// every platform, because the cache is process-wide.
    pub(crate) static TEST_SETTINGS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    pub(crate) fn test_settings_path() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "bloom-settings-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).expect("create test directory");
        dir.join("settings.json")
    }

    #[test]
    fn settings_bom_is_read_and_a_damaged_file_is_never_overwritten() {
        let _guard = TEST_SETTINGS_LOCK.lock().unwrap();
        let path = test_settings_path();
        std::fs::write(&path, "\u{feff}{\"bloom-existing\":\"true\"}").unwrap();
        let settings = super::read_settings_file(&path).unwrap();
        assert_eq!(settings["bloom-existing"], "true");

        super::save_setting_file(&path, "bloom-new".into(), "false".into()).unwrap();
        let settings = super::read_settings_file(&path).unwrap();
        assert_eq!(settings.len(), 2);

        std::fs::write(&path, "{invalid").unwrap();
        assert!(super::save_setting_file(&path, "bloom-another".into(), "true".into()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{invalid");
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn simultaneous_setting_saves_keep_every_key() {
        let _guard = TEST_SETTINGS_LOCK.lock().unwrap();
        let path = test_settings_path();
        let workers: Vec<_> = (0..12)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || {
                    super::save_setting_file(
                        &path,
                        format!("bloom-key-{index}"),
                        serde_json::json!(index.to_string()),
                    )
                    .unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(super::read_settings_file(&path).unwrap().len(), 12);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
