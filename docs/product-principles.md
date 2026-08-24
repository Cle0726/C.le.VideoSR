# C.le.VideoSR Product Principles

## Core promise

C.le.VideoSR is built as a local-first video enhancement workstation.

The core desktop experience is intended to remain free to use:

- Local video super-resolution is free.
- Local frame interpolation is free.
- Local media processing has no per-video fee.
- Core local processing has no usage-count limit.
- Core local processing does not require a cloud account or paid API.
- Users should be able to process their own media locally without uploading it to a remote service.

## Future paid features

Future optional paid features may be considered, but they must not remove or lock the basic local super-resolution and frame-interpolation capabilities described above.

Examples of future optional features could include workflow convenience, managed model delivery, advanced enterprise tooling, or other services that are separate from the core local AI pipeline.

## Open-source licensing

Free-to-use product policy and open-source licensing are separate decisions.

The C.le.VideoSR repository does not currently declare a project license. A project license should only be selected after third-party runtime, model, dependency, and redistribution obligations are reviewed.

Third-party components must continue to be tracked separately, including their licenses and redistribution requirements.

## Privacy

Local processing is the default product behavior. Media should not be uploaded unless a future feature explicitly requires it and the user deliberately chooses that feature.

## Hardware accessibility

The product should degrade gracefully on lower-end hardware instead of treating low-spec systems as unsupported by default. Fast mode should prefer lightweight NCNN/Vulkan backends, conservative memory settings, and automatic fallbacks where practical.
