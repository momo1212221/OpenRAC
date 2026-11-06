// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once

#include <string>

namespace openrac::platform {

enum class RayTracingBackend {
    None,
    VulkanRayQuery,
    VulkanPipeline,
    Metal,
};

struct RayTracingSupport {
    RayTracingBackend backend = RayTracingBackend::None;
    bool supported = false;
    bool hardware_accelerated = false;
    bool from_render = false;
    std::string device;
};

const char* ray_tracing_backend_name(RayTracingBackend backend);
RayTracingSupport ray_tracing_support();

// Builds one real acceleration structure and waits for the GPU to finish it.
// This validates the native backend without changing any game state.
bool ray_tracing_acceleration_structure_self_test(std::string& error);

#if defined(__APPLE__)
// MoltenVK exposes its underlying Metal objects even though it does not expose Vulkan KHR RT.
struct MetalVulkanBridge {
    void* device = nullptr;
    void* command_queue = nullptr;

    explicit operator bool() const { return device != nullptr && command_queue != nullptr; }
};

MetalVulkanBridge metal_vulkan_bridge();
void* metal_texture_from_vulkan_image(void* image);
void* metal_buffer_from_vulkan_buffer(void* buffer);
#endif

}  // namespace openrac::platform
