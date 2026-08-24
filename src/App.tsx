import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type HardwareInfo = {
  os: string;
  arch: string;
  cpu_cores: number;
  total_memory_mb: number;
  gpu_hint: string | null;
};

type Mode = "fast" | "quality" | "restore";

const modes: Array<{ id: Mode; title: string; detail: string }> = [
  { id: "fast", title: "Fast", detail: "Low VRAM · Vulkan · broad GPU support" },
  { id: "quality", title: "Quality", detail: "Higher fidelity · CUDA / TensorRT ready" },
  { id: "restore", title: "AI Restore", detail: "Temporal restoration · high VRAM" },
];

export default function App() {
  const [mode, setMode] = useState<Mode>("fast");
  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [hardwareError, setHardwareError] = useState<string | null>(null);

  useEffect(() => {
    invoke<HardwareInfo>("detect_hardware")
      .then(setHardware)
      .catch((error) => setHardwareError(String(error)));
  }, []);

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
            <h2>Drop a video here</h2>
            <p>MP4 · MKV · MOV · WEBM</p>
            <button type="button" disabled>
              Select video · M1
            </button>
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
        <span>M0 Foundation</span>
        <span>FFmpeg pipeline: next</span>
        <span>NCNN/Vulkan adapter: planned</span>
      </section>
    </main>
  );
}
