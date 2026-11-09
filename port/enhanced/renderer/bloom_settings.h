// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Player-configurable bloom values; no graphics backend dependency.
#pragma once

namespace openrac::enhanced {

struct BloomSettings {
    bool enabled = true;
    float strength = 0.055f;   // 0..1; the in-game slider shows 0..100%.
    float threshold = 0.82f;   // 0..2; first soft bright pass.
    float knee = 0.18f;        // 0..1; transition around the threshold.
    float scatter = 0.62f;     // 0..1; low-resolution halo mixing.
};

}  // namespace openrac::enhanced
