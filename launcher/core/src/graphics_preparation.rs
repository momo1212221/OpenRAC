// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent offline preparation: lighting/GI, water, clouds, textures and Metal sunlight.
//! The renderer consumes prepared data; switching this module never switches the renderer.
mod bakes;
pub use crate::enhanced::{BAKE_ACTION, NO_BAKER_HERE, SHIPS_BAKER};
pub use bakes::{bake_log, bake_status, delete_bakes, sync_cloud_profile, version_bake_status, BakeStatus};
pub const ID: &str = "graphics-preparation";

pub fn owns(key: &str) -> bool {
    key.starts_with("cloud_") || matches!(key, "hardware_auto" | "offline_ultra_textures" | "ray_tracing")
}

pub fn view(home: &std::path::Path, game: &str, version: &str, bake: BakeStatus) -> crate::enhanced::EnhancedView {
    let mut view = crate::enhanced::view_all(home, game, version, bake);
    view.entry.id = ID.into();
    view.entry.name = "Graphics preparation".into();
    view.entry.author = "OpenRAC fork".into();
    view.entry.version = env!("CARGO_PKG_VERSION").into();
    view.entry.description = "Prepare lighting/GI, water, clouds and Ultra/AI assets. Native Metal sunlight support works with Enhanced graphics. Preparation also works while the renderer is off.".into();
    view.entry.enabled = crate::launcher_addons::enabled(home, ID, game, version);
    view.entry.state = if view.entry.enabled { "ready" } else { "off" }.into();
    view.entry.settings.retain(|s| owns(&s.spec.key));
    for preset in &mut view.entry.presets {
        preset.values.retain(|key, _| owns(key));
    }
    view.entry.presets.retain(|p| !p.values.is_empty());
    view
}

/// Settings owned by preparation, separate from the renderer schema.
pub fn settings() -> Vec<crate::plugins::SettingSpec> {
    use crate::enhanced::{group, range, spec};
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
            spec(
                "offline_ultra_textures",
                "Ultra textures before Play",
                "bool",
                true.into(),
                "Pre-renders known game textures once at 8x up to a 4096-pixel edge, saves them as a persistent texture pack, and reuses that pack on later starts. This keeps 2x/4x/8x reconstruction out of the render loop.",
            ),
            "textures",
        ),
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
    ]
}
