# Architecture

## Product boundary

C.le.VideoSR is a local desktop orchestrator. It should not make the UI responsible for inference processes and should not make any single model part of the product architecture.

## High-level flow

```text
Media source
    |
    v
Source probe / analysis
    |
    v
Pipeline planner
    |
    v
Bounded frame scheduler
    |
    +--> preprocessing stages
    +--> super-resolution engine
    +--> frame interpolation engine
    +--> post-processing stages
    |
    v
FFmpeg encode / mux
    |
    v
Output media
```

## Runtime tiers

### Fast
Primary target: broad compatibility and low operational cost.

- NCNN / Vulkan
- Real-ESRGAN / Real-CUGAN family
- RIFE family
- NVIDIA, AMD and Intel where the selected runtime supports the device

### Quality
Primary target: higher throughput/quality on supported NVIDIA hardware.

- TensorRT / CUDA
- FP16/FP8 where model/runtime support is verified
- Persistent model sessions

### AI Restore
Primary target: temporal and generative video restoration.

- Isolated worker process
- PyTorch/CUDA first
- Temporal/diffusion VSR models
- Chunking and VRAM-aware scheduling
- Worker crash must not crash the desktop application

## Engine contract

All inference backends implement the same logical lifecycle:

1. Describe capabilities.
2. Run a self-test.
3. Load or prepare a model session.
4. Process bounded work.
5. Report structured progress/errors.
6. Release resources.

The first Rust trait is intentionally small. It will evolve toward frame-stream processing when the FFmpeg layer lands.

## Media I/O

Do not make image-sequence extraction the default architecture. Long videos should use a bounded streaming path to avoid excessive temporary storage and I/O.

The FFmpeg integration is planned in two steps:

1. M1 uses controlled ffprobe/ffmpeg processes for fast iteration and reliable metadata/job plumbing.
2. A native libav path can be evaluated after measurements show the process boundary is a bottleneck.

This keeps M1 shippable without locking the final core to a fragile PNG-frame workflow.

## Process isolation

The desktop process owns:

- UI state
- job database/state
- pipeline planning
- engine registry
- runtime health

Heavy Python models live in a separate worker process. A worker may be restarted independently and communicates through a versioned local protocol.

## Model manifests

Models should eventually be described by manifests rather than hardcoded UI options. A manifest should include:

- id / display name
- task type
- scale factor
- supported engine/runtime
- precision
- recommended VRAM
- tile constraints
- source/model license metadata
- checksum and download source

## Safety and integrity constraints

- Local media is never uploaded by default.
- Runtime/model downloads require explicit user action.
- Model/runtime packages are checksum-verified before activation.
- Third-party licenses are tracked per component.
- Generated/restored detail is presented as enhancement, not guaranteed reconstruction of ground truth.
