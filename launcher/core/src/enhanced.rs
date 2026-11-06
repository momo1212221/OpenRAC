// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

//! Enhanced graphics: the experimental probe lighting, modern shadows and ambient occlusion of the
//! native port (port/enhanced), as a built-in entry of the Mods tab. It is not a mod the player
//! installs: the game carries the code, off unless `enhanced_rendering = 1` is in the version's
//! settings file (docs/port/SETTINGS.md), and the entry writes that key and the others it shows
//! there, a line at a time, as the game fixes are (differences.rs).
//!
//! With it on, Play starts the game with the Vulkan renderer, the only one that has it, and with
//! `--probes <data>/probes/<version>`: the probe bakes made on this computer from the player's own
//! extracted levels by the `probe-bake` action (openrac-enhanced-baker --bake-all --update).
//! Each baked level records what it was made from (`openrac-bake.json`); a level whose export or
//! baker changed since counts as not ready and is baked again. The baker is named by what it says
//! it bakes (`--print-profile`: its probe format, bake version and quality), not by its file, which
//! every build of a new commit changes. A level without a bake keeps the
//! original lighting. Intel Macs ship without the baker (the maintainers' decision: Embree's static
//! library cannot be built there, tools/build_port.py), so the entry says that there instead of
//! offering a bake.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::differences::{read_switch, read_value, settings_file, with_value, write_settings};
use crate::plugins::{check_value, Choice, PluginView, Preset, SettingSpec, SettingView};

mod bakes;
pub use bakes::{bake_log, bake_status, delete_bakes, sync_cloud_profile, version_bake_status, BakeStatus};

/// The entry's id on the Mods tab (`?mod=enhanced-graphics`).
pub const ID: &str = "enhanced-graphics";
/// The settings key that switches enhanced rendering on (port/game/common/settings.h).
pub const SWITCH: &str = "enhanced_rendering";
/// The action that bakes the probes (launcher/actions.json).
pub const BAKE_ACTION: &str = "probe-bake";
/// Why an Intel Mac has no bakes, as tools/build_port.py says it when it leaves the baker out.
pub const NO_BAKER_HERE: &str =
    "Intel Macs ship without the probe baker: their enhanced graphics keep the original lighting.";
/// Whether this computer gets the probe baker: every one but an Intel Mac (NO_BAKER_HERE). It is
/// the launcher's own build that counts, as `platform.machine()` does for build_port.py.
pub const SHIPS_BAKER: bool = !cfg!(all(target_os = "macos", target_arch = "x86_64"));

/// Whether a version has the entry: the bake planner reads RAC1's levels (PAL and NTSC-U) and
/// RAC2's (NTSC-U), as the baker's `bake_identity_supported` does.
pub fn supported(game: &str, version: &str) -> bool {
    (game == "rac1" && matches!(version, "pal" | "ntsc")) || (game == "rac2" && version == "ntsc")
}

/// Where a version's probe bakes are kept: `<data>/probes/<version>`.
pub fn probes_dir(data: &Path, version: &str) -> PathBuf {
    data.join("probes").join(version)
}

fn spec(key: &str, name: &str, kind: &str, default: Value, description: &str) -> SettingSpec {
    SettingSpec {
        key: key.into(),
        name: name.into(),
        kind: kind.into(),
        default,
        description: description.into(),
        min: None,
        max: None,
        step: None,
        choices: Vec::new(),
        max_length: None,
        binds: None,
        group: String::new(),
    }
}

fn range(mut s: SettingSpec, min: f64, max: f64, step: f64) -> SettingSpec {
    (s.min, s.max, s.step) = (Some(min), Some(max), Some(step));
    s
}

fn group(mut s: SettingSpec, name: &str) -> SettingSpec {
    s.group = name.into();
    s
}

fn advanced(s: SettingSpec) -> SettingSpec {
    group(s, "advanced")
}

fn preset(id: &str, name: &str, values: &[(&str, Value)]) -> Preset {
    Preset {
        id: id.into(),
        name: name.into(),
        values: values.iter().map(|(key, value)| ((*key).into(), value.clone())).collect(),
    }
}

