#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Shared state still mirrors Windows; macOS uses more of it with each port stage.
#![cfg_attr(target_os = "macos", allow(dead_code, unused_imports))]

// Modules without an attribute build on every platform; the rest still talk
// to Windows directly and build only there.
#[cfg(windows)]
mod agent_alerts;
#[cfg(windows)]
mod battery;
#[cfg(windows)]
mod bluetooth;
#[cfg(windows)]
mod buds;
#[cfg(windows)]
mod commands;
mod custom_icons;
mod diagnostics;
#[cfg(windows)]
mod dock_extras;
mod i18n;
#[cfg(windows)]
mod jack;
#[cfg(windows)]
mod onboarding;
#[cfg(windows)]
mod services;
mod settings_commands;
mod settings_store;
#[cfg(windows)]
mod settings_window;
#[cfg(windows)]
mod shelf;
#[cfg(windows)]
mod shelf_preview;
mod state;
#[cfg(windows)]
mod tray;
mod types;
#[cfg(windows)]
mod updater;
#[cfg(windows)]
mod usb;
#[cfg(windows)]
mod utils;
#[cfg(windows)]
mod wifi;
#[cfg(windows)]
mod window_region;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(windows)]
mod win_main;

fn main() {
    #[cfg(windows)]
    win_main::run();
    #[cfg(target_os = "macos")]
    mac::run();
}
