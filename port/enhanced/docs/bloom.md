# Enhanced renderer bloom

The enhanced renderer's bloom is intentionally small and conservative. It gives bright authored
materials a soft halo without changing the game's overall contrast or making ordinary geometry look
self-lit.

The pass runs after completed world colour and before the native HUD:

1. half-resolution soft bright pass;
2. a five-level Gaussian reduction pyramid;
3. compute reconstruction from the smallest level;
4. one additive fullscreen composite.

The reduction and reconstruction stages are compute shaders. The reduction kernel performs the
Gaussian filtering while it downsamples, avoiding separate horizontal/vertical blur passes. The
final composite is a single raster draw so it never samples and stores the world framebuffer through
the same image.

The pyramid is RGBA8 rather than RGBA32F. Bloom is a bounded, subtle secondary signal, so this keeps
the 4K working set small while the source frame can remain HDR. At 3840x2160 the five pyramid
textures plus the largest compute buffer are about 19 MiB instead of well over 100 MiB for the first
prototype.

Defaults are deliberately restrained: the soft threshold starts around 0.82, its knee is 0.18 and
the final bloom strength is 0.055. These are implementation defaults rather than user-facing
settings.

## Lifetime and fallback

Bloom is created only when enhanced rendering is available, which currently means the Vulkan
enhanced renderer. Compatibility rendering is unchanged. Resources are released explicitly before
the graphics context closes and when the frontend resets a world.

If shader preparation or a compute output copy fails, bloom disables itself for the current graphics
lifetime and leaves subsequent world colour unchanged rather than repeatedly failing every frame.

## Test scene

Planet Aridia is a useful visual test because the enhanced material work already marks several
surfaces emissive there. Bloom should be visible around those materials while remaining subtle on
ordinary sunlit geometry and UI.

The GPU regression test checks compatibility bypass, multiple target sizes, alpha preservation,
visible spread outside a bright patch, a weaker default-strength result and Vulkan validation.

## Outpost X11 render-graph regression (test branch)

The bloom placement is unchanged: after temporal reconstruction/motion blur (where active),
before AgX tone mapping and native HUD overlays. Each Gaussian downsample and reconstruction
dispatch now enters the enhanced compute graph with explicit sampled-image reads and a full
storage-buffer write. Every completed buffer-to-RGBA8 copy is a separate transfer pass,
declaring the exact buffer bytes and the destination image write; its output is exported for
shader sampling. The final additive composite is a preserved-colour raster pass with an explicit
pyramid-texture read and the framebuffer's colour read/write declaration. No bloom shader
algorithm or cache key changes are required.

The regression exercises the native before-HUD callback (FSR off) and the temporal-chain
callback (FSR on), including odd target sizes, alpha preservation and a zero-strength bypass.
These are callback integration checks rather than full live-game FSR playback. A separate
Outpost X11 manual validation should cover FSR enabled, FSR disabled, enhanced rendering off,
window resizing, and the Vulkan validation layers. No runtime test result is implied by the
presence of this test.
