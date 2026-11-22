//! macOS entry point. Mirrors `win_main::run` with macOS implementations of
//! the same commands, so the interface works unchanged on both systems.

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
