//! Bluetooth devices on macOS. The list (with AirPods' left, right and case
//! levels) comes from `system_profiler`, which works in any build; connecting
//! needs the IOBluetooth API and so the bundled app (see hardware.rs).

use serde::Serialize;
use serde_json::Value;

/// Same shape as Windows' `bluetooth::BtDevice`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub form: Option<&'static str>,
    pub connected: bool,
    pub battery: Option<u8>,
    pub can_connect: bool,
}

fn percent(props: &Value, key: &str) -> Option<u8> {
    props.get(key)?.as_str()?.trim_end_matches('%').trim().parse().ok()
}

fn kind_and_form(minor: &str, name: &str) -> (&'static str, Option<&'static str>) {
    let lower = name.to_lowercase();
    match minor {
        "Headphones" | "Headset" => {
            let earbuds = lower.contains("airpods") && !lower.contains("max") || lower.contains("buds");
            ("audio", Some(if earbuds { "earbuds" } else if minor == "Headset" { "headset" } else { "headphones" }))
        }
        "Speaker" | "Loudspeaker" | "Portable Audio" => ("audio", Some("speaker")),
        "Mouse" | "Trackpad" => ("mouse", None),
        "Keyboard" => ("keyboard", None),
        "Gamepad" | "Joystick" => ("gamepad", None),
        _ if lower.contains("iphone") || lower.contains("phone") => ("phone", None),
        _ if lower.contains("watch") => ("wearable", None),
        _ => ("other", None),
    }
}

/// Parses `system_profiler SPBluetoothDataType -json`.
pub fn parse_devices(json: &str) -> Vec<BtDevice> {
    let Ok(root) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let Some(controller) = root["SPBluetoothDataType"].get(0) else { return Vec::new() };
    let mut devices = Vec::new();
    for (section, connected) in [("device_connected", true), ("device_not_connected", false)] {
        let Some(list) = controller.get(section).and_then(Value::as_array) else { continue };
        for entry in list.iter().filter_map(Value::as_object) {
            for (name, props) in entry {
                let Some(address) = props.get("device_address").and_then(Value::as_str) else { continue };
                let minor = props.get("device_minorType").and_then(Value::as_str).unwrap_or("");
                let (kind, form) = kind_and_form(minor, name);
                // Earbuds report each side; the lower one is what runs out first.
                let battery = percent(props, "device_batteryLevelMain").or_else(|| {
                    match (percent(props, "device_batteryLevelLeft"), percent(props, "device_batteryLevelRight")) {
                        (Some(l), Some(r)) => Some(l.min(r)),
                        (one, other) => one.or(other),
                    }
                });
                devices.push(BtDevice {
                    id: address.to_string(),
                    name: name.clone(),
                    kind,
                    form,
                    connected,
                    battery,
                    can_connect: kind == "audio",
                });
            }
        }
    }
    devices
}

pub fn devices() -> Result<Vec<BtDevice>, String> {
    let output = std::process::Command::new("/usr/sbin/system_profiler")
        .args(["SPBluetoothDataType", "-json"])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(parse_devices(&String::from_utf8_lossy(&output.stdout)))
}

/// Connects or disconnects a paired device (bundled app only).
pub fn set_connected(address: &str, connect: bool) -> Result<(), String> {
    if !super::hardware::bluetooth_allowed() {
        return Err("Bluetooth control needs the bundled Bloom.app".into());
    }
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    super::hardware::load_iobluetooth()?;
    let class = AnyClass::get(c"IOBluetoothDevice").ok_or("IOBluetooth unavailable")?;
    let address = NSString::from_str(address);
    // SAFETY: documented IOBluetoothDevice class and instance methods; the
    // device object is autoreleased and only used within this call.
    unsafe {
        let device: *mut AnyObject = msg_send![class, deviceWithAddressString: &*address];
        let device = device.as_ref().ok_or("device not found")?;
        let status: i32 = if connect {
            msg_send![device, openConnection]
        } else {
            msg_send![device, closeConnection]
        };
        (status == 0).then_some(()).ok_or_else(|| format!("Bluetooth error {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_connected_and_paired_devices_with_kinds_and_batteries() {
        let json = r#"{"SPBluetoothDataType":[{
            "controller_properties":{},
            "device_connected":[{"AirPods Pro":{"device_address":"AA:BB","device_minorType":"Headphones",
                "device_batteryLevelLeft":"80%","device_batteryLevelRight":"65%","device_batteryLevelCase":"40%"}}],
            "device_not_connected":[
                {"iPhone de Nicolas":{"device_address":"CC:DD"}},
                {"X3 Mouse":{"device_address":"EE:FF","device_minorType":"Mouse","device_batteryLevelMain":"55%"}}]
        }]}"#;
        let devices = parse_devices(json);
        assert_eq!(devices.len(), 3);
        assert_eq!(
            devices[0],
            BtDevice {
                id: "AA:BB".into(),
                name: "AirPods Pro".into(),
                kind: "audio",
                form: Some("earbuds"),
                connected: true,
                battery: Some(65),
                can_connect: true,
            }
        );
        assert_eq!((devices[1].kind, devices[1].connected), ("phone", false));
        assert_eq!((devices[2].kind, devices[2].battery, devices[2].can_connect), ("mouse", Some(55), false));
    }

    #[test]
    fn bad_output_lists_nothing() {
        assert!(parse_devices("").is_empty());
        assert!(parse_devices(r#"{"SPBluetoothDataType":[]}"#).is_empty());
    }
}
