//! Wi-Fi on macOS through CoreWLAN, in the shapes the Windows commands use.
//!
//! macOS only reveals network names to apps allowed to use location; without
//! that, the current network has no name and scans come back unnamed, which
//! is reported as `location_denied` like Windows does.

use crate::types::WifiStatus;
use objc2::rc::Retained;
use objc2_core_wlan::{CWInterface, CWNetwork, CWSecurity, CWWiFiClient};
use objc2_foundation::NSString;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u32,
    pub security: &'static str,
    pub connected: bool,
    pub saved: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct WifiNetworkList {
    pub networks: Vec<WifiNetwork>,
    pub location_denied: bool,
}

/// RSSI (dBm) to the 0-100 quality Windows reports: -100 dBm or weaker is 0,
/// -50 dBm or stronger is 100.
pub fn signal_quality(rssi: i64) -> u32 {
    ((rssi + 100) * 2).clamp(0, 100) as u32
}

/// The Windows security name for the strongest scheme a network supports.
pub fn security_name(supports: impl Fn(CWSecurity) -> bool) -> &'static str {
    if supports(CWSecurity::Enterprise)
        || supports(CWSecurity::WPA2Enterprise)
        || supports(CWSecurity::WPA3Enterprise)
        || supports(CWSecurity::WPAEnterprise)
    {
        "enterprise"
    } else if supports(CWSecurity::WPA3Personal) || supports(CWSecurity::WPA3Transition) {
        "wpa3"
    } else if supports(CWSecurity::WPA2Personal) || supports(CWSecurity::Personal) {
        "wpa2"
    } else if supports(CWSecurity::WPAPersonal) || supports(CWSecurity::WPAPersonalMixed) {
        "wpa"
    } else if supports(CWSecurity::WEP) || supports(CWSecurity::DynamicWEP) {
        "wep"
    } else if supports(CWSecurity::OWE) || supports(CWSecurity::OWETransition) {
        "owe"
    } else if supports(CWSecurity::None) {
        "open"
    } else {
        "other"
    }
}

fn interface() -> Result<Retained<CWInterface>, String> {
    // SAFETY: CoreWLAN's shared client and its default interface.
    unsafe { CWWiFiClient::sharedWiFiClient().interface() }.ok_or_else(|| "no Wi-Fi hardware".into())
}

pub fn status() -> Result<WifiStatus, String> {
    let wifi = interface()?;
    // SAFETY: plain property reads on a live interface.
    unsafe {
        let enabled = wifi.powerOn();
        let ssid = wifi.ssid().map(|s| s.to_string());
        let connected = enabled && (ssid.is_some() || wifi.rssiValue() != 0);
        Ok(WifiStatus { enabled, connected, ssid })
    }
}

pub fn set_power(on: bool) -> Result<(), String> {
    // SAFETY: plain call on a live interface.
    unsafe { interface()?.setPower_error(on) }.map_err(|e| e.localizedDescription().to_string())
}

fn scan(name: Option<&str>) -> Result<Vec<Retained<CWNetwork>>, String> {
    let wifi = interface()?;
    let name = name.map(NSString::from_str);
    // SAFETY: blocking scan on a live interface; the set is copied out.
    unsafe { wifi.scanForNetworksWithName_error(name.as_deref()) }
        .map(|set| set.allObjects().to_vec())
        .map_err(|e| e.localizedDescription().to_string())
}

pub fn list() -> Result<WifiNetworkList, String> {
    let current = status().ok().and_then(|s| s.ssid);
    let found = scan(None)?;
    let unnamed = found.iter().all(|n| unsafe { n.ssid() }.is_none());
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for network in &found {
        // SAFETY: plain property reads on a scanned network.
        let (ssid, rssi) = unsafe { (network.ssid(), network.rssiValue()) };
        let Some(ssid) = ssid.map(|s| s.to_string()) else { continue };
        let signal = signal_quality(rssi as i64);
        // The same name on several access points: keep the strongest.
        if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ssid) {
            existing.signal = existing.signal.max(signal);
            continue;
        }
        networks.push(WifiNetwork {
            connected: current.as_deref() == Some(ssid.as_str()),
            security: security_name(|s| unsafe { network.supportsSecurity(s) }),
            saved: false,
            signal,
            ssid,
        });
    }
    networks.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
    Ok(WifiNetworkList { networks, location_denied: unnamed && !found.is_empty() })
}

pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let network = scan(Some(ssid))?
        .into_iter()
        .max_by_key(|n| unsafe { n.rssiValue() })
        .ok_or_else(|| format!("{ssid} is not in range"))?;
    let password = password.map(NSString::from_str);
    // SAFETY: blocking association on a live interface with a scanned network.
    unsafe { interface()?.associateToNetwork_password_error(&network, password.as_deref()) }
        .map_err(|e| e.localizedDescription().to_string())
}

pub fn disconnect() -> Result<(), String> {
    // SAFETY: plain call on a live interface.
    unsafe { interface()?.disassociate() };
    Ok(())
}

/// Removes a saved network (macOS may ask for an administrator password).
pub fn forget(ssid: &str) -> Result<(), String> {
    let device = unsafe { interface()?.interfaceName() }
        .map(|n| n.to_string())
        .ok_or("no Wi-Fi interface")?;
    let status = std::process::Command::new("/usr/sbin/networksetup")
        .args(["-removepreferredwirelessnetwork", &device, ssid])
        .status()
        .map_err(|e| e.to_string())?;
    status.success().then_some(()).ok_or_else(|| format!("could not forget {ssid}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rssi_maps_to_windows_quality() {
        assert_eq!(signal_quality(-100), 0);
        assert_eq!(signal_quality(-75), 50);
        assert_eq!(signal_quality(-40), 100);
        assert_eq!(signal_quality(-120), 0);
    }

    #[test]
    fn security_names_pick_the_strongest_scheme() {
        let has = |list: &'static [CWSecurity]| move |s: CWSecurity| list.contains(&s);
        assert_eq!(security_name(has(&[CWSecurity::WPA2Personal, CWSecurity::WPA3Transition])), "wpa3");
        assert_eq!(security_name(has(&[CWSecurity::WPA2Personal])), "wpa2");
        assert_eq!(security_name(has(&[CWSecurity::WPA2Enterprise])), "enterprise");
        assert_eq!(security_name(has(&[CWSecurity::None])), "open");
        assert_eq!(security_name(has(&[])), "other");
    }
}