/// The settings the entry's page shows, with the port's defaults and ranges (settings.cpp). The
/// diagnostic ones (probe spheres, receiver biases, GTAO directions and steps) stay in the game's
/// own Enhanced menu.
pub fn settings() -> Vec<SettingSpec> {
    let mut quality = spec("gtao_quality", "Occlusion quality", "choice", "performance".into(), "");
    quality.group = "ambient_occlusion".into();
    quality.choices = vec![
        Choice { value: "performance".into(), name: "Performance".into() },
        Choice { value: "quality".into(), name: "Quality".into() },
    ];
    let mut shadows = spec(
        "shadow_quality",
        "Shadow quality",
        "choice",
        "high".into(),
        "Low uses the smallest shadow textures. Medium balances detail and cost. High keeps the sharpest shadows.",
    );
    shadows.choices = vec![
        Choice { value: "off".into(), name: "Off".into() },
        Choice { value: "low".into(), name: "Low".into() },
        Choice { value: "medium".into(), name: "Medium".into() },
        Choice { value: "high".into(), name: "High".into() },
    ];
    shadows.group = "lighting".into();
    let mut texture_filter = spec(
        "texture_filter",
        "Texture upscaling",
        "choice",
        "edge-aware".into(),
        "Enlarges world textures while keeping edges sharp.",
    );
    texture_filter.choices = vec![
        Choice { value: "normal".into(), name: "Normal".into() },
        Choice { value: "edge-aware".into(), name: "Edge-aware (2x)".into() },
        Choice { value: "edge-aware-4x".into(), name: "Edge-aware (4x)".into() },
        Choice { value: "edge-aware-8x".into(), name: "Edge-aware (8x)".into() },
    ];
    texture_filter.group = "textures".into();
    let mut temporal = spec(
        "temporal_quality",
        "Temporal upscaling",
        "choice",
        "quality".into(),
        "Reconstructs a lower-resolution world image (Arm ASR on Snapdragon GPUs, AMD FSR 3.1 elsewhere). The HUD stays at full resolution.",
    );
    temporal.choices = vec![
        Choice { value: "off".into(), name: "Off".into() },
        Choice { value: "native-aa".into(), name: "Native AA".into() },
        Choice { value: "quality".into(), name: "Quality".into() },
        Choice { value: "balanced".into(), name: "Balanced".into() },
        Choice { value: "performance".into(), name: "Performance".into() },
    ];
    temporal.group = "rendering".into();
    let mut volumetric = spec(
        "volumetric_quality",
        "Volumetric lighting",
        "choice",
        "off".into(),
        "Participating-light/fog quality for beams, glow and atmospheric depth.",
    );
    volumetric.choices = vec![
        Choice { value: "off".into(), name: "Off".into() },
        Choice { value: "low".into(), name: "Low".into() },
        Choice { value: "medium".into(), name: "Medium".into() },
        Choice { value: "high".into(), name: "High".into() },
    ];
    volumetric.group = "lighting".into();

    let mut msaa =
        spec("msaa", "MSAA", "choice", "1".into(), "Multisample anti-aliasing before temporal reconstruction.");
    msaa.choices = ["1", "2", "4", "8", "16"]
        .into_iter()
        .map(|value| Choice {
            value: value.into(),
            name: if value == "1" { "Off".into() } else { format!("{value}x") },
        })
        .collect();
    msaa.group = "rendering".into();

    let mut anisotropy = spec(
        "anisotropy",
        "Anisotropic filtering",
        "choice",
        "auto".into(),
        "Keeps textures sharp at oblique viewing angles.",
    );
    anisotropy.choices = ["auto", "1", "2", "4", "8", "16"]
        .into_iter()
        .map(|value| Choice {
            value: value.into(),
            name: match value {
                "auto" => "Auto".into(),
                "1" => "Off".into(),
                _ => format!("{value}x"),
            },
        })
        .collect();
    anisotropy.group = "textures".into();
    vec![
        group(
            spec(
                "hardware_auto",
                "Hardware auto tuning",
                "bool",
                true.into(),
                "Detects this device before Play and chooses safe runtime quality automatically. RTX/Apple Silicon can use the Ultra path; weaker or unknown GPUs get a compatible balanced path. Verified ray tracing is enabled only when the detected backend can run it safely.",
            ),
            "rendering",
        ),
        group(
            range(
                spec(
                    "render_scale",
                    "Render scale",
                    "int",
                    100.into(),
                    "Internal world resolution as a percentage of the window. Temporal upscaling then reconstructs to the final output.",
                ),
                25.0,
                400.0,
                5.0,
            ),
            "rendering",
        ),
        msaa,
        temporal,
        group(
            range(
                spec(
                    "temporal_sharpness",
                    "Upscaling sharpening",
                    "float",
                    0.2.into(),
                    "RCAS sharpening after temporal reconstruction.",
                ),
                0.0,
                1.0,
                0.05,
            ),
            "rendering",
        ),
        texture_filter,
        group(
            spec(
                "offline_ultra_textures",
                "Ultra textures before Play",
                "bool",
                true.into(),
                "Pre-renders known game textures once at 8x up to a 4096-pixel edge, saves them as a persistent texture pack, and reuses that pack on later starts. This keeps 2x/4x/8x reconstruction out of the render loop.",
            ),
            "textures",
        ),
        anisotropy,
        shadows,
        group(
            spec(
                "ray_tracing",
                "Ray-traced sunlight",
                "bool",
                true.into(),
                "Uses the native Metal ray-tracing path for static direct-sun visibility on Apple Silicon. M3 and newer Apple GPUs use hardware RT; M1/M2 use the compute fallback once when a level loads, then reuse the result during rendering.",
            ),
            "lighting",
        ),
        volumetric,
        group(spec("cloud_enabled", "Prepared clouds", "bool", true.into(), "Enable static clouds. Prepare clouds and lighting after changing any cloud setting."), "clouds"),
        group(range(spec("cloud_resolution", "Cloud preparation quality", "int", 64.into(), "Higher values store a more detailed cloud field; preparation takes longer."), 32.0, 128.0, 16.0), "clouds"),
        group(range(spec("cloud_coverage", "Cloud coverage threshold", "float", 0.58.into(), "Lower values fill more of the sky."), 0.0, 0.99, 0.01), "clouds"),
        group(range(spec("cloud_density", "Cloud density", "float", 0.018.into(), "Opacity of the prepared cloud layer."), 0.0, 1.0, 0.001), "clouds"),
        group(range(spec("cloud_base", "Cloud base height", "float", 90.0.into(), "World-space height of the cloud layer."), -100000.0, 100000.0, 1.0), "clouds"),
        group(range(spec("cloud_thickness", "Cloud layer thickness", "float", 50.0.into(), "Vertical extent of the prepared clouds."), 0.01, 2000.0, 0.01), "clouds"),
        group(range(spec("cloud_scale", "Cloud pattern scale", "float", 0.035.into(), "Smaller values produce broader cloud formations."), 1e-05, 1.0, 0.00001), "clouds"),
        group(range(spec("cloud_seed", "Cloud variation", "int", 1.into(), "A fixed variation number; identical settings reproduce the same clouds."), 0.0, 65535.0, 1.0), "clouds"),
        group(range(spec("cloud_red", "Cloud red", "float", 0.95.into(), "Linear cloud colour. Prepare again after changing it."), 0.0, 4.0, 0.01), "clouds"),
        group(range(spec("cloud_green", "Cloud green", "float", 0.97.into(), "Linear cloud colour. Prepare again after changing it."), 0.0, 4.0, 0.01), "clouds"),
        group(range(spec("cloud_blue", "Cloud blue", "float", 1.0.into(), "Linear cloud colour. Prepare again after changing it."), 0.0, 4.0, 0.01), "clouds"),
        group(
            spec(
                "local_lights",
                "Dynamic lights",
                "bool",
                true.into(),
                "Lets native projectile/explosion lights and authored lamp/object emitters illuminate the world.",
            ),
            "lighting",
        ),
        group(
            spec(
                "authored_object_lights",
                "Additional lamp lights",
                "bool",
                false.into(),
                "Adds the class-bound lamp/object emitters. Leave this off for the older Enhanced lighting look.",
            ),
            "lighting",
        ),
        group(
            range(
                spec(
                    "probe_gi_multiplier",
                    "Global illumination strength",
                    "float",
                    1.0.into(),
                    "Multiplies the baked indirect lighting without requiring a new bake.",
                ),
                0.0,
                5.0,
                0.05,
            ),
            "lighting",
        ),
        group(
            spec(
                "rim_light",
                "Rim lighting",
                "bool",
                true.into(),
                "Adds a restrained edge light to characters and objects so silhouettes read more clearly.",
            ),
            "lighting",
        ),
        group(
            spec(
                "bloom",
                "Bloom",
                "bool",
                true.into(),
                "Lets bright lamps, projectiles and emissive materials cast a soft optical glow before the HUD.",
            ),
            "post_processing",
        ),
        group(
            range(
                spec(
                    "bloom_strength",
                    "Bloom strength",
                    "float",
                    0.35.into(),
                    "Controls how strongly bright weapon effects, explosions, lamps and emissive materials glow.",
                ),
                0.0,
                1.0,
                0.05,
            ),
            "post_processing",
        ),
        group(
            range(
                spec(
                    "bloom_radius",
                    "Bloom radius",
                    "float",
                    1.0.into(),
                    "Controls the spread of bright world emission without changing the native-resolution HUD.",
                ),
                0.5,
                2.0,
                0.1,
            ),
            "post_processing",
        ),
        group(
            spec(
                "agx_preview",
                "Color correction",
                "bool",
                true.into(),
                "Balances colour and contrast for a softer, more natural image.",
            ),
            "color",
        ),
        group(
            range(
                spec("agx_exposure", "Exposure", "float", 0.25.into(), "Base exposure in photographic stops."),
                -4.0,
                4.0,
                0.05,
            ),
            "color",
        ),
        group(
            range(
                spec("agx_brightness", "Brightness", "float", 0.5.into(), "Makes the image darker or brighter."),
                0.0,
                1.0,
                0.01,
            ),
            "color",
        ),
        group(
            spec("gtao", "Ambient occlusion", "bool", true.into(), "Soft shade in corners and under objects."),
            "ambient_occlusion",
        ),
        quality,
        group(
            range(
                spec(
                    "gtao_radius",
                    "Occlusion radius",
                    "float",
                    1.25.into(),
                    "How far ambient occlusion searches around a surface point.",
                ),
                0.05,
                8.0,
                0.05,
            ),
            "ambient_occlusion",
        ),
        group(
            range(
                spec(
                    "gtao_strength",
                    "Occlusion strength",
                    "float",
                    1.0.into(),
                    "How dark the ambient occlusion contribution becomes.",
                ),
                0.0,
                3.0,
                0.05,
            ),
            "ambient_occlusion",
        ),
        group(
            spec("motion_blur", "Motion blur", "bool", true.into(), "Subtle motion blur to enhance animations."),
            "motion",
        ),
        group(
            range(
                spec(
                    "motion_blur_strength",
                    "Motion blur strength",
                    "float",
                    0.5.into(),
                    "Exposure strength of the motion reconstruction.",
                ),
                0.0,
                1.0,
                0.05,
            ),
            "motion",
        ),
        advanced(range(
            spec("gtao_slices", "GTAO directions", "int", 4.into(), "Horizon-search directions."),
            1.0,
            12.0,
            1.0,
        )),
        advanced(range(
            spec("gtao_steps", "GTAO steps", "int", 6.into(), "Samples on each side of every GTAO direction."),
            4.0,
            24.0,
            1.0,
        )),
        advanced(spec(
            "gtao_half_resolution",
            "Half-resolution GTAO",
            "bool",
            true.into(),
            "Evaluates GTAO at half resolution and upsamples it along depth edges.",
        )),
        advanced(spec(
            "gtao_hi_z",
            "Hierarchical-depth GTAO",
            "bool",
            false.into(),
            "Uses hierarchical depth for longer-distance GTAO horizon samples.",
        )),
        advanced(range(
            spec(
                "rim_light_strength",
                "Rim-light strength",
                "float",
                0.30.into(),
                "Intensity of the enhanced edge light.",
            ),
            0.0,
            1.0,
            0.01,
        )),
        advanced(range(
            spec(
                "rim_light_falloff",
                "Rim-light falloff",
                "float",
                6.46.into(),
                "Higher values make the lit edge narrower.",
            ),
            1.0,
            8.0,
            0.01,
        )),
        advanced(spec(
            "rim_shadow_lift",
            "Lift rim-light shadows",
            "bool",
            true.into(),
            "Keeps rim-lit silhouettes readable in deep shadow.",
        )),
        advanced(range(
            spec(
                "motion_blur_radius",
                "Motion blur radius",
                "int",
                64.into(),
                "Maximum motion-blur trail radius in display pixels.",
            ),
            1.0,
            128.0,
            1.0,
        )),
        advanced(range(
            spec(
                "motion_blur_samples",
                "Motion blur samples",
                "int",
                16.into(),
                "Reconstruction samples used along each motion trail.",
            ),
            8.0,
            64.0,
            1.0,
        )),
        advanced(range(
            spec(
                "motion_blur_min_motion",
                "Motion blur threshold",
                "float",
                1.0.into(),
                "Ignores exposed motion below this many display pixels.",
            ),
            0.1,
            4.0,
            0.1,
        )),
        advanced(range(
            spec(
                "motion_blur_spread",
                "Motion blur spread",
                "float",
                0.5.into(),
                "Perpendicular trail allowance relative to motion length.",
            ),
            0.0,
            1.0,
            0.05,
        )),
        advanced(range(
            spec(
                "motion_blur_depth_tolerance",
                "Motion blur depth tolerance",
                "float",
                0.009.into(),
                "Depth rejection threshold used to avoid bleeding across surfaces.",
            ),
            0.001,
            0.05,
            0.001,
        )),
        advanced(range(
            spec(
                "motion_blur_cutscene_multiplier",
                "Cutscene blur multiplier",
                "float",
                1.0.into(),
                "Scales motion blur during real-time cutscenes.",
            ),
            0.0,
            2.0,
            0.05,
        )),
        advanced(range(
            spec(
                "probe_normal_bias",
                "GI normal bias",
                "float",
                0.5.into(),
                "Offsets GI receivers along the surface normal to reduce leaking.",
            ),
            0.0,
            2.0,
            0.05,
        )),
        advanced(range(
            spec(
                "probe_view_bias",
                "GI view bias",
                "float",
                0.5.into(),
                "Offsets GI sampling toward the camera to reduce surface artifacts.",
            ),
            0.0,
            2.0,
            0.05,
        )),
    ]
}

