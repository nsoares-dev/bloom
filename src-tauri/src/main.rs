#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
#[cfg(windows)]
mod settings_window;
#[cfg(windows)]
mod shelf;
#[cfg(windows)]
mod shelf_preview;
#[cfg(windows)]
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
