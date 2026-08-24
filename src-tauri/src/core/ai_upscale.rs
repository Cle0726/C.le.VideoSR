use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

use super::{
    engine::EnhancementEngine,
    models::bundled_model_catalog,
    processing::ProcessingEvent,
    realesrgan::RealEsrganNcnnEngine,
};

const CANCELLED: &str = "__C_LE_CANCELLED__";
const DEFAULT_CHUNK_SECONDS: f64 = 2.0;

#[derive(Debug, Default)]
pub struct UpscaleState {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpscaleRequest {
    pub input_path: String,
    pub output_path: String,
    pub model_id: String,
    pub video_codec: String,
    pub duration_seconds: f64,
    pub frame_rate: f64,
    pub chunk_seconds: Option<f64>,
    pub tile_size: Option<u32>,
    pub gpu_id: Option<u32>,
    pub tta: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct StartUpscaleResponse {
    pub job_id: String,
}

fn emit(
    app: &AppHandle,
    job_id: &str,
    status: &str,
    progress: f64,
    processed_seconds: f64,
    message: impl Into<Option<String>>,
) {
    let _ = app.emit(
        "job-progress",
        ProcessingEvent {
            job_id: job_id.to_string(),
            status: status.to_string(),
            progress: progress.clamp(0.0, 100.0),
            out_time_seconds: processed_seconds.max(0.0),
            speed: None,
            message: message.into(),
        },
    );
}

fn job_id() -> Result<String, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    Ok(format!("upscale-{}-{millis}", std::process::id()))
}

fn codec_args(codec: &str) -> Result<Vec<&'static str>, String> {
    match codec {
        "h264" => Ok(vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p",
        ]),
        "h265" => Ok(vec![
            "-c:v", "libx265", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
        ]),
        "copy" => Err("Video stream copy cannot be used after AI super-resolution.".into()),
        other => Err(format!("Unsupported AI output codec: {other}")),
    }
}

fn drain_stderr(stderr: ChildStderr) -> (Arc<Mutex<String>>, thread::JoinHandle<()>) {
    let last_line = Arc::new(Mutex::new(String::new()));
    let thread_line = Arc::clone(&last_line);
    let handle = thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                if let Ok(mut latest) = thread_line.lock() {
                    *latest = line;
                }
            }
        }
    });
    (last_line, handle)
}

fn child_error(last_line: &Arc<Mutex<String>>, fallback: String) -> String {
    last_line
        .lock()
        .ok()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.clone())
        .unwrap_or(fallback)
}

fn wait_child(
    mut child: Child,
    cancel: &AtomicBool,
    last_line: Arc<Mutex<String>>,
    stderr_thread: thread::JoinHandle<()>,
    label: &str,
) -> Result<(), String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_thread.join();
            return Err(CANCELLED.into());
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = stderr_thread.join();
                return if status.success() {
                    Ok(())
                } else {
                    Err(child_error(
                        &last_line,
                        format!("{label} exited with status {status}"),
                    ))
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_thread.join();
                return Err(format!("Unable to poll {label}: {error}"));
            }
        }
    }
}

fn run(mut command: Command, cancel: &AtomicBool, label: &str) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Unable to start {label}: {error}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Unable to capture {label} stderr"))?;
    let (last_line, stderr_thread) = drain_stderr(stderr);
    wait_child(child, cancel, last_line, stderr_thread, label)
}

fn png_frames(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut frames = fs::read_dir(directory)
        .map_err(|error| format!("Unable to read {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("png"))
        })
        .collect::<Vec<_>>();
    frames.sort();
    Ok(frames)
}

fn extract_command(input: &Path, start: f64, length: f64, pattern: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-ss")
        .arg(format!("{start:.6}"))
        .arg("-i")
        .arg(input)
        .arg("-t")
        .arg(format!("{length:.6}"))
        .arg("-map")
        .arg("0:v:0")
        .arg("-vsync")
        .arg("0")
        .arg(pattern);
    command
}

fn encoder_command(frame_rate: f64, codec: &str, output: &Path) -> Result<Command, String> {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-f")
        .arg("image2pipe")
        .arg("-framerate")
        .arg(format!("{frame_rate:.8}"))
        .arg("-vcodec")
        .arg("png")
        .arg("-i")
        .arg("pipe:0")
        .arg("-an");
    for arg in codec_args(codec)? {
        command.arg(arg);
    }
    command
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok(command)
}

