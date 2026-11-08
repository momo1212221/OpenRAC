// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include <cstdint>
#include <dlfcn.h>

#include "platform/ray_tracing.h"
#include "platform/window.h"

#import <Metal/Metal.h>

namespace openrac::platform {
namespace {

template <typename Fn>
Fn moltenvk_symbol(const char* name) {
    return reinterpret_cast<Fn>(dlsym(RTLD_DEFAULT, name));
}

id<MTLDevice> system_device() {
    return MTLCreateSystemDefaultDevice();
}

}  // namespace

RayTracingSupport macos_metal_ray_tracing_support() {
    @autoreleasepool {
        id<MTLDevice> device = system_device();
        if (device == nil || ![device respondsToSelector:@selector(supportsRaytracing)]
            || !device.supportsRaytracing) {
            return {};
        }

        RayTracingSupport support;
        support.backend = RayTracingBackend::Metal;
        support.supported = true;
        support.from_render = [device respondsToSelector:@selector(supportsRaytracingFromRender)]
                              && device.supportsRaytracingFromRender;
#if defined(__MAC_OS_X_VERSION_MAX_ALLOWED) && __MAC_OS_X_VERSION_MAX_ALLOWED >= 140000
        if (@available(macOS 14.0, *)) {
            support.hardware_accelerated = [device supportsFamily:MTLGPUFamilyApple9];
        }
#endif
        support.device = device.name.UTF8String != nullptr ? device.name.UTF8String : "Apple GPU";
        return support;
    }
}

bool macos_ray_tracing_acceleration_structure_self_test(std::string& error) {
    @autoreleasepool {
        id<MTLDevice> device = system_device();
        if (device == nil || !device.supportsRaytracing) {
            error = "Metal ray tracing is unavailable";
            return false;
        }

        const float triangle[] = {
            -1.0f,
            -1.0f,
            0.0f,
            1.0f,
            -1.0f,
            0.0f,
            0.0f,
            1.0f,
            0.0f,
        };
        id<MTLBuffer> vertices = [device newBufferWithBytes:triangle
                                                     length:sizeof(triangle)
                                                    options:MTLResourceStorageModeShared];
        if (vertices == nil) {
            error = "could not allocate Metal ray-tracing vertex buffer";
            return false;
        }

        MTLAccelerationStructureTriangleGeometryDescriptor* geometry =
            [MTLAccelerationStructureTriangleGeometryDescriptor descriptor];
        geometry.vertexBuffer = vertices;
        geometry.vertexBufferOffset = 0;
        geometry.vertexStride = sizeof(float) * 3;
        geometry.triangleCount = 1;
        geometry.opaque = YES;

        MTLPrimitiveAccelerationStructureDescriptor* descriptor =
            [MTLPrimitiveAccelerationStructureDescriptor descriptor];
        descriptor.geometryDescriptors = @[geometry];

        const MTLAccelerationStructureSizes sizes =
            [device accelerationStructureSizesWithDescriptor:descriptor];
        id<MTLAccelerationStructure> structure =
            [device newAccelerationStructureWithSize:sizes.accelerationStructureSize];
        id<MTLBuffer> scratch = [device newBufferWithLength:sizes.buildScratchBufferSize
                                                    options:MTLResourceStorageModePrivate];
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (structure == nil || scratch == nil || queue == nil) {
            error = "could not allocate Metal acceleration-structure resources";
            return false;
        }

        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLAccelerationStructureCommandEncoder> encoder =
            [command accelerationStructureCommandEncoder];
        if (command == nil || encoder == nil) {
            error = "could not create Metal acceleration-structure command encoder";
            return false;
        }

        [encoder buildAccelerationStructure:structure
                                 descriptor:descriptor
                              scratchBuffer:scratch
                        scratchBufferOffset:0];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];

        if (command.status != MTLCommandBufferStatusCompleted) {
            NSString* message = command.error.localizedDescription;
            error = message.UTF8String != nullptr ? message.UTF8String
                                                  : "Metal acceleration-structure build failed";
            return false;
        }

        static const char* const kRayTestSource = R"metal(
#include <metal_stdlib>
#include <metal_raytracing>
using namespace metal;
using namespace raytracing;

kernel void openrac_rt_self_test(
    acceleration_structure<> scene [[buffer(0)]],
    device uint* result [[buffer(1)]])
{
    intersector<> trace;
    ray test_ray(float3(0.0f, 0.0f, 1.0f), float3(0.0f, 0.0f, -1.0f));
    const auto hit = trace.intersect(test_ray, scene);
    result[0] = hit.type == intersection_type::triangle ? 1u : 0u;
}
)metal";
        NSString* ray_source = [NSString stringWithUTF8String:kRayTestSource];

