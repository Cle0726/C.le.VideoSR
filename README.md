# C.le.VideoSR

Local-first AI video enhancement workstation.

C.le.VideoSR is designed as a desktop application for video restoration, super-resolution and frame interpolation. The application keeps media local and separates the desktop shell, orchestration core and inference backends so new engines can be added without rewriting the product.

## Direction

- Local processing by default
- Bounded decode -> enhance -> encode orchestration
- Hardware-aware engine selection
- Fast / Quality / AI Restore user modes
- Pluggable inference backends (NCNN/Vulkan first, TensorRT and PyTorch workers later)
- Resume-friendly jobs and bounded temporary storage
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
- H.264 / H.265 output plus an M1 stream-copy validation path
- Rust hardware capability boundary
- Job and multi-stage enhancement pipeline domain models
- Pluggable inference engine interface and registry
- Versioned model manifest catalog
- Managed NCNN runtime resolution with source-checkout `PATH` fallback
- Runtime/model manifest verifier for CI and release staging
- NCNN runtime probes for Real-ESRGAN, Real-CUGAN and RIFE
- `RealEsrganNcnnEngine` with scale, tile, GPU, TTA and managed-model-directory support
- End-to-end Real-ESRGAN video upscale command exposed to the desktop UI
- Bounded chunk frame spool: only a short source/enhanced chunk is stored at a time
- One long-lived FFmpeg image-pipe encoder, so enhanced frames are encoded once instead of per chunk
- Audio/metadata remux after enhancement
- Cancellable AI jobs with temporary-directory and child-process cleanup
- GitHub CI for manifest validation, frontend build and Rust `cargo check`

## Current Fast-mode flow

```text
Input video
   ↓
ffprobe
   ↓
2-second bounded frame chunk
   ↓
Real-ESRGAN NCNN/Vulkan
   ↓
enhanced PNG frames
   ↓
one persistent FFmpeg image2pipe encoder
   ↓
next chunk (previous chunk deleted)
   ↓
restore source audio + metadata
   ↓
output video
```

The current CLI-backed NCNN integration intentionally uses bounded temporary PNG chunks because upstream `realesrgan-ncnn-vulkan` accepts files/directories rather than an FFmpeg raw-frame pipe. A future native-library backend can replace this spool layer without changing the desktop/job API.

## Current limitations

- NCNN binaries and model payloads are not committed or redistributed yet; license review is required before release bundling.
- Development builds can use `PATH`, `CLE_VIDEOSR_RUNTIME_DIR` and `CLE_VIDEOSR_MODEL_DIR`.
- The current image-pipe encoder uses the probed frame rate. Variable-frame-rate sources are therefore normalized to that rate in M2.
- Audio is remuxed/transcoded after enhancement; subtitle-stream restoration is not implemented yet.
- Quality and AI Restore modes are visible product tiers but remain disabled until their backends land.
- Automatic tile selection currently delegates to the NCNN engine (`tile=0`); VRAM-aware C.le. tile policy is still planned.

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
- [x] Bounded chunk frame hand-off
- [x] Single-pass enhanced-frame encoding
- [x] End-to-end video super-resolution job
- [x] Managed NCNN runtime/model path resolution
- [x] Runtime manifest and validation script
- [ ] Real-CUGAN adapter
- [ ] VRAM-aware automatic tile policy
- [ ] Reviewed runtime/model payload packaging
- [ ] VFR timestamp-preserving frame transport

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
- Real-ESRGAN NCNN/Vulkan available either through the managed runtime layout or `PATH` to run Fast AI enhancement

```bash
npm install
npm run runtime:verify
npm run tauri dev
```

For a strict staged-runtime check:

```bash
npm run runtime:verify:strict
```

Managed runtime layout and overrides are documented in [`docs/runtime-layout.md`](docs/runtime-layout.md). The Tauri bundle maps staged `runtime/` payloads into the application resources directory, while large binaries/model files remain ignored by Git.

No inference binary or model payload is bundled in this source repository yet. `models/manifest.json` describes supported model profiles and `runtime/manifest.json` describes expected runtime components; redistribution remains gated on per-component license review.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for the runtime tiers, engine boundary, media flow and process-isolation strategy.

## License

No project license has been selected yet. Do not assume third-party model/runtime licenses are inherited by this repository. Each engine and model will be tracked separately before distribution.
