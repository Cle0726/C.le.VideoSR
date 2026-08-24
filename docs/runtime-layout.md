# Managed Runtime Layout

C.le.VideoSR separates the application code from large third-party inference payloads.

```text
C.le.VideoSR/
├─ runtime/
│  ├─ manifest.json
│  ├─ bin/                 # ignored by Git
│  │  ├─ realesrgan-ncnn-vulkan[.exe]
│  │  ├─ realcugan-ncnn-vulkan[.exe]
│  │  └─ rife-ncnn-vulkan[.exe]
│  └─ models/              # ignored by Git
│     └─ model files
└─ models/
   └─ manifest.json        # tracked model profiles
```

## Resolution order

For NCNN binaries the application resolves locations in this order:

1. `CLE_VIDEOSR_RUNTIME_DIR`
2. Runtime folders adjacent to the packaged executable
3. macOS application `Contents/Resources/runtime`
4. System `PATH` as a development fallback

For model payloads:

1. `CLE_VIDEOSR_MODEL_DIR`
2. `<runtime>/models`
3. Packaged resource model folders
4. The NCNN binary's own default model lookup behavior

The environment variables are intended for development, CI and portable builds. End users should not need to configure them in a packaged release.

## Distribution rule

`runtime/manifest.json` describes expected components but does not grant redistribution rights. Each binary and model must pass license review before it is bundled into release artifacts.
