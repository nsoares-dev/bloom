//! Icons the user picked for dock apps, stored as PNGs next to settings.
//! Shared by every platform.

use std::collections::HashMap;
use tauri::{AppHandle, Manager};

pub(crate) fn get_cache_key(path: &str, name: Option<&str>) -> String {
    let path_lc = path.to_lowercase();
    let name_lc = name.map(|n| n.to_lowercase()).unwrap_or_default();
    if path_lc.contains("msedge.exe")
        || path_lc.contains("chrome.exe")
        || path_lc.contains("brave.exe")
        || path_lc.contains("applicationframehost.exe")
    {
        format!("{}:{}", path, name_lc)
    } else {
        path.to_string()
    }
}

pub(crate) fn get_custom_icons_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("custom_icons");
    let _ = std::fs::create_dir_all(&dir);
    Ok(dir)
}

pub(crate) fn sanitize_filename(key: &str) -> String {
    key.replace(
        |c: char| {
            c == ':'
                || c == '\\'
                || c == '/'
                || c == '*'
                || c == '?'
                || c == '"'
                || c == '<'
                || c == '>'
                || c == '|'
        },
        "_",
    )
}

#[tauri::command]
pub async fn set_custom_icon(
    app: AppHandle,
    cache_key: String,
    icon_data: String,
) -> Result<String, String> {
    use base64::Engine;
    use image::GenericImageView;

    // Validate and decode the base64 data URI
    let b64_str = if icon_data.starts_with("data:") {
        icon_data.split(',').nth(1).ok_or("Invalid data URI")?
    } else {
        &icon_data
    };

    let raw_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64_str)
        .map_err(|e| format!("Invalid base64: {}", e))?;

    // Decode image to validate and get dimensions
    let img = image::load_from_memory(&raw_bytes).map_err(|e| format!("Invalid image: {}", e))?;

    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return Err("Icon must be at least 16x16 pixels".into());
    }

    // Resize to fit within 256x256 preserving aspect ratio, then center on transparent canvas
    let target = 256u32;
    let final_img = if w > target || h > target {
        let scaled = img.resize(target, target, image::imageops::FilterType::Lanczos3);
        let (sw, sh) = scaled.dimensions();
        let mut canvas = image::RgbaImage::new(target, target);
        let ox = (target - sw) / 2;
        let oy = (target - sh) / 2;
        image::imageops::overlay(&mut canvas, &scaled, ox as i64, oy as i64);
        image::DynamicImage::ImageRgba8(canvas)
    } else {
        img
    };

    // Encode to PNG bytes
    let mut png_bytes: Vec<u8> = Vec::new();
    final_img
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    // Save to custom_icons directory
    let icons_dir = get_custom_icons_dir(&app)?;
    let filename = format!("{}.png", sanitize_filename(&cache_key));
    let file_path = icons_dir.join(&filename);
    std::fs::write(&file_path, &png_bytes).map_err(|e| e.to_string())?;

    // Update manifest mapping sanitized filename -> original cache key
    let manifest_path = icons_dir.join("custom_icons.json");
    let mut manifest: std::collections::HashMap<String, String> =
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
    manifest.insert(sanitize_filename(&cache_key), cache_key);
    let _ = std::fs::write(
        &manifest_path,
        serde_json::to_string(&manifest).unwrap_or_default(),
    );

    // Return the data URI for immediate use
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(format!("data:image/png;base64,{}", b64))
}

#[tauri::command]
pub async fn remove_custom_icon(
    app: AppHandle,
    path: String,
    name: Option<String>,
) -> Result<(), String> {
    let cache_key = get_cache_key(&path, name.as_deref());
    let icons_dir = get_custom_icons_dir(&app)?;
    let sanitized = sanitize_filename(&cache_key);
    let filename = format!("{}.png", sanitized);
    let file_path = icons_dir.join(&filename);
    if file_path.exists() {
        std::fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    // Remove from manifest
    let manifest_path = icons_dir.join("custom_icons.json");
    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
        if let Ok(mut manifest) =
            serde_json::from_str::<std::collections::HashMap<String, String>>(&content)
        {
            manifest.remove(&sanitized);
            let _ = std::fs::write(
                &manifest_path,
                serde_json::to_string(&manifest).unwrap_or_default(),
            );
        }
    }
    // Also remove from persistent cache so the original icon is re-fetched
    if let Some(c) = crate::state::ICON_CACHE.get() {
        if let Ok(mut lock) = c.lock() {
            lock.remove(&cache_key);
        }
    }
    #[cfg(windows)]
    crate::commands::schedule_icon_cache_save(app);
    #[cfg(target_os = "macos")]
    crate::mac::apps::forget_icon(&path);
    Ok(())
}

#[tauri::command]
pub async fn get_custom_icons(app: AppHandle) -> Result<HashMap<String, String>, String> {
    let icons_dir = get_custom_icons_dir(&app)?;
    let manifest_path = icons_dir.join("custom_icons.json");
    let manifest: std::collections::HashMap<String, String> =
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
    let mut result = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(&icons_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    // Use original cache key from manifest, fallback to sanitized stem
                    let key = manifest
                        .get(stem)
                        .cloned()
                        .unwrap_or_else(|| stem.to_string());
                    if let Ok(data) = std::fs::read(&path) {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                        result.insert(key, format!("data:image/png;base64,{}", b64));
                    }
                }
            }
        }
    }
    Ok(result)
}
