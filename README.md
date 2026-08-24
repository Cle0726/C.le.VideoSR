# C.le.VideoSR

Local-first AI video enhancement workstation.

C.le.VideoSR is designed as a desktop application for video restoration, super-resolution and frame interpolation. The application keeps media local and separates the desktop shell, orchestration core and inference backends so new engines can be added without rewriting the product.

## Direction

- Local processing by default
- Stream-oriented decode -> process -> encode pipeline
- Hardware-aware engine selection
- Fast / Quality / AI Restore user modes
- Pluggable inference backends (NCNN/Vulkan first, TensorRT and PyTorch workers later)
- Resume-friendly jobs and bounded frame queues
- FFmpeg-based media I/O

## Current milestone: M2 fast enhancement backend

The repository now contains:

- Tauri 2 + React + TypeScript desktop shell
- Initial C.le. dark workstation UI
- Native local input and output file pickers
- `ffprobe` media inspection for resolution, FPS, duration, codec, pixel format, audio and container
- Cancellable Rust-owned FFmpeg child processes
- FFmpeg `-progress` parsing and live Tauri progress events
- Structured FFmpeg error-log events and failure messages
- FFmpeg runtime and encoder self-test (`ffmpeg`, `ffprobe`, `libx264`, `libx265`)
- H.264 / H.265 / source-video-copy output modes
- MP4-safe AAC audio output for the validation pipeline
- Rust hardware capability boundary
- Job and multi-stage enhancement pipeline domain models
- Pluggable inference engine interface and registry
- Versioned model manifest catalog
- NCNN runtime probes for Real-ESRGAN, Real-CUGAN and RIFE
- First `RealEsrganNcnnEngine` implementation with scale, tile, GPU and TTA controls
- GitHub CI for frontend build and Rust `cargo check`

The current visible processing button still validates the local media pipeline only. The Real-ESRGAN engine adapter exists in the core, but video frame streaming is not connected to it yet, so the UI does not claim AI super-resolution is active.

## Milestones

### M1 - Local media pipeline
- [x] Import video
- [x] Probe metadata with ffprobe
- [x] Select output path and codec
- [x] Start/cancel a processing job
- [x] Live structured progress events
- [x] Structured FFmpeg error/log capture
- [x] Runtime/encoder self-test
- [ ] Managed FFmpeg runtime for release builds

### M2 - Fast enhancement backend
- [x] NCNN runtime probing
- [x] Versioned Real-ESRGAN model manifests
- [x] Real-ESRGAN NCNN engine adapter
- [ ] Real-CUGAN adapter
- [ ] Automatic tile sizing from GPU memory
- [ ] Bounded streaming frame hand-off
- [ ] End-to-end video super-resolution job

### M3 - Frame interpolation
- [ ] RIFE adapter
- [ ] Scene-change handling
- [ ] 2x/4x FPS presets

### M4 - Quality backends
- [ ] TensorRT/CUDA adapter
- [ ] Model manager and runtime self-test

### M5 - AI Restore
- [ ] Isolated Python worker protocol
- [ ] Temporal/diffusion VSR backends
- [ ] Chunking, VRAM-aware scheduling and crash recovery

## Development

Requirements:

- Node.js 20+
- Rust stable
- Tauri 2 system prerequisites
- FFmpeg (`ffmpeg` and `ffprobe`) available on `PATH` during development
- NCNN executables are currently discovered from `PATH`; managed sidecars are planned for release builds

```bash
npm install
npm run tauri dev
```

If the application reports that `ffprobe` or `ffmpeg` is missing, install FFmpeg or add its binaries to `PATH`. A managed FFmpeg runtime is planned so end users will not need to configure this manually in a release build.

No inference binary or model is bundled yet. `models/manifest.json` describes supported model profiles, while runtime binaries and model licensing remain separate until distribution review is complete.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for the runtime tiers, engine boundary, media flow and process-isolation strategy.

## License

No project license has been selected yet. Do not assume third-party model/runtime licenses are inherited by this repository. Each engine and model will be tracked separately before distribution.
