# C.le.VideoSR

Local-first AI video enhancement workstation.

C.le.VideoSR is designed as a desktop application for video restoration, super-resolution and frame interpolation. The application keeps media local and separates the desktop shell, orchestration core and inference backends so new engines can be added without rewriting the product.

## Direction

- Local processing by default
- Bounded decode -> AI -> encode orchestration
- Hardware-aware engine selection
- Fast / Quality / AI Restore user modes
- Pluggable inference backends (NCNN/Vulkan first, TensorRT and Python workers later)
- Bounded temporary storage and cancellable jobs
- Managed FFmpeg and inference runtime layout

## Current milestone: M3 frame interpolation

The repository now contains:

- Tauri 2 + React + TypeScript desktop shell
- C.le. dark workstation UI
- Native local input/output file pickers and `ffprobe` media inspection
- Cancellable Rust-owned FFmpeg and AI jobs with structured progress/errors
- H.264 / H.265 output plus an M1 stream-copy validation path
- Versioned runtime and model manifests with CI validation
- Managed `runtime/bin` staging with system `PATH` fallback for development
- Tauri resource mapping for staged runtime payloads
- NCNN runtime probes for Real-ESRGAN, Real-CUGAN and RIFE
- Real-ESRGAN NCNN adapter for general/photo and animation profiles
- Real-CUGAN NCNN adapter with its own noise/syncgap/model-directory semantics
- RIFE NCNN adapter with general and anime 2x-FPS profiles
- Shared directory-CLI engine boundary so video orchestration is not tied to one model
- Fast-mode model selection between Real-ESRGAN and Real-CUGAN
- Conservative GPU-memory detection and low-VRAM tile fallback
- Bounded chunk super-resolution and frame-interpolation jobs
- One long-lived FFmpeg image-pipe encoder per transformed-video job
- Audio/metadata remux after AI processing
- Temporary-directory and child-process cleanup on success, failure and cancellation

## Fast super-resolution flow

```text
Input video
   ↓
ffprobe + GPU/runtime detection
   ↓
selected model profile
   ├─ Real-ESRGAN NCNN/Vulkan
   └─ Real-CUGAN NCNN/Vulkan
   ↓
2-second bounded frame chunk
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

## M3 RIFE interpolation flow

```text
Input video
   ↓
ffprobe
   ↓
short source-frame chunk
   ↓
+ previous chunk's final source frame
(one-frame overlap)
   ↓
scene-score detection
   ↓
RIFE NCNN/Vulkan · N -> 2N
   ↓
remove duplicate chunk-boundary frames
   ↓
replace cross-scene midpoint with next source frame
   ↓
one persistent FFmpeg encoder at 2x FPS
   ↓
restore source audio + metadata
   ↓
output video
```

The overlap rule is required because frame interpolation depends on pairs of neighboring source frames. Each chunk after the first includes the previous chunk's final frame. When the RIFE output is stitched together, the duplicate boundary output is removed so the chunked result follows the same frame-count behavior as the upstream directory-mode `N -> 2N` pipeline.

Scene protection is deliberately conservative. Each short source chunk is scanned with FFmpeg's scene score (current threshold `0.42`). When a detected cut lies between two source frames, C.le. does not use the synthesized midpoint across that cut; it uses the next source frame instead. This avoids the most obvious cross-shot morphing without pretending to be a full semantic scene detector.

## GPU / tile behavior

`tile=0` in the product means **C.le Auto**. The policy is deliberately conservative:

- dedicated GPU memory <= 2 GB -> tile 128
- dedicated GPU memory <= 4 GB -> tile 256
- larger/unknown VRAM or unified memory -> leave tile 0 and use the NCNN engine's own automatic policy

GPU memory detection currently uses `nvidia-smi` when available, Linux DRM VRAM data, a Windows WMI estimate, or unified-memory reporting on Apple Silicon. Unknown hardware does not block processing.

## Current limitations

- Runtime binaries and model payloads are not committed or redistributed yet; license review is required before release bundling.
- Current transformed-video paths use the probed source frame rate as a constant-rate timeline. Variable-frame-rate sources are therefore normalized; timestamp-preserving VFR transport remains planned.
- RIFE UI currently exposes 2x FPS. The model/schema is prepared for multipliers, but 4x has not been enabled yet.
- Scene protection is a short-chunk FFmpeg scene-score heuristic, not semantic shot-boundary analysis.
- Audio is restored after AI processing; subtitle-stream restoration is not implemented yet.
- Super-resolution and frame interpolation are currently separate jobs in the UI; a combined one-click SR + RIFE pipeline is not yet wired.
- Quality and AI Restore modes remain disabled until their backends land.
- GPU detection is advisory and intentionally falls back to NCNN defaults when memory information is uncertain.

## Milestones

### M1 - Local media pipeline
- [x] Import video
- [x] Probe metadata with ffprobe
- [x] Select output path and codec
- [x] Start/cancel a processing job
- [x] Live structured progress events
- [x] Structured FFmpeg error/log capture
- [x] Runtime/encoder self-test
- [x] Managed FFmpeg/ffprobe runtime path for release staging

### M2 - Fast enhancement backend
- [x] NCNN runtime probing
- [x] Versioned model manifests
- [x] Real-ESRGAN adapter and end-to-end video upscale
- [x] Real-CUGAN adapter and shared video upscale path
- [x] Bounded chunk frame hand-off
- [x] Single-pass enhanced-frame encoding
- [x] Managed NCNN runtime/model resolution
- [x] Runtime manifest and validation script
- [x] Conservative VRAM-aware automatic tile policy
- [ ] Reviewed runtime/model payload packaging
- [ ] VFR timestamp-preserving frame transport

### M3 - Frame interpolation
- [x] RIFE NCNN adapter
- [x] Versioned RIFE model profiles
- [x] Bounded 2x-FPS interpolation job
- [x] One-frame overlap across chunks
- [x] Scene-change protection heuristic
- [x] Desktop target-FPS controls and cancellation
- [ ] 4x FPS preset
- [ ] Combined SR + interpolation pipeline
- [ ] Timestamp-preserving VFR interpolation

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
- During development, FFmpeg/NCNN may be supplied by the managed runtime layout or system `PATH`

```bash
npm install
npm run runtime:verify
npm run tauri dev
```

For a strict staged-runtime check:

```bash
npm run runtime:verify:strict
```

Managed runtime layout and overrides are documented in [`docs/runtime-layout.md`](docs/runtime-layout.md). Large binaries/model files remain ignored by Git; `runtime/manifest.json` and `models/manifest.json` remain tracked.

No inference binary or model payload is bundled in this source repository yet. Redistribution remains gated on per-component license review.

## Architecture

See [`docs/architecture.md`](docs/architecture.md) for the runtime tiers, engine boundary, media flow and process-isolation strategy.

## License

No project license has been selected yet. Do not assume third-party model/runtime licenses are inherited by this repository. Each engine and model will be tracked separately before distribution.
