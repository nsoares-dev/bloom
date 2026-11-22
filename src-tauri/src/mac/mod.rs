//! macOS entry point. Mirrors `win_main::run` with macOS implementations of
//! the same commands, so the interface works unchanged on both systems.

pub mod commands;
pub mod apps;
mod ax;
mod extras;
mod interaction;
mod native_dock;
mod window;

use crate::{i18n, settings_commands};

pub fn run() {
    tauri::Builder::default()
        // Registered first so a second launch exits before building anything;
        // the running instance opens Settings, as on Windows.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::open_settings_window(app.clone());
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::agent_alert_status,
            commands::configure_agent_alert,
            commands::test_agent_alert,
            commands::agent_alerts_quiet,
            commands::hide_native_osd,
            commands::open_settings_window,
            commands::close_settings_window,
            commands::open_wifi_settings,
            commands::open_sound_settings,
            commands::open_notification_center,
            commands::open_system_tray,
            commands::set_ignore_cursor_events,
            commands::set_window_height,
            commands::resize_settings_window,
            commands::hide_overlay,
            commands::set_splash_fullscreen,
            commands::get_onboarding_completed,
            commands::complete_onboarding,
            commands::open_onboarding,
            commands::set_onboarding_active,
            commands::sync_overlay_position,
            commands::media_play_pause,
            commands::media_next,
            commands::media_previous,
            commands::media_seek,
            commands::open_media_source_app,
            commands::init_dock,
            commands::toggle_dock,
            commands::change_dock_mode,
            commands::change_notch_mode,
            commands::sync_appbar,
            commands::open_app,
            commands::launch_new_instance,
            commands::update_dock_rect,
            commands::update_notch_rect,
            commands::set_window_region,
            commands::set_dock_hovered,
            commands::set_dock_popup_active,
            commands::set_notch_hovered,
            commands::set_notch_visible,
            commands::setup_shelf_drop,
            commands::shelf_thumbnail,
            commands::shelf_drag_icon,
            commands::shelf_prune,
            commands::shelf_open,
            commands::get_active_windows,
            commands::get_tray_apps,
            commands::show_tray_context_menu,
            commands::activate_tray_icon,
            commands::open_system_action,
            commands::get_app_icon,
            commands::get_installed_apps,
            commands::refresh_installed_apps,
            commands::save_pinned_apps,
            commands::load_pinned_apps,
            commands::clear_icon_cache,
            crate::custom_icons::set_custom_icon,
            crate::custom_icons::remove_custom_icon,
            crate::custom_icons::get_custom_icons,
            commands::set_menu_open,
            commands::focus_window,
            commands::focus_app_windows,
            commands::close_window,
            commands::quit_bloom,
            commands::restart_bloom,
            commands::get_volume,
            commands::get_volume_state,
            commands::get_brightness,
            commands::set_volume,
            commands::get_audio_sessions,
            commands::set_app_volume,
            commands::set_app_mute,
            commands::set_volume_mixer_rect,
            commands::clear_volume_mixer_rect,
            settings_commands::save_setting,
            i18n::set_tray_language,
            settings_commands::load_settings,
            settings_commands::settings_file_readable,
            commands::capture_window_thumbnail,
            commands::get_wifi_status,
            commands::set_wifi_state,
            commands::get_bluetooth_state,
            commands::set_bluetooth_state,
            commands::open_bluetooth_settings,
            commands::open_airplane_mode_settings,
            commands::set_brightness,
            commands::get_battery_saver_state,
            commands::open_battery_saver_settings,
            commands::get_battery_details,
            commands::get_system_accent_color,
            commands::get_cpu_usage,
            commands::get_ram_usage,
            commands::get_disk_space,
            commands::get_network_speed,
            settings_commands::export_settings_to_file,
            settings_commands::import_settings_from_file,
            settings_commands::reset_settings,
            commands::check_for_updates,
            commands::install_update,
            commands::get_update_state,
            commands::get_dock_extras,
            commands::open_dock_extra,
            commands::eject_dock_extra,
            commands::get_dock_drives,
            commands::wifi_list_networks,
            commands::wifi_scan,
            commands::wifi_connect,
            commands::wifi_disconnect,
            commands::wifi_forget,
            commands::wifi_open_settings,
            commands::bluetooth_list_devices,
            commands::bluetooth_set_connected,
            commands::bluetooth_discovery,
            commands::bluetooth_pair,
            commands::bluetooth_pairing_respond,
            commands::bluetooth_unpair,
            commands::bluetooth_refresh_buds,
            commands::usb_list_devices,
            commands::jack_list_devices,
            commands::get_notch
        ])
        .setup(|app| {
            crate::diagnostics::initialize();
            crate::settings_store::init_settings_cache(app.handle());
            // A previous run that crashed may have left the macOS Dock hidden;
            // the dock's init_dock hides it again if Bloom's dock is on.
            commands::restore_native_dock(app.handle());
            let _ = commands::NOTCH.set(window::notch());
            place_windows(app.handle());
            interaction::spawn_worker(app.handle().clone());
            watch_windows(app.handle().clone());
            restore_on_signals(app.handle().clone());
            build_menu_bar_icon(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commands::restore_native_dock(app);
            }
        });
}

