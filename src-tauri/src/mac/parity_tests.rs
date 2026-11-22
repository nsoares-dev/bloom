//! The interface calls the same commands on every system, so macOS must
//! register exactly what Windows registers, and everything the interface
//! invokes must exist.

use std::collections::BTreeSet;
use std::path::Path;

/// Final segment of every path inside the first `generate_handler![...]`.
fn handler_names(src: &str) -> BTreeSet<String> {
    let start = match src.find("generate_handler![") {
        Some(i) => i + "generate_handler![".len(),
        None => return BTreeSet::new(),
    };
    let end = start
        + src[start..]
            .find(']')
            .expect("unterminated generate_handler!");
    src[start..end]
        .split(',')
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| item.rsplit("::").next().unwrap().to_string())
        .collect()
}

fn collect_invokes(dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_invokes(&path, out);
            continue;
        }
        let is_source = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("ts") | Some("tsx")
        );
        if !is_source {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("invoke") {
            rest = &rest[i + "invoke".len()..];
            // Skip an optional type argument: invoke<T>("name"
            let mut tail = rest;
            if tail.starts_with('<') {
                match tail.find('>') {
                    Some(j) => tail = &tail[j + 1..],
                    None => continue,
                }
            }
            let Some(tail) = tail.strip_prefix('(') else {
                continue;
            };
            let tail = tail.trim_start();
            let Some(quote) = tail.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            let name: String = tail[1..].chars().take_while(|c| *c != quote).collect();
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                out.insert(name);
            }
        }
    }
}

fn frontend_invokes() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_invokes(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../src"),
        &mut names,
    );
    names
}

/// Commands only macOS has; the interface calls them only on a Mac.
const MAC_ONLY: &[&str] = &["get_notch"];

#[test]
fn mac_registers_same_commands_as_windows() {
    let win = handler_names(include_str!("../win_main.rs"));
    let mut mac = handler_names(include_str!("mod.rs"));
    for name in MAC_ONLY {
        assert!(
            mac.remove(*name),
            "MAC_ONLY lists `{name}`, which macOS doesn't register"
        );
    }
    assert!(
        win.len() > 100,
        "parsed only {} Windows commands",
        win.len()
    );
    let missing: Vec<_> = win.difference(&mac).collect();
    let extra: Vec<_> = mac.difference(&win).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing on macOS: {missing:?}; only on macOS: {extra:?}"
    );
}

#[test]
fn every_frontend_invoke_is_registered_on_mac() {
    let mac = handler_names(include_str!("mod.rs"));
    let invokes = frontend_invokes();
    assert!(
        invokes.len() > 50,
        "found only {} frontend invokes",
        invokes.len()
    );
    let missing: Vec<_> = invokes.iter().filter(|n| !mac.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "frontend calls these, not registered on macOS: {missing:?}"
    );
}
