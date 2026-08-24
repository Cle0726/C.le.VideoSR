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

const modes: Array<{ id: Mode; title: string; detail: string }> = [
  { id: "fast", title: "Fast", detail: "Low VRAM · Vulkan · broad GPU support" },
  { id: "quality", title: "Quality", detail: "Higher fidelity · CUDA / TensorRT ready" },
  { id: "restore", title: "AI Restore", detail: "Temporal restoration · high VRAM" },
];

function formatDuration(seconds: number | null) {
  if (seconds == null || !Number.isFinite(seconds)) return "Unknown";
  const total = Math.max(0, Math.round(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  return [hours, minutes, secs]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
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
  const [media, setMedia] = useState<MediaProbe | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [codec, setCodec] = useState<Codec>("h264");
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
  }, []);

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

  async function startProcessing() {
    if (!media || jobStatus === "running") return;
    setJobMessage(null);

    const target = outputPath ?? (await chooseOutput());
    if (!target) return;

    setJobStatus("running");
    setProgress(0);
    setOutTime(0);
    setSpeed(null);
    setJobMessage("Starting local FFmpeg pipeline…");

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

  async function cancelProcessing() {
    const currentJob = activeJobRef.current ?? jobId;
    if (!currentJob) return;
    try {
      const cancelled = await invoke<boolean>("cancel_processing", { jobId: currentJob });
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
                <button className="primary-button" type="button" onClick={selectVideo} disabled={probing || jobStatus === "running"}>
                  {probing ? "Inspecting…" : "Choose another video"}
                </button>
              </>
            ) : (
              <>
                <h2>Select a video</h2>
                <p>MP4 · MKV · MOV · WEBM · AVI</p>
                <button className="primary-button" type="button" onClick={selectVideo} disabled={probing}>
                  {probing ? "Inspecting…" : "Select video"}
                </button>
                {mediaError && <p className="error-message">{mediaError}</p>}
              </>
            )}
          </div>

          {media && (
            <div className="job-panel">
              <div className="job-header">
                <div>
                  <small>M1 MEDIA PIPELINE</small>
                  <strong>{jobStatus === "idle" ? "Ready to validate local processing" : jobStatus}</strong>
                </div>
                {jobStatus !== "idle" && <span>{progress.toFixed(1)}%</span>}
              </div>
              <div className="progress-track" aria-label="Processing progress">
                <div className="progress-fill" style={{ width: `${Math.max(0, Math.min(100, progress))}%` }} />
              </div>
              <div className="job-meta">
                <span>{formatDuration(outTime)} processed</span>
                <span>{speed ? `${speed} speed` : "Waiting for FFmpeg"}</span>
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
                <span>Output</span>
                <small>M1 validation</small>
              </div>
              <label className="field-label" htmlFor="codec">Video codec</label>
              <select id="codec" value={codec} onChange={(event) => setCodec(event.target.value as Codec)} disabled={jobStatus === "running"}>
                <option value="h264">H.264 · libx264</option>
                <option value="h265">H.265 · libx265</option>
                <option value="copy">Copy source video stream</option>
              </select>
              <button className="path-button" type="button" onClick={chooseOutput} disabled={jobStatus === "running"}>
                <span>{outputPath ? fileName(outputPath) : "Choose output file"}</span>
                <small>{outputPath ?? "MP4 / MKV"}</small>
              </button>
              {jobStatus === "running" ? (
                <button className="danger-button" type="button" onClick={cancelProcessing}>Cancel processing</button>
              ) : (
                <button className="run-button" type="button" onClick={startProcessing}>
                  Run media pipeline
                </button>
              )}
              <p className="pipeline-note">This M1 step validates decode / encode / progress / cancel. AI super-resolution is not enabled yet.</p>
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
              <span>System</span>
              <small>{hardware ? "Detected" : "Checking"}</small>
            </div>
            {hardware ? (
              <dl>
                <div><dt>Platform</dt><dd>{hardware.os} · {hardware.arch}</dd></div>
                <div><dt>CPU</dt><dd>{hardware.cpu_cores} logical cores</dd></div>
                <div><dt>Memory</dt><dd>{Math.round(hardware.total_memory_mb / 1024)} GB</dd></div>
                <div><dt>GPU</dt><dd>{hardware.gpu_hint ?? "Backend probe planned for M2"}</dd></div>
              </dl>
            ) : (
              <p className="muted">{hardwareError ?? "Reading local capabilities…"}</p>
            )}
          </div>
        </aside>
      </section>

      <section className="status-strip">
        <span>M1 ffprobe ✓</span>
        <span>M1 FFmpeg runner ✓</span>
        <span>M1 progress / cancel ✓</span>
        <span>M2 NCNN/Vulkan: next</span>
      </section>
    </main>
  );
}