fn mux_command(video_only: &Path, source: &Path, output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(video_only)
        .arg("-i")
        .arg(source)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a?")
        .arg("-map_metadata")
        .arg("1")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg("-b:a")
        .arg("192k")
        .arg("-shortest");

    if output
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
    {
        command.arg("-movflags").arg("+faststart");
    }
    command.arg(output);
    command
}

fn process_video(
    app: &AppHandle,
    id: &str,
    cancel: &AtomicBool,
    request: &UpscaleRequest,
    engine: &RealEsrganNcnnEngine,
    work_root: &Path,
) -> Result<(), String> {
    let input = Path::new(&request.input_path);
    let output = Path::new(&request.output_path);
    let duration = request.duration_seconds;
    let fps = request.frame_rate;
    let chunk_seconds = request.chunk_seconds.unwrap_or(DEFAULT_CHUNK_SECONDS);

    if !duration.is_finite() || duration <= 0.0 {
        return Err("AI upscale requires a known positive video duration.".into());
    }
    if !fps.is_finite() || fps <= 0.0 || fps > 240.0 {
        return Err("AI upscale requires a frame rate between 0 and 240 FPS.".into());
    }
    if !(0.25..=10.0).contains(&chunk_seconds) {
        return Err("Chunk duration must be between 0.25 and 10 seconds.".into());
    }

    fs::create_dir_all(work_root)
        .map_err(|error| format!("Unable to create temporary work directory: {error}"))?;
    let source_frames = work_root.join("source");
    let enhanced_frames = work_root.join("enhanced");
    let video_only = work_root.join("video-only.mkv");

    let mut encoder = encoder_command(fps, &request.video_codec, &video_only)?
        .spawn()
        .map_err(|error| format!("Unable to start output encoder: {error}"))?;
    let mut encoder_input = encoder
        .stdin
        .take()
        .ok_or_else(|| "Unable to open output encoder stdin.".to_string())?;
    let encoder_stderr = encoder
        .stderr
        .take()
        .ok_or_else(|| "Unable to capture output encoder stderr.".to_string())?;
    let (encoder_error, encoder_stderr_thread) = drain_stderr(encoder_stderr);

    let total_chunks = (duration / chunk_seconds).ceil().max(1.0) as usize;

    for index in 0..total_chunks {
        if cancel.load(Ordering::Relaxed) {
            let _ = encoder.kill();
            let _ = encoder.wait();
            let _ = encoder_stderr_thread.join();
            return Err(CANCELLED.into());
        }

        let start = index as f64 * chunk_seconds;
        if start >= duration {
            break;
        }
        let length = (duration - start).min(chunk_seconds);
        let processed_before = start.min(duration);
        let base = processed_before / duration * 90.0;

        let _ = fs::remove_dir_all(&source_frames);
        let _ = fs::remove_dir_all(&enhanced_frames);
        fs::create_dir_all(&source_frames)
            .map_err(|error| format!("Unable to create source frame spool: {error}"))?;
        fs::create_dir_all(&enhanced_frames)
            .map_err(|error| format!("Unable to create enhanced frame spool: {error}"))?;

        emit(
            app,
            id,
            "running",
            base,
            processed_before,
            Some(format!("Extracting chunk {}/{}", index + 1, total_chunks)),
        );
        run(
            extract_command(input, start, length, &source_frames.join("frame_%08d.png")),
            cancel,
            "FFmpeg frame extraction",
        )?;

        emit(
            app,
            id,
            "running",
            (base + 1.0).min(90.0),
            processed_before,
            Some(format!("Upscaling chunk {}/{} with NCNN/Vulkan", index + 1, total_chunks)),
        );
        run(
            engine.build_command(&source_frames, &enhanced_frames),
            cancel,
            "Real-ESRGAN NCNN",
        )?;

        let frames = png_frames(&enhanced_frames)?;
        if frames.is_empty() {
            return Err(format!("Real-ESRGAN produced no frames for chunk {}.", index + 1));
        }

        emit(
            app,
            id,
            "running",
            (base + 2.0).min(90.0),
            processed_before,
            Some(format!("Streaming chunk {}/{} into encoder", index + 1, total_chunks)),
        );

        for frame in frames {
            if cancel.load(Ordering::Relaxed) {
                let _ = encoder.kill();
                let _ = encoder.wait();
                let _ = encoder_stderr_thread.join();
                return Err(CANCELLED.into());
            }
            let bytes = fs::read(&frame)
                .map_err(|error| format!("Unable to read {}: {error}", frame.display()))?;
            encoder_input.write_all(&bytes).map_err(|error| {
                format!(
                    "Unable to stream enhanced frame to FFmpeg: {error}. {}",
                    child_error(&encoder_error, "encoder closed its input".into())
                )
            })?;
        }

        let _ = fs::remove_dir_all(&source_frames);
        let _ = fs::remove_dir_all(&enhanced_frames);
        let processed = (start + length).min(duration);
        emit(
            app,
            id,
            "running",
            processed / duration * 90.0,
            processed,
            Some(format!("Chunk {}/{} complete", index + 1, total_chunks)),
        );
    }

    drop(encoder_input);
    wait_child(
        encoder,
        cancel,
        encoder_error,
        encoder_stderr_thread,
        "output encoder",
    )?;

    emit(
        app,
        id,
        "running",
        95.0,
        duration,
        Some("Restoring source audio and metadata…".into()),
    );
    run(
        mux_command(&video_only, input, output),
        cancel,
        "FFmpeg final mux",
    )?;
    Ok(())
}

