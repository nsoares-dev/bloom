//! Battery, Low Power Mode and accent color on macOS, read from the same
//! tools the system uses (pmset, ioreg, the global defaults).

use serde::Serialize;
use std::process::Command;

/// Same shape as Windows' `battery::BatteryDetails`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct BatteryDetails {
    pub present: bool,
    pub percent: Option<u8>,
    pub plugged_in: bool,
    pub charging: bool,
    pub seconds_left: Option<u64>,
    pub seconds_to_full: Option<u64>,
    pub health_percent: Option<u8>,
    pub power_mode: Option<&'static str>,
}

fn hm_seconds(text: &str) -> Option<u64> {
    let (h, m) = text.split_once(':')?;
    Some(h.trim().parse::<u64>().ok()? * 3600 + m.trim().parse::<u64>().ok()? * 60)
}

/// Parses `pmset -g batt`, e.g.
/// "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t85%; charging; 1:20 remaining present: true".
pub fn parse_pmset_batt(output: &str) -> BatteryDetails {
    let plugged_in = output.contains("'AC Power'");
    let Some(line) = output.lines().find(|l| l.contains("InternalBattery")) else {
        return BatteryDetails { plugged_in, ..Default::default() };
    };
    let fields: Vec<&str> = line.split('\t').nth(1).unwrap_or("").split(';').map(str::trim).collect();
    let percent = fields.first().and_then(|p| p.trim_end_matches('%').parse::<u8>().ok());
    let state = fields.get(1).copied().unwrap_or("");
    let charging = state == "charging";
    let remaining = fields
        .get(2)
        .and_then(|r| r.split_whitespace().next())
        .and_then(hm_seconds)
        .filter(|s| *s > 0);
    BatteryDetails {
        present: true,
        percent,
        plugged_in,
        charging,
        seconds_left: remaining.filter(|_| !plugged_in),
        seconds_to_full: remaining.filter(|_| charging),
        health_percent: None,
        power_mode: None,
    }
}

fn ioreg_number(output: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{key}\" = ");
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix(&needle))
        .and_then(|v| v.trim().parse::<f64>().ok())
}

/// Battery health from `ioreg -rn AppleSmartBattery`: full-charge capacity
/// against design capacity. Apple Silicon reports the real one as
/// AppleRawMaxCapacity (MaxCapacity is a percentage there).
pub fn parse_health(output: &str) -> Option<u8> {
    let design = ioreg_number(output, "DesignCapacity")?;
    let max = ioreg_number(output, "AppleRawMaxCapacity").or_else(|| ioreg_number(output, "MaxCapacity"))?;
    (design > 0.0).then(|| ((max / design) * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// Parses `pmset -g`, where Low Power Mode shows as "lowpowermode 1".
pub fn parse_low_power(output: &str) -> bool {
    output.lines().any(|l| {
        let mut words = l.split_whitespace();
        words.next() == Some("lowpowermode") && words.next() == Some("1")
    })
}

/// The accent color for a value of `defaults read -g AppleAccentColor`
/// (absent means the default blue).
pub fn accent_hex(value: Option<&str>) -> &'static str {
    match value.map(str::trim) {
        Some("-1") => "#8c8c8c",
        Some("0") => "#ff5257",
        Some("1") => "#f7821b",
        Some("2") => "#ffc600",
        Some("3") => "#62ba46",
        Some("5") => "#a550a7",
        Some("6") => "#f74f9e",
        _ => "#007aff",
    }
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn battery() -> BatteryDetails {
    let mut details = run("/usr/bin/pmset", &["-g", "batt"]).map(|o| parse_pmset_batt(&o)).unwrap_or_default();
    if details.present {
        details.health_percent =
            run("/usr/sbin/ioreg", &["-rn", "AppleSmartBattery"]).and_then(|o| parse_health(&o));
        details.power_mode = low_power().then_some("Low Power Mode");
    }
    details
}

pub fn low_power() -> bool {
    run("/usr/bin/pmset", &["-g"]).is_some_and(|o| parse_low_power(&o))
}

pub fn accent_color() -> String {
    let value = run("/usr/bin/defaults", &["read", "-g", "AppleAccentColor"]);
    accent_hex(value.as_deref()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_charging_battery() {
        let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=24117347)\t85%; charging; 1:20 remaining present: true\n";
        let d = parse_pmset_batt(out);
        assert!(d.present && d.plugged_in && d.charging);
        assert_eq!(d.percent, Some(85));
        assert_eq!(d.seconds_to_full, Some(4800));
        assert_eq!(d.seconds_left, None);
    }

    #[test]
    fn reads_a_discharging_battery_and_unknown_estimates() {
        let out = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t42%; discharging; 3:05 remaining present: true\n";
        let d = parse_pmset_batt(out);
        assert!(!d.plugged_in && !d.charging);
        assert_eq!(d.seconds_left, Some(11100));
        let out = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t42%; discharging; (no estimate) present: true\n";
        assert_eq!(parse_pmset_batt(out).seconds_left, None);
    }

    #[test]
    fn a_desktop_mac_has_no_battery() {
        let d = parse_pmset_batt("Now drawing from 'AC Power'\n");
        assert!(!d.present && d.plugged_in && d.percent.is_none());
    }

    #[test]
    fn health_uses_the_raw_capacity_on_apple_silicon() {
        let out = "  \"MaxCapacity\" = 100\n  \"AppleRawMaxCapacity\" = 4300\n  \"DesignCapacity\" = 4790\n";
        assert_eq!(parse_health(out), Some(90));
        assert_eq!(parse_health("\"MaxCapacity\" = 100\n"), None);
    }

    #[test]
    fn low_power_mode_and_accent_colors() {
        assert!(parse_low_power(" lowpowermode         1\n powermode 0\n"));
        assert!(!parse_low_power(" lowpowermode         0\n"));
        assert_eq!(accent_hex(None), "#007aff");
        assert_eq!(accent_hex(Some("3\n")), "#62ba46");
    }
}
