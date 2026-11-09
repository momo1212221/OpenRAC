// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Bloom stays off in compatibility rendering, preserves alpha and spreads bright colour on Vulkan.
#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

#include "enhanced/renderer/bloom.h"
#include "renderer/gl.h"
#include "renderer/surface_chain.h"
#if defined(OPENRAC_VULKAN)
#include "renderer/vulkan/temporal_graph.h"
#endif
#include "tests/check.h"
#include "tests/renderer_gl_context.h"

namespace {
int rgb_sum(const std::vector<std::uint8_t>& pixels, int width, int x, int y) {
    const std::size_t p = (static_cast<std::size_t>(y) * width + x) * 4;
    return int(pixels[p]) + int(pixels[p + 1]) + int(pixels[p + 2]);
}

std::uint64_t rgb_total(const std::vector<std::uint8_t>& pixels) {
    std::uint64_t total = 0;
    for (std::size_t p = 0; p < pixels.size(); p += 4) {
        total += pixels[p] + pixels[p + 1] + pixels[p + 2];
    }
    return total;
}

int draw_patch(openrac::renderer::FrameBuffer& target) {
    using namespace openrac::gl;
    glBindFramebuffer(GL_FRAMEBUFFER, target.id());
    glViewport(0, 0, target.width(), target.height());
    glDisable(GL_DEPTH_TEST);
    glDisable(GL_BLEND);
    glDisable(GL_SCISSOR_TEST);
    glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
    glClearColor(0.f, 0.f, 0.f, 77.f / 255.f);
    glClear(GL_COLOR_BUFFER_BIT);

    const int side = std::min({8, target.width() - 2, target.height() - 2});
    const int left = (target.width() - side) / 2;
    const int bottom = (target.height() - side) / 2;
    glEnable(GL_SCISSOR_TEST);
    glScissor(left, bottom, side, side);
    glClearColor(1.f, 0.85f, 0.35f, 77.f / 255.f);
    glClear(GL_COLOR_BUFFER_BIT);
    glDisable(GL_SCISSOR_TEST);
    return std::min(target.width() - 1, left + side);
}

// Regression for both callback entry paths: native before-HUD colour and the temporal
// post-resolve callback used when FSR is enabled. Either path must declare every compute
// dispatch, transfer and additive raster pass to the shared surface graph.
void apply_in_chain(openrac::enhanced::Bloom& bloom,
                    openrac::renderer::FrameBuffer& target, bool fsr_path,
                    const openrac::enhanced::BloomSettings& options) {
    using namespace openrac;
    const auto result = renderer::run_surface_chain([&] {
#if defined(OPENRAC_VULKAN)
        if (fsr_path && gl::is_vulkan()) {
            const auto temporal = vkgl::temporal_chain([&] { bloom.apply(target, options); });
            CHECK(temporal.has_value());
            return;
        }
#else
        (void)fsr_path;
#endif
        bloom.apply(target, options);
    });
    CHECK(result.has_value());
}

void apply_in_chain(openrac::enhanced::Bloom& bloom,
                    openrac::renderer::FrameBuffer& target, bool fsr_path, float strength) {
    openrac::enhanced::BloomSettings options;
    options.strength = strength;
    apply_in_chain(bloom, target, fsr_path, options);
}

void check_frame(openrac::enhanced::Bloom& bloom, int width, int height, bool fsr_path) {
    using namespace openrac;
    using namespace openrac::gl;

    renderer::FrameBuffer target;
    std::string error;
    CHECK(target.create(width, height, error, 1));

    const int near_x = draw_patch(target);
    const int cy = target.height() / 2;
    const auto before = target.read_rgba();
    apply_in_chain(bloom, target, fsr_path, 1.0f);
    const auto strong = target.read_rgba();

    if (enhanced_rendering_available()) {
        CHECK(rgb_total(strong) > rgb_total(before));

        // On normal frame sizes the Gaussian halo is stronger next to the patch than at a corner.
        if (width >= 32 && height >= 24) {
            const int near_before = rgb_sum(before, target.width(), near_x, cy);
            const int near_strong = rgb_sum(strong, target.width(), near_x, cy);
            const int far_strong = rgb_sum(strong, target.width(), 0, 0);
            CHECK(near_before == 0);
            CHECK(near_strong > near_before);
            CHECK(near_strong > far_strong);
        }

        for (std::size_t p = 3; p < strong.size(); p += 4) {
            CHECK(strong[p] == before[p]);
        }

        draw_patch(target);
        apply_in_chain(bloom, target, fsr_path, 0.055f);
        const auto subtle = target.read_rgba();
        CHECK(rgb_total(subtle) > rgb_total(before));
        CHECK(rgb_total(subtle) < rgb_total(strong));
    } else {
        CHECK(strong == before);
    }

    target.release();
}
}  // namespace

