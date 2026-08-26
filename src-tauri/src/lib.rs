mod core;

use core::ai_upscale::{
    cancel_upscale as cancel_ai_job, start_upscale as start_ai_job, StartUpscaleResponse,
    UpscaleRequest, UpscaleState,
};
use core::hardware::{detect_hardware_info, HardwareInfo};
use core::interpolation::{
    cancel_interpolation as cancel_interpolation_job,
    start_interpolation as start_interpolation_job,
    InterpolationRequest,
    InterpolationState,
    StartInterpolationResponse,
};
use core::media::{probe_media as inspect_media, MediaProbe};
use core::ncnn::{detect_ncnn_runtime as inspect_ncnn_runtime, NcnnRuntimeInfo};
use core::processing::{
    cancel_processing as cancel_job, start_processing as start_job, ProcessingRequest, ProcessingState,
    StartJobResponse,
};
use core::runtime::{
    configure_managed_runtime_path, detect_media_runtime as inspect_runtime, MediaRuntimeInfo,
};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

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

#[tauri::command]
fn start_interpolation(
    app: AppHandle,
    state: State<'_, InterpolationState>,
    request: InterpolationRequest,
) -> Result<StartInterpolationResponse, String> {
    start_interpolation_job(app, state, request)
}

#[tauri::command]
fn cancel_interpolation(
    state: State<'_, InterpolationState>,
    job_id: String,
) -> Result<bool, String> {
    cancel_interpolation_job(state, job_id)
}

pub fn run() {
    let _managed_runtime = configure_managed_runtime_path();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingState::default())
        .manage(UpscaleState::default())
        .manage(InterpolationState::default())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(error) = window_vibrancy::apply_acrylic(&window, None) {
                        eprintln!("C.le.VideoSR acrylic backdrop unavailable: {error}");
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_hardware,
            detect_media_runtime,
            detect_ncnn_runtime,
            probe_media,
            start_processing,
            cancel_processing,
            start_upscale,
            cancel_upscale,
            start_interpolation,
            cancel_interpolation
        ])
        .run(tauri::generate_context!())
        .expect("error while running C.le.VideoSR");
}
