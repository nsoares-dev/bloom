//! What's playing in Spotify or Apple Music, and their transport controls.
//!
//! macOS 15.4 closed the system Now Playing API to other apps, so Bloom asks
//! the two players directly through AppleScript (macOS asks the user once to
//! let Bloom control each). A player that isn't running is never launched.

use crate::types::MediaInfo;
use std::process::Command;

/// Field separator in the script's output (ASCII unit separator).
const SEP: char = '\u{1f}';

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Player {
    Spotify,
    Music,
}

impl Player {
    pub fn app_name(self) -> &'static str {
        match self {
            Player::Spotify => "Spotify",
            Player::Music => "Music",
        }
    }
}

/// One script reports both players, one line each:
/// player, state, title, artist, duration, position, artwork URL.
const STATUS_SCRIPT: &str = r#"
set us to (ASCII character 31)
set out to ""
if application "Spotify" is running then
    tell application "Spotify"
        try
            set out to out & "Spotify" & us & (player state as string) & us & (name of current track) & us & (artist of current track) & us & ((duration of current track) / 1000) & us & player position & us & (artwork url of current track) & linefeed
        end try
    end tell
end if
if application "Music" is running then
    tell application "Music"
        try
            set out to out & "Music" & us & (player state as string) & us & (name of current track) & us & (artist of current track) & us & (duration of current track) & us & player position & us & "" & linefeed
        end try
    end tell
end if
return out
"#;

/// A player's line from the status script.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub player: Player,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub duration_ms: i64,
    pub position_ms: i64,
    pub artwork_url: Option<String>,
}

/// AppleScript numbers can use a decimal comma in some locales.
fn seconds_to_ms(text: &str) -> i64 {
    text.trim().replace(',', ".").parse::<f64>().map(|s| (s * 1000.0) as i64).unwrap_or(0)
}

pub fn parse_status(output: &str) -> Vec<Status> {
    output
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(SEP).collect();
            if f.len() < 7 {
                return None;
            }
            let player = match f[0] {
                "Spotify" => Player::Spotify,
                "Music" => Player::Music,
                _ => return None,
            };
            let state = f[1].trim();
            if state == "stopped" || f[2].trim().is_empty() {
                return None;
            }
            Some(Status {
                player,
                playing: state == "playing",
                title: f[2].trim().to_string(),
                artist: f[3].trim().to_string(),
                duration_ms: seconds_to_ms(f[4]),
                position_ms: seconds_to_ms(f[5]),
                artwork_url: Some(f[6].trim().to_string()).filter(|u| u.starts_with("https://")),
            })
        })
        .collect()
}

/// The player to show: the one playing, else the first one with a track.
pub fn current(statuses: &[Status]) -> Option<&Status> {
    statuses.iter().find(|s| s.playing).or_else(|| statuses.first())
}

pub fn to_media_info(status: Option<&Status>, artwork: Option<String>, now_ms: u64) -> MediaInfo {
    match status {
        Some(s) => MediaInfo {
            title: s.title.clone(),
            artist: s.artist.clone(),
            is_playing: s.playing,
            has_media: true,
            artwork: artwork.map(|a| vec![a]),
            position_ms: s.position_ms,
            duration_ms: s.duration_ms,
            seek_enabled: s.duration_ms > 0,
            position_updated_at: now_ms,
        },
        None => MediaInfo {
            title: String::new(),
            artist: String::new(),
            is_playing: false,
            has_media: false,
            artwork: None,
            position_ms: 0,
            duration_ms: 0,
            seek_enabled: false,
            position_updated_at: now_ms,
        },
    }
}

