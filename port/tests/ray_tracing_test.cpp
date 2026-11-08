// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include <cstdio>
#include <string>

#include "platform/ray_tracing.h"

int main() {
    const openrac::platform::RayTracingSupport support = openrac::platform::ray_tracing_support();
    std::printf(
        "backend=%s supported=%d hardware=%d from_render=%d device=%s\n",
        openrac::platform::ray_tracing_backend_name(support.backend),
        support.supported ? 1 : 0,
        support.hardware_accelerated ? 1 : 0,
        support.from_render ? 1 : 0,
        support.device.c_str()
    );
    if (!support.supported) {
        return 0;
    }

    std::string error;
    if (!openrac::platform::ray_tracing_acceleration_structure_self_test(error)) {
        std::fprintf(stderr, "acceleration-structure self-test failed: %s\n", error.c_str());
        return 1;
    }
    std::puts("acceleration-structure self-test: OK");
    return 0;
}