pub fn start_upscale(
    app: AppHandle,
    state: State<'_, UpscaleState>,
    request: UpscaleRequest,
) -> Result<StartUpscaleResponse, String> {
    let input = Path::new(&request.input_path);
    let output = Path::new(&request.output_path);
    if !input.is_file() {
        return Err("Input video does not exist or is not a file.".into());
    }
    if input == output {
        return Err("Output path must be different from the input video.".into());
    }
    codec_args(&request.video_codec)?;

    let model = bundled_model_catalog()?
        .models
        .into_iter()
        .find(|model| model.id == request.model_id)
        .ok_or_else(|| format!("Unknown model profile: {}", request.model_id))?;
    if model.engine != "realesrgan-ncnn-vulkan" {
        return Err(format!("Model {} does not target Real-ESRGAN NCNN.", model.id));
    }

    let mut engine = RealEsrganNcnnEngine::new("realesrgan-ncnn-vulkan", model)
        .map_err(|error| error.to_string())?
        .with_tile_size(request.tile_size.unwrap_or(0))
        .map_err(|error| error.to_string())?;
    if let Some(gpu_id) = request.gpu_id {
        engine = engine.with_gpu_id(gpu_id);
    }
    engine = engine.with_tta(request.tta.unwrap_or(false));
    engine.self_test().map_err(|error| error.to_string())?;

    let id = job_id()?;
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .map_err(|_| "Upscale state lock is poisoned.".to_string())?
        .insert(id.clone(), Arc::clone(&cancel));

    let thread_id = id.clone();
    let thread_app = app.clone();
    let work_root = std::env::temp_dir().join("c-le-videosr").join(&thread_id);

    emit(
        &app,
        &id,
        "running",
        0.0,
        0.0,
        Some("Starting bounded NCNN/Vulkan upscale pipeline…".into()),
    );

    thread::spawn(move || {
        let result = process_video(
            &thread_app,
            &thread_id,
            cancel.as_ref(),
            &request,
            &engine,
            &work_root,
        );
        let _ = fs::remove_dir_all(&work_root);
        if let Some(state) = thread_app.try_state::<UpscaleState>() {
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.remove(&thread_id);
            }
        }

        match result {
            Ok(()) => emit(
                &thread_app,
                &thread_id,
                "completed",
                100.0,
                request.duration_seconds,
                Some("AI super-resolution completed".into()),
            ),
            Err(error) if error == CANCELLED => emit(
                &thread_app,
                &thread_id,
                "cancelled",
                0.0,
                0.0,
                Some("AI super-resolution cancelled".into()),
            ),
            Err(error) => emit(
                &thread_app,
                &thread_id,
                "failed",
                0.0,
                0.0,
                Some(error),
            ),
        }
    });

    Ok(StartUpscaleResponse { job_id: id })
}

pub fn cancel_upscale(state: State<'_, UpscaleState>, job_id: String) -> Result<bool, String> {
    let cancel = state
        .jobs
        .lock()
        .map_err(|_| "Upscale state lock is poisoned.".to_string())?
        .get(&job_id)
        .cloned();

    let Some(cancel) = cancel else {
        return Ok(false);
    };
    cancel.store(true, Ordering::Relaxed);
    Ok(true)
}
