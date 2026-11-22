//! The dock's extras on macOS: mounted volumes, the user's standard folders,
//! folders they added, and the Trash. Same shape and ids as on Windows.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DockExtra {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub path: String,
    pub icon: Option<String>,
    pub removable: bool,
}

#[derive(Deserialize)]
struct CustomFolder {
    name: String,
    path: String,
}

/// id, display name, folder under the home directory. Same ids and order as
/// Windows.
const KNOWN_FOLDERS: &[(&str, &str, &str)] = &[
    ("desktop", "Desktop", "Desktop"),
    ("downloads", "Downloads", "Downloads"),
    ("documents", "Documents", "Documents"),
    ("pictures", "Pictures", "Pictures"),
    ("music", "Music", "Music"),
    ("videos", "Movies", "Movies"),
];

fn extra(id: String, kind: &str, name: String, path: String, removable: bool) -> DockExtra {
    DockExtra { id, kind: kind.into(), name, path, icon: None, removable }
}

/// Volumes mounted under `volumes_dir`, except the startup disk (a link to /).
pub fn volumes(volumes_dir: &Path) -> Vec<DockExtra> {
    let Ok(entries) = std::fs::read_dir(volumes_dir) else { return Vec::new() };
    let mut found: Vec<DockExtra> = entries
        .flatten()
        .filter(|e| {
            let is_boot = std::fs::read_link(e.path()).is_ok_and(|target| target == Path::new("/"));
            !is_boot && e.path().is_dir()
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path().to_string_lossy().into_owned();
            extra(format!("drive:{name}"), "drive", name, path, true)
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The standard folders whose ids are listed in `enabled` ("downloads,music").
pub fn known_folders(home: &Path, enabled: &str) -> Vec<DockExtra> {
    let ids: Vec<String> = enabled.split(',').map(|id| id.trim().to_lowercase()).collect();
    KNOWN_FOLDERS
        .iter()
        .filter(|(id, _, _)| ids.iter().any(|e| e == id))
        .map(|(id, name, dir)| {
            let path = home.join(dir).to_string_lossy().into_owned();
            extra(format!("folder:{id}"), "folder", name.to_string(), path, false)
        })
        .filter(|e| Path::new(&e.path).is_dir())
        .collect()
}

/// Folders the user added (a JSON list of {name, path}) that still exist.
pub fn custom_folders(raw: &str) -> Vec<DockExtra> {
    serde_json::from_str::<Vec<CustomFolder>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| Path::new(&f.path).is_dir())
        .map(|f| extra(format!("custom:{}", f.path.to_lowercase()), "custom", f.name, f.path, false))
        .collect()
}

pub fn trash(home: &Path) -> DockExtra {
    let path = home.join(".Trash").to_string_lossy().into_owned();
    extra("recycle-bin".into(), "recycle-bin", "Trash".into(), path, false)
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bloom-extras-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn volumes_skip_the_startup_disk() {
        let dir = temp_dir("volumes");
        std::fs::create_dir(dir.join("USB Stick")).unwrap();
        std::os::unix::fs::symlink("/", dir.join("Macintosh HD")).unwrap();
        let found = volumes(&dir);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "drive:USB Stick");
        assert!(found[0].removable);
    }

    #[test]
    fn known_folders_follow_the_enabled_list_in_windows_order() {
        let home = temp_dir("home");
        for dir in ["Downloads", "Movies", "Desktop"] {
            std::fs::create_dir(home.join(dir)).unwrap();
        }
        let found = known_folders(&home, "videos, downloads,missing");
        let ids: Vec<_> = found.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["folder:downloads", "folder:videos"]);
        assert_eq!(found[1].name, "Movies");
    }

    #[test]
    fn custom_folders_drop_missing_ones_and_bad_json() {
        let home = temp_dir("custom");
        let raw = format!(
            r#"[{{"name":"Work","path":"{}"}},{{"name":"Gone","path":"/no/such/dir"}}]"#,
            home.display()
        );
        let found = custom_folders(&raw);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Work");
        assert!(custom_folders("not json").is_empty());
    }
}
