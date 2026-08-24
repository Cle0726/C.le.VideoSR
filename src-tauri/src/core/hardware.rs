use serde::Serialize;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub total_memory_mb: u64,
    pub gpu_hint: Option<String>,
}

pub fn detect_hardware_info() -> HardwareInfo {
    let mut system = System::new_all();
    system.refresh_all();

    HardwareInfo {
        os: System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: system.cpus().len(),
        total_memory_mb: system.total_memory() / 1024 / 1024,
        gpu_hint: None,
    }
}
