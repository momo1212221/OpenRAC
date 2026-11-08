// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include "settings.h"

#include <algorithm>
#include <atomic>
#include <cfenv>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <format>
#include <fstream>
#include <iomanip>
#include <system_error>

#include "common/log.h"
#include "fixes.h"

namespace openrac::settings {

void Settings::reset_enhanced() {
    const Settings defaults;
    enhanced_graphics = defaults.enhanced_graphics;
    enhanced_shadows = defaults.enhanced_shadows;
    ray_tracing = defaults.ray_tracing;
    shadow_quality = defaults.shadow_quality;
    volumetric_quality = defaults.volumetric_quality;
    clouds = defaults.clouds;
    rim_light = defaults.rim_light;
    rim_shadow_lift = defaults.rim_shadow_lift;
    rim_lift_exponent = defaults.rim_lift_exponent;
    rim_lift_max_gain = defaults.rim_lift_max_gain;
    rim_light_strength = defaults.rim_light_strength;
    rim_light_falloff = defaults.rim_light_falloff;
    probe_spheres = defaults.probe_spheres;
    ceiling_area_lights = defaults.ceiling_area_lights;
    probe_gi_multiplier = defaults.probe_gi_multiplier;
    probe_normal_bias = defaults.probe_normal_bias;
    probe_view_bias = defaults.probe_view_bias;
    sun_direction_override = defaults.sun_direction_override;
    sun_azimuth = defaults.sun_azimuth;
    sun_elevation = defaults.sun_elevation;
    gtao = defaults.gtao;
    gtao_settings = defaults.gtao_settings;
    temporal_quality = defaults.temporal_quality;
    agx_preview = defaults.agx_preview;
    agx_exposure = defaults.agx_exposure;
    agx_brightness = defaults.agx_brightness;
    bloom = defaults.bloom;
    bloom_strength = defaults.bloom_strength;
    bloom_radius = defaults.bloom_radius;
    motion_blur = defaults.motion_blur;
    motion_blur_settings = defaults.motion_blur_settings;
    motion_blur_strength = defaults.motion_blur_strength;
    lily_momentum = defaults.lily_momentum;
    temporal_sharpness = defaults.temporal_sharpness;
    texture_filter = defaults.texture_filter;
    local_lights = defaults.local_lights;
    authored_object_lights = defaults.authored_object_lights;
}

namespace {

void preset_common(Settings& settings) {
    settings.enhanced_graphics = true;
    settings.enhanced_shadows = true;
    settings.gtao = true;
    settings.bloom = true;
    settings.agx_preview = true;
    settings.texture_filter = renderer::TextureFilter::Normal;
    settings.local_lights = true;
}

bool same_gtao(
    const renderer::GtaoSettings& a,
    renderer::GtaoQuality quality,
    int slices,
    int steps,
    bool half_resolution,
    bool hi_z
) {
    return a.quality == quality && a.slices == slices && a.steps == steps
           && a.half_resolution == half_resolution && a.hi_z == hi_z;
}

}  // namespace

void apply_graphics_preset(Settings& settings, GraphicsPreset preset) {
    if (preset == GraphicsPreset::Custom || preset == GraphicsPreset::Count) {
        return;
    }
    preset_common(settings);
    switch (preset) {
        case GraphicsPreset::SteamDeck:
            settings.temporal_quality = renderer::TemporalQuality::Balanced;
            settings.temporal_sharpness = .15f;
            settings.shadow_quality = renderer::ShadowQuality::Medium;
            settings.volumetric_quality = renderer::VolumetricQuality::Low;
            settings.gtao_settings = {
                renderer::GtaoQuality::Performance, 4, 6, true, false, 1.25f, 1.0f
            };
            settings.anisotropy = 4;
            settings.original_lod = true;
            settings.draw_distance = 200;
            settings.ray_tracing = false;
            settings.motion_blur = false;
            break;
        case GraphicsPreset::AppleSilicon:
            settings.temporal_quality = renderer::TemporalQuality::Quality;
            settings.temporal_sharpness = .15f;
            settings.shadow_quality = renderer::ShadowQuality::High;
            settings.volumetric_quality = renderer::VolumetricQuality::Medium;
            settings.gtao_settings = {
                renderer::GtaoQuality::Performance, 6, 8, true, true, 1.25f, 1.0f
            };
            settings.anisotropy = 8;
            settings.original_lod = false;
            settings.draw_distance = 400;
            settings.ray_tracing = false;
            settings.motion_blur = true;
            break;
        case GraphicsPreset::Balanced:
            settings.temporal_quality = renderer::TemporalQuality::Quality;
            settings.temporal_sharpness = .2f;
            settings.shadow_quality = renderer::ShadowQuality::High;
            settings.volumetric_quality = renderer::VolumetricQuality::Medium;
            settings.gtao_settings = {
                renderer::GtaoQuality::Performance, 4, 6, true, false, 1.25f, 1.0f
            };
            settings.anisotropy = 8;
            settings.original_lod = false;
            settings.draw_distance = 400;
            settings.ray_tracing = true;
            settings.motion_blur = true;
            break;
        case GraphicsPreset::Rtx:
            settings.temporal_quality = renderer::TemporalQuality::NativeAA;
            settings.temporal_sharpness = .1f;
            settings.shadow_quality = renderer::ShadowQuality::High;
            settings.volumetric_quality = renderer::VolumetricQuality::High;
            settings.gtao_settings = {
                renderer::GtaoQuality::Quality, 8, 12, false, true, 1.5f, 1.0f
            };
            settings.anisotropy = 16;
            settings.original_lod = false;
            settings.draw_distance = 0;
            settings.ray_tracing = true;
            settings.motion_blur = true;
            break;
        case GraphicsPreset::Custom:
        case GraphicsPreset::Count:
            break;
    }
}

GraphicsPreset graphics_preset(const Settings& settings) {
    const auto matches = [&](GraphicsPreset preset) {
        Settings expected = settings;
        apply_graphics_preset(expected, preset);
        return settings.enhanced_graphics == expected.enhanced_graphics
               && settings.enhanced_shadows == expected.enhanced_shadows
               && settings.gtao == expected.gtao && settings.bloom == expected.bloom
               && settings.agx_preview == expected.agx_preview
               && settings.texture_filter == expected.texture_filter
               && settings.local_lights == expected.local_lights
               && settings.temporal_quality == expected.temporal_quality
               && settings.temporal_sharpness == expected.temporal_sharpness
               && settings.shadow_quality == expected.shadow_quality
               && settings.volumetric_quality == expected.volumetric_quality
               && settings.gtao_settings.quality == expected.gtao_settings.quality
               && settings.gtao_settings.slices == expected.gtao_settings.slices
               && settings.gtao_settings.steps == expected.gtao_settings.steps
               && settings.gtao_settings.half_resolution == expected.gtao_settings.half_resolution
               && settings.gtao_settings.hi_z == expected.gtao_settings.hi_z
               && settings.gtao_settings.radius == expected.gtao_settings.radius
               && settings.gtao_settings.strength == expected.gtao_settings.strength
               && settings.anisotropy == expected.anisotropy
               && settings.original_lod == expected.original_lod
               && settings.draw_distance == expected.draw_distance
               && settings.ray_tracing == expected.ray_tracing
               && settings.motion_blur == expected.motion_blur;
    };
    for (const auto preset : {
             GraphicsPreset::SteamDeck,
             GraphicsPreset::AppleSilicon,
             GraphicsPreset::Balanced,
             GraphicsPreset::Rtx,
         }) {
        if (matches(preset)) {
            return preset;
        }
    }
    return GraphicsPreset::Custom;
}

const char* name(GraphicsPreset preset) {
    switch (preset) {
        case GraphicsPreset::SteamDeck: return "Steam Deck";
        case GraphicsPreset::AppleSilicon: return "Apple Silicon";
        case GraphicsPreset::Balanced: return "Balanced";
        case GraphicsPreset::Rtx: return "RTX / High-end";
        case GraphicsPreset::Custom: return "Custom";
        case GraphicsPreset::Count: break;
    }
    return "Custom";
}

std::optional<std::array<float, 3>> Settings::sunlight_direction() const {
    if (!sun_direction_override) {
        return std::nullopt;
    }

    constexpr float radians = 3.14159265358979323846f / 180.f;
    const float azimuth = sun_azimuth * radians, elevation = sun_elevation * radians;
    // The shader and shadow renderer use ray travel, opposite the source direction.
    return std::array<float, 3>{
        -std::cos(elevation) * std::cos(azimuth),
        -std::cos(elevation) * std::sin(azimuth),
        -std::sin(elevation)
    };
}

namespace {

Settings g_settings;
std::atomic<float> g_gain{1.0f};  // g_settings.volume, read by the sound's thread
std::atomic<bool> g_muted{false};
std::filesystem::path g_file;
bool g_display_mode_in_file = false;  // the file load() read said display_mode or fullscreen

// How far from 0 a game's option in the file may be (game_option): no option a game keeps comes
// near it (rac1's volumes go to 0x400), and a hand-edited number past it is reported.
constexpr int kGameOptionLimit = 1'000'000;

// The names in the file, by value.
constexpr const char* kDisplayMode[] = {"windowed", "borderless"};
constexpr const char* kAspect[] = {"4:3", "16:10", "16:9", "21:9", "32:9", "window", "custom"};
constexpr const char* kHud[] = {"centre", "edges"};
constexpr const char* kGtaoQuality[] = {"performance", "quality"};
constexpr const char* kTextureFilter[] = {"normal", "edge-aware", "edge-aware-4x", "edge-aware-8x"};
constexpr const char* kVSync[] = {"off", "on", "adaptive"};

std::string trimmed(const std::string& s) {
    const auto first = s.find_first_not_of(" \t\r\n");

    if (first == std::string::npos) {
        return {};
    }

    return s.substr(first, s.find_last_not_of(" \t\r\n") - first + 1);
}

template <typename E, std::size_t N>
bool parse(const std::string& value, const char* const (&names)[N], E& out) {
    for (std::size_t i = 0; i < N; ++i) {
        if (value == names[i]) {
            out = static_cast<E>(i);
            return true;
        }
    }

    return false;
}

bool parse_bool(const std::string& value, bool& out) {
    if (value == "1" || value == "true" || value == "on" || value == "yes") {
        out = true;
        return true;
    }

    if (value == "0" || value == "false" || value == "off" || value == "no") {
        out = false;
        return true;
    }

    return false;
}

// A whole number within [low, high], with nothing after it.
bool parse_int(const std::string& value, int low, int high, int& out) {
    int number = 0;
    char after = 0;

    if (std::sscanf(value.c_str(), "%d%c", &number, &after) != 1 || number < low || number > high) {
        return false;
    }

    out = number;
    return true;
}

bool parse_float(const std::string& value, float low, float high, float& out) {
    float number = 0.0f;
    char after = 0;

    if (std::sscanf(value.c_str(), "%f%c", &number, &after) != 1 || !std::isfinite(number)
        || number < low || number > high) {
        return false;
    }

    out = number;
    return true;
}

// Cloud choices must retain the same f32 identity as the offline profile, even while the game
// executes retail code with FE_TOWARDZERO.
bool parse_cloud_float(const std::string& value, float low, float high, float& out) {
    const int previous = std::fegetround();
    std::fesetround(FE_TONEAREST);
    const bool valid = parse_float(value, low, high, out);
    std::fesetround(previous);
    return valid;
}

// Two whole numbers with `between` between them ("1920x1080", "64:27"), each at least 1.
bool parse_pair(const std::string& value, char between, int& a, int& b) {
    int first = 0;
    int second = 0;
    char sep = 0;
    char after = 0;

    if (std::sscanf(value.c_str(), "%d%c%d%c", &first, &sep, &second, &after) != 3 || sep != between
        || first < 1 || second < 1 || first > 100000 || second > 100000) {
        return false;
    }

    a = first;
    b = second;
    return true;
}

// One of a list's numbers ("off" is 1: a sample a pixel, no anisotropy).
template <std::size_t N>
bool parse_listed(const std::string& value, const int (&list)[N], int& out) {
    int number = 0;

    if (value == "off") {
        number = 1;
    } else if (!parse_int(value, 0, 1000, number)) {
        return false;
    }

    number = std::max(number, 1);  // 0 is "off" too

    if (std::find(std::begin(list), std::end(list), number) == std::end(list)) {
        return false;
    }

    out = number;
    return true;
}

// A whole number from a list's first to its last (any between, not only the list's own).
bool parse_within(const std::string& value, std::span<const int> list, int& out) {
    return parse_int(value, list.front(), list.back(), out);
}

// frame_rate: a name, or a number of frames a second (any within the limits, not only the ones the
// options page offers: a display's own rate, say).
bool parse_frame_rate(const std::string& value, Settings& s) {
    if (value == "original") {
        s.frame_rate = FrameRate::Original;
    } else if (value == "display") {
        s.frame_rate = FrameRate::Display;
    } else if (value == "unlimited") {
        s.frame_rate = FrameRate::Unlimited;
    } else if (parse_int(value, kFrameCapMin, kFrameCapMax, s.frame_cap)) {
        s.frame_rate = FrameRate::Capped;
    } else {
        return false;
    }

    return true;
}

std::string frame_rate_value(const Settings& s) {
    switch (s.frame_rate) {
        case FrameRate::Original:
            return "original";
        case FrameRate::Capped:
            return std::to_string(s.frame_cap);
        case FrameRate::Unlimited:
            return "unlimited";
        case FrameRate::Display:
        case FrameRate::Count:
            break;
    }

    return "display";
}

// The game fixes switched off, for the summary: ", as the console: fix_a, fix_b", or nothing.
std::string fixes_off() {
    std::string off;

    for (int i = 0; i < fixes::count(); ++i) {
        if (!fixes::on(i)) {
            off += (off.empty() ? ", as the console: " : ", ") + std::string(fixes::fix(i).key);
        }
    }

    return off;
}

float gain_now() {
    return g_muted ? 0.0f : static_cast<float>(g_settings.volume) / 100.0f;
}

}  // namespace

Settings& get() {
    return g_settings;
}

void load(const std::filesystem::path& file) {
    g_file = file;
    g_display_mode_in_file = false;
    g_settings = Settings{};
    fixes::reset();
    Settings& s = g_settings;
    std::ifstream in(file);
    // A file from before `vsync` was a key of its own: a frame waited for the display only at the
    // "display" frame rate, and so it does until the player says otherwise.
    bool has_vsync = false;
    bool has_menu_button = false;  // menu_button read: an older menu_bar_button does not count
    bool has_frame_rate = false;
    // A file from before the strafe was a mod (2026-10-21): its `strafe` or `lock_strafe` is left
    // out, and said once.
    bool old_strafe = false;
    std::string line;

    while (in && std::getline(in, line)) {
        const auto eq = line.find('=');

        if (eq == std::string::npos || trimmed(line).starts_with('#')) {
            continue;
        }

        const std::string key = trimmed(line.substr(0, eq));
        const std::string value = trimmed(line.substr(eq + 1));
        bool known = true;

        if (key == "display_mode" || key == "fullscreen") {
            // Even a value the port does not know: the player chose something.
            g_display_mode_in_file = true;
        }

        if (key == "display_mode") {
            known = parse(value, kDisplayMode, s.display_mode);

            // The exclusive full screen the port had once: borderless now.
            if (value == "fullscreen") {
                s.display_mode = DisplayMode::Borderless;
                known = true;
            }
        } else if (key == "fullscreen") {
            // The key display_mode replaced: a window covering the desktop, or not. A file that
            // has both is the newer one's.
            bool on = false;
            known = parse_bool(value, on);

            if (known && on && s.display_mode == DisplayMode::Windowed) {
                s.display_mode = DisplayMode::Borderless;
            }
        } else if (key == "monitor") {
            known = parse_int(value, 0, 64, s.monitor);
        } else if (key == "resolution") {
            // Exclusive full screen's display mode, which the port no longer has: read, unused.
        } else if (key == "window_size") {
            known = parse_pair(value, 'x', s.window_width, s.window_height);
        } else if (key == "aspect") {
            known = parse(value, kAspect, s.aspect);
        } else if (key == "aspect_custom") {
            known = parse_pair(value, ':', s.aspect_custom_x, s.aspect_custom_y);
        } else if (key == "hud") {
            known = parse(value, kHud, s.hud);
        } else if (key == "gameplay_fov") {
            known = parse_int(value, kMinGameplayFov, kMaxGameplayFov, s.gameplay_fov);
            // Snap preferences saved before the slider used five-point steps.
            s.gameplay_fov =
                (s.gameplay_fov + kGameplayFovStep / 2) / kGameplayFovStep * kGameplayFovStep;
        } else if (key == "render_scale") {
            known = parse_int(value, 25, 400, s.render_scale);
        } else if (key == "msaa") {
            known = parse_listed(value, kSampleCounts, s.msaa);
        } else if (key == "enhanced_rendering") {
            known = parse_bool(value, s.enhanced_rendering);
        } else if (key == "enhanced_graphics") {
            known = parse_bool(value, s.enhanced_graphics);
        } else if (key == "volumetric_quality") {
            known = parse(value, renderer::kVolumetricQualityKeys, s.volumetric_quality);
        } else if (key == "cloud_enabled") {
            known = parse_bool(value, s.clouds.enabled);
        } else if (key == "cloud_resolution") {
            int resolution = int(s.clouds.resolution);
            known = parse_int(value, 32, 128, resolution) && resolution % 16 == 0;

            if (known) {
                s.clouds.resolution = unsigned(resolution);
            }
        } else if (key == "cloud_seed") {
            int seed = int(s.clouds.seed);
            known = parse_int(value, 0, 65535, seed);

            if (known) {
                s.clouds.seed = unsigned(seed);
            }
        } else if (key == "cloud_coverage") {
            known = parse_cloud_float(value, 0.0f, 0.99f, s.clouds.coverage);
        } else if (key == "cloud_density") {
            known = parse_cloud_float(value, 0.0f, 1.0f, s.clouds.density);
        } else if (key == "cloud_base") {
            known = parse_cloud_float(value, -100000.0f, 100000.0f, s.clouds.base);
        } else if (key == "cloud_thickness") {
            known = parse_cloud_float(value, 0.01f, 2000.0f, s.clouds.thickness);
        } else if (key == "cloud_scale") {
            known = parse_cloud_float(value, 1e-05f, 1.0f, s.clouds.scale);
        } else if (key == "cloud_red") {
            known = parse_cloud_float(value, 0.0f, 4.0f, s.clouds.colour[0]);
        } else if (key == "cloud_green") {
            known = parse_cloud_float(value, 0.0f, 4.0f, s.clouds.colour[1]);
        } else if (key == "cloud_blue") {
            known = parse_cloud_float(value, 0.0f, 4.0f, s.clouds.colour[2]);
        } else if (key == "shadow_quality") {
            known = parse(value, renderer::kShadowQualityKeys, s.shadow_quality);
        } else if (key == "enhanced_shadows") {
            known = parse_bool(value, s.enhanced_shadows);
        } else if (key == "ray_tracing") {
            known = parse_bool(value, s.ray_tracing);
        } else if (key == "rim_light") {
            known = parse_bool(value, s.rim_light);
        } else if (key == "texture_pack") {
            known = parse_bool(value, s.texture_pack);
        } else if (key == "rim_shadow_lift") {
            known = parse_bool(value, s.rim_shadow_lift);
        } else if (key == "rim_lift_exponent") {
            known = parse_float(value, 0.01f, 1.0f, s.rim_lift_exponent);
        } else if (key == "rim_lift_max_gain") {
            known = parse_float(value, 1.0f, 16.0f, s.rim_lift_max_gain);
        } else if (key == "rim_light_strength") {
            known = parse_float(value, 0.0f, 1.0f, s.rim_light_strength);
        } else if (key == "rim_light_falloff") {
            known = parse_float(value, 1.0f, 8.0f, s.rim_light_falloff);
        } else if (key == "probe_lighting") {
            // Legacy preference: probe lighting now follows enhanced graphics.
            bool ignored = true;
            known = parse_bool(value, ignored);
        } else if (key == "probe_spheres") {
            known = parse_bool(value, s.probe_spheres);
        } else if (key == "medial_axis_debug") {
            known = parse_bool(value, s.medial_axis_debug);
        } else if (key == "ceiling_area_lights") {
            known = parse_bool(value, s.ceiling_area_lights);
        } else if (key == "environment_probe_debug") {
            known = parse_bool(value, s.environment_probe_debug);
        } else if (key == "probe_normal_bias") {
            known = parse_float(value, 0.0f, 2.0f, s.probe_normal_bias);
        } else if (key == "probe_view_bias") {
            known = parse_float(value, 0.0f, 2.0f, s.probe_view_bias);
        } else if (key == "probe_gi_multiplier") {
            known = parse_float(value, 0.0f, 5.0f, s.probe_gi_multiplier);
        } else if (key == "sun_direction_override") {
            known = parse_bool(value, s.sun_direction_override);
        } else if (key == "sun_azimuth") {
            known = parse_float(value, -180.f, 180.f, s.sun_azimuth);
        } else if (key == "sun_elevation") {
            known = parse_float(value, -90.f, 90.f, s.sun_elevation);
        } else if (key == "gtao") {
            known = parse_bool(value, s.gtao);
        } else if (key == "gtao_quality") {
            known = parse(value, kGtaoQuality, s.gtao_settings.quality);
        } else if (key == "gtao_slices") {
            known = parse_int(value, 1, 12, s.gtao_settings.slices);
        } else if (key == "gtao_steps") {
            known = parse_int(value, 4, 24, s.gtao_settings.steps);
        } else if (key == "gtao_half_resolution") {
            known = parse_bool(value, s.gtao_settings.half_resolution);
        } else if (key == "gtao_hi_z") {
            known = parse_bool(value, s.gtao_settings.hi_z);
        } else if (key == "gtao_radius") {
            known = parse_float(value, 0.05f, 8.0f, s.gtao_settings.radius);
        } else if (key == "gtao_strength") {
            known = parse_float(value, 0.0f, 3.0f, s.gtao_settings.strength);
        } else if (key == "temporal_quality") {
            known = parse(value, renderer::kTemporalKeys, s.temporal_quality);
        } else if (key == "agx_preview") {
            known = parse_bool(value, s.agx_preview);
        } else if (key == "agx_exposure") {
            known = parse_float(value, -4.0f, 4.0f, s.agx_exposure);
        } else if (key == "agx_brightness") {
            known = parse_float(value, 0.0f, 1.0f, s.agx_brightness);
        } else if (key == "bloom") {
            known = parse_bool(value, s.bloom);
        } else if (key == "bloom_strength") {
            known = parse_float(value, 0.0f, 1.0f, s.bloom_strength);
        } else if (key == "bloom_radius") {
            known = parse_float(value, 0.5f, 2.0f, s.bloom_radius);
        } else if (key == "motion_blur") {
            known = parse_bool(value, s.motion_blur);
        } else if (key == "lily_momentum") {
            known = parse_float(value, 0.0f, 1.0f, s.lily_momentum);
        } else if (key == "motion_blur_strength") {
            known = parse_float(value, 0.0f, 1.0f, s.motion_blur_strength);
        } else if (key == "motion_blur_radius") {
            known = parse_int(value, 1, 128, s.motion_blur_settings.max_radius);
        } else if (key == "motion_blur_samples") {
            known = parse_int(value, 8, 64, s.motion_blur_settings.samples);
        } else if (key == "motion_blur_min_motion") {
            known = parse_float(value, 0.1f, 4.0f, s.motion_blur_settings.min_motion);
        } else if (key == "motion_blur_spread") {
            known = parse_float(value, 0.0f, 1.0f, s.motion_blur_settings.spread);
        } else if (key == "motion_blur_depth_tolerance") {
            known = parse_float(value, 0.001f, 0.05f, s.motion_blur_settings.depth_tolerance);
        } else if (key == "motion_blur_cutscene_multiplier") {
            known = parse_float(value, 0.0f, 2.0f, s.motion_blur_settings.cutscene_multiplier);
        } else if (key == "temporal_sharpness") {
            known = parse_float(value, 0.0f, 1.0f, s.temporal_sharpness);
        } else if (key == "texture_filter") {
            known = parse(value, kTextureFilter, s.texture_filter);
        } else if (key == "local_lights") {
            known = parse_bool(value, s.local_lights);
        } else if (key == "authored_object_lights") {
            known = parse_bool(value, s.authored_object_lights);
        } else if (key == "anisotropy") {
            if (value == "auto") {
                s.anisotropy = 0;
            } else {
                known = parse_listed(value, kAnisotropies, s.anisotropy);
            }
        } else if (key == "hdr") {
            known = parse(value, hdr::kModeKeys, s.hdr);
        } else if (key == "hdr_paper_white") {
            known = parse_int(value, 0, hdr::kPaperWhiteMax, s.hdr_paper_white);
        } else if (key == "hdr_peak") {
            known = parse_int(value, 0, hdr::kPeakMax, s.hdr_peak);
        } else if (key == "vsync") {
            known = parse(value, kVSync, s.vsync);
            has_vsync = has_vsync || known;
        } else if (key == "frame_rate") {
            known = parse_frame_rate(value, s);
            has_frame_rate = has_frame_rate || known;
        } else if (key == "gpu_breadcrumbs") {
            known = parse_bool(value, s.gpu_breadcrumbs);
        } else if (key == "show_fps") {
            bool enabled = false;

            // Existing files also accept boolean words for the original counter.
            if (value == "2") {
                s.show_fps = FrameCounter::Timings;
            } else if (parse_bool(value, enabled)) {
                s.show_fps = enabled ? FrameCounter::Basic : FrameCounter::Off;
            } else {
                known = false;
            }
        } else if (key == "menu_bar") {
            // An older port's switch for the menu bar, which is always there now: read and dropped.
        } else if (key == "menu_bar_key") {
            // Any key name SDL knows; the overlay uses the default for one it does not.
            known = !value.empty();

            if (known) {
                s.menu_bar_key = value;
            }
        } else if (key == "menu_button") {
            // Any button name SDL knows, or "none"; the overlay uses the default for another.
            known = !value.empty();

            if (known) {
                s.menu_bar_button = value;
                has_menu_button = true;
            }
        } else if (key == "menu_bar_button") {
            // An older port's key, whose default was L3 (the game's own button): a player's other
            // pick carries over, L3 becomes Start+Select. menu_button wins over it.
            if (!has_menu_button && !value.empty() && value != "leftstick") {
                s.menu_bar_button = value;
            }
        } else if (key == "mute_unfocused") {
            known = parse_bool(value, s.mute_unfocused);
        } else if (key == "original_lod") {
            known = parse_bool(value, s.original_lod);
        } else if (key == "draw_distance") {
            known = parse_listed(value, kDrawDistances, s.draw_distance);
        } else if (key == "responsive_controls") {
            known = parse_bool(value, s.responsive_controls);
        } else if (key == "low_latency_frames") {
            known = parse_bool(value, s.low_latency_frames);
        } else if (key == "fresh_input" || key == "camera_latch") {
            // Old switches merged into responsive_controls: dropped, so it starts off.
        } else if (key == "skip_cutscenes") {
            known = parse_bool(value, s.skip_cutscenes);
        } else if (key == "strafe" || key == "lock_strafe") {
            old_strafe = true;
        } else if (key == "volume") {
            known = parse_int(value, 0, 100, s.volume);
        } else if (key == "touch_scale") {
            known = parse_within(value, kTouchScales, s.touch_scale);
        } else if (key == "touch_camera") {
            known = parse_within(value, kTouchCameraSpeeds, s.touch_camera);
        } else if (key == "touch_style") {
            known = parse_int(value, 0, 1, s.touch_style);
        } else if (key == "touch_layout") {
            s.touch_layout = value;  // TouchPad::set_layout leaves out what it does not know
        } else if (key == "verbose_log") {
            known = parse_bool(value, s.verbose_log);
        } else if (key.starts_with(kBindPrefix)) {
            // What the player bound a pad button to. The names are read where the pad is
            // (platform/bindings.h), not here: this file knows nothing of SDL. An empty value is
            // kept as it is, and means the port's default.
            s.bindings[key] = value;
        } else if (key.starts_with(kGameOptionPrefix)) {
            // An option the game keeps itself (game_option): any whole number in the file's range.
            int number = 0;
            known = parse_int(value, -kGameOptionLimit, kGameOptionLimit, number);

            if (known) {
                s.game_options[key] = number;
            }
        } else if (const int fix = fixes::find(key.c_str()); fix >= 0) {
            // A game fix (fixes.h): on is the port's, off the console's behaviour.
            bool on = true;
            known = parse_bool(value, on);

            if (known) {
                fixes::set(fix, on);
            }
        }

        if (!known) {
            log::warn(
                "{}: {} = {} is not a value the port knows; the default is used",
                file.string(),
                key,
                value
            );
        }
    }

    if (has_frame_rate && !has_vsync) {
        s.vsync = s.frame_rate == FrameRate::Display ? VSync::On : VSync::Off;
    }

    if (old_strafe) {
        log::info(
            "{}: the strafe is a mod now, lock-strafe (OpenRAC/openrac-mods): add it in the "
            "launcher's "
            "Mods tab. This file's strafe setting is left out.",
            file.string()
        );
    }

    g_gain = gain_now();
    log::info("settings from {}: {}", file.string(), summary());
}

bool first_start_display(bool scripted) {
    if (g_display_mode_in_file || scripted) {
        return false;
    }

    g_settings.display_mode = DisplayMode::Borderless;
    return true;
}

std::string summary() {
    const Settings& s = g_settings;
    const std::string window = s.window_width > 0 && s.window_height > 0
                                   ? std::format("{}x{}", s.window_width, s.window_height)
                                   : std::string("default size");
    return std::format(
        "{} ({}, display {}), picture {}, {}% scale, {} sample(s), anisotropy {}, frames {}, vsync "
        "{}, "
        "volume {}{}{}",
        kDisplayMode[static_cast<int>(s.display_mode)],
        window,
        s.monitor,
        kAspect[static_cast<int>(s.aspect)],
        s.render_scale,
        s.msaa,
        s.anisotropy ? std::to_string(s.anisotropy) : "auto",
        frame_rate_value(s),
        kVSync[static_cast<int>(s.vsync)],
        s.volume,
        s.verbose_log ? ", detailed log" : "",
        fixes_off()
    );
}

void save() {
    if (g_file.empty()) {
        return;
    }

    std::error_code made;
    std::filesystem::create_directories(g_file.parent_path(), made);
    // Through a file beside it, then over the old one: a stop mid-write (Android ends a game in
    // the background) leaves the settings as they were, not an empty file.
    std::filesystem::path written = g_file;
    written += ".new";
    std::ofstream out(written, std::ios::trunc);

    if (!out) {
        log::warn("the settings could not be written to {}", g_file.string());
        return;
    }

    const Settings& s = g_settings;
    out << "# OpenRAC: the port's settings (the Video options page of the game's menu changes "
           "them;\n"
        << "# docs/port/SETTINGS.md says what each is)\n"
        << "display_mode = " << kDisplayMode[static_cast<int>(s.display_mode)] << "\n"
        << "monitor = " << s.monitor << "\n";

    if (s.window_width > 0 && s.window_height > 0) {
        out << "window_size = " << s.window_width << "x" << s.window_height << "\n";
    }

    out << "aspect = " << kAspect[static_cast<int>(s.aspect)] << "\n"
        << "aspect_custom = " << s.aspect_custom_x << ":" << s.aspect_custom_y << "\n"
        << "hud = " << kHud[static_cast<int>(s.hud)] << "\n"
        << "gameplay_fov = " << s.gameplay_fov << "\n"
        << "render_scale = " << s.render_scale << "\n"
        << "msaa = " << s.msaa << "\n"
        << "enhanced_rendering = " << (s.enhanced_rendering ? 1 : 0) << "\n"
        << "enhanced_graphics = " << (s.enhanced_graphics ? 1 : 0) << "\n"
        << "volumetric_quality = "
        << renderer::kVolumetricQualityKeys[static_cast<int>(s.volumetric_quality)] << "\n"
        << std::setprecision(9) << "cloud_enabled = " << (s.clouds.enabled ? 1 : 0) << "\n"
        << "cloud_resolution = " << s.clouds.resolution << "\n"
        << "cloud_seed = " << s.clouds.seed << "\n"
        << "cloud_coverage = " << s.clouds.coverage << "\n"
        << "cloud_density = " << s.clouds.density << "\n"
        << "cloud_base = " << s.clouds.base << "\n"
        << "cloud_thickness = " << s.clouds.thickness << "\n"
        << "cloud_scale = " << s.clouds.scale << "\n"
        << "cloud_red = " << s.clouds.colour[0] << "\n"
        << "cloud_green = " << s.clouds.colour[1] << "\n"
        << "cloud_blue = " << s.clouds.colour[2] << "\n"
        << std::setprecision(6)
        << "shadow_quality = " << renderer::kShadowQualityKeys[static_cast<int>(s.shadow_quality)]
        << "\n"
        << "enhanced_shadows = " << (s.enhanced_shadows ? 1 : 0) << "\n"
        << "ray_tracing = " << (s.ray_tracing ? 1 : 0) << "\n"
        << "rim_light = " << (s.rim_light ? 1 : 0) << "\n"
        << "texture_pack = " << (s.texture_pack ? 1 : 0) << "\n"
        << "rim_shadow_lift = " << (s.rim_shadow_lift ? 1 : 0) << "\n"
        << "rim_lift_exponent = " << s.rim_lift_exponent << "\n"
        << "rim_lift_max_gain = " << s.rim_lift_max_gain << "\n"
        << "rim_light_strength = " << s.rim_light_strength << "\n"
        << "rim_light_falloff = " << s.rim_light_falloff << "\n"
        << "probe_spheres = " << (s.probe_spheres ? 1 : 0) << "\n"
        << "medial_axis_debug = " << (s.medial_axis_debug ? 1 : 0) << "\n"
        << "ceiling_area_lights = " << (s.ceiling_area_lights ? 1 : 0) << "\n"
        << "environment_probe_debug = " << (s.environment_probe_debug ? 1 : 0) << "\n"
        << "probe_gi_multiplier = " << s.probe_gi_multiplier << "\n"
        << "probe_normal_bias = " << s.probe_normal_bias << "\n"
        << "probe_view_bias = " << s.probe_view_bias << "\n"
        << "sun_direction_override = " << (s.sun_direction_override ? 1 : 0) << "\n"
        << "sun_azimuth = " << s.sun_azimuth << "\n"
        << "sun_elevation = " << s.sun_elevation << "\n"
        << "gtao = " << (s.gtao ? 1 : 0) << "\n"
        << "gtao_quality = " << kGtaoQuality[static_cast<int>(s.gtao_settings.quality)] << "\n"
        << "gtao_slices = " << s.gtao_settings.slices << "\n"
        << "gtao_steps = " << s.gtao_settings.steps << "\n"
        << "gtao_half_resolution = " << (s.gtao_settings.half_resolution ? 1 : 0) << "\n"
        << "gtao_hi_z = " << (s.gtao_settings.hi_z ? 1 : 0) << "\n"
        << "gtao_radius = " << s.gtao_settings.radius << "\n"
        << "gtao_strength = " << s.gtao_settings.strength << "\n"
        << "texture_filter = " << kTextureFilter[static_cast<int>(s.texture_filter)] << "\n"
        << "local_lights = " << (s.local_lights ? 1 : 0) << "\n"
        << "temporal_quality = " << renderer::kTemporalKeys[static_cast<int>(s.temporal_quality)]
        << "\n"
        << "agx_preview = " << (s.agx_preview ? 1 : 0) << "\n"
        << "agx_exposure = " << s.agx_exposure << "\n"
        << "agx_brightness = " << s.agx_brightness << "\n"
        << "bloom = " << (s.bloom ? 1 : 0) << "\n"
        << "bloom_strength = " << s.bloom_strength << "\n"
        << "bloom_radius = " << s.bloom_radius << "\n"
        << "motion_blur = " << (s.motion_blur ? 1 : 0) << "\n"
        << "lily_momentum = " << s.lily_momentum << "\n"
        << "motion_blur_strength = " << s.motion_blur_strength << "\n"
        << "motion_blur_radius = " << s.motion_blur_settings.max_radius << "\n"
        << "motion_blur_samples = " << s.motion_blur_settings.samples << "\n"
        << "motion_blur_min_motion = " << s.motion_blur_settings.min_motion << "\n"
        << "motion_blur_spread = " << s.motion_blur_settings.spread << "\n"
        << "motion_blur_depth_tolerance = " << s.motion_blur_settings.depth_tolerance << "\n"
        << "motion_blur_cutscene_multiplier = " << s.motion_blur_settings.cutscene_multiplier
        << "\n"
        << "temporal_sharpness = " << s.temporal_sharpness << "\n"
        << "anisotropy = " << (s.anisotropy ? std::to_string(s.anisotropy) : "auto") << "\n"
        << "hdr = " << hdr::kModeKeys[static_cast<int>(s.hdr)] << "\n"
        << "hdr_paper_white = " << s.hdr_paper_white << "\n"
        << "hdr_peak = " << s.hdr_peak << "\n"
        << "vsync = " << kVSync[static_cast<int>(s.vsync)] << "\n"
        << "frame_rate = " << frame_rate_value(s) << "\n"
        << "show_fps = " << static_cast<int>(s.show_fps) << "\n"
        << "gpu_breadcrumbs = " << (s.gpu_breadcrumbs ? 1 : 0) << "\n"
        << "menu_bar_key = " << s.menu_bar_key << "\n"
        << "menu_button = " << s.menu_bar_button << "\n"
        << "mute_unfocused = " << (s.mute_unfocused ? 1 : 0) << "\n"
        << "original_lod = " << (s.original_lod ? 1 : 0) << "\n"
        << "draw_distance = " << s.draw_distance << "\n"
        << "responsive_controls = " << (s.responsive_controls ? 1 : 0) << "\n"
        << "low_latency_frames = " << (s.low_latency_frames ? 1 : 0) << "\n"
        << "skip_cutscenes = " << (s.skip_cutscenes ? 1 : 0) << "\n"
        << "volume = " << s.volume << "\n"
        << "touch_scale = " << s.touch_scale << "\n"
        << "touch_camera = " << s.touch_camera << "\n"
        << "touch_style = " << s.touch_style << "\n";
    if (!s.touch_layout.empty()) {
        out << "touch_layout = " << s.touch_layout << "\n";
    }

    if (s.verbose_log) {
        out << "verbose_log = 1\n";
    }

    // The game's own options, as it last set them (set_game_option).
    for (const auto& [key, value] : s.game_options) {
        out << key << " = " << value << "\n";
    }

    // The player's bindings, written back as they were read: this file is rewritten whole, so a
    // key left out here would be lost the next time any setting changed.
    if (!s.bindings.empty()) {
        out << "# What the player bound each pad button to (the launcher's Controls)\n";

        for (const auto& [key, value] : s.bindings) {
            out << key << " = " << value << "\n";
        }
    }

    if (fixes::count() > 0) {
        out << "# Game fixes: 1 the port's, 0 the console's behaviour (Options, Game fixes)\n";

        for (int i = 0; i < fixes::count(); ++i) {
            out << fixes::fix(i).key << " = " << (fixes::on(i) ? 1 : 0) << "\n";
        }
    }

    out.close();
    std::error_code moved;

    if (!out.fail()) {
        std::filesystem::rename(written, g_file, moved);
    }

    if (out.fail() || moved) {
        log::warn("the settings could not be written to {}", g_file.string());
        std::filesystem::remove(written, moved);
    }
}

bool set_hdr_mode(std::string_view name) {
    for (std::size_t i = 0; i < std::size(hdr::kModeKeys); ++i) {
        if (name == hdr::kModeKeys[i]) {
            g_settings.hdr = static_cast<hdr::Mode>(i);
            return true;
        }
    }

    return false;
}

void set_volume(int volume) {
    g_settings.volume = std::clamp(volume, 0, 100);
    g_gain = gain_now();
}

void set_muted(bool muted) {
    g_muted = muted;
    g_gain = gain_now();
}

float output_gain() {
    return g_gain;
}

float aspect_ratio(const Settings& settings, int window_width, int window_height) {
    switch (settings.aspect) {
        case Aspect::Wide16x10:
            return 16.0f / 10.0f;
        case Aspect::Wide16x9:
            return 16.0f / 9.0f;
        case Aspect::Wide21x9:
            return 21.0f / 9.0f;
        case Aspect::Wide32x9:
            return 32.0f / 9.0f;
        case Aspect::Custom:
            // Never narrower than the game's own picture, as the window's shape is not.
            return std::max(
                4.0f / 3.0f,
                static_cast<float>(settings.aspect_custom_x)
                    / static_cast<float>(std::max(1, settings.aspect_custom_y))
            );
        case Aspect::Window:
            // The game's picture is never narrower than its own: a tall window gets bars.
            return window_width > 0 && window_height > 0
                       ? std::max(
                             4.0f / 3.0f,
                             static_cast<float>(window_width) / static_cast<float>(window_height)
                         )
                       : 4.0f / 3.0f;
        case Aspect::Standard:
        case Aspect::Count:
            break;
    }

    return 4.0f / 3.0f;
}

int step_listed(std::span<const int> list, int now, int by) {
    if (by > 0) {
        const auto next = std::upper_bound(list.begin(), list.end(), now);
        return next != list.end() ? *next : list.back();
    }

    const auto at = std::lower_bound(list.begin(), list.end(), now);
    return at != list.begin() ? *(at - 1) : list.front();
}

void step_frame_counter(int by) {
    constexpr int kModes = static_cast<int>(FrameCounter::Count);
    const int next = (static_cast<int>(g_settings.show_fps) + by % kModes + kModes) % kModes;
    g_settings.show_fps = static_cast<FrameCounter>(next);
}

const char* name(FrameCounter mode) {
    constexpr const char* kNames[] = {"Off", "On", "CPU / GPU"};
    return kNames[std::clamp(static_cast<int>(mode), 0, static_cast<int>(FrameCounter::Count) - 1)];
}

const char* name(DisplayMode mode) {
    constexpr const char* names[] = {"Window", "Borderless"};
    return names[std::min<int>(static_cast<int>(mode), 1)];
}

const char* name(Aspect aspect) {
    constexpr const char* names[] = {"4:3", "16:10", "16:9", "21:9", "32:9", "Window", "Custom"};
    return names[std::min<int>(static_cast<int>(aspect), 6)];
}

const char* name(Hud hud) {
    constexpr const char* names[] = {"Centre", "Edges"};
    return names[std::min<int>(static_cast<int>(hud), 1)];
}

const char* name(VSync vsync) {
    constexpr const char* names[] = {"Off", "On", "Adaptive"};
    return names[std::min<int>(static_cast<int>(vsync), 2)];
}

const char* name(renderer::TextureFilter filter) {
    constexpr const char* names[] =
        {"Normal", "Edge-aware (2x)", "Edge-aware (4x)", "Edge-aware (8x)"};
    return names[std::clamp(static_cast<int>(filter), 0, static_cast<int>(std::size(names)) - 1)];
}

std::string frame_rate_name(const Settings& settings) {
    switch (settings.frame_rate) {
        case FrameRate::Original:
            return "Original";
        case FrameRate::Capped:
            return std::to_string(settings.frame_cap);
        case FrameRate::Unlimited:
            return "Unlimited";
        case FrameRate::Display:
        case FrameRate::Count:
            break;
    }

    return "Display";
}

int game_option(std::string_view key, int fallback) {
    const auto it = g_settings.game_options.find(key);
    return it != g_settings.game_options.end() ? it->second : fallback;
}

void set_game_option(std::string_view key, int value) {
    const auto it = g_settings.game_options.find(key);

    // The game sets its options every frame (rac1/host/options.c): the file is written only when
    // one changes, which the game does seldom (its menus, a loaded save).
    if (it != g_settings.game_options.end() && it->second == value) {
        return;
    }

    g_settings.game_options.insert_or_assign(std::string(key), value);
    save();
}

}  // namespace openrac::settings
