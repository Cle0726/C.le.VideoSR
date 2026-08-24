use serde::Serialize;
use std::process::Command;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub memory_mb: Option<u64>,
    pub memory_kind: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub total_memory_mb: u64,
    pub gpu_hint: Option<String>,
    pub gpu: Option<GpuInfo>,
    pub recommended_ncnn_tile: u32,
}

fn detect_nvidia_smi() -> Option<GpuInfo> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let first = text.lines().find(|line| !line.trim().is_empty())?;
    let mut fields = first.split(',').map(str::trim);
    let name = fields.next()?.to_string();
    let memory_mb = fields.next().and_then(|value| value.parse::<u64>().ok());

    Some(GpuInfo {
        name,
        memory_mb,
        memory_kind: "dedicated".into(),
        source: "nvidia-smi".into(),
    })
}

#[cfg(target_os = "linux")]
fn detect_linux_drm_vram() -> Option<GpuInfo> {
    let drm = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in drm.filter_map(Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }

        let device = entry.path().join("device");
        let bytes = std::fs::read_to_string(device.join("mem_info_vram_total"))
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok());
        let Some(bytes) = bytes else {
            continue;
        };

        let vendor = std::fs::read_to_string(device.join("vendor"))
            .ok()
            .map(|value| value.trim().to_ascii_lowercase());
        let label = match vendor.as_deref() {
            Some("0x1002") => "AMD GPU",
            Some("0x10de") => "NVIDIA GPU",
            Some("0x8086") => "Intel GPU",
            _ => "DRM GPU",
        };

        return Some(GpuInfo {
            name: label.into(),
            memory_mb: Some(bytes / 1024 / 1024),
            memory_kind: "dedicated".into(),
            source: "linux-drm-sysfs".into(),
        });
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn detect_linux_drm_vram() -> Option<GpuInfo> {
    None
}

#[cfg(target_os = "windows")]
fn detect_windows_wmi() -> Option<GpuInfo> {
    let script = "Get-CimInstance Win32_VideoController | Select-Object -First 1 Name,AdapterRAM | ConvertTo-Json -Compress";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let name = value.get("Name")?.as_str()?.to_string();
    let memory_mb = value
        .get("AdapterRAM")
        .and_then(|value| value.as_u64())
        .map(|bytes| bytes / 1024 / 1024)
        .filter(|value| *value > 0);

    Some(GpuInfo {
        name,
        memory_mb,
        memory_kind: "estimated-dedicated".into(),
        source: "windows-wmi".into(),
    })
}

#[cfg(not(target_os = "windows"))]
fn detect_windows_wmi() -> Option<GpuInfo> {
    None
}

#[cfg(target_os = "macos")]
fn detect_macos_unified_memory(total_memory_mb: u64) -> Option<GpuInfo> {
    if std::env::consts::ARCH != "aarch64" {
        return None;
    }
    Some(GpuInfo {
        name: "Apple Silicon GPU".into(),
        memory_mb: Some(total_memory_mb),
        memory_kind: "unified".into(),
        source: "system-unified-memory".into(),
    })
}

#[cfg(not(target_os = "macos"))]
fn detect_macos_unified_memory(_total_memory_mb: u64) -> Option<GpuInfo> {
    None
}

fn detect_gpu(total_memory_mb: u64) -> Option<GpuInfo> {
    detect_nvidia_smi()
        .or_else(detect_linux_drm_vram)
        .or_else(detect_windows_wmi)
        .or_else(|| detect_macos_unified_memory(total_memory_mb))
}

pub fn recommend_ncnn_tile(gpu: Option<&GpuInfo>) -> u32 {
    let Some(gpu) = gpu else {
        return 0;
    };
    if gpu.memory_kind == "unified" {
        return 0;
    }

    match gpu.memory_mb {
        Some(memory) if memory <= 2_048 => 128,
        Some(memory) if memory <= 4_096 => 256,
        _ => 0,
    }
}

pub fn detect_hardware_info() -> HardwareInfo {
    let mut system = System::new_all();
    system.refresh_all();
    let total_memory_mb = system.total_memory() / 1024 / 1024;
    let gpu = detect_gpu(total_memory_mb);
    let recommended_ncnn_tile = recommend_ncnn_tile(gpu.as_ref());
    let gpu_hint = gpu.as_ref().map(|info| match info.memory_mb {
        Some(memory) => format!("{} · {} MB", info.name, memory),
        None => info.name.clone(),
    });

    HardwareInfo {
        os: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: system.cpus().len(),
        total_memory_mb,
        gpu_hint,
        gpu,
        recommended_ncnn_tile,
    }
}
