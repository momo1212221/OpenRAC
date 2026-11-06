// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Own cached colour textures independently of the original texture pool.
#pragma once
#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <unordered_map>

#include "renderer/shader.h"
#include "renderer/texture_filter.h"
#include "renderer/texture_pool.h"

namespace openrac::enhanced {
// Installs the optional colour provider once, after a supported graphics context is current.
void install_texture_upscaler(renderer::TexturePool& textures);

// Immutable 2x/4x/8x reconstructed RGB textures. Original alpha, mipmaps and non-colour uses stay in the pool.
class TextureUpscaler final : public renderer::UpscaledTextureColours {
public:
    unsigned prepare(
        unsigned source,
        int width,
        int height,
        unsigned wrap,
        renderer::TextureFilter filter,
        std::uint64_t frame
    ) override;
    void forget(unsigned source) override;
    void clear_cache() override;
    void release() override;

    std::size_t cache_bytes() const { return m_cache_bytes; }

    std::uint64_t cache_builds() const { return m_cache_builds; }

    static constexpr std::size_t kCacheBudget = 512ull * 1024 * 1024;

private:
    struct Entry {
        // Three scale levels (2x/4x/8x), each with four wrap combinations.
        std::array<unsigned, 12> textures{};
        std::array<std::size_t, 12> bytes{};
        std::uint64_t last_used = 0;
    };

    unsigned create(
        unsigned source, int width, int height, unsigned wrap, int scale, std::string& error
    );
    bool reserve(std::size_t bytes, std::uint64_t frame);
    void release(Entry& entry);

    static constexpr std::size_t byte_size(int width, int height, int scale) {
        return static_cast<std::size_t>(width) * static_cast<std::size_t>(height)
               * static_cast<std::size_t>(scale) * static_cast<std::size_t>(scale) * 4;
    }

    static constexpr unsigned cache_slot(int scale, unsigned wrap) {
        return (scale == 2 ? 0u : scale == 4 ? 1u : 2u) * 4u + wrap;
    }

    std::array<renderer::Shader, 5> m_shaders;  // Four wrap combinations and a reference.
    unsigned m_buffer = 0;
    std::size_t m_buffer_capacity = 0;
    std::unordered_map<unsigned, Entry> m_cache;
    std::size_t m_cache_bytes = 0;
    std::uint64_t m_cache_builds = 0;
};
}  // namespace openrac::enhanced
