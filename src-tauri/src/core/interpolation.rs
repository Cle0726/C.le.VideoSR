use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
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
    rife::RifeNcnnEngine,
};

const CANCELLED: &str = "__C_LE_INTERPOLATION_CANCELLED__";
const DEFAULT_CHUNK_SECONDS: f64 = 2.0;
const DEFAULT_SCENE_THRESHOLD: f64 = 0.42;

#[derive(Debug, Default)]
pub struct InterpolationState {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InterpolationRequest {
    pub input_path: String,
    pub output_path: String,
    pub model_id: String,
    pub video_codec: String,
    pub duration_seconds: f64,
    pub frame_rate: f64,
    pub chunk_seconds: Option<f64>,
    pub gpu_id: Option<u32>,
    pub spatial_tta: Option<bool>,
    pub temporal_tta: Option<bool>,
    pub uhd: Option<bool>,
    pub scene_threshold: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StartInterpolationResponse {
    pub job_id: String,
    pub output_frame_rate: f64,
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child guard is armed")
    }

    fn take(mut self) -> Child {
        self.child.take().expect("child guard is armed")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
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

fn new_job_id() -> Result<String, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    Ok(format!("interpolate-{}-{millis}", std::process::id()))
}

fn codec_args(codec: &str) -> Result<Vec<&'static str>, String> {
    match codec {
        "h264" => Ok(vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "18", "-pix_fmt", "yuv420p",
        ]),
        "h265" => Ok(vec![
            "-c:v", "libx265", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
        ]),
        "copy" => Err("Video stream copy cannot be used after frame interpolation.".into()),
        other => Err(format!("Unsupported interpolation output codec: {other}")),
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

fn last_error(last_line: &Arc<Mutex<String>>, fallback: String) -> String {
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
                    Err(last_error(
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

fn extract_command(
    input: &Path,
    start_frame: usize,
    frame_count: usize,
    fps: f64,
    pattern: &Path,
) -> Command {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .arg("-ss")
        .arg(format!("{:.9}", start_frame as f64 / fps))
        .arg("-map")
        .arg("0:v:0")
        .arg("-frames:v")
        .arg(frame_count.to_string())
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

fn detect_scene_cuts(frame_dir: &Path, fps: f64, threshold: f64) -> HashSet<usize> {
    let input_pattern = frame_dir.join("frame_%08d.png");
    let filter = format!("select=gt(scene\\,{threshold:.3}),showinfo");
    let output = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("info")
        .arg("-framerate")
        .arg(format!("{fps:.8}"))
        .arg("-i")
        .arg(input_pattern)
        .arg("-vf")
        .arg(filter)
        .arg("-an")
        .arg("-f")
        .arg("null")
        .arg("-")
        .output();

    let Ok(output) = output else {
        return HashSet::new();
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .filter_map(|line| {
            let marker = "pts_time:";
            let start = line.find(marker)? + marker.len();
            let value = line[start..].split_whitespace().next()?;
            let seconds = value.parse::<f64>().ok()?;
            Some((seconds * fps).round().max(0.0) as usize)
        })
        .collect()
}

fn process_video(
    app: &AppHandle,
    id: &str,
    cancel: &AtomicBool,
    request: &InterpolationRequest,
    engine: &RifeNcnnEngine,
    multiplier: u32,
    work_root: &Path,
) -> Result<(), String> {
    let input = Path::new(&request.input_path);
    let output = Path::new(&request.output_path);
    let duration = request.duration_seconds;
    let source_fps = request.frame_rate;
    let output_fps = source_fps * multiplier as f64;
    let chunk_seconds = request.chunk_seconds.unwrap_or(DEFAULT_CHUNK_SECONDS);
    let scene_threshold = request.scene_threshold.unwrap_or(DEFAULT_SCENE_THRESHOLD);

    if multiplier != 2 {
        return Err("M3 interpolation currently supports a 2x frame multiplier only.".into());
    }
    if !duration.is_finite() || duration <= 0.0 {
        return Err("Frame interpolation requires a known positive duration.".into());
    }
    if !source_fps.is_finite() || source_fps <= 0.0 || source_fps > 240.0 {
        return Err("Frame interpolation requires a source FPS between 0 and 240.".into());
    }
    if !(0.25..=10.0).contains(&chunk_seconds) {
        return Err("Interpolation chunk duration must be between 0.25 and 10 seconds.".into());
    }
    if !(0.0..=1.0).contains(&scene_threshold) {
        return Err("Scene threshold must be between 0 and 1.".into());
    }

    let total_frames = ((duration * source_fps).round() as usize).max(2);
    let frames_per_chunk = ((chunk_seconds * source_fps).round() as usize).max(2);

    fs::create_dir_all(work_root)
        .map_err(|error| format!("Unable to create interpolation work directory: {error}"))?;
    let source_frames_dir = work_root.join("source");
    let output_frames_dir = work_root.join("rife");
    let video_only = work_root.join("video-only.mkv");

    let encoder_child = encoder_command(output_fps, &request.video_codec, &video_only)?
        .spawn()
        .map_err(|error| format!("Unable to start interpolation output encoder: {error}"))?;
    let mut encoder = ChildGuard::new(encoder_child);
    let mut encoder_input = encoder
        .child_mut()
        .stdin
        .take()
        .ok_or_else(|| "Unable to open interpolation encoder stdin.".to_string())?;
    let encoder_stderr = encoder
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| "Unable to capture interpolation encoder stderr.".to_string())?;
    let (encoder_error, encoder_stderr_thread) = drain_stderr(encoder_stderr);

    let mut unique_start = 0usize;
    let mut chunk_index = 0usize;

    while unique_start < total_frames {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }

        let unique_end = (unique_start + frames_per_chunk).min(total_frames);
        let source_start = if unique_start == 0 { 0 } else { unique_start - 1 };
        let requested_count = unique_end.saturating_sub(source_start);
        if requested_count < 2 {
            break;
        }

        chunk_index += 1;
        let nominal_final = unique_end >= total_frames;
        let processed_before = unique_start as f64 / source_fps;
        let base_progress = unique_start as f64 / total_frames as f64 * 90.0;

        let _ = fs::remove_dir_all(&source_frames_dir);
        let _ = fs::remove_dir_all(&output_frames_dir);
        fs::create_dir_all(&source_frames_dir)
            .map_err(|error| format!("Unable to create interpolation source spool: {error}"))?;
        fs::create_dir_all(&output_frames_dir)
            .map_err(|error| format!("Unable to create RIFE output spool: {error}"))?;

        emit(
            app,
            id,
            "running",
            base_progress,
            processed_before,
            Some(format!("Extracting interpolation chunk {chunk_index}")),
        );
        run(
            extract_command(
                input,
                source_start,
                requested_count,
                source_fps,
                &source_frames_dir.join("frame_%08d.png"),
            ),
            cancel,
            "FFmpeg interpolation frame extraction",
        )?;

        let source_frames = png_frames(&source_frames_dir)?;
        if source_frames.len() < 2 {
            return Err(format!(
                "Interpolation chunk {chunk_index} contains fewer than two source frames."
            ));
        }
        let actual_final = nominal_final || source_frames.len() < requested_count;
        let scene_cuts = detect_scene_cuts(&source_frames_dir, source_fps, scene_threshold);

        emit(
            app,
            id,
            "running",
            (base_progress + 1.0).min(90.0),
            processed_before,
            Some(format!(
                "RIFE chunk {chunk_index} · {} scene cut(s) protected",
                scene_cuts.len()
            )),
        );
        run(
            engine.build_directory_command(&source_frames_dir, &output_frames_dir),
            cancel,
            "RIFE NCNN",
        )?;

        let rife_frames = png_frames(&output_frames_dir)?;
        let expected = source_frames.len() * multiplier as usize;
        if rife_frames.len() < expected {
            return Err(format!(
                "RIFE produced {} frames for {} source frames; expected at least {expected}.",
                rife_frames.len(),
                source_frames.len()
            ));
        }

        let write_start = if unique_start == 0 { 0 } else { 1 };
        let write_end = if actual_final { expected } else { expected - 1 };

        emit(
            app,
            id,
            "running",
            (base_progress + 2.0).min(90.0),
            processed_before,
            Some(format!("Streaming interpolated chunk {chunk_index} into encoder")),
        );

        for output_index in write_start..write_end {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.into());
            }

            let source_replacement = if output_index % 2 == 1 {
                let second_source_index = (output_index + 1) / 2;
                scene_cuts
                    .contains(&second_source_index)
                    .then(|| source_frames.get(second_source_index))
                    .flatten()
            } else {
                None
            };

            let frame_path = source_replacement.unwrap_or(&rife_frames[output_index]);
            let bytes = fs::read(frame_path)
                .map_err(|error| format!("Unable to read {}: {error}", frame_path.display()))?;
            encoder_input.write_all(&bytes).map_err(|error| {
                format!(
                    "Unable to stream interpolated frame to FFmpeg: {error}. {}",
                    last_error(&encoder_error, "encoder closed its input".into())
                )
            })?;
        }

        let _ = fs::remove_dir_all(&source_frames_dir);
        let _ = fs::remove_dir_all(&output_frames_dir);

        let completed_source_frames = if actual_final {
            total_frames
        } else {
            unique_end
        };
        let processed_seconds = (completed_source_frames as f64 / source_fps).min(duration);
        emit(
            app,
            id,
            "running",
            completed_source_frames as f64 / total_frames as f64 * 90.0,
            processed_seconds,
            Some(format!("Interpolation chunk {chunk_index} complete")),
        );

        if actual_final {
            break;
        }
        unique_start = unique_end;
    }

    drop(encoder_input);
    let encoder = encoder.take();
    wait_child(
        encoder,
        cancel,
        encoder_error,
        encoder_stderr_thread,
        "interpolation output encoder",
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
        "FFmpeg interpolation final mux",
    )?;

    Ok(())
}

pub fn start_interpolation(
    app: AppHandle,
    state: State<'_, InterpolationState>,
    request: InterpolationRequest,
) -> Result<StartInterpolationResponse, String> {
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
        .ok_or_else(|| format!("Unknown interpolation model profile: {}", request.model_id))?;
    if model.engine != "rife-ncnn-vulkan" || model.task != "frame_interpolation" {
        return Err(format!("Model {} is not a RIFE interpolation profile.", model.id));
    }

    let multiplier = model.frame_multiplier.unwrap_or(2);
    let mut engine = RifeNcnnEngine::new("rife-ncnn-vulkan", model)
        .map_err(|error| error.to_string())?;
    if let Some(gpu_id) = request.gpu_id {
        engine = engine.with_gpu_id(gpu_id);
    }
    engine = engine
        .with_spatial_tta(request.spatial_tta.unwrap_or(false))
        .with_temporal_tta(request.temporal_tta.unwrap_or(false))
        .with_uhd(request.uhd.unwrap_or(false));
    engine.self_test().map_err(|error| error.to_string())?;

    let id = new_job_id()?;
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .jobs
        .lock()
        .map_err(|_| "Interpolation state lock is poisoned.".to_string())?
        .insert(id.clone(), Arc::clone(&cancel));

    let output_frame_rate = request.frame_rate * multiplier as f64;
    let thread_id = id.clone();
    let thread_app = app.clone();
    let work_root = std::env::temp_dir()
        .join("c-le-videosr")
        .join(&thread_id);

    emit(
        &app,
        &id,
        "running",
        0.0,
        0.0,
        Some(format!(
            "Starting RIFE interpolation · {:.3} → {:.3} FPS",
            request.frame_rate, output_frame_rate
        )),
    );

    thread::spawn(move || {
        let result = process_video(
            &thread_app,
            &thread_id,
            cancel.as_ref(),
            &request,
            &engine,
            multiplier,
            &work_root,
        );
        let _ = fs::remove_dir_all(&work_root);
        if let Some(state) = thread_app.try_state::<InterpolationState>() {
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
                Some("RIFE frame interpolation completed".into()),
            ),
            Err(error) if error == CANCELLED => emit(
                &thread_app,
                &thread_id,
                "cancelled",
                0.0,
                0.0,
                Some("RIFE frame interpolation cancelled".into()),
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

    Ok(StartInterpolationResponse {
        job_id: id,
        output_frame_rate,
    })
}

pub fn cancel_interpolation(
    state: State<'_, InterpolationState>,
    job_id: String,
) -> Result<bool, String> {
    let cancel = state
        .jobs
        .lock()
        .map_err(|_| "Interpolation state lock is poisoned.".to_string())?
        .get(&job_id)
        .cloned();

    let Some(cancel) = cancel else {
        return Ok(false);
    };
    cancel.store(true, Ordering::Relaxed);
    Ok(true)
}
