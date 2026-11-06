// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once

#include <string>

#include "renderer/math.h"

namespace openrac::scene {
struct LevelData;
}

namespace openrac::enhanced {

struct RayTracedSunStats {
    bool active = false;
    bool hardware = false;
    std::size_t triangles = 0;
    std::size_t receivers = 0;
    std::string backend;
};

RayTracedSunStats ray_trace_static_sun(
    scene::LevelData& level, renderer::Vec3 sunlight_travel, std::string& error
);

}  // namespace openrac::enhanced
