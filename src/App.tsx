import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

type HardwareInfo = {
  os: string;
  arch: string;
  cpu_cores: number;
  total_memory_mb: number;
  gpu_hint: string | null;
};

type MediaRuntimeInfo = {
  ffmpeg_available: boolean;
  ffprobe_available: boolean;
  ffmpeg_version: string | null;
  libx264: boolean;
  libx265: boolean;
};

type ModelManifest = {
  id: string;
  display_name: string;
  task: string;
  engine: string;
  scale: number;
  content: string;
  model_stem: string;
  bundled: boolean;
  license_status: string;
};

type BinaryProbe = {
  name: string;
  available: boolean;
  detail: string | null;
};

type NcnnRuntimeInfo = {
  realesrgan: BinaryProbe;
  realcugan: BinaryProbe;
  rife: BinaryProbe;
  models: ModelManifest[];
};

type MediaProbe = {
  path: string;
  duration_seconds: number | null;
  width: number | null;
  height: number | null;
  frame_rate: number | null;
  video_codec: string | null;
  pixel_format: string | null;
  audio_codec: string | null;
  container: string | null;
};

type ProcessingEvent = {
  job_id: string;
  status: "running" | "completed" | "failed" | "cancelled";
  progress: number;
  out_time_seconds: number;
  speed: string | null;
  message: string | null;
};

type StartJobResponse = { job_id: string };
type Mode = "fast" | "quality" | "restore";
type Codec = "h264" | "h265" | "copy";
type JobStatus = "idle" | ProcessingEvent["status"];
type JobKind = "media" | "upscale" | null;

const modes: Array<{ id: Mode; title: string; detail: string }> = [
  { id: "fast", title: "Fast", detail: "NCNN/Vulkan · broad GPU support" },
  { id: "quality", title: "Quality", detail: "TensorRT / CUDA · planned" },
  { id: "restore", title: "AI Restore", detail: "Temporal restoration · planned" },
];

