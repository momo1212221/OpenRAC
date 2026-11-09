// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Subtle multi-resolution bloom for the enhanced renderer.
#pragma once

#include <array>
#include <cstddef>
#include <string>

#include "enhanced/renderer/bloom_settings.h"
#include "renderer/framebuffer.h"
#include "renderer/shader.h"

namespace openrac::enhanced {

// Whether the real in-game render worker completed the GPU bloom pass at least once.
enum class BloomPassStatus { Never, Completed, Failed };
[[nodiscard]] BloomPassStatus bloom_pass_status();

class Bloom {
public:
    ~Bloom();

    // Applies configurable bloom to completed world colour; changes take effect on the next frame.
    void apply(renderer::FrameBuffer& output, const BloomSettings& options);
    // Compatibility entry point used by existing rendering tests.
    void apply(renderer::FrameBuffer& output, float strength = 0.055f);

    // Releases graphics resources on their owning context.
    void release();

private:
    static constexpr int kMaxLevels = 5;

    struct Level {
        unsigned colour = 0;
        int width = 0;
        int height = 0;
    };

    [[nodiscard]] bool prepare(int width, int height, std::string& error);
    void release_level(Level& level);
    void allocate_texture(Level& level, int width, int height);
    [[nodiscard]] bool ensure_buffer(std::size_t bytes);
    [[nodiscard]] bool copy_output(unsigned texture, int width, int height, std::string& error);

    [[nodiscard]] bool downsample(
        unsigned source,
        int source_width,
        int source_height,
        Level& destination,
        bool first,
        const BloomSettings& options,
        std::string& error
    );
    [[nodiscard]] bool upsample(
        unsigned high, unsigned low, unsigned destination, int width, int height,
        const BloomSettings& options, std::string& error
    );
    [[nodiscard]] bool composite(
        renderer::FrameBuffer& output, float strength, bool preview_only, std::string& error
    );

    std::array<Level, kMaxLevels> m_levels{};
    int m_count = 0;

    renderer::Shader m_downsample;
    renderer::Shader m_upsample;
    renderer::Shader m_composite;

    unsigned m_buffer = 0;
    std::size_t m_buffer_capacity = 0;
    unsigned m_vao = 0;
    bool m_failed = false;
    bool m_logged_success = false;
    unsigned m_failed_frames = 0;  // Retry a transient graphics/resize failure after 120 frames.
};

}  // namespace openrac::enhanced
