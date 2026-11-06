// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Bounded Vulkan colour-reconstruction cache for world textures.
#include "enhanced/renderer/texture_upscale.h"

#include <algorithm>
#include <array>
#include <utility>

#include "common/log.h"
#include "enhanced_texture_shaders.h"
#include "renderer/gl.h"
#include "renderer/render_opt.h"
#include "renderer/texture_pool.h"
#if defined(OPENRAC_VULKAN) && OPENRAC_VULKAN
#include "renderer/vulkan/vkgl.h"
#endif
namespace openrac::enhanced {
using namespace openrac::gl;

void install_texture_upscaler(renderer::TexturePool& textures) {
    // Applications register enhanced providers; the scene depends only on the pool's interface.
    if (enhanced_rendering_available() && !textures.has_upscaled_texture_colours()) {
        textures.set_upscaled_texture_colours(std::make_unique<TextureUpscaler>());
    }
}

void TextureUpscaler::release(Entry& entry) {
    for (std::size_t i = 0; i < entry.textures.size(); ++i) {
        auto& texture = entry.textures[i];
        if (texture) {
            glDeleteTextures(1, &texture);
            m_cache_bytes -= std::min(m_cache_bytes, entry.bytes[i]);
            texture = 0;
            entry.bytes[i] = 0;
        }
    }
}

bool TextureUpscaler::reserve(std::size_t bytes, std::uint64_t frame) {
    if (bytes > kCacheBudget) {
        return false;
    }

    // Keep recently used colours to avoid rebuilding an oversized working set each frame.
    while (m_cache_bytes + bytes > kCacheBudget) {
        Entry* oldest = nullptr;

        for (auto& [source, entry] : m_cache) {
            const bool cached =
                std::any_of(entry.textures.begin(), entry.textures.end(), [](unsigned id) {
                    return id != 0;
                });

            if (cached && frame - entry.last_used >= renderer::TexturePool::kKeepFrames
                && (!oldest || entry.last_used < oldest->last_used)) {
                oldest = &entry;
            }
        }

        if (!oldest) {
            return false;
        }

        release(*oldest);
    }

    return true;
}

unsigned TextureUpscaler::prepare(
    unsigned source,
    int width,
    int height,
    unsigned wrap,
    renderer::TextureFilter filter,
    std::uint64_t frame
) {
    const int scale = renderer::texture_filter_scale(filter);
    if (!enhanced_rendering_available() || scale <= 1 || width <= 1 || height <= 1) {
        return 0;
    }

    wrap &= 3u;
    auto& entry = m_cache[source];
    entry.last_used = frame;
    const unsigned slot = cache_slot(scale, wrap);
    auto& result = entry.textures[slot];

    if (result) {
        return result;
    }

    const auto bytes = byte_size(width, height, scale);
    if (!reserve(bytes, frame)) {
        return 0;
    }

    std::string error;
    result = create(source, width, height, wrap, scale, error);

    if (result) {
        entry.bytes[slot] = bytes;
        m_cache_bytes += bytes;
        ++m_cache_builds;
    } else {
        static log::Capped told(3);
        if (told()) {
            log::warn("{}; using Normal filtering", error);
        }
    }

    return result;
}

void TextureUpscaler::forget(unsigned source) {
    auto found = m_cache.find(source);

    if (found != m_cache.end()) {
        release(found->second);
        m_cache.erase(found);
    }
}

unsigned TextureUpscaler::create(
    unsigned source, int width, int height, unsigned wrap, int scale, std::string& error
) {
#if defined(OPENRAC_VULKAN) && OPENRAC_VULKAN
    if (!enhanced_rendering_available()) {
        return 0;
    }
    if (scale != 2 && scale != 4 && scale != 8) {
        error = "texture upscale scale must be 2x, 4x or 8x";
        return 0;
    }

    GLint max_texture_size = 0;
    glGetIntegerv(GL_MAX_TEXTURE_SIZE, &max_texture_size);
    if (width > max_texture_size / scale || height > max_texture_size / scale) {
        error = "texture upscale output exceeds the GPU maximum texture size";
        return 0;
    }

    auto& shader = m_shaders
        [renderer::render_opt::enabled(renderer::render_opt::Opt::ShaderVariants) ? wrap : 4u];
    if (!shader.id()) {
        std::string source_code(shaders::texture_upscale_comp);
        if (renderer::render_opt::enabled(renderer::render_opt::Opt::ShaderVariants)) {
            source_code =
                renderer::shader_constants(std::move(source_code), {{"wrap_mode", int(wrap)}});
        }
        if (!shader.build_compute("texture upscale", source_code, error)) {
            return 0;
        }
    }
    if (!m_buffer) {
        glGenBuffers(1, &m_buffer);
    }

    const int passes = scale == 2 ? 1 : scale == 4 ? 2 : 3;
    std::array<unsigned, 3> stages{};
    unsigned current = source;
    int current_width = width, current_height = height;

    for (int pass = 0; pass < passes; ++pass) {
        const int output_width = current_width * 2;
        const int output_height = current_height * 2;
        unsigned output = 0;
        glGenTextures(1, &output);
        stages[pass] = output;
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, output);
        glTexImage2D(
            GL_TEXTURE_2D,
            0,
            GL_RGBA8,
            output_width,
            output_height,
            0,
            GL_RGBA,
            GL_UNSIGNED_BYTE,
            nullptr
        );
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAX_LEVEL, 0);
        glTexParameteri(
            GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, (wrap & 1) ? GL_REPEAT : GL_CLAMP_TO_EDGE
        );
        glTexParameteri(
            GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, (wrap & 2) ? GL_REPEAT : GL_CLAMP_TO_EDGE
        );

