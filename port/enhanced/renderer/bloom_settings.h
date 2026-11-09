// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Player-configurable bloom values; no graphics backend dependency.
#pragma once

namespace openrac::enhanced {

struct BloomSettings {
    bool enabled = true;
    float strength = 0.055f;   // 0..2; the in-game slider shows 0..200%.
    float threshold = 0.82f;   // 0..1; first soft bright pass on display-referred SDR.
    float knee = 0.18f;        // 0..1; transition around the threshold.
    float scatter = 0.62f;     // 0..1; low-resolution halo mixing.
    bool preview_only = false; // Diagnostic: display the glow mask instead of world RGB.
};

[[nodiscard]] inline constexpr bool bloom_requested(const BloomSettings& options) {
    return options.enabled && (options.strength > 0.0f || options.preview_only);
}

}  // namespace openrac::enhanced
