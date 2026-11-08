// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once

namespace openrac::renderer {
// Medium halves sun map dimensions; Low quarters them; Ultra doubles High. Ratchet keeps a separate resolution.
enum class ShadowQuality {
    Low,
    Medium,
    High,
    Ultra,
    Count
};
inline constexpr const char* kShadowQualityKeys[] = {"low", "medium", "high", "ultra"};
inline constexpr const char* kShadowQualityNames[] = {"Low", "Medium", "High", "Ultra"};

constexpr int shadow_resolution_divisor(ShadowQuality quality) {
    return quality == ShadowQuality::Low ? 4 : quality == ShadowQuality::Medium ? 2 : 1;
}

constexpr int sun_shadow_resolution(int high_resolution, ShadowQuality quality) {
    if (quality == ShadowQuality::Ultra) {
        return high_resolution * 2;
    }
    return high_resolution / shadow_resolution_divisor(quality);
}

constexpr int hero_shadow_resolution(ShadowQuality quality) {
    return quality == ShadowQuality::Low ? 256
         : quality == ShadowQuality::Ultra ? 1024
                                           : 512;
}
}  // namespace openrac::renderer
