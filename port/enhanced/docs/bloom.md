# Enhanced renderer bloom

The enhanced renderer's Bloom is intentionally subtle at its default, but can now be increased
and diagnosed independently. It gives bright authored materials a soft halo without changing the
native HUD or texture assets. Small emissive points remain visible because the first pass extracts
bright samples **before** the Gaussian blur rather than filtering their average after it.

The pass runs after completed world colour and before the native HUD:

1. per-sample soft bright extraction and half-resolution Gaussian pass;
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

Defaults are deliberately restrained: the soft threshold starts around 0.82, its knee is 0.18,
halo scatter is 0.62 and the final bloom strength is 0.055.

## In-game controls and persistence

In the native in-game debug menu, open **Enhanced Settings → Post processing → Bloom**.
The same Vulkan enhanced renderer now exposes Bloom enabled, Bloom intensity (0–200%,
logarithmic for precision near 5.5%), brightness threshold (0–1), soft knee (0–1) and halo
spread (0–1). Threshold values above 1 are not useful because the world image is stored as
display-referred UNORM, not as an overbright HDR image. The control range and config parser
reject these permanently invisible values. Settings with an old out-of-range threshold fall
back to the original default.

Use the **Subtle**, **Visible** or **Strong** presets rather than adjusting five variables just
to see the effect. The original Subtle preset is strength 0.055, threshold 0.82, knee 0.18 and
spread 0.62. Visible is 0.35 / 0.60 / 0.24 / 0.85; Strong is
0.80 / 0.40 / 0.28 / 0.90. Strong can intentionally bloom more ordinary surfaces.
The launcher offers **Defaults**, **Visible**, **Strong**, **Glow only** preset buttons on
Mods → Enhanced graphics; only Bloom settings are affected. Custom changes are stored per game.

**Show only glow (diagnostic)** replaces the world RGB with the extracted blurred glow
mask (at full diagnostic brightness, even when intensity is zero) and leaves its alpha and
the game's native HUD intact. Turn the diagnostic back off for normal gameplay. On a scene
with a bright surface the mask should be visible. If the scene instead stays black with
Glow only enabled, verify the Vulkan enhanced renderer is active, that the game binary
contains these changes, and that the frame actually contains bright pixels. This is a
diagnostic, not a permanent image-quality option.

Adjustments in the game apply on the next newly drawn world frame. Launcher adjustments
apply at the next game launch. Disabled Bloom skips its compute, transfer and raster
passes entirely. Reset Bloom restores original defaults without modifying AgX, motion blur
or lighting. The normal settings file stores `bloom`, `bloom_strength`,
`bloom_threshold`, `bloom_knee`, `bloom_scatter` and `bloom_preview`.
Settings absent from older files use defaults; malformed/non-finite values are ignored.

**Bloom does not need prepared probe lighting.** Only the Vulkan enhanced renderer must be
enabled (`enhanced_rendering = 1` and `OPENRAC_GL_API=vulkan`); advanced probe lighting and FSR
still require valid bakes where applicable. The in-game menu shows a warning when probe
lighting is missing but keeps Bloom's post-processing controls available.
Compatibility/OpenGL rendering does not apply Bloom.

## Lifetime and fallback

Bloom is created only when enhanced rendering is available, which currently means the Vulkan
enhanced renderer. Compatibility rendering is unchanged. Resources are released explicitly before
the graphics context closes and when the frontend resets a world.

If shader preparation or a compute output copy fails, Bloom leaves world colour unchanged, reports
the failure in the log and retries after 120 subsequent eligible frames instead of silently
remaining disabled for the whole game session. Normal scene and cache assets are never modified.

## Test scene

Planet Aridia is a useful visual test because the enhanced material work already marks several
surfaces emissive there. Bloom should be visible around those materials while remaining subtle on
ordinary sunlit geometry and UI.

The GPU regression test checks compatibility bypass, multiple target sizes, alpha preservation,
visible spread outside a bright patch, a weaker default-strength result, small 2x2 emitters
that disappeared with post-blur thresholding, diagnostic-only mask output at zero intensity
and Vulkan validation.

## Outpost X11 render-graph regression (test branch)

The bloom placement is unchanged: after temporal reconstruction/motion blur (where active),
before AgX tone mapping and native HUD overlays. Each Gaussian downsample and reconstruction
dispatch now enters the enhanced compute graph with explicit sampled-image reads and a full
storage-buffer write. Every completed buffer-to-RGBA8 copy is a separate transfer pass,
declaring the exact buffer bytes and the destination image write; its output is exported for
shader sampling. The final composite is a preserved-colour raster pass with an explicit pyramid-texture
read and the framebuffer's colour read/write declaration. RGB uses additive blending in
normal mode and replacement in Glow-only diagnostic mode, keeping world alpha unchanged.
Only the per-sample threshold ordering has changed in the shader. No texture pack, bake or
cache keys have been altered.

The regression exercises the native before-HUD callback (FSR off) and the temporal-chain
callback (FSR on), including odd target sizes, alpha preservation and a zero-strength bypass.
These are callback integration checks rather than full live-game FSR playback. A separate
Outpost X11 manual validation should cover FSR enabled, FSR disabled, enhanced rendering off,
window resizing, and the Vulkan validation layers. No runtime test result is implied by the
presence of this test.
