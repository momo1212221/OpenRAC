# Universal Enhanced Graphics hardware support

The graphics extension uses one launcher and one settings format on supported desktop platforms.
The launcher detects the local GPU before Play and selects a conservative profile; users can turn
`Hardware auto tuning` off and keep manual settings instead.

## Runtime renderer

Enhanced graphics uses the Vulkan renderer on Windows and Linux and Vulkan through MoltenVK on
macOS. This keeps the game renderer vendor-neutral: NVIDIA, AMD and Intel GPUs do not need separate
game binaries.

| Platform / GPU                             | Architecture | Runtime renderer      | Automatic baseline                            |
| ------------------------------------------ | ------------ | --------------------- | --------------------------------------------- |
| Windows, NVIDIA RTX                        | x86_64       | Vulkan                | Ultra / Classic Lighting                      |
| Windows, modern AMD Radeon RX / Radeon Pro | x86_64       | Vulkan                | Ultra / Classic Lighting                      |
| Windows, Intel Arc                         | x86_64       | Vulkan                | Ultra / Classic Lighting                      |
| Windows, older/unknown GPU                 | x86_64       | Vulkan when supported | Balanced                                      |
| Linux, NVIDIA / AMD / Intel                | x86_64       | Vulkan                | capability dependent                          |
| macOS Apple Silicon                        | arm64        | Vulkan via MoltenVK   | Ultra / Classic Lighting                      |
| macOS Intel / AMD                          | x86_64       | Vulkan via MoltenVK   | Balanced; probe baker availability may differ |

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

Hardware ray-tracing capability is detected separately from renderer readiness. On macOS, OpenRAC
uses a native Metal side path at level load: it builds an acceleration structure from opaque static
terrain/tie/shrub geometry, traces direct-sun visibility for terrain receivers, and feeds that
visibility back into the existing Vulkan/MoltenVK renderer. This avoids depending on Vulkan KHR RT
extensions on macOS and adds no per-frame acceleration-structure rebuild.

Apple M3 and newer GPUs expose hardware-accelerated ray tracing. M1/M2 execute the same Metal ray
queries through the compute fallback. Because this pass runs once when the level loads and stores its
visibility result, both paths are enabled automatically on Apple Silicon without adding a per-frame
ray-tracing workload. The existing cascaded shadows and probe GI remain in place and combine with the
ray visibility. Windows/Linux keep renderer readiness false until their Vulkan ray-query
implementation is connected to the same runtime feature.

## Compatibility rule

Auto tuning may reduce quality to keep a machine compatible, but it must not enable an unsupported
feature. The same fork should remain launchable on x86_64 systems without CUDA and on ARM systems;
GPU-specific acceleration is an optional fast path, not a requirement.

## Prepared clouds

Enhanced graphics → Clouds saves cloud coverage, density, height, thickness,
pattern scale, variation, colour and preparation detail. **Prepare clouds and
lighting** builds a persistent cloud volume and GI together before Play. Changing
cloud choices requires preparation again; the next level load reads the result.
Volumetric lighting must be enabled to display the cloud layer. Turning prepared
clouds on selects Medium volumetrics if the saved setting was Off.
