//! Hides the macOS Dock while Bloom's dock replaces it, and puts it back.
//!
//! macOS has no supported way to remove its Dock, so Bloom turns on the Dock's
//! own auto-hide with a reveal delay long enough that it never appears. What
//! the user had before is saved in a marker file first, and restored on quit,
//! on disabling Bloom's dock, and on the next start after a crash.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reveal delay while hidden, in seconds: effectively never.
const HIDDEN_DELAY: &str = "1000";

/// The user's own Dock preferences, as `defaults read` reports them. `None`
/// means the key was not set (the system default applies).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DockPrefs {
    pub autohide: Option<String>,
    pub autohide_delay: Option<String>,
}

/// One `defaults` invocation, as its argument list.
pub type DefaultsCall = Vec<String>;

fn call(args: &[&str]) -> DefaultsCall {
    args.iter().map(|a| a.to_string()).collect()
}

/// The `defaults` calls that hide the Dock.
pub fn hide_calls() -> Vec<DefaultsCall> {
    vec![
        call(&["write", "com.apple.dock", "autohide", "-bool", "true"]),
        call(&["write", "com.apple.dock", "autohide-delay", "-float", HIDDEN_DELAY]),
    ]
}

/// The `defaults` calls that bring back what `saved` describes: keys the user
/// had are written back, keys they didn't have are deleted.
pub fn restore_calls(saved: &DockPrefs) -> Vec<DefaultsCall> {
    let autohide = match saved.autohide.as_deref() {
        Some(value) => {
            let enabled = matches!(value.trim(), "1" | "true" | "YES");
            call(&["write", "com.apple.dock", "autohide", "-bool", if enabled { "true" } else { "false" }])
        }
        None => call(&["delete", "com.apple.dock", "autohide"]),
    };
    let delay = match saved.autohide_delay.as_deref() {
        Some(value) => call(&["write", "com.apple.dock", "autohide-delay", "-float", value.trim()]),
        None => call(&["delete", "com.apple.dock", "autohide-delay"]),
    };
    vec![autohide, delay]
}

fn read_pref(key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.dock", key])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(calls: &[DefaultsCall]) -> Result<(), String> {
    for args in calls {
        let status = Command::new("/usr/bin/defaults")
            .args(args)
            .status()
            .map_err(|e| format!("defaults {}: {e}", args.join(" ")))?;
        // Deleting a key that isn't set fails harmlessly.
        if !status.success() && args.first().map(String::as_str) != Some("delete") {
            return Err(format!("defaults {} failed", args.join(" ")));
        }
    }
    // The Dock rereads its preferences when it restarts; launchd restarts it.
    let _ = Command::new("/usr/bin/killall").arg("Dock").status();
    Ok(())
}

fn marker(dir: &Path) -> PathBuf {
    dir.join("native_dock_hidden.json")
}

/// Hides the Dock, saving the user's preferences unless a save already exists
/// (a second hide must not record Bloom's own hidden state as theirs).
pub fn hide(config_dir: &Path) -> Result<(), String> {
    let path = marker(config_dir);
    if !path.exists() {
        let saved = DockPrefs {
            autohide: read_pref("autohide"),
            autohide_delay: read_pref("autohide-delay"),
        };
        std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec(&saved).map_err(|e| e.to_string())?;
        crate::settings_store::atomic_write_file(&path, &json)?;
    }
    run(&hide_calls())
}

/// Restores the saved preferences if Bloom hid the Dock; otherwise does nothing.
pub fn restore(config_dir: &Path) -> Result<(), String> {
    let path = marker(config_dir);
    let Ok(content) = std::fs::read(&path) else {
        return Ok(());
    };
    let saved: DockPrefs = serde_json::from_slice(&content)
        .map_err(|e| format!("invalid {}: {e}", path.display()))?;
    run(&restore_calls(&saved))?;
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hiding_turns_on_autohide_with_a_long_delay() {
        assert_eq!(
            hide_calls(),
            vec![
                call(&["write", "com.apple.dock", "autohide", "-bool", "true"]),
                call(&["write", "com.apple.dock", "autohide-delay", "-float", "1000"]),
            ]
        );
    }

    #[test]
    fn restoring_deletes_keys_the_user_never_set() {
        assert_eq!(
            restore_calls(&DockPrefs::default()),
            vec![
                call(&["delete", "com.apple.dock", "autohide"]),
                call(&["delete", "com.apple.dock", "autohide-delay"]),
            ]
        );
    }

    #[test]
    fn restoring_writes_back_the_users_own_values() {
        let saved = DockPrefs {
            autohide: Some("0\n".into()),
            autohide_delay: Some("0.5".into()),
        };
        assert_eq!(
            restore_calls(&saved),
            vec![
                call(&["write", "com.apple.dock", "autohide", "-bool", "false"]),
                call(&["write", "com.apple.dock", "autohide-delay", "-float", "0.5"]),
            ]
        );
        let saved = DockPrefs { autohide: Some("1".into()), autohide_delay: None };
        assert_eq!(restore_calls(&saved)[0], call(&["write", "com.apple.dock", "autohide", "-bool", "true"]));
    }

    #[test]
    fn restore_without_a_marker_changes_nothing() {
        let dir = std::env::temp_dir().join(format!("bloom-dock-test-{}", std::process::id()));
        assert_eq!(restore(&dir), Ok(()));
    }
}
