// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include <cmath>
#include <cstdio>
#include <string>

#include "enhanced/renderer/ray_traced_sun.h"
#include "platform/ray_tracing.h"
#include "scene/level.h"

namespace {

openrac::scene::Vertex vertex(float x, float y, float z) {
    openrac::scene::Vertex out{};
    out.position[0] = x;
    out.position[1] = y;
    out.position[2] = z;
    out.normal[0] = 0.0f;
    out.normal[1] = 0.0f;
    out.normal[2] = 1.0f;
    out.colour[0] = 1.0f;
    out.colour[1] = 1.0f;
    out.colour[2] = 1.0f;
    out.colour[3] = 1.0f;
    return out;
}

openrac::scene::Model model(std::uint32_t first) {
    openrac::scene::Model out{};
    openrac::scene::Primitive primitive{};
    primitive.first_index = first;
    primitive.index_count = 3;
    primitive.material = 0;
    out.primitives.push_back(primitive);
    return out;
}

openrac::scene::Instance instance(std::uint32_t model_index) {
    openrac::scene::Instance out{};
    out.model = model_index;
    out.matrix = openrac::renderer::identity();
    return out;
}

}  // namespace

int main() {
    using namespace openrac;

    const platform::RayTracingSupport support = platform::ray_tracing_support();
    if (!support.supported || support.backend != platform::RayTracingBackend::Metal) {
        std::puts("Metal ray tracing unavailable; skipping");
        return 77;
    }

    scene::LevelData level;
    scene::Material material{};
    material.colour = {1.0f, 1.0f, 1.0f, 1.0f};
    level.materials.push_back(material);

    // Receiver triangle at z=0.
    level.vertices.push_back(vertex(-1.0f, -1.0f, 0.0f));
    level.vertices.push_back(vertex(1.0f, -1.0f, 0.0f));
    level.vertices.push_back(vertex(0.0f, 1.0f, 0.0f));
    // Larger blocker above it at z=1.
    level.vertices.push_back(vertex(-2.0f, -2.0f, 1.0f));
    level.vertices.push_back(vertex(2.0f, -2.0f, 1.0f));
    level.vertices.push_back(vertex(0.0f, 2.0f, 1.0f));
    level.indices = {0, 1, 2, 3, 4, 5};
    level.models.push_back(model(0));
    level.models.push_back(model(3));
    level.instances[static_cast<std::size_t>(scene::Layer::Terrain)].push_back(instance(0));
    level.instances[static_cast<std::size_t>(scene::Layer::Ties)].push_back(instance(1));

    std::string error;
    const enhanced::RayTracedSunStats stats =
        enhanced::ray_trace_static_sun(level, {0.0f, 0.0f, -1.0f}, error);
    if (!stats.active) {
        std::fprintf(stderr, "ray-traced sun failed: %s\n", error.c_str());
        return 1;
    }
    if (stats.triangles != 2 || stats.receivers != 3
        || level.rt_sun_visibility.size() != level.vertices.size()) {
        std::fprintf(
            stderr,
            "unexpected RT scene: triangles=%zu receivers=%zu visibility=%zu\n",
            stats.triangles,
            stats.receivers,
            level.rt_sun_visibility.size()
        );
        return 1;
    }
    for (std::size_t i = 0; i < 3; ++i) {
        if (level.rt_sun_visibility[i] > 0.01f) {
            std::fprintf(
                stderr,
                "receiver %zu should be shadowed, visibility=%f\n",
                i,
                level.rt_sun_visibility[i]
            );
            return 1;
        }
    }

    std::printf(
        "RT static sun: OK (%s, hardware=%d)\n", stats.backend.c_str(), stats.hardware ? 1 : 0
    );
    return 0;
}