/// A setting's value as the settings file holds it, or its default.
fn read_setting(text: &str, spec: &SettingSpec) -> Value {
    if spec.key == "shadow_quality" && read_switch(text, "enhanced_shadows") == Some(false) {
        return "off".into();
    }
    let raw = read_value(text, &spec.key);
    let value = match (spec.kind.as_str(), raw) {
        ("bool", _) => read_switch(text, &spec.key).map(Value::Bool),
        ("int", Some(v)) => v.parse::<i64>().ok().map(Value::from),
        ("float", Some(v)) if spec.key.starts_with("cloud_") => {
            v.parse::<f32>().ok().and_then(|v| v.to_string().parse::<f64>().ok()).map(Value::from)
        }
        ("float", Some(v)) => v.parse::<f64>().ok().map(Value::from),
        ("choice", Some(v)) => Some(Value::String(v)),
        _ => None,
    };
    value
        .and_then(|v| check_value(spec, &v))
        .filter(|v| spec.key != "cloud_resolution" || v.as_i64().is_some_and(|n| n % 16 == 0))
        .unwrap_or_else(|| spec.default.clone())
}

/// A value as the port reads it: 1|0, a plain number, a choice's name.
fn written(value: &Value) -> String {
    match value {
        Value::Bool(on) => (if *on { "1" } else { "0" }).to_string(),
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(0.0);
            let text = format!("{x:.3}");
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The entry as the Mods tab shows it: a mod's row and page, with the bake's status beside it.
#[derive(Debug, Clone, Serialize, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EnhancedView {
    #[serde(rename = "mod")]
    pub entry: PluginView,
    pub bake: BakeStatus,
}

/// The entry for `game`/`version`, its values from the version's settings file in `data_home`.
pub fn view(data_home: &Path, game: &str, version: &str, bake: BakeStatus) -> EnhancedView {
    let text = std::fs::read_to_string(settings_file(data_home, game, version)).unwrap_or_default();
    let enabled = read_switch(&text, SWITCH).unwrap_or(false);
    let settings = settings()
        .into_iter()
        .map(|spec| {
            let value = read_setting(&text, &spec);
            SettingView { spec, value, conflicts: Vec::new() }
        })
        .collect();
    EnhancedView {
        entry: PluginView {
            id: ID.into(),
            name: "Enhanced graphics".into(),
            version: "Experimental".into(),
            author: "OpenRAC".into(),
            description:
                "Improved lighting and effects whilst preserving the charm of the original visuals we all love.".into(),
            homepage: String::new(),
            license: "GPL-3.0-or-later".into(),
            enabled,
            state: if enabled { "ready" } else { "off" }.into(),
            reason: String::new(),
            blocked: String::new(),
            settings,
            presets: vec![
                preset(
                    "ultra-classic",
                    "Ultra / Classic Lighting",
                    &[
                        ("hardware_auto", true.into()),
                        ("ray_tracing", true.into()),
                        ("shadow_quality", "high".into()),
                        ("gtao", true.into()),
                        ("gtao_quality", "quality".into()),
                        ("offline_ultra_textures", true.into()),
                        ("texture_filter", "normal".into()),
                        ("temporal_quality", "native-aa".into()),
                        ("volumetric_quality", "high".into()),
                        ("local_lights", true.into()),
                        ("authored_object_lights", false.into()),
                        ("bloom", false.into()),
                        ("bloom_strength", 0.35.into()),
                        ("bloom_radius", 1.0.into()),
                        ("agx_preview", true.into()),
                        ("agx_brightness", 0.5.into()),
                        ("motion_blur", true.into()),
                        ("temporal_sharpness", 0.2.into()),
                    ],
                ),
                preset(
                    "ultra",
                    "Ultra / Maximum",
                    &[
                        ("hardware_auto", true.into()),
                        ("ray_tracing", true.into()),
                        ("shadow_quality", "high".into()),
                        ("gtao", true.into()),
                        ("gtao_quality", "quality".into()),
                        ("offline_ultra_textures", true.into()),
                        ("texture_filter", "normal".into()),
                        ("temporal_quality", "native-aa".into()),
                        ("volumetric_quality", "high".into()),
                        ("local_lights", true.into()),
                        ("authored_object_lights", true.into()),
                        ("bloom", true.into()),
                        ("bloom_strength", 0.35.into()),
                        ("bloom_radius", 1.0.into()),
                        ("agx_preview", true.into()),
                        ("agx_brightness", 0.5.into()),
                        ("motion_blur", true.into()),
                        ("temporal_sharpness", 0.2.into()),
                    ],
                ),
                preset(
                    "performance",
                    "Performance",
                    &[
                        ("render_scale", 85.into()),
                        ("msaa", "1".into()),
                        ("temporal_quality", "performance".into()),
                        ("shadow_quality", "medium".into()),
                        ("volumetric_quality", "low".into()),
                        ("gtao_quality", "performance".into()),
                        ("gtao_half_resolution", true.into()),
                        ("motion_blur", false.into()),
                    ],
                ),
                preset(
                    "balanced",
                    "Balanced",
                    &[
                        ("render_scale", 100.into()),
                        ("msaa", "1".into()),
                        ("temporal_quality", "quality".into()),
                        ("shadow_quality", "high".into()),
                        ("volumetric_quality", "medium".into()),
                        ("gtao_quality", "quality".into()),
                        ("gtao_half_resolution", true.into()),
                        ("probe_gi_multiplier", 1.0.into()),
                        ("bloom_strength", 0.35.into()),
                    ],
                ),
                preset(
                    "maximum",
                    "Maximum",
                    &[
                        ("render_scale", 125.into()),
                        ("msaa", "4".into()),
                        ("temporal_quality", "native-aa".into()),
                        ("shadow_quality", "high".into()),
                        ("volumetric_quality", "high".into()),
                        ("gtao_quality", "quality".into()),
                        ("gtao_half_resolution", false.into()),
                        ("gtao_slices", 8.into()),
                        ("gtao_steps", 16.into()),
                        ("anisotropy", "16".into()),
                        ("probe_gi_multiplier", 1.0.into()),
                    ],
                ),
                preset(
                    "cinematic",
                    "Cinematic",
                    &[
                        ("render_scale", 100.into()),
                        ("temporal_quality", "quality".into()),
                        ("volumetric_quality", "high".into()),
                        ("bloom", true.into()),
                        ("bloom_strength", 0.45.into()),
                        ("bloom_radius", 1.2.into()),
                        ("motion_blur", true.into()),
                        ("motion_blur_strength", 0.65.into()),
                        ("agx_preview", true.into()),
                        ("agx_exposure", 0.25.into()),
                    ],
                ),
            ],
            hooks: Vec::new(),
            shared_hooks: Vec::new(),
            native: false,
            kind: "built-in".into(),
            path: String::new(),
            missing: Vec::new(),
        },
        bake,
    }
}

/// Switches enhanced rendering on or off for the version.
pub fn set_enabled(data_home: &Path, game: &str, version: &str, on: bool) -> Result<(), String> {
    let file = settings_file(data_home, game, version);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    write_settings(&file, &with_value(&text, SWITCH, if on { "1" } else { "0" }))
}

/// Sets one of the entry's settings, checked as the port would read it.
pub fn set_setting(data_home: &Path, game: &str, version: &str, key: &str, value: &Value) -> Result<(), String> {
    let all = settings();
    let spec = all.iter().find(|s| s.key == key).ok_or_else(|| format!("{key} is not an enhanced graphics setting"))?;
    let value = check_value(spec, value).ok_or_else(|| format!("{} cannot be {value}", spec.name))?;
    if key == "cloud_resolution" && value.as_i64().is_none_or(|n| n % 16 != 0) {
        return Err("Cloud preparation quality must be a multiple of 16".into());
    }
    let file = settings_file(data_home, game, version);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let text = if key == "shadow_quality" {
        // Keep the renderer's quality enum and legacy enable switch compatible.
        let enabled = value != "off";
        let text = with_value(&text, "enhanced_shadows", if enabled { "1" } else { "0" });
        if enabled {
            with_value(&text, key, &written(&value))
        } else {
            text
        }
    } else {
        with_value(&text, key, &written(&value))
    };
    let text = if key == "cloud_enabled"
        && value == Value::Bool(true)
        && read_value(&text, "volumetric_quality").as_deref().unwrap_or("off") == "off"
    {
        with_value(&text, "volumetric_quality", "medium")
    } else {
        text
    };
    write_settings(&file, &text)
}

/// What Play adds: environment variables, then arguments.
pub type PlayAdditions = (Vec<(String, String)>, Vec<String>);

/// What Play adds when the version's settings switch enhanced rendering on: the Vulkan renderer
/// and the probe bakes' folder. `settings` is the version's settings file's text.
pub fn play_additions(settings: &str, data: &Path, version: &str) -> Option<PlayAdditions> {
    if !read_switch(settings, SWITCH).unwrap_or(false) {
        return None;
    }
    let env = vec![("OPENRAC_GL_API".to_string(), "vulkan".to_string())];
    let args = vec!["--probes".to_string(), probes_dir(data, version).display().to_string()];
    Some((env, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openrac-enhanced-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cloud_settings_are_saved_validated_and_enable_the_atmosphere_grid() {
        let home = temp("cloud-settings");
        set_setting(&home, "rac1", "pal", "cloud_enabled", &true.into()).unwrap();
        set_setting(&home, "rac1", "pal", "cloud_seed", &42.into()).unwrap();
        assert!(set_setting(&home, "rac1", "pal", "cloud_resolution", &33.into()).is_err());
        assert!(set_setting(&home, "rac1", "pal", "cloud_density", &(-1.0).into()).is_err());
        let text = std::fs::read_to_string(settings_file(&home, "rac1", "pal")).unwrap();
        assert_eq!(read_switch(&text, "cloud_enabled"), Some(true));
        assert_eq!(read_value(&text, "cloud_seed").as_deref(), Some("42"));
        assert_eq!(read_value(&text, "volumetric_quality").as_deref(), Some("medium"));
        let all = settings();
        let red = all.iter().find(|s| s.key == "cloud_red").unwrap();
        assert_eq!(read_setting("cloud_red = 0.949999988\n", red), red.default);
        let coverage = all.iter().find(|s| s.key == "cloud_coverage").unwrap();
        assert_eq!(read_setting("cloud_coverage = 0.990000010\n", coverage), Value::from(0.99));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn simplified_settings_preserve_shadow_compatibility() {
        let home = temp("shadow-options");
        let all = settings();
        assert_eq!(
            all.iter().filter(|s| s.group == "advanced").map(|s| s.key.as_str()).collect::<Vec<_>>(),
            vec![
                "gtao_slices",
                "gtao_steps",
                "gtao_half_resolution",
                "gtao_hi_z",
                "rim_light_strength",
                "rim_light_falloff",
                "rim_shadow_lift",
                "motion_blur_radius",
                "motion_blur_samples",
                "motion_blur_min_motion",
                "motion_blur_spread",
                "motion_blur_depth_tolerance",
                "motion_blur_cutscene_multiplier",
                "probe_normal_bias",
                "probe_view_bias",
            ]
        );
        assert!(all.iter().all(|s| s.key != "enhanced_shadows"));
        for hidden_debug in [
            "probe_spheres",
            "medial_axis_debug",
            "ceiling_area_lights",
            "environment_probe_debug",
            "sun_direction_override",
        ] {
            assert!(all.iter().all(|s| s.key != hidden_debug));
        }
        for version in ["pal", "ntsc"] {
            let file = settings_file(&home, "rac1", version);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "enhanced_shadows = 0\nshadow_quality = low\nmotion_blur = 0\n").unwrap();
            let shadow = all.iter().find(|s| s.key == "shadow_quality").unwrap();
            let text = std::fs::read_to_string(&file).unwrap();
            assert_eq!(read_setting(&text, shadow), Value::from("off"));
            for quality in ["low", "medium", "high", "off"] {
                set_setting(&home, "rac1", version, "shadow_quality", &Value::from(quality)).unwrap();
                let text = std::fs::read_to_string(&file).unwrap();
                assert_eq!(read_setting(&text, shadow), Value::from(quality));
                assert_eq!(read_switch(&text, "enhanced_shadows"), Some(quality != "off"));
                assert_eq!(read_switch(&text, "motion_blur"), Some(false));
                assert_ne!(read_value(&text, "shadow_quality"), Some("off".into()));
            }
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn off_until_switched_on_and_settings_land_in_the_ports_file() {
        let home = temp("settings");
        let file = settings_file(&home, "rac1", "pal");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "msaa = 4\ngtao = 1\nprobe_lighting = 0\nagx_exposure = 1.5\n").unwrap();
        let v = view(&home, "rac1", "pal", BakeStatus::default());
        assert!(!v.entry.enabled);
        let value =
            |v: &EnhancedView, key: &str| v.entry.settings.iter().find(|s| s.spec.key == key).unwrap().value.clone();
        assert_eq!(value(&v, "gtao"), Value::Bool(true));
        assert!(v.entry.settings.iter().all(|s| s.spec.key != "probe_lighting"));
        assert!(set_setting(&home, "rac1", "pal", "probe_lighting", &Value::Bool(false)).is_err());
        assert_eq!(value(&v, "texture_filter"), Value::from("edge-aware"));
        assert_eq!(value(&v, "temporal_quality"), Value::from("quality"));
        assert_eq!(value(&v, "shadow_quality"), Value::from("high"));
        assert_eq!(value(&v, "temporal_sharpness"), Value::from(0.2));
        assert_eq!(value(&v, "agx_preview"), Value::Bool(true));
        assert_eq!(value(&v, "motion_blur"), Value::Bool(true));

        assert_eq!(value(&v, "agx_brightness"), Value::from(0.5));
        for brightness in [0.0, 0.5, 1.0] {
            set_setting(&home, "rac1", "pal", "agx_brightness", &Value::from(brightness)).unwrap();
            assert_eq!(
                value(&view(&home, "rac1", "pal", BakeStatus::default()), "agx_brightness"),
                Value::from(brightness)
            );
        }
        assert!(set_setting(&home, "rac1", "pal", "agx_brightness", &Value::from(1.01)).is_err());
        assert!(set_setting(&home, "rac1", "pal", "agx_brightness", &Value::from(-0.01)).is_err());
        set_setting(&home, "rac1", "pal", "agx_preview", &Value::Bool(false)).unwrap();
        assert_eq!(value(&view(&home, "rac1", "pal", BakeStatus::default()), "agx_preview"), Value::Bool(false));
        set_enabled(&home, "rac1", "pal", true).unwrap();
        set_setting(&home, "rac1", "pal", "motion_blur", &Value::Bool(true)).unwrap();
        set_setting(&home, "rac1", "pal", "shadow_quality", &Value::from("low")).unwrap();
        assert_eq!(value(&view(&home, "rac1", "pal", BakeStatus::default()), "shadow_quality"), Value::from("low"));
        set_setting(&home, "rac1", "pal", "shadow_quality", &Value::from("medium")).unwrap();
        set_setting(&home, "rac1", "pal", "gtao_quality", &Value::from("quality")).unwrap();
        set_setting(&home, "rac1", "pal", "shadow_quality", &Value::from("off")).unwrap();
        assert_eq!(value(&view(&home, "rac1", "pal", BakeStatus::default()), "shadow_quality"), Value::from("off"));
        set_setting(&home, "rac1", "pal", "shadow_quality", &Value::from("medium")).unwrap();
        set_setting(&home, "rac1", "pal", "texture_filter", &Value::from("normal")).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("msaa = 4\ngtao = 1\n"), "{text}");
        for line in [
            "enhanced_rendering = 1",
            "gtao_quality = quality",
            "enhanced_shadows = 1",
            "shadow_quality = medium",
            "texture_filter = normal",
            "agx_exposure = 1.5",
            "agx_preview = 0",
            "motion_blur = 1",
        ] {
            assert!(text.lines().any(|l| l == line), "{line} in {text}");
        }
        let v = view(&home, "rac1", "pal", BakeStatus::default());
        assert!(v.entry.enabled);
        assert_eq!(value(&v, "texture_filter"), Value::from("normal"));
        assert_eq!(value(&v, "shadow_quality"), Value::from("medium"));
        assert_eq!(value(&v, "motion_blur"), Value::Bool(true));
        // Out of the port's range, another type, or not one of the entry's: refused, file kept.
        assert!(set_setting(&home, "rac1", "pal", "gtao", &Value::from("yes")).is_err());
        assert!(set_setting(&home, "rac1", "pal", "probe_spheres", &Value::Bool(true)).is_err());
        assert!(set_setting(&home, "rac1", "pal", "texture_filter", &Value::from("unknown")).is_err());
        assert!(set_setting(&home, "rac1", "pal", "shadow_quality", &Value::from("unknown")).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_versions_the_baker_reads_have_the_entry() {
        assert!(supported("rac1", "pal") && supported("rac1", "ntsc") && supported("rac2", "ntsc"));
        assert!(!supported("rac2", "pal") && !supported("rac3", "ntsc") && !supported("rac1", "jp"));
        // Going Commando's settings file is its own: switching it on leaves Ratchet & Clank's alone.
        let home = temp("rac2-entry");
        set_enabled(&home, "rac2", "ntsc", true).unwrap();
        assert!(view(&home, "rac2", "ntsc", BakeStatus::default()).entry.enabled);
        assert!(!view(&home, "rac1", "ntsc", BakeStatus::default()).entry.enabled);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn play_takes_vulkan_and_the_bakes_only_when_on() {
        let data = Path::new("/data/rac1");
        assert_eq!(play_additions("", data, "pal"), None);
        assert_eq!(play_additions("enhanced_rendering = 0\n", data, "pal"), None);
        let (env, args) = play_additions("enhanced_rendering = 1\n", data, "pal").unwrap();
        assert_eq!(env, vec![("OPENRAC_GL_API".to_string(), "vulkan".to_string())]);
        // Joined, not spelled out: Windows joins with a backslash.
        let probes = data.join("probes").join("pal");
        assert_eq!(args, vec!["--probes".to_string(), probes.display().to_string()]);
    }

    #[test]
    fn written_values_read_back_as_the_port_parses_them() {
        assert_eq!(written(&Value::from(1.0)), "1");
        assert_eq!(written(&Value::from(1.25)), "1.25");
        assert_eq!(written(&Value::from(0.05)), "0.05");
        assert_eq!(written(&Value::Bool(false)), "0");
    }
}
