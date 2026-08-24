mod core;

use core::hardware::{detect_hardware_info, HardwareInfo};
use core::media::{probe_media as inspect_media, MediaProbe};
use std::path::PathBuf;

#[tauri::command]
fn detect_hardware() -> HardwareInfo {
    detect_hardware_info()
}

#[tauri::command]
fn probe_media(path: String) -> Result<MediaProbe, String> {
    inspect_media(&PathBuf::from(path))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![detect_hardware, probe_media])
        .run(tauri::generate_context!())
        .expect("error while running C.le.VideoSR");
}
