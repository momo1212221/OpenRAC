// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include "platform/ray_tracing.h"

#include <cstring>
#include <vector>

#include "platform/window.h"

#if defined(OPENRAC_VULKAN)
#include <vulkan/vulkan.h>
#endif

namespace openrac::platform {

#if defined(__APPLE__)
RayTracingSupport macos_metal_ray_tracing_support();
bool macos_ray_tracing_acceleration_structure_self_test(std::string& error);
#endif

const char* ray_tracing_backend_name(RayTracingBackend backend) {
    switch (backend) {
        case RayTracingBackend::None:
            return "none";
        case RayTracingBackend::VulkanRayQuery:
            return "Vulkan KHR ray query";
        case RayTracingBackend::VulkanPipeline:
            return "Vulkan KHR ray tracing pipeline";
        case RayTracingBackend::Metal:
            return "Metal ray tracing";
    }
    return "none";
}

namespace {

#if defined(OPENRAC_VULKAN)
bool has_extension(const std::vector<VkExtensionProperties>& extensions, const char* name) {
    for (const VkExtensionProperties& extension : extensions) {
        if (std::strcmp(extension.extensionName, name) == 0) {
            return true;
        }
    }
    return false;
}

RayTracingSupport vulkan_ray_tracing_support() {
    const VulkanNative native = vulkan_native();
    if (native.physical_device == nullptr) {
        return {};
    }

    const auto instance = reinterpret_cast<VkInstance>(native.instance);
    const auto physical = reinterpret_cast<VkPhysicalDevice>(native.physical_device);
    const auto get_instance_proc =
        reinterpret_cast<PFN_vkGetInstanceProcAddr>(native.get_instance_proc_addr);
    if (instance == VK_NULL_HANDLE || get_instance_proc == nullptr) {
        return {};
    }

    const auto enumerate_extensions = reinterpret_cast<PFN_vkEnumerateDeviceExtensionProperties>(
        get_instance_proc(instance, "vkEnumerateDeviceExtensionProperties")
    );
    const auto get_features = reinterpret_cast<PFN_vkGetPhysicalDeviceFeatures2>(
        get_instance_proc(instance, "vkGetPhysicalDeviceFeatures2")
    );
    const auto get_properties = reinterpret_cast<PFN_vkGetPhysicalDeviceProperties>(
        get_instance_proc(instance, "vkGetPhysicalDeviceProperties")
    );
    if (enumerate_extensions == nullptr || get_features == nullptr || get_properties == nullptr) {
        return {};
    }

    std::uint32_t count = 0;
    if (enumerate_extensions(physical, nullptr, &count, nullptr) != VK_SUCCESS) {
        return {};
    }
    std::vector<VkExtensionProperties> extensions(count);
    if (count != 0
        && enumerate_extensions(physical, nullptr, &count, extensions.data()) != VK_SUCCESS) {
        return {};
    }

    const bool has_acceleration =
        has_extension(extensions, VK_KHR_ACCELERATION_STRUCTURE_EXTENSION_NAME);
    const bool has_pipeline = has_extension(extensions, VK_KHR_RAY_TRACING_PIPELINE_EXTENSION_NAME);
    const bool has_query = has_extension(extensions, VK_KHR_RAY_QUERY_EXTENSION_NAME);
    const bool has_deferred =
        has_extension(extensions, VK_KHR_DEFERRED_HOST_OPERATIONS_EXTENSION_NAME);
    if (!has_acceleration || !has_deferred || (!has_pipeline && !has_query)) {
        return {};
    }

    VkPhysicalDeviceRayQueryFeaturesKHR query{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_RAY_QUERY_FEATURES_KHR
    };
    VkPhysicalDeviceRayTracingPipelineFeaturesKHR pipeline{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_RAY_TRACING_PIPELINE_FEATURES_KHR
    };
    VkPhysicalDeviceAccelerationStructureFeaturesKHR acceleration{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ACCELERATION_STRUCTURE_FEATURES_KHR
    };
    VkPhysicalDeviceBufferDeviceAddressFeatures bda{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_BUFFER_DEVICE_ADDRESS_FEATURES
    };
    VkPhysicalDeviceFeatures2 features{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2};
    features.pNext = &bda;
    bda.pNext = &acceleration;
    acceleration.pNext = &pipeline;
    pipeline.pNext = &query;
    get_features(physical, &features);

    if (!bda.bufferDeviceAddress || !acceleration.accelerationStructure) {
        return {};
    }
    const bool pipeline_ready = has_pipeline && pipeline.rayTracingPipeline;
    const bool query_ready = has_query && query.rayQuery;
    if (!pipeline_ready && !query_ready) {
        return {};
    }

    VkPhysicalDeviceProperties properties{};
    get_properties(physical, &properties);

    RayTracingSupport support;
    support.backend =
        pipeline_ready ? RayTracingBackend::VulkanPipeline : RayTracingBackend::VulkanRayQuery;
    support.supported = true;
    support.hardware_accelerated = true;
    support.from_render = query_ready;
    support.device = properties.deviceName;
    return support;
}
#endif

}  // namespace

RayTracingSupport ray_tracing_support() {
#if defined(__APPLE__)
    RayTracingSupport metal = macos_metal_ray_tracing_support();
    if (metal.supported) {
        return metal;
    }
#endif
#if defined(OPENRAC_VULKAN)
    return vulkan_ray_tracing_support();
#else
    return {};
#endif
}

bool ray_tracing_acceleration_structure_self_test(std::string& error) {
#if defined(__APPLE__)
    const RayTracingSupport metal_support = macos_metal_ray_tracing_support();
    if (metal_support.supported) {
        return macos_ray_tracing_acceleration_structure_self_test(error);
    }
#endif
    const RayTracingSupport support = ray_tracing_support();
    if (!support.supported) {
        error = "ray tracing is not supported by the active graphics device";
        return false;
    }
    error = "Vulkan acceleration-structure self-test is not wired yet";
    return false;
}

}  // namespace openrac::platform
