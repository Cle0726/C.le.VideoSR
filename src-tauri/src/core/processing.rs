use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Default)]
pub struct ProcessingState {
    jobs: Mutex<HashMap<String, Arc<Mutex<Child>>>>,
}

#[derive(Debug, Deserialize)]
pub struct ProcessingRequest {
    pub input_path: String,
    pub output_path: String,
    pub video_codec: String,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StartJobResponse {
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessingEvent {
    pub job_id: String,
    pub status: String,
    pub progress: f64,
    pub out_time_seconds: f64,
    pub speed: Option<String>,
    pub message: Option<String>,
}

fn codec_args(codec: &str) -> Result<Vec<&'static str>, String> {
    match codec {
        "h264" => Ok(vec!["-c:v", "libx264", "-preset", "medium", "-crf", "18"]),
        "h265" => Ok(vec!["-c:v", "libx265", "-preset", "medium", "-crf", "20"]),
        "copy" => Ok(vec!["-c:v", "copy"]),
        _ => Err(format!("Unsupported video codec: {codec}")),
    }
}

fn emit_event(app: &AppHandle, event: ProcessingEvent) {
    let _ = app.emit("job-progress", event);
}

pub fn start_processing(
    app: AppHandle,
    state: State<'_, ProcessingState>,
    request: ProcessingRequest,
) -> Result<StartJobResponse, String> {
    let input = Path::new(&request.input_path);
    let output = Path::new(&request.output_path);

    if !input.is_file() {
        return Err("Input video does not exist or is not a file.".into());
    }

    if input == output {
        return Err("Output path must be different from the input video.".into());
    }

    let codec = codec_args(&request.video_codec)?;

    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("0:a?");

    for arg in codec {
        command.arg(arg);
    }

    command
        .arg("-c:a")
        .arg("copy")
        .arg("-progress")
        .arg("pipe:1")
        .arg("-nostats")
        .arg(output)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|error| format!("Unable to start FFmpeg. Ensure ffmpeg is installed and available in PATH. {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Unable to read FFmpeg progress output.".to_string())?;

    let job_id = format!(
        "job-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis()
    );

    let child = Arc::new(Mutex::new(child));
    state
        .jobs
        .lock()
        .map_err(|_| "Processing state lock is poisoned.".to_string())?
        .insert(job_id.clone(), Arc::clone(&child));

    let thread_job_id = job_id.clone();
    let duration = request.duration_seconds.unwrap_or(0.0).max(0.0);
    let thread_app = app.clone();

    emit_event(
        &app,
        ProcessingEvent {
            job_id: job_id.clone(),
            status: "running".into(),
            progress: 0.0,
            out_time_seconds: 0.0,
            speed: None,
            message: Some("FFmpeg job started".into()),
        },
    );

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut out_time_seconds = 0.0;
        let mut speed: Option<String> = None;

        for line in reader.lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_us=") {
                if let Ok(microseconds) = value.parse::<f64>() {
                    out_time_seconds = microseconds / 1_000_000.0;
                }
            } else if let Some(value) = line.strip_prefix("speed=") {
                speed = Some(value.to_string());
            } else if line.starts_with("progress=") {
                let progress = if duration > 0.0 {
                    (out_time_seconds / duration * 100.0).clamp(0.0, 99.9)
                } else {
                    0.0
                };

                emit_event(
                    &thread_app,
                    ProcessingEvent {
                        job_id: thread_job_id.clone(),
                        status: "running".into(),
                        progress,
                        out_time_seconds,
                        speed: speed.clone(),
                        message: None,
                    },
                );
            }
        }

        let exit_status = child.lock().ok().and_then(|mut child| child.wait().ok());

        if let Ok(state) = thread_app.try_state::<ProcessingState>() {
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.remove(&thread_job_id);
            }
        }

        match exit_status {
            Some(status) if status.success() => emit_event(
                &thread_app,
                ProcessingEvent {
                    job_id: thread_job_id,
                    status: "completed".into(),
                    progress: 100.0,
                    out_time_seconds: duration.max(out_time_seconds),
                    speed,
                    message: Some("Processing completed".into()),
                },
            ),
            Some(status) => emit_event(
                &thread_app,
                ProcessingEvent {
                    job_id: thread_job_id,
                    status: "failed".into(),
                    progress: if duration > 0.0 {
                        (out_time_seconds / duration * 100.0).clamp(0.0, 99.9)
                    } else {
                        0.0
                    },
                    out_time_seconds,
                    speed,
                    message: Some(format!("FFmpeg exited with status {status}")),
                },
            ),
            None => emit_event(
                &thread_app,
                ProcessingEvent {
                    job_id: thread_job_id,
                    status: "failed".into(),
                    progress: 0.0,
                    out_time_seconds,
                    speed,
                    message: Some("Unable to read FFmpeg exit status".into()),
                },
            ),
        }
    });

    Ok(StartJobResponse { job_id })
}

pub fn cancel_processing(state: State<'_, ProcessingState>, job_id: String) -> Result<bool, String> {
    let child = {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "Processing state lock is poisoned.".to_string())?;
        jobs.get(&job_id).cloned()
    };

    let Some(child) = child else {
        return Ok(false);
    };

    child
        .lock()
        .map_err(|_| "FFmpeg process lock is poisoned.".to_string())?
        .kill()
        .map_err(|error| format!("Unable to cancel FFmpeg job: {error}"))?;

    Ok(true)
}
