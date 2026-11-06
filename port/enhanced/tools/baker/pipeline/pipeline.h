// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Automatic native enhanced baking from exported levels, with in-process water source decoding.
#pragma once

#include <filesystem>
#include <ostream>
#include <span>
#include <string>

namespace openrac::lighting {
// Owns cache locking, memory-admitted levels, verification and recoverable publication.
// Arguments omit the executable; diagnostics retain the launcher's progress and error protocol.
int run_bake_pipeline(std::span<const std::string> arguments, std::ostream& output);
// Non-destructively reports whether another process currently owns this bake cache.
bool bake_cache_busy(const std::filesystem::path& directory);
}  // namespace openrac::lighting
