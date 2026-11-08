// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include "enhanced/renderer/ray_traced_sun.h"

#include "platform/ray_tracing.h"
#include "scene/level.h"

namespace openrac::enhanced {

#if defined(__APPLE__)
RayTracedSunStats ray_trace_static_sun_metal(
    scene::LevelData& level, renderer::Vec3 sunlight_travel, std::string& error
);
#endif

RayTracedSunStats ray_trace_static_sun(
    scene::LevelData& level, renderer::Vec3 sunlight_travel, std::string& error
) {
    const platform::RayTracingSupport support = platform::ray_tracing_support();
    if (!support.supported) {
        return {};
    }
#if defined(__APPLE__)
    if (support.backend == platform::RayTracingBackend::Metal) {
        return ray_trace_static_sun_metal(level, sunlight_travel, error);
    }
#endif
    error = "runtime static-sun ray tracing is not implemented for this backend yet";
    return {};
}

}  // namespace openrac::enhanced