int main(int argc, char**) {
    using namespace openrac;
    using namespace openrac::gl;

    test::GlContext context;
    std::string error;
    if (!context.open(error)) {
        std::fprintf(stderr, "%s\n", error.c_str());
        return test::kSkip;
    }

    if (argc > 1) {
        CHECK(is_vulkan());
        CHECK(enhanced_rendering_available());
    }

    enhanced::BloomSettings choice;
    CHECK(enhanced::bloom_requested(choice));
    choice.enabled = false;
    CHECK(!enhanced::bloom_requested(choice));
    choice.enabled = true;
    choice.strength = 0.0f;
    CHECK(!enhanced::bloom_requested(choice));
    choice.preview_only = true;
    CHECK(enhanced::bloom_requested(choice));

    enhanced::Bloom bloom;
    CHECK(enhanced::bloom_pass_status() == enhanced::BloomPassStatus::Never);
    // Exercise both graph entry points, across large, odd and very small render targets.
    for (bool fsr_path : {false, true}) {
        check_frame(bloom, 129, 97, fsr_path);
        check_frame(bloom, 47, 31, fsr_path);
        check_frame(bloom, 9, 7, fsr_path);
    }
    CHECK(enhanced::bloom_pass_status()
          == (enhanced_rendering_available() ? enhanced::BloomPassStatus::Completed
                                              : enhanced::BloomPassStatus::Never));
    // A zero-strength/disabled enhanced effect must issue no dispatch and preserve colour.
    renderer::FrameBuffer disabled;
    CHECK(disabled.create(47, 31, error, 1));
    draw_patch(disabled);
    const auto disabled_before = disabled.read_rgba();
    apply_in_chain(bloom, disabled, false, 0.0f);
    CHECK(disabled.read_rgba() == disabled_before);
    enhanced::BloomSettings options;
    options.enabled = false;
    options.strength = 1.0f;
    apply_in_chain(bloom, disabled, false, options);
    CHECK(disabled.read_rgba() == disabled_before);
    options.enabled = true;
    options.threshold = 1.0f;
    options.knee = 0.0f;
    apply_in_chain(bloom, disabled, false, options);
    CHECK(disabled.read_rgba() == disabled_before);
    options.threshold = 0.20f;
    options.knee = 0.35f;
    options.scatter = 0.85f;
    apply_in_chain(bloom, disabled, true, options);
    if (enhanced_rendering_available()) {
        CHECK(rgb_total(disabled.read_rgba()) > rgb_total(disabled_before));
    } else {
        CHECK(disabled.read_rgba() == disabled_before);
    }
    disabled.release();

    // A small emitter must survive thresholding even if its neighbourhood is dark.
    // The old blur-before-threshold kernel lost this 2x2 source entirely.
    renderer::FrameBuffer tiny;
    CHECK(tiny.create(65, 65, error, 1));
    glBindFramebuffer(GL_FRAMEBUFFER, tiny.id());
    glViewport(0, 0, 65, 65);
    glDisable(GL_BLEND);
    glDisable(GL_SCISSOR_TEST);
    glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
    glClearColor(0.0f, 0.0f, 0.0f, 77.0f / 255.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glEnable(GL_SCISSOR_TEST);
    glScissor(31, 31, 2, 2);
    glClearColor(1.0f, 1.0f, 1.0f, 77.0f / 255.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glDisable(GL_SCISSOR_TEST);
    const auto small_before = tiny.read_rgba();
    enhanced::BloomSettings tiny_settings;
    tiny_settings.strength = 1.0f;
    apply_in_chain(bloom, tiny, false, tiny_settings);
    const auto small_after = tiny.read_rgba();
    if (enhanced_rendering_available()) {
        CHECK(rgb_total(small_after) > rgb_total(small_before));
        CHECK(rgb_sum(small_after, 65, 35, 32) > rgb_sum(small_before, 65, 35, 32));
    } else {
        CHECK(small_after == small_before);
    }

    // Preview-only mode works with intensity zero: mask RGB replaces world RGB.
    tiny_settings.preview_only = true;
    tiny_settings.strength = 0.0f;
    glBindFramebuffer(GL_FRAMEBUFFER, tiny.id());
    glDisable(GL_SCISSOR_TEST);
    glClearColor(0.0f, 0.0f, 0.0f, 77.0f / 255.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glEnable(GL_SCISSOR_TEST);
    glScissor(31, 31, 2, 2);
    glClearColor(1.0f, 1.0f, 1.0f, 77.0f / 255.0f);
    glClear(GL_COLOR_BUFFER_BIT);
    glDisable(GL_SCISSOR_TEST);
    apply_in_chain(bloom, tiny, true, tiny_settings);
    const auto small_preview = tiny.read_rgba();
    if (enhanced_rendering_available()) {
        CHECK(rgb_sum(small_preview, 65, 31, 31) < rgb_sum(small_before, 65, 31, 31));
        CHECK(rgb_sum(small_preview, 65, 35, 32) > 0);
    } else {
        CHECK(small_preview == small_before);
    }
    for (std::size_t p = 3; p < small_preview.size(); p += 4) {
        CHECK(small_preview[p] == small_before[p]);
    }
    tiny.release();
    bloom.release();
    CHECK(enhanced::bloom_pass_status() == enhanced::BloomPassStatus::Never);

    CHECK(glGetError() == GL_NO_ERROR);
    std::puts("Bloom graph callbacks, live tuning, bypass, alpha and resize checks complete");
    return test::result();
}
