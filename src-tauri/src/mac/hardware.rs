//! Output volume, display brightness and Bluetooth power on macOS.
//!
//! Volume goes through Core Audio. Brightness and Bluetooth power have no
//! public API, so their system functions are looked up at runtime: on a macOS
//! without them the calls report an error instead of crashing.

use std::ffi::{c_void, CStr};

const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const SYSTEM_OBJECT: u32 = 1;
const DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"outp");
const VIRTUAL_MAIN_VOLUME: u32 = fourcc(b"vmvc");
const MUTE: u32 = fourcc(b"mute");

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}
#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioHardwareServiceGetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioHardwareServiceSetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}

fn output_device() -> Result<u32, String> {
    let address = PropertyAddress { selector: DEFAULT_OUTPUT, scope: SCOPE_GLOBAL, element: 0 };
    let mut device = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `device` and `size` match the property's u32 type.
    let status = unsafe {
        AudioObjectGetPropertyData(SYSTEM_OBJECT, &address, 0, std::ptr::null(), &mut size, (&mut device as *mut u32).cast())
    };
    if status == 0 && device != 0 {
        Ok(device)
    } else {
        Err(format!("no output device ({status})"))
    }
}

/// Output volume, 0.0 to 1.0.
pub fn volume() -> Result<f32, String> {
    let device = output_device()?;
    let address = PropertyAddress { selector: VIRTUAL_MAIN_VOLUME, scope: SCOPE_OUTPUT, element: 0 };
    let mut value = 0f32;
    let mut size = std::mem::size_of::<f32>() as u32;
    // SAFETY: the virtual main volume is an f32.
    let status = unsafe {
        AudioHardwareServiceGetPropertyData(device, &address, 0, std::ptr::null(), &mut size, (&mut value as *mut f32).cast())
    };
    (status == 0).then_some(value).ok_or_else(|| format!("volume unreadable ({status})"))
}

pub fn set_volume(value: f32) -> Result<(), String> {
    let device = output_device()?;
    let address = PropertyAddress { selector: VIRTUAL_MAIN_VOLUME, scope: SCOPE_OUTPUT, element: 0 };
    let value = value.clamp(0.0, 1.0);
    // SAFETY: the virtual main volume is an f32.
    let status = unsafe {
        AudioHardwareServiceSetPropertyData(device, &address, 0, std::ptr::null(), std::mem::size_of::<f32>() as u32, (&value as *const f32).cast())
    };
    if status != 0 {
        return Err(format!("volume not settable ({status})"));
    }
    // Moving the slider up from silence unmutes, as the menu bar slider does.
    if value > 0.0 && muted().unwrap_or(false) {
        set_muted(false)?;
    }
    Ok(())
}

pub fn muted() -> Result<bool, String> {
    let device = output_device()?;
    let address = PropertyAddress { selector: MUTE, scope: SCOPE_OUTPUT, element: 0 };
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: mute is a u32.
    let status = unsafe {
        AudioObjectGetPropertyData(device, &address, 0, std::ptr::null(), &mut size, (&mut value as *mut u32).cast())
    };
    (status == 0).then_some(value != 0).ok_or_else(|| format!("mute unreadable ({status})"))
}

pub fn set_muted(muted: bool) -> Result<(), String> {
    let device = output_device()?;
    let address = PropertyAddress { selector: MUTE, scope: SCOPE_OUTPUT, element: 0 };
    let value = muted as u32;
    // SAFETY: mute is a u32.
    let status = unsafe {
        AudioObjectSetPropertyData(device, &address, 0, std::ptr::null(), std::mem::size_of::<u32>() as u32, (&value as *const u32).cast())
    };
    (status == 0).then_some(()).ok_or_else(|| format!("mute not settable ({status})"))
}

// ---- Private system functions, looked up at runtime ----

