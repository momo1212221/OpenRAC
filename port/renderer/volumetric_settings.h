// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once
#include <algorithm>

namespace openrac::renderer {
enum class VolumetricQuality {
    Off,
    Low,
    Medium,
    High,
    Ultra,
    Count
};
inline constexpr const char* kVolumetricQualityKeys[] = {"off", "low", "medium", "high", "ultra"};

struct VolumetricGrid {
    int width = 0, height = 0, slices = 0;
};

// Density of samples, not density of the medium. Medium retains the original grid; Ultra is an opt-in high-end sampling tier.
constexpr VolumetricGrid volumetric_grid(VolumetricQuality quality, int width, int height) {
    if (quality == VolumetricQuality::Off || width <= 0 || height <= 0) {
        return {};
    }
    const int pixels = quality == VolumetricQuality::Low     ? 64
                       : quality == VolumetricQuality::High  ? 16
                       : quality == VolumetricQuality::Ultra ? 8
                                                             : 32;
    const int max_width = quality == VolumetricQuality::Low     ? 48
                          : quality == VolumetricQuality::High  ? 160
                          : quality == VolumetricQuality::Ultra ? 240
                                                                : 96;
    const int max_height = quality == VolumetricQuality::Low     ? 27
                           : quality == VolumetricQuality::High  ? 90
                           : quality == VolumetricQuality::Ultra ? 135
                                                                 : 54;
    const int slices = quality == VolumetricQuality::Low     ? 24
                       : quality == VolumetricQuality::High  ? 96
                       : quality == VolumetricQuality::Ultra ? 144
                                                             : 48;
    return {
        std::clamp(1 + (width - 1) / pixels, 1, max_width),
        std::clamp(1 + (height - 1) / pixels, 1, max_height),
        slices
    };
}
}  // namespace openrac::renderer
