use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
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
use tauri::{AppHandle, Emitter, State};

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

fn emit_progress(
    app: &AppHandle,
    job_id: &str,
    status: &str,
    progress: f64,
    out_time_seconds: f64,
    message: Option<String>,
) {
    let _ = app.emit(
        "job-progress",
        ProcessingEvent {
            job_id: job_id.to_string(),
            status: status.to_string(),
            progress: progress.clamp(0.0, 100.0),
            out_time_seconds,
            speed: None,
            message,
        },
    );
}

fn next_job_id() -> Result<String, String> {
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
        _ => Err(format!("Unsupported video codec: {codec}")),
    }
}

fn stderr_collector(
    stderr: ChildStderr,
) -> (
    Arc<Mutex<VecDeque<String>>>,
    thread::JoinHandle<()>,
) {
    let tail = Arc::new(Mutex::new(VecDeque::with_capacity(24)));
    let thread_tail = Arc::clone(&tail);
    let handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(mut lines) = thread_tail.lock() {
                if lines.len() >= 24 {
                    lines.pop_front();
                }
                lines.push_back(line);
            }
        }
    });
    (tail, handle)
}

fn stderr_message(tail: &Arc<Mutex<VecDeque<String>>>, fallback: &str) -> String {
    tail.lock()
        .ok()
        .and_then(|lines| {
            lines
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned()
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn wait_child(
    mut child: Child,
    cancel: &AtomicBool,
    tail: Arc<Mutex<VecDeque<String>>>,
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
                if status.success() {
                    return Ok(());
                }
                return Err(stderr_message(
                    &tail,
                    &format!("{label} exited with status {status}"),
                ));
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

fn run_command(mut command: Command, cancel: &AtomicBool, label: &str) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Unable to start {label}: {error}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Unable to capture {label} stderr"))?;
    let (tail, stderr_thread) = stderr_collector(stderr);
    wait_child(child, cancel, tail, stderr_thread, label)
}

fn sorted_pngs(directory: &Path) -> Result<Vec<PathBuf>, String> {
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

fn make_extract_command(
    input: &Path,
    start: f64,
    duration: f64,
    output_pattern: &Path,
) -> Command {
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
        .arg(format!("{duration:.6}"))
        .arg("-map")
        .arg("0:v:0")
        .arg("-fps_mode")
        .arg("passthrough")
        .arg(output_pattern);
    command
}

fn make_encoder_command(frame_rate: f64, codec: &str, output: &Path) -> Result<Command, String> {
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

fn make_mux_command(video_only: &Path, source: &Path, output: &Path) -> Command {
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

fn run_upscale(
    app: &AppHandle,
    job_id: &str,
    cancel: &AtomicBool,
    request: &UpscaleRequest,
    engine: &RealEsrganNcnnEngine,
    work_root: &Path,
) -> Result<(), String> {
    let input = Path::new(&request.input_path);
    let output = Path::new(&request.output_path);
    let duration = request.duration_seconds;
    let frame_rate = request.frame_rate;
    let chunk_seconds = request.chunk_seconds.unwrap_or(DEFAULT_CHUNK_SECONDS);

    if !(duration.is_finite() && duration > 0.0) {
        return Err("AI upscale requires a known positive video duration.".into());
    }
    if !(frame_rate.is_finite() && frame_rate > 0.0 && frame_rate <= 240.0) {
        return Err("AI upscale requires a valid frame rate between 0 and 240 FPS.".into());
    }
    if !(0.25..=10.0).contains(&chunk_seconds) {
        return Err("Chunk duration must be between 0.25 and 10 seconds.".into());
    }
    if input == output {
        return Err("Output path must be different from the input video.".into());
    }

    fs::create_dir_all(work_root)
        .map_err(|error| format!("Unable to create temporary work directory: {error}"))?;

    let input_frames = work_root.join("input-frames");
    let output_frames = work_root.join("output-frames");
    let video_only = work_root.join("enhanced-video.mkv");

    engine.self_test().map_err(|error| error.to_string())?;

    let mut encoder = make_encoder_command(frame_rate, &request.video_codec, &video_only)?
        .spawn()
        .map_err(|error| format!("Unable to start enhanced-video encoder: {error}"))?;
    let mut encoder_stdin = encoder
        .stdin
        .take()
        .ok_or_else(|| "Unable to open enhanced-video encoder stdin.".to_string())?;
    let encoder_stderr = encoder
        .stderr
        .take()
        .ok_or_else(|| "Unable to capture enhanced-video encoder stderr.".to_string())?;
    let (encoder_tail, encoder_stderr_thread) = stderr_collector(encoder_stderr);

    let total_chunks = (duration / chunk_seconds).ceil().max(1.0) as usize;

    for chunk_index in 0..total_chunks {
        if cancel.load(Ordering::Relaxed) {
            let _ = encoder.kill();
            let _ = encoder.wait();
            let _ = encoder_stderr_thread.join();
            return Err(CANCELLED.into());
        }

        let start = chunk_index as f64 * chunk_seconds;
        if start >= duration {
            break;
        }
        let current_duration = (duration - start).min(chunk_seconds);
        let processed_before = start.min(duration);
        let base_progress = processed_before / duration * 90.0;

        let _ = fs::remove_dir_all(&input_frames);
        let _ = fs::remove_dir_all(&output_frames);
        fs::create_dir_all(&input_frames)
            .map_err(|error| format!("Unable to create input frame spool: {error}"))?;
        fs::create_dir_all(&output_frames)
            .map_err(|error| format!("Unable to create output frame spool: {error}"))?;

        emit_progress(
            app,
            job_id,
            "running",
            base_progress,
            processed_before,
            Some(format!("Extracting chunk {}/{}", chunk_index + 1, total_chunks)),
        );

        let pattern = input_frames.join("frame_%08d.png");
        run_command(
            make_extract_command(input, start, current_duration, &pattern),
            cancel,
            "FFmpeg frame extraction",
        )?;

        emit_progress(
            app,
            job_id,
            "running",
            (base_progress + 1.0).min(90.0),
            processed_before,
            Some(format!("Upscaling chunk {}/{} with NCNN/Vulkan", chunk_index + 1, total_chunks)),
        );

        run_command(
            engine.build_command(&input_frames, &output_frames),
            cancel,
            "Real-ESRGAN NCNN",
        )?;

        let frames = sorted_pngs(&output_frames)?;
        if frames.is_empty() {
            return Err(format!(
                "Real-ESRGAN produced no PNG frames for chunk {}.",
                chunk_index + 1
            ));
        }

        emit_progress(
            app,
            job_id,
            "running",
            (base_progress + 2.0).min(90.0),
            processed_before,
            Some(format!("Encoding chunk {}/{}", chunk_index + 1, total_chunks)),
        );

        for frame in frames {
            if cancel.load(Ordering::Relaxed) {
                let _ = encoder.kill();
                let _ = encoder.wait();
                let _ = encoder_stderr_thread.join();
                return Err(CANCELLED.into());
            }
            let bytes = fs::read(&frame)
                .map_err(|error| format!("Unable to read enhanced frame {}: {error}", frame.display()))?;
            encoder_stdin.write_all(&bytes).map_err(|error| {
                let detail = stderr_message(&encoder_tail, "enhanced-video encoder closed its input");
                format!("Unable to stream enhanced frame to FFmpeg: {error}. {detail}")
            })?;
        }

        let _ = fs::remove_dir_all(&input_frames);
        let _ = fs::remove_dir_all(&output_frames);

        let processed = (start + current_duration).min(duration);
        emit_progress(
            app,
            job_id,
            "running",
            processed / duration * 90.0,
            processed,
            Some(format!("Chunk {}/{} complete", chunk_index + 1, total_chunks)),
        );
    }

    drop(encoder_stdin);
    wait_child(
        encoder,
        cancel,
        encoder_tail,
        encoder_stderr_thread,
        "enhanced-video encoder",
    )?;

    emit_progress(
        app,
        job_id,
        "running",
        95.0,
        duration,
        Some("Restoring source audio and metadata…".into()),
    );

    run_command(
        make_mux_command(&video_only, input, output),
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

    let catalog = bundled_model_catalog()?;
    let model = catalog
        .models
        .into_iter()
        .find(|model| model.id == request.model_id)
        .ok_or_else(|| format!("Unknown model profile: {}", request.model_id))?;

    if model.engine != "realesrgan-ncnn-vulkan" {
        return Err(format!(
            "Model {} is not compatible with the Real-ESRGAN NCNN backend.",
            model.id
        ));
    }

    let mut engine = RealEsrganNcnnEngine::new("realesrgan-ncnn-vulkan", model)
        .map_err(|error| error.to_string())?;
    engine = engine
        .with_tile_size(request.tile_size.unwrap_or(0))
        .map_err(|error| error.to_string())?;
    if let Some(gpu_id) = request.gpu_id {
        engine = engine.with_gpu_id(gpu_id);
    }
    engine = engine.with_tta(request.tta.unwrap_or(false));

    engine.self_test().map_err(|error| error.to_string())?;

    let job_id = next_job_id()?;
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .map_err(|_| "Upscale state lock is poisoned.".to_string())?
        .insert(job_id.clone(), Arc::clone(&cancel));

    let thread_job_id = job_id.clone();
    let thread_app = app.clone();
    let work_root = std::env::temp_dir()
        .join("c-le-videosr")
        .join(&thread_job_id);

    emit_progress(
        &app,
        &job_id,
        "running",
        0.0,
        0.0,
        Some("Starting bounded NCNN/Vulkan upscale pipeline…".into()),
    );

    thread::spawn(move || {
        let result = run_upscale(
            &thread_app,
            &thread_job_id,
            cancel.as_ref(),
            &request,
            &engine,
            &work_root,
        );

        let _ = fs::remove_dir_all(&work_root);
        if let Some(state) = thread_app.try_state::<UpscaleState>() {
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.remove(&thread_job_id);
            }
        }

        match result {
            Ok(()) => emit_progress(
                &thread_app,
                &thread_job_id,
                "completed",
                100.0,
                request.duration_seconds,
                Some("AI super-resolution completed".into()),
            ),
            Err(error) if error == CANCELLED => emit_progress(
                &thread_app,
                &thread_job_id,
                "cancelled",
                0.0,
                0.0,
                Some("AI super-resolution cancelled".into()),
            ),
            Err(error) => emit_progress(
                &thread_app,
                &thread_job_id,
                "failed",
                0.0,
                0.0,
                Some(error),
            ),
        }
    });

    Ok(StartUpscaleResponse { job_id })
}

pub fn cancel_upscale(state: State<'_, UpscaleState>, job_id: String) -> Result<bool, String> {
    let cancel = {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "Upscale state lock is poisoned.".to_string())?;
        jobs.get(&job_id).cloned()
    };

    let Some(cancel) = cancel else {
        return Ok(false);
    };

    cancel.store(true, Ordering::Relaxed);
    Ok(true)
}