extern "C" {
    fn dlopen(path: *const i8, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
}
const RTLD_LAZY: i32 = 1;

fn symbol(library: &CStr, name: &CStr) -> Result<*mut c_void, String> {
    // SAFETY: both strings are NUL-terminated; a null result is checked.
    unsafe {
        let handle = dlopen(library.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            return Err(format!("{} unavailable", library.to_string_lossy()));
        }
        let f = dlsym(handle, name.as_ptr());
        if f.is_null() {
            Err(format!("{} unavailable", name.to_string_lossy()))
        } else {
            Ok(f)
        }
    }
}

const DISPLAY_SERVICES: &CStr = c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices";
const IOBLUETOOTH: &CStr = c"/System/Library/Frameworks/IOBluetooth.framework/IOBluetooth";

/// Built-in display brightness, 0 to 100.
pub fn brightness() -> Result<u32, String> {
    let f = symbol(DISPLAY_SERVICES, c"DisplayServicesGetBrightness")?;
    let get: extern "C" fn(u32, *mut f32) -> i32 = unsafe { std::mem::transmute(f) };
    let mut value = 0f32;
    // SAFETY: matches DisplayServicesGetBrightness(CGDirectDisplayID, float *).
    let status = get(unsafe { CGMainDisplayID() }, &mut value);
    (status == 0).then(|| (value * 100.0).round() as u32).ok_or_else(|| format!("brightness unreadable ({status})"))
}

pub fn set_brightness(percent: u32) -> Result<(), String> {
    let f = symbol(DISPLAY_SERVICES, c"DisplayServicesSetBrightness")?;
    let set: extern "C" fn(u32, f32) -> i32 = unsafe { std::mem::transmute(f) };
    // SAFETY: matches DisplayServicesSetBrightness(CGDirectDisplayID, float).
    let status = set(unsafe { CGMainDisplayID() }, percent.min(100) as f32 / 100.0);
    (status == 0).then_some(()).ok_or_else(|| format!("brightness not settable ({status})"))
}

/// macOS ends any process that touches Bluetooth unless it runs from an .app
/// whose Info.plist says why (src-tauri/Info.plist); a development build run
/// as a bare binary would be killed.
pub fn bluetooth_allowed() -> bool {
    use objc2_foundation::{NSBundle, NSString};
    let bundle = NSBundle::mainBundle();
    bundle.bundlePath().to_string().ends_with(".app")
        && bundle
            .objectForInfoDictionaryKey(&NSString::from_str("NSBluetoothAlwaysUsageDescription"))
            .is_some()
}

/// Reads the state without the Bluetooth API: `system_profiler` reports it
/// for any process.
fn bluetooth_powered_from_profiler() -> Result<bool, String> {
    let output = std::process::Command::new("/usr/sbin/system_profiler")
        .args(["SPBluetoothDataType"])
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|l| l.trim().strip_prefix("State: "))
        .map(|state| state.trim() == "On")
        .ok_or_else(|| "Bluetooth state not found".to_string())
}

/// Loads IOBluetooth so its Objective-C classes can be looked up.
pub fn load_iobluetooth() -> Result<(), String> {
    // SAFETY: NUL-terminated path; a null handle is reported.
    let handle = unsafe { dlopen(IOBLUETOOTH.as_ptr(), RTLD_LAZY) };
    (!handle.is_null()).then_some(()).ok_or_else(|| "IOBluetooth unavailable".into())
}

pub fn bluetooth_powered() -> Result<bool, String> {
    if !bluetooth_allowed() {
        return bluetooth_powered_from_profiler();
    }
    let f = symbol(IOBLUETOOTH, c"IOBluetoothPreferenceGetControllerPowerState")?;
    let get: extern "C" fn() -> i32 = unsafe { std::mem::transmute(f) };
    Ok(get() != 0)
}

pub fn set_bluetooth_powered(on: bool) -> Result<(), String> {
    if !bluetooth_allowed() {
        return Err("Bluetooth control needs the bundled Bloom.app".into());
    }
    let f = symbol(IOBLUETOOTH, c"IOBluetoothPreferenceSetControllerPowerState")?;
    let set: extern "C" fn(i32) = unsafe { std::mem::transmute(f) };
    set(on as i32);
    Ok(())
}

#[cfg(test)]
mod live {
    /// Reads the real devices; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_volume_brightness_and_bluetooth() {
        let volume = super::volume().unwrap();
        assert!((0.0..=1.0).contains(&volume));
        println!("volume {volume} muted {:?}", super::muted());
        println!("brightness {:?}", super::brightness());
        println!("bluetooth {:?}", super::bluetooth_powered());
    }
}
