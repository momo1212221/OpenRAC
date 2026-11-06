// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once
#include <array>
#include <memory>
#include <string>

#include "renderer/effects.h"
#include "renderer/shader.h"

namespace openrac::scene {
class LevelScene;
}

namespace openrac::enhanced {
// Session controls are captured once per rendered frame, like water diagnostics.
struct AtmosphereVolume {
    std::array<float, 4> centre_radius{0, 0, 0, 8};
    std::array<float, 4> colour_density{.65f, .7f, .75f, 0};
};

struct AtmosphereSettings {
    bool enabled = true;
    float haze = 1;
    // Thin default aerial medium: enough for visible shadowed sun shafts and atmospheric depth without
    // replacing the authored skybox or requiring an expensive extra post-process.
    float height_density = .00022f;
    float base_height = 0;
    float height_falloff = 0;
    float distance = 750;
    float sun_strength = .65f;
    float anisotropy = .70f;
    float jitter_size = 1.0f;  // Fraction of the froxel used for sun-shadow jitter.
    float cloud_coverage = .58f;
    float cloud_density = .018f;
    std::array<float, 3> cloud_colour{.95f, .97f, 1.0f};
    float cloud_base = 90;
    float cloud_thickness = 50;
    float cloud_scale = .035f;
    float cloud_seed = 1;
    std::array<AtmosphereVolume, 8> volumes{};
};

AtmosphereSettings atmosphere_settings();
void set_atmosphere_settings(AtmosphereSettings value);

class Atmosphere {
public:
    Atmosphere() = default;
    ~Atmosphere();
    Atmosphere(const Atmosphere&) = delete;
    Atmosphere& operator=(const Atmosphere&) = delete;
    static std::string fragment_source(std::string source);
    static std::string sampling_source();
    void load();
    void prepare(
        const scene::LevelScene&,
        const renderer::FrameInput&,
        const renderer::RenderTarget&,
        const std::array<float, 4>& colour,
        const std::array<float, 4>& fog,
        bool enabled
    );
    void bind(const renderer::Shader&) const;
    void draw_sky(const renderer::RenderTarget&);
    void release();

private:
    std::array<renderer::Shader, 2> m_compute;
    renderer::Shader m_sky;
    unsigned m_buffer = 0, m_texture = 0, m_vao = 0, m_dummy_depth = 0;
    int m_width = 0, m_height = 0, m_slices = 0;
    bool m_active = false, m_failed = false, m_sky_active = false;
    renderer::Mat4 m_view = renderer::identity();
    std::array<float, 4> m_screen{};
    float m_far = 750;
};

// Wrap an existing liquid material, or ordinary world particles, without changing their blend.
std::unique_ptr<renderer::EffectMaterial> atmosphere_material(
    scene::LevelScene&, std::unique_ptr<renderer::EffectMaterial> material = {}
);
}  // namespace openrac::enhanced