function formatDuration(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) return "Unknown";
  const total = Math.max(0, Math.round(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  return [hours, minutes, secs].map((value) => String(value).padStart(2, "0")).join(":");
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function suggestedOutputPath(path: string) {
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const directory = slash >= 0 ? path.slice(0, slash + 1) : "";
  const name = slash >= 0 ? path.slice(slash + 1) : path;
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  return `${directory}${stem}_enhanced.mp4`;
}

export default function App() {
  const [mode, setMode] = useState<Mode>("fast");
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);
  const [runtime, setRuntime] = useState<MediaRuntimeInfo | null>(null);
  const [ncnnRuntime, setNcnnRuntime] = useState<NcnnRuntimeInfo | null>(null);
  const [media, setMedia] = useState<MediaProbe | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [codec, setCodec] = useState<Codec>("h264");
  const [selectedModelId, setSelectedModelId] = useState("");
  const [jobKind, setJobKind] = useState<JobKind>(null);
  const [jobStatus, setJobStatus] = useState<JobStatus>("idle");
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  const [outTime, setOutTime] = useState(0);
  const [speed, setSpeed] = useState<string | null>(null);
  const [jobMessage, setJobMessage] = useState<string | null>(null);
  const activeJobRef = useRef<string | null>(null);

  useEffect(() => {
    invoke<HardwareInfo>("detect_hardware")
      .then(setHardware)
      .catch((error) => setHardwareError(String(error)));
    invoke<MediaRuntimeInfo>("detect_media_runtime")
      .then(setRuntime)
      .catch(() => setRuntime(null));
    invoke<NcnnRuntimeInfo>("detect_ncnn_runtime")
      .then(setNcnnRuntime)
      .catch(() => setNcnnRuntime(null));
  }, []);

  useEffect(() => {
    if (!runtime) return;
    if (codec === "h264" && !runtime.libx264) {
      setCodec(runtime.libx265 ? "h265" : "copy");
    } else if (codec === "h265" && !runtime.libx265) {
      setCodec(runtime.libx264 ? "h264" : "copy");
    }
  }, [runtime, codec]);

  useEffect(() => {
    if (!ncnnRuntime || selectedModelId) return;
    const models = ncnnRuntime.models.filter((model) => model.engine === "realesrgan-ncnn-vulkan");
    const preferred = models.find((model) => model.id === "realesrgan-x4plus") ?? models[0];
    if (preferred) setSelectedModelId(preferred.id);
  }, [ncnnRuntime, selectedModelId]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<ProcessingEvent>("job-progress", ({ payload }) => {
      if (activeJobRef.current && activeJobRef.current !== payload.job_id) return;
      activeJobRef.current = payload.job_id;
      setJobId(payload.job_id);
      setJobStatus(payload.status);
      setProgress(payload.progress);
      setOutTime(payload.out_time_seconds);
      setSpeed(payload.speed);
      if (payload.message) setJobMessage(payload.message);
      if (payload.status !== "running") activeJobRef.current = null;
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => unlisten?.();
  }, []);

  function resetJob() {
    activeJobRef.current = null;
    setJobKind(null);
    setJobId(null);
    setJobStatus("idle");
    setProgress(0);
    setOutTime(0);
    setSpeed(null);
    setJobMessage(null);
  }

  async function selectVideo() {
    if (jobStatus === "running") return;
    setMediaError(null);
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Video",
          extensions: ["mp4", "mkv", "mov", "webm", "avi", "m4v", "ts", "m2ts"],
        },
      ],
    });

    if (!selected || Array.isArray(selected)) return;

    setProbing(true);
    try {
      const result = await invoke<MediaProbe>("probe_media", { path: selected });
      setMedia(result);
      setOutputPath(null);
      resetJob();
    } catch (error) {
      setMedia(null);
      setMediaError(String(error));
    } finally {
      setProbing(false);
    }
  }

  async function chooseOutput() {
    if (!media) return null;
    const selected = await save({
      defaultPath: outputPath ?? suggestedOutputPath(media.path),
      filters: [
        { name: "MP4 video", extensions: ["mp4"] },
        { name: "Matroska video", extensions: ["mkv"] },
      ],
    });

    if (!selected) return null;
    setOutputPath(selected);
    return selected;
  }

  const aiModels = ncnnRuntime?.models.filter((model) => model.engine === "realesrgan-ncnn-vulkan") ?? [];
  const selectedModel = aiModels.find((model) => model.id === selectedModelId) ?? null;
  const codecAvailable =
    codec === "copy" ||
    (codec === "h264" && Boolean(runtime?.libx264)) ||
    (codec === "h265" && Boolean(runtime?.libx265));
  const upscaleCodecAvailable =
    codec !== "copy" &&
    ((codec === "h264" && Boolean(runtime?.libx264)) ||
      (codec === "h265" && Boolean(runtime?.libx265)));
  const canRunMedia = Boolean(media && runtime?.ffmpeg_available && codecAvailable && jobStatus !== "running");
  const canRunUpscale = Boolean(
    media &&
      mode === "fast" &&
      runtime?.ffmpeg_available &&
      ncnnRuntime?.realesrgan.available &&
      selectedModel &&
      media.duration_seconds &&
      media.frame_rate &&
      upscaleCodecAvailable &&
      jobStatus !== "running",
  );
  const targetResolution =
    media?.width && media?.height && selectedModel
      ? `${media.width * selectedModel.scale} × ${media.height * selectedModel.scale}`
      : null;

  function beginJob(kind: Exclude<JobKind, null>, message: string) {
    activeJobRef.current = null;
    setJobKind(kind);
    setJobStatus("running");
    setProgress(0);
    setOutTime(0);
    setSpeed(null);
    setJobMessage(message);
  }

  async function startMediaProcessing() {
    if (!media || !canRunMedia) return;
    const target = outputPath ?? (await chooseOutput());
    if (!target) return;

    beginJob("media", "Starting local FFmpeg validation pipeline…");

    try {
      const response = await invoke<StartJobResponse>("start_processing", {
        request: {
          input_path: media.path,
          output_path: target,
          video_codec: codec,
          duration_seconds: media.duration_seconds,
        },
      });
      activeJobRef.current = response.job_id;
      setJobId(response.job_id);
    } catch (error) {
      activeJobRef.current = null;
      setJobStatus("failed");
      setJobMessage(String(error));
    }
  }

  async function startUpscale() {
    if (!media || !selectedModel || !canRunUpscale || !media.duration_seconds || !media.frame_rate) return;
    const target = outputPath ?? (await chooseOutput());
    if (!target) return;

    beginJob("upscale", `Starting ${selectedModel.display_name}…`);

    try {
      const response = await invoke<StartJobResponse>("start_upscale", {
        request: {
          input_path: media.path,
          output_path: target,
          model_id: selectedModel.id,
          video_codec: codec,
          duration_seconds: media.duration_seconds,
          frame_rate: media.frame_rate,
          chunk_seconds: 2,
          tile_size: 0,
          tta: false,
        },
      });
      activeJobRef.current = response.job_id;
      setJobId(response.job_id);
    } catch (error) {
      activeJobRef.current = null;
      setJobStatus("failed");
      setJobMessage(String(error));
    }
  }

  async function cancelActiveJob() {
    const currentJob = activeJobRef.current ?? jobId;
    if (!currentJob || !jobKind) return;
    try {
      const command = jobKind === "upscale" ? "cancel_upscale" : "cancel_processing";
      const cancelled = await invoke<boolean>(command, { jobId: currentJob });
      if (!cancelled) setJobMessage("The processing job is no longer active.");
      else setJobMessage("Cancelling…");
    } catch (error) {
      setJobMessage(String(error));
    }
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">C.le.</p>
          <h1>VideoSR</h1>
        </div>
        <div className="local-badge">Local processing</div>
      </header>

      <section className="hero-grid">
        <div className="panel drop-panel">
          <div className="drop-zone">
            <span className="drop-icon">＋</span>
            {media ? (
              <>
                <h2>{fileName(media.path)}</h2>
                <p className="media-path" title={media.path}>{media.path}</p>
                <div className="media-summary">
                  <span>{media.width ?? "?"} × {media.height ?? "?"}</span>
                  <span>{media.frame_rate ? `${media.frame_rate.toFixed(3)} FPS` : "FPS unknown"}</span>
                  <span>{formatDuration(media.duration_seconds)}</span>
                  <span>{media.video_codec?.toUpperCase() ?? "Codec unknown"}</span>
                </div>
                {targetResolution && <p className="target-resolution">AI target · {targetResolution}</p>}
                <button className="primary-button" type="button" onClick={selectVideo} disabled={probing || jobStatus === "running"}>
                  {probing ? "Inspecting…" : "Choose another video"}
                </button>
              </>
            ) : (
              <>
                <h2>Select a video</h2>
                <p>MP4 · MKV · MOV · WEBM · AVI</p>
                <button className="primary-button" type="button" onClick={selectVideo} disabled={probing || runtime?.ffprobe_available === false}>
                  {probing ? "Inspecting…" : "Select video"}
                </button>
                {runtime?.ffprobe_available === false && <p className="error-message">ffprobe is not available in PATH.</p>}
                {mediaError && <p className="error-message">{mediaError}</p>}
              </>
            )}
          </div>

          {media && (
            <div className="job-panel">
              <div className="job-header">
                <div>
                  <small>{jobKind === "upscale" ? "M2 AI UPSCALE" : "M1 MEDIA PIPELINE"}</small>
                  <strong>
                    {jobStatus === "idle"
                      ? ncnnRuntime?.realesrgan.available
                        ? "Ready for local AI enhancement"
                        : "Media pipeline ready · AI runtime missing"
                      : jobStatus}
                  </strong>
                </div>
                {jobStatus !== "idle" && <span>{progress.toFixed(1)}%</span>}
              </div>
              <div className="progress-track" aria-label="Processing progress">
                <div className="progress-fill" style={{ width: `${Math.max(0, Math.min(100, progress))}%` }} />
              </div>
              <div className="job-meta">
                <span>{formatDuration(outTime)} processed</span>
                <span>{speed ? `${speed} speed` : jobKind === "upscale" ? "NCNN chunk pipeline" : "Waiting for FFmpeg"}</span>
                {jobId && <span title={jobId}>{jobId.slice(-10)}</span>}
              </div>
              {jobMessage && <p className={jobStatus === "failed" ? "error-message" : "job-message"}>{jobMessage}</p>}
            </div>
          )}
        </div>

        <aside className="panel settings-panel">
          <div className="section-heading">
            <span>Enhancement mode</span>
            <small>Engine-aware</small>
          </div>

          <div className="mode-list">
            {modes.map((item) => (
              <button
                key={item.id}
                type="button"
                className={`mode-card ${mode === item.id ? "active" : ""}`}
                onClick={() => setMode(item.id)}
                disabled={jobStatus === "running"}
              >
                <span>{item.title}</span>
                <small>{item.detail}</small>
              </button>
            ))}
          </div>

          {media && (
            <div className="hardware-card output-card">
              <div className="section-heading">
                <span>Enhancement</span>
                <small>{mode === "fast" ? "M2 · NCNN/Vulkan" : "Planned backend"}</small>
              </div>

              {mode === "fast" && (
                <>
                  <label className="field-label" htmlFor="model">Model</label>
                  <select
                    id="model"
                    value={selectedModelId}
                    onChange={(event) => setSelectedModelId(event.target.value)}
                    disabled={jobStatus === "running" || aiModels.length === 0}
                  >
                    {aiModels.length === 0 && <option value="">No Real-ESRGAN profiles</option>}
                    {aiModels.map((model) => (
                      <option key={model.id} value={model.id}>{model.display_name} · {model.scale}×</option>
                    ))}
                  </select>
                </>
              )}

              <label className="field-label" htmlFor="codec">Video codec</label>
              <select id="codec" value={codec} onChange={(event) => setCodec(event.target.value as Codec)} disabled={jobStatus === "running"}>
                <option value="h264" disabled={runtime ? !runtime.libx264 : false}>H.264 · libx264</option>
                <option value="h265" disabled={runtime ? !runtime.libx265 : false}>H.265 · libx265</option>
                <option value="copy">Copy source video stream · M1 only</option>
              </select>

              <button className="path-button" type="button" onClick={chooseOutput} disabled={jobStatus === "running"}>
                <span>{outputPath ? fileName(outputPath) : "Choose output file"}</span>
                <small>{outputPath ?? "MP4 / MKV"}</small>
              </button>

              {jobStatus === "running" ? (
                <button className="danger-button" type="button" onClick={cancelActiveJob}>Cancel processing</button>
              ) : (
                <div className="action-stack">
                  <button className="run-button" type="button" onClick={startUpscale} disabled={!canRunUpscale}>
                    Enhance video · {selectedModel ? `${selectedModel.scale}×` : "NCNN"}
                  </button>
                  <button className="secondary-button" type="button" onClick={startMediaProcessing} disabled={!canRunMedia}>
                    Validate media pipeline
                  </button>
                </div>
              )}

              {!runtime?.ffmpeg_available && <p className="error-message">FFmpeg is required for video processing.</p>}
              {mode === "fast" && ncnnRuntime?.realesrgan.available === false && (
                <p className="error-message">realesrgan-ncnn-vulkan is not installed or not available in PATH.</p>
              )}
              {codec === "copy" && mode === "fast" && (
                <p className="pipeline-note">AI enhancement creates new video frames, so H.264 or H.265 must be selected instead of stream copy.</p>
              )}
              {mode !== "fast" && <p className="pipeline-note">Quality and AI Restore backends are intentionally disabled until their runtimes land.</p>}
              {mode === "fast" && (
                <p className="pipeline-note">M2 keeps only a short frame chunk on disk, streams enhanced PNG frames into one FFmpeg encoder, then removes each chunk.</p>
              )}
            </div>
          )}

          {media && (
            <div className="hardware-card">
              <div className="section-heading">
                <span>Source</span>
                <small>ffprobe</small>
              </div>
              <dl>
                <div><dt>Video</dt><dd>{media.video_codec ?? "Unknown"} · {media.pixel_format ?? "pixel format ?"}</dd></div>
                <div><dt>Audio</dt><dd>{media.audio_codec ?? "None / unknown"}</dd></div>
                <div><dt>Container</dt><dd>{media.container ?? "Unknown"}</dd></div>
              </dl>
            </div>
          )}

          <div className="hardware-card">
            <div className="section-heading">
              <span>Media runtime</span>
              <small>{runtime?.ffmpeg_available ? "Ready" : "Unavailable"}</small>
            </div>
            <dl>
              <div><dt>FFmpeg</dt><dd>{runtime?.ffmpeg_available ? "Detected" : "Missing"}</dd></div>
              <div><dt>ffprobe</dt><dd>{runtime?.ffprobe_available ? "Detected" : "Missing"}</dd></div>
              <div><dt>H.264</dt><dd>{runtime?.libx264 ? "libx264" : "Unavailable"}</dd></div>
              <div><dt>H.265</dt><dd>{runtime?.libx265 ? "libx265" : "Unavailable"}</dd></div>
            </dl>
            {runtime?.ffmpeg_version && <p className="runtime-version" title={runtime.ffmpeg_version}>{runtime.ffmpeg_version}</p>}
          </div>

          <div className="hardware-card">
            <div className="section-heading">
              <span>AI runtime</span>
              <small>M2 · NCNN/Vulkan</small>
            </div>
            <dl>
              <div><dt>Real-ESRGAN</dt><dd>{ncnnRuntime?.realesrgan.available ? "Detected" : "Not installed"}</dd></div>
              <div><dt>Real-CUGAN</dt><dd>{ncnnRuntime?.realcugan.available ? "Detected" : "Not installed"}</dd></div>
              <div><dt>RIFE</dt><dd>{ncnnRuntime?.rife.available ? "Detected" : "Not installed"}</dd></div>
              <div><dt>Catalog</dt><dd>{ncnnRuntime ? `${ncnnRuntime.models.length} model profiles` : "Loading"}</dd></div>
            </dl>
          </div>

          <div className="hardware-card">
            <div className="section-heading">
              <span>System</span>
              <small>{hardware ? "Detected" : "Checking"}</small>
            </div>
            {hardware ? (
              <dl>
                <div><dt>Platform</dt><dd>{hardware.os} · {hardware.arch}</dd></div>
                <div><dt>CPU</dt><dd>{hardware.cpu_cores} logical cores</dd></div>
                <div><dt>Memory</dt><dd>{Math.round(hardware.total_memory_mb / 1024)} GB</dd></div>
                <div><dt>GPU</dt><dd>{hardware.gpu_hint ?? "NCNN will select Vulkan GPU automatically"}</dd></div>
              </dl>
            ) : (
              <p className="muted">{hardwareError ?? "Reading local capabilities…"}</p>
            )}
          </div>
        </aside>
      </section>

      <section className="status-strip">
        <span>M1 media pipeline ✓</span>
        <span>M2 NCNN probe ✓</span>
        <span>M2 Real-ESRGAN adapter ✓</span>
        <span>M2 bounded chunk upscale ✓</span>
        <span>M2 model/runtime packaging: next</span>
      </section>
    </main>
  );
}