/// Ctrl+C in a terminal, `kill`, or a logout's SIGTERM skip Tauri's exit
/// event; give the macOS Dock back before going.
fn restore_on_signals(app: tauri::AppHandle) {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP]) else {
        crate::diagnostics::record("signals", "could not install handlers");
        return;
    };
    std::thread::Builder::new()
        .name("bloom-signals".into())
        .spawn(move || {
            if let Some(signal) = signals.forever().next() {
                crate::diagnostics::record("shutdown", &format!("signal {signal}"));
                crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                commands::restore_native_dock(&app);
                std::process::exit(0);
            }
        })
        .expect("failed to start the signal watcher");
}

/// Tells the dock when apps or windows come and go, checking once a second.
fn watch_windows(app: tauri::AppHandle) {
    use tauri::Emitter;
    std::thread::Builder::new()
        .name("bloom-window-watch".into())
        .spawn(move || {
            let mut last = apps::fingerprint();
            while !crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let now = apps::fingerprint();
                if now != last {
                    let _ = app.emit("windows-changed", ());
                    last = now;
                }
            }
        })
        .expect("failed to start the window watcher");
}

/// Spreads Bloom's windows over the main screen: the island across the top,
/// the dock along the bottom and the overlay over everything. Sizes come from
/// tauri.conf.json; positions are in logical points.
fn place_windows(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(monitor) = app.primary_monitor().ok().flatten() else {
        crate::diagnostics::record("place_windows", "no primary monitor");
        return;
    };
    let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
    let origin = monitor.position().to_logical::<f64>(monitor.scale_factor());
    let screen_rect = interaction::Rect {
        x: origin.x,
        y: origin.y,
        width: screen.width,
        height: screen.height,
    };
    let config_height = |label: &str| {
        app.config()
            .app
            .windows
            .iter()
            .find(|w| w.label == label)
            .map(|w| w.height)
            .unwrap_or(0.0)
    };
    for (label, y, height, show) in [
        ("main", 0.0, config_height("main"), true),
        (
            "dock",
            screen.height - config_height("dock"),
            config_height("dock"),
            true,
        ),
        ("overlay", 0.0, screen.height, false),
    ] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        window::float_and_place(&window, y, height);
        interaction::set_window_origin(label, screen_rect, (origin.x, origin.y + y));
        // Click-through until the cursor worker sees the pointer over what the
        // window draws; these transparent windows cover most of the screen.
        let _ = window.set_ignore_cursor_events(true);
        if show {
            let _ = window.show();
        }
    }
}

/// Bloom's icon in the menu bar, with the same menu as the Windows tray.
fn build_menu_bar_icon(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri::Manager;
    let settings = MenuItem::with_id(app, "settings", "Open Settings", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart Bloom", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bloom", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &restart, &quit])?;
    app.manage(i18n::TrayMessages {
        settings,
        restart,
        quit,
    });
    let locale =
        crate::settings_store::get_setting_str(app.handle(), "bloom-language").unwrap_or_default();
    let _ = i18n::set_tray_language(app.handle().clone(), locale);
    TrayIconBuilder::new()
        .icon(app.default_window_icon().cloned().expect("bundle icon"))
        .tooltip("Bloom")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => commands::open_settings_window(app.clone()),
            "restart" => {
                crate::diagnostics::record("restart", "source=menu bar");
                crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                commands::restore_native_dock(app);
                app.restart();
            }
            "quit" => {
                crate::diagnostics::record("shutdown", "source=menu bar");
                crate::state::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                commands::restore_native_dock(app);
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod parity_tests;
