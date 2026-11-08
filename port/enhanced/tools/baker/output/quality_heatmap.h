// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once

#include <span>
#include <string>
#include <string_view>

#include "enhanced/lighting/probes.h"

namespace openrac::lighting {

struct ProbeQualityGrid {
    std::string_view name;
    const ProbeVolume* volume = nullptr;
};

// Browser-viewable, dependency-free diagnostic for manual bake review.
// The 32 grids with the highest invalid-probe ratio are shown, aggregated top-down.
std::string probe_quality_svg(std::span<const ProbeQualityGrid> grids);

}  // namespace openrac::lighting
