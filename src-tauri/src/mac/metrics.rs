//! CPU, memory, disk and network figures for the island's status widgets, in
//! the units the Windows commands return.

use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{Disks, Networks, System};

static SYSTEM: Mutex<Option<System>> = Mutex::new(None);
static NETWORKS: Mutex<Option<(Networks, Instant)>> = Mutex::new(None);

/// CPU use since the previous call, 0-100 (the first call reads ~0).
pub fn cpu_percent() -> u32 {
    let Ok(mut system) = SYSTEM.lock() else { return 0 };
    let system = system.get_or_insert_with(System::new);
    system.refresh_cpu_usage();
    system.global_cpu_usage().round().clamp(0.0, 100.0) as u32
}

/// Memory in use, 0-100.
pub fn ram_percent() -> f32 {
    let Ok(mut system) = SYSTEM.lock() else { return 0.0 };
    let system = system.get_or_insert_with(System::new);
    system.refresh_memory();
    let total = system.total_memory();
    if total == 0 {
        return 0.0;
    }
    (system.used_memory() as f64 / total as f64 * 100.0).round() as f32
}

/// Free space on the startup disk, in whole GiB.
pub fn free_disk_gb() -> Result<u64, String> {
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .find(|d| d.mount_point() == std::path::Path::new("/"))
        .map(|d| d.available_space() / (1024 * 1024 * 1024))
        .ok_or_else(|| "startup disk not found".into())
}

/// Bytes per second received and sent since the previous call.
pub fn network_bytes_per_second() -> (u64, u64) {
    let Ok(mut state) = NETWORKS.lock() else { return (0, 0) };
    let Some((networks, last)) = state.as_mut() else {
        *state = Some((Networks::new_with_refreshed_list(), Instant::now()));
        return (0, 0);
    };
    networks.refresh(true);
    let elapsed = last.elapsed().as_secs_f64().max(0.001);
    *last = Instant::now();
    let (down, up) = networks
        .iter()
        .filter(|(name, _)| !name.starts_with("lo"))
        .fold((0u64, 0u64), |(d, u), (_, data)| (d + data.received(), u + data.transmitted()));
    ((down as f64 / elapsed) as u64, (up as f64 / elapsed) as u64)
}
