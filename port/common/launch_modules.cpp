// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#include "common/launch_modules.h"

namespace openrac::launch_modules {
namespace {
// This module owns the process's launch state; main sets it before worker threads exist.
Config& state() {
    static Config config;
    return config;
}
}  // namespace

void configure(const Config& config) {
    state() = config;
}

const Config& get() {
    return state();
}
}  // namespace openrac::launch_modules