        NSError* library_error = nil;
        id<MTLLibrary> library = [device newLibraryWithSource:ray_source
                                                      options:nil
                                                        error:&library_error];
        id<MTLFunction> function = [library newFunctionWithName:@"openrac_rt_self_test"];
        NSError* pipeline_error = nil;
        id<MTLComputePipelineState> pipeline =
            function != nil
                ? [device newComputePipelineStateWithFunction:function error:&pipeline_error]
                : nil;
        id<MTLBuffer> result = [device newBufferWithLength:sizeof(std::uint32_t)
                                                   options:MTLResourceStorageModeShared];
        if (library == nil || function == nil || pipeline == nil || result == nil) {
            NSString* message = pipeline_error != nil ? pipeline_error.localizedDescription
                                                      : library_error.localizedDescription;
            error = message.UTF8String != nullptr ? message.UTF8String
                                                  : "could not create Metal ray-query pipeline";
            return false;
        }
        *static_cast<std::uint32_t*>(result.contents) = 0;

        id<MTLCommandBuffer> trace_command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> compute = [trace_command computeCommandEncoder];
        [compute setComputePipelineState:pipeline];
        [compute setAccelerationStructure:structure atBufferIndex:0];
        [compute setBuffer:result offset:0 atIndex:1];
        [compute dispatchThreads:MTLSizeMake(1, 1, 1) threadsPerThreadgroup:MTLSizeMake(1, 1, 1)];
        [compute endEncoding];
        [trace_command commit];
        [trace_command waitUntilCompleted];

        if (trace_command.status != MTLCommandBufferStatusCompleted) {
            NSString* message = trace_command.error.localizedDescription;
            error = message.UTF8String != nullptr ? message.UTF8String
                                                  : "Metal ray-query dispatch failed";
            return false;
        }
        if (*static_cast<std::uint32_t*>(result.contents) != 1) {
            error = "Metal ray-query pipeline missed the validation triangle";
            return false;
        }
        return true;
    }
}

MetalVulkanBridge metal_vulkan_bridge() {
    @autoreleasepool {
        const VulkanNative native = vulkan_native();
        if (native.physical_device == nullptr || native.queue == nullptr) {
            return {};
        }

        using GetDevice = void (*)(void*, void**);
        using GetQueue = void (*)(void*, void**);
        GetDevice get_device = moltenvk_symbol<GetDevice>("vkGetMTLDeviceMVK");
        GetQueue get_queue = moltenvk_symbol<GetQueue>("vkGetMTLCommandQueueMVK");
        if (get_device == nullptr || get_queue == nullptr) {
            return {};
        }

        MetalVulkanBridge bridge;
        get_device(native.physical_device, &bridge.device);
        get_queue(native.queue, &bridge.command_queue);
        return bridge;
    }
}

void* metal_texture_from_vulkan_image(void* image) {
    using GetTexture = void (*)(void*, void**);
    GetTexture get_texture = moltenvk_symbol<GetTexture>("vkGetMTLTextureMVK");
    if (get_texture == nullptr || image == nullptr) {
        return nullptr;
    }
    void* texture = nullptr;
    get_texture(image, &texture);
    return texture;
}

void* metal_buffer_from_vulkan_buffer(void* buffer) {
    using GetBuffer = void (*)(void*, void**);
    GetBuffer get_buffer = moltenvk_symbol<GetBuffer>("vkGetMTLBufferMVK");
    if (get_buffer == nullptr || buffer == nullptr) {
        return nullptr;
    }
    void* metal = nullptr;
    get_buffer(buffer, &metal);
    return metal;
}

}  // namespace openrac::platform