fn osascript(script: &str) -> Result<String, String> {
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn status() -> Result<Vec<Status>, String> {
    osascript(STATUS_SCRIPT).map(|out| parse_status(&out))
}

/// Sends a command such as "playpause" or "next track" to a running player.
pub fn control(player: Player, command: &str) -> Result<(), String> {
    let name = player.app_name();
    osascript(&format!(
        "if application \"{name}\" is running then tell application \"{name}\" to {command}"
    ))
    .map(|_| ())
}


// ---- Runtime ----

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

/// The player the island is showing, for the transport buttons.
static CURRENT: Mutex<Option<Player>> = Mutex::new(None);
/// Last artwork URL and its data URI (the page may only load data: images).
static ARTWORK: Mutex<Option<(String, String)>> = Mutex::new(None);

pub fn current_player() -> Option<Player> {
    CURRENT.lock().ok().and_then(|p| *p)
}

fn players_running() -> bool {
    use objc2_app_kit::NSWorkspace;
    NSWorkspace::sharedWorkspace().runningApplications().iter().any(|app| {
        app.bundleIdentifier()
            .is_some_and(|id| matches!(id.to_string().as_str(), "com.spotify.client" | "com.apple.Music"))
    })
}

fn artwork_data_uri(url: &str) -> Option<String> {
    if let Some((cached_url, uri)) = ARTWORK.lock().ok()?.as_ref() {
        if cached_url == url {
            return Some(uri.clone());
        }
    }
    let output = Command::new("/usr/bin/curl")
        .args(["-fsL", "--max-time", "5", url])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    use base64::Engine;
    let uri = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&output.stdout)
    );
    *ARTWORK.lock().ok()? = Some((url.to_string(), uri.clone()));
    Some(uri)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Checks the players once a second and sends `media-update` on changes:
/// another track, play/pause, artwork, or a position jump (seeking).
pub fn spawn_worker(app: AppHandle) {
    std::thread::Builder::new()
        .name("bloom-media".into())
        .spawn(move || {
            let mut last: Option<(String, String, bool, bool, bool, i64)> = None;
            let mut last_error = String::new();
            while !crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                let statuses = if players_running() {
                    match status() {
                        Ok(s) => s,
                        Err(error) => {
                            if error != last_error {
                                crate::diagnostics::record("media_status", &error);
                                last_error = error;
                            }
                            Vec::new()
                        }
                    }
                } else {
                    Vec::new()
                };
                let shown = current(&statuses).cloned();
                if let Ok(mut player) = CURRENT.lock() {
                    *player = shown.as_ref().map(|s| s.player);
                }
                let artwork = shown.as_ref().and_then(|s| s.artwork_url.as_deref()).and_then(artwork_data_uri);
                let info = to_media_info(shown.as_ref(), artwork, now_ms());
                // While playing, the position advances ~1000ms per check; only
                // a bigger jump (a seek) needs a new update.
                let key = (info.title.clone(), info.artist.clone(), info.is_playing, info.has_media, info.artwork.is_some(), info.position_ms);
                let changed = last.as_ref().is_none_or(|l| {
                    (&l.0, &l.1, l.2, l.3, l.4) != (&key.0, &key.1, key.2, key.3, key.4)
                        || ((key.5 - l.5) - if key.2 { 1000 } else { 0 }).abs() > 1500
                });
                if changed {
                    let _ = app.emit("media-update", info);
                }
                last = Some(key);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        })
        .expect("failed to start the media worker");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(fields: &[&str]) -> String {
        fields.join(&SEP.to_string())
    }

    #[test]
    fn parses_a_playing_spotify_track() {
        let out = line(&["Spotify", "playing", "Song", "Band", "215,5", "12.25", "https://i.scdn.co/image/x"]);
        let parsed = parse_status(&out);
        assert_eq!(
            parsed,
            vec![Status {
                player: Player::Spotify,
                playing: true,
                title: "Song".into(),
                artist: "Band".into(),
                duration_ms: 215_500,
                position_ms: 12_250,
                artwork_url: Some("https://i.scdn.co/image/x".into()),
            }]
        );
    }

    #[test]
    fn skips_stopped_players_and_malformed_lines() {
        let out = [
            line(&["Music", "stopped", "Old", "Artist", "100", "0", ""]),
            "garbage".to_string(),
            line(&["Music", "paused", "Kept", "Artist", "100", "5", ""]),
        ]
        .join("\n");
        let parsed = parse_status(&out);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Kept");
        assert!(!parsed[0].playing);
        assert_eq!(parsed[0].artwork_url, None);
    }

    #[test]
    fn prefers_the_player_that_is_playing() {
        let out = [
            line(&["Spotify", "paused", "A", "x", "1", "0", ""]),
            line(&["Music", "playing", "B", "y", "1", "0", ""]),
        ]
        .join("\n");
        let parsed = parse_status(&out);
        assert_eq!(current(&parsed).unwrap().title, "B");
    }

    #[test]
    fn no_player_means_no_media() {
        let info = to_media_info(None, None, 7);
        assert!(!info.has_media && !info.is_playing && info.artwork.is_none());
    }
}
