// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Optional world-texture colour reconstruction shared by renderer backends.
#pragma once
#include <cstdint>

namespace openrac::renderer {

inline constexpr unsigned kUpscaledTextureColoursUnit = 13;
enum class TextureFilter {
    Normal,
    EdgeAware,
    EdgeAware4x,
    EdgeAware8x,
    Count
};

inline constexpr int texture_filter_scale(TextureFilter filter) {
    switch (filter) {
        case TextureFilter::EdgeAware:
            return 2;
        case TextureFilter::EdgeAware4x:
            return 4;
        case TextureFilter::EdgeAware8x:
            return 8;
        default:
            return 1;
    }
}

// Optional provider of upscaled texture colours. The source remains authoritative
// for alpha and minification. GPU objects are released while the context is current.
class UpscaledTextureColours {
public:
    virtual ~UpscaledTextureColours() = default;
    virtual unsigned prepare(
        unsigned source,
        int width,
        int height,
        unsigned wrap,
        TextureFilter filter,
        std::uint64_t frame
    ) = 0;
    virtual void forget(unsigned source) = 0;
    virtual void clear_cache() = 0;
    virtual void release() = 0;
};
}  // namespace openrac::renderer
