// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
// The launcher's independently saved module choices for this game process. Configured once by
// main before workers start; renderer code reads them without knowing the launcher or game.
#pragma once
#include <filesystem>

namespace openrac::launch_modules {
struct Config {
    bool enabled = true;
    bool ultra_textures = true;
    std::filesystem::path meshes;
};

// Sets this process's launch choices before any renderer worker starts.
void configure(const Config& config);
// Returns the launch choices. Readers must not overlap configure().
const Config& get();
}  // namespace openrac::launch_modules
