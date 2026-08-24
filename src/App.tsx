import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

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

type Mode = "fast" | "quality" | "restore";

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

export default function App() {
  const [mode, setMode] = useState<Mode>("fast");
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);
  const [media, setMedia] = useState<MediaProbe | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);

  useEffect(() => {
    invoke<HardwareInfo>("detect_hardware")
      .then(setHardware)
      .catch((error) => setHardwareError(String(error)));
  }, []);

  async function selectVideo() {
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
    } catch (error) {
      setMedia(null);
      setMediaError(String(error));
    } finally {
      setProbing(false);
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
                <button className="primary-button" type="button" onClick={selectVideo} disabled={probing}>
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
              >
                <span>{item.title}</span>
                <small>{item.detail}</small>
              </button>
            ))}
          </div>

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
                <div><dt>GPU</dt><dd>{hardware.gpu_hint ?? "Backend probe planned for M1"}</dd></div>
              </dl>
            ) : (
              <p className="muted">{hardwareError ?? "Reading local capabilities…"}</p>
            )}
          </div>
        </aside>
      </section>

      <section className="status-strip">
        <span>M1 Media probe started</span>
        <span>Streaming processing: next</span>
        <span>NCNN/Vulkan adapter: planned</span>
      </section>
    </main>
  );
}
