# Universal Enhanced Graphics hardware support

The graphics extension uses one launcher and one settings format on supported desktop platforms.
The launcher detects the local GPU before Play and selects a conservative profile; users can turn
`Hardware auto tuning` off and keep manual settings instead.

## Runtime renderer

Enhanced graphics uses the Vulkan renderer on Windows and Linux and Vulkan through MoltenVK on
macOS. This keeps the game renderer vendor-neutral: NVIDIA, AMD and Intel GPUs do not need separate
game binaries.

| Platform / GPU | Architecture | Runtime renderer | Automatic baseline |
| --- | --- | --- | --- |
| Windows, NVIDIA RTX | x86_64 | Vulkan | Ultra / Classic Lighting |
| Windows, modern AMD Radeon RX / Radeon Pro | x86_64 | Vulkan | Ultra / Classic Lighting |
| Windows, Intel Arc | x86_64 | Vulkan | Ultra / Classic Lighting |
| Windows, older/unknown GPU | x86_64 | Vulkan when supported | Balanced |
| Linux, NVIDIA / AMD / Intel | x86_64 | Vulkan | capability dependent |
| macOS Apple Silicon | arm64 | Vulkan via MoltenVK | Ultra / Classic Lighting |
| macOS Intel / AMD | x86_64 | Vulkan via MoltenVK | Balanced; probe baker availability may differ |

The launcher reports the detected architecture, GPU names, vendor, renderer backend and compute
backend in its capability object. NVIDIA systems with a working `nvidia-smi` are marked as CUDA
capable. CUDA is **not required for the game renderer**; RTX cards render through Vulkan just like
AMD cards. CUDA capability is reserved for optional offline compute/AI backends.

## Offline texture preparation

Ultra textures are generated before Play and cached. macOS uses CoreGraphics for the fast native
resize. Windows and Linux use the launcher's in-process premultiplied-alpha PNG resizer, so the
feature no longer depends on a macOS-only `sips` executable. Runtime 2x/4x/8x reconstruction can
therefore stay off while the prepared pack is active.

The portable path is deliberately CPU compatible. GPU/AI preprocessors can be added on top without
changing the pack format: NVIDIA may use CUDA or Vulkan compute, AMD uses Vulkan compute, Apple can
use Metal/Core ML, and every platform retains the CPU fallback.

## Ray tracing

Hardware ray-tracing capability is detected separately from renderer readiness. RTX, modern Radeon,
Intel Arc and supported Apple Silicon can be marked hardware-RT capable, but the launcher keeps
`rayTracingRendererReady = false` until OpenRAC's actual hardware-RT renderer path is implemented and
verified. Auto tuning therefore never enables an unfinished RT path.

## Compatibility rule

Auto tuning may reduce quality to keep a machine compatible, but it must not enable an unsupported
feature. The same fork should remain launchable on x86_64 systems without CUDA and on ARM systems;
GPU-specific acceleration is an optional fast path, not a requirement.