        shader.use();
        glUniform1i(shader.uniform("tex_T0"), 0);
        glUniform1i(shader.uniform("wrap_mode"), static_cast<int>(wrap));
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, current);

        glBindBuffer(GL_SHADER_STORAGE_BUFFER, m_buffer);
        const auto pass_bytes =
            static_cast<std::size_t>(output_width) * static_cast<std::size_t>(output_height) * 4;
        if (pass_bytes > m_buffer_capacity) {
            glBufferData(
                GL_SHADER_STORAGE_BUFFER,
                static_cast<GLsizeiptr>(pass_bytes),
                nullptr,
                GL_DYNAMIC_DRAW
            );
            m_buffer_capacity = pass_bytes;
        }
        glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 0, m_buffer);
        glDispatchCompute(
            static_cast<unsigned>((output_width + 7) / 8),
            static_cast<unsigned>((output_height + 7) / 8),
            1
        );
        const bool ok = vkgl::copy_buffer_to_texture(
            m_buffer, output, output_width, output_height, /*reuse_source=*/true
        );
        glBindBufferBase(GL_SHADER_STORAGE_BUFFER, 0, 0);

        if (!ok) {
            for (int i = 0; i <= pass; ++i) {
                if (stages[i]) {
                    glDeleteTextures(1, &stages[i]);
                }
            }
            error = "texture upscale output failed";
            return 0;
        }

        current = output;
        current_width = output_width;
        current_height = output_height;
    }

    // 4x/8x are successive edge-aware 2x reconstructions. Keep only the final level cached.
    for (int pass = 0; pass + 1 < passes; ++pass) {
        glDeleteTextures(1, &stages[pass]);
    }
    return stages[passes - 1];
#else
    (void)source;
    (void)width;
    (void)height;
    (void)wrap;
    (void)scale;
    error = "edge-aware texture upscaling requires Vulkan";
    return 0;
#endif
}

void TextureUpscaler::clear_cache() {
    for (auto& [source, entry] : m_cache) {
        release(entry);
    }

    m_cache.clear();
}

void TextureUpscaler::release() {
    clear_cache();
    for (auto& shader : m_shaders) {
        shader.release();
    }

    if (m_buffer) {
        glDeleteBuffers(1, &m_buffer);
    }

    m_buffer = 0;
    m_buffer_capacity = 0;
}
}  // namespace openrac::enhanced
