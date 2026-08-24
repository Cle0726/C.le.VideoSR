mod core;

use core::hardware::{detect_hardware_info, HardwareInfo};

#[tauri::command]
fn detect_hardware() -> HardwareInfo {
    detect_hardware_info()
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![detect_hardware])
        .run(tauri::generate_context!())
        .expect("error while running C.le.VideoSR");
}
