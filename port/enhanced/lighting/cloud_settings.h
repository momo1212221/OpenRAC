// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once
#include <array>

namespace openrac::lighting {
// One static, periodic world-space cloud field; shared by offline GI and runtime sampling.
struct CloudParameters {
    bool enabled = true;
    unsigned resolution = 64;
    float coverage = .58f, density = .018f, base = 90, thickness = 50, scale = .035f;
    unsigned seed = 1;
    std::array<float, 3> colour{.95f, .97f, 1};
    bool operator==(const CloudParameters&) const = default;
};

}  // namespace openrac::lighting
