mod core;

use core::hardware::{detect_hardware_info, HardwareInfo};
use core::media::{probe_media as inspect_media, MediaProbe};
use core::ncnn::{detect_ncnn_runtime as inspect_ncnn_runtime, NcnnRuntimeInfo};
use core::processing::{
    cancel_processing as cancel_job, start_processing as start_job, ProcessingRequest, ProcessingState,
    StartJobResponse,
};
use core::runtime::{detect_media_runtime as inspect_runtime, MediaRuntimeInfo};
use core::upscale::{
    cancel_upscale as cancel_ai_job, start_upscale as start_ai_job, StartUpscaleResponse,
    UpscaleRequest, UpscaleState,
};
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
fn detect_hardware() -> HardwareInfo {
    detect_hardware_info()
}

#[tauri::command]
fn detect_media_runtime() -> MediaRuntimeInfo {
    inspect_runtime()
}

#[tauri::command]
fn detect_ncnn_runtime() -> NcnnRuntimeInfo {
    inspect_ncnn_runtime()
}

#[tauri::command]
fn probe_media(path: String) -> Result<MediaProbe, String> {
    inspect_media(&PathBuf::from(path))
}

#[tauri::command]
fn start_processing(
    app: AppHandle,
    state: State<'_, ProcessingState>,
    request: ProcessingRequest,
) -> Result<StartJobResponse, String> {
    start_job(app, state, request)
}

#[tauri::command]
fn cancel_processing(state: State<'_, ProcessingState>, job_id: String) -> Result<bool, String> {
    cancel_job(state, job_id)
}

#[tauri::command]
fn start_upscale(
    app: AppHandle,
    state: State<'_, UpscaleState>,
    request: UpscaleRequest,
) -> Result<StartUpscaleResponse, String> {
    start_ai_job(app, state, request)
}

#[tauri::command]
fn cancel_upscale(state: State<'_, UpscaleState>, job_id: String) -> Result<bool, String> {
    cancel_ai_job(state, job_id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingState::default())
        .manage(UpscaleState::default())
        .invoke_handler(tauri::generate_handler![
            detect_hardware,
            detect_media_runtime,
            detect_ncnn_runtime,
            probe_media,
            start_processing,
            cancel_processing,
            start_upscale,
            cancel_upscale
        ])
        .run(tauri::generate_context!())
        .expect("error while running C.le.VideoSR");
}
