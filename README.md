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

## Current milestone: M0 foundation

This repository currently contains the first application skeleton:

- Tauri 2 + React + TypeScript desktop shell
- Rust core boundary
- Hardware capability probing
- Pipeline and job domain models
- Engine adapter interface
- Initial C.le. dark workstation UI

## Planned milestones

### M1 - Local media pipeline
- Import video
- Probe metadata with ffprobe
- Select output path and codec
- Start/cancel a processing job
- Progress and structured logs

### M2 - Fast enhancement backend
- NCNN/Vulkan adapter
- Real-ESRGAN / Real-CUGAN model manifests
- Automatic tile sizing
- Streaming frame hand-off

### M3 - Frame interpolation
- RIFE adapter
- Scene-change handling
- 2x/4x FPS presets

### M4 - Quality backends
- TensorRT/CUDA adapter
- Model manager and runtime self-test

### M5 - AI Restore
- Isolated Python worker protocol
- Temporal/diffusion VSR backends
- Chunking, VRAM-aware scheduling and crash recovery

## Development

Requirements:

- Node.js 20+
- Rust stable
- Tauri 2 system prerequisites

```bash
npm install
npm run tauri dev
```

The application does not bundle an inference model yet. M0 intentionally establishes the architecture first.

## License

No project license has been selected yet. Do not assume third-party model/runtime licenses are inherited by this repository. Each engine and model will be tracked separately before distribution.
