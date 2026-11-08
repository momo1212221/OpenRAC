// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include "enhanced/renderer/ray_traced_sun.h"

#include <algorithm>
#include <cmath>
#include <vector>

#include "platform/ray_tracing.h"
#include "scene/level.h"

#import <Metal/Metal.h>

namespace openrac::enhanced {
namespace {

struct Packed3 {
    float x, y, z;
};

struct RayReceiver {
    float position[4];
    float normal[4];
};

bool opaque_caster(const scene::LevelData& level, const scene::Primitive& primitive) {
    if (primitive.material < 0
        || static_cast<std::size_t>(primitive.material) >= level.materials.size()) {
        return false;
    }
    const scene::Material& material = level.materials[static_cast<std::size_t>(primitive.material)];
    return !material.metal && !material.blend && !material.cutout && material.colour[3] >= 0.99f;
}

renderer::Vec3 point(const scene::Vertex& vertex) {
    return {vertex.position[0], vertex.position[1], vertex.position[2]};
}

renderer::Vec3 normal(const scene::Vertex& vertex) {
    return {vertex.normal[0], vertex.normal[1], vertex.normal[2]};
}

bool finite(const renderer::Vec3& v) {
    return std::isfinite(v[0]) && std::isfinite(v[1]) && std::isfinite(v[2]);
}

void append_static_layer(
    const scene::LevelData& level, scene::Layer layer, std::vector<Packed3>& triangles
) {
    const auto& instances = level.instances[static_cast<std::size_t>(layer)];
    for (const scene::Instance& instance : instances) {
        if (instance.model >= level.models.size() || instance.stand_in || instance.lighting_only) {
            continue;
        }
        const scene::Model& model = level.models[instance.model];
        for (const scene::Primitive& primitive : model.primitives) {
            if (!opaque_caster(level, primitive) || primitive.index_count < 3
                || primitive.index_count % 3 != 0) {
                continue;
            }
            const std::size_t end =
                static_cast<std::size_t>(primitive.first_index) + primitive.index_count;
            if (end > level.indices.size()) {
                continue;
            }
            for (std::size_t at = primitive.first_index; at < end; at += 3) {
                renderer::Vec3 world[3];
                bool valid = true;
                for (unsigned corner = 0; corner < 3; ++corner) {
                    const std::uint32_t index = level.indices[at + corner];
                    if (index >= level.vertices.size()) {
                        valid = false;
                        break;
                    }
                    world[corner] =
                        renderer::transform_point(instance.matrix, point(level.vertices[index]));
                    valid &= finite(world[corner]);
                }
                if (!valid) {
                    continue;
                }
                const renderer::Vec3 a{
                    world[1][0] - world[0][0], world[1][1] - world[0][1], world[1][2] - world[0][2]
                };
                const renderer::Vec3 b{
                    world[2][0] - world[0][0], world[2][1] - world[0][1], world[2][2] - world[0][2]
                };
                const renderer::Vec3 area = renderer::cross(a, b);
                if (renderer::dot(area, area) <= 1e-10f) {
                    continue;
                }
                for (const auto& p : world) {
                    triangles.push_back({p[0], p[1], p[2]});
                }
            }
        }
    }
}

void append_terrain_receivers(
    const scene::LevelData& level,
    std::vector<RayReceiver>& receivers,
    std::vector<std::uint32_t>& receiver_vertices
) {
    std::vector<bool> seen(level.vertices.size(), false);
    const auto& instances = level.instances[static_cast<std::size_t>(scene::Layer::Terrain)];
    for (const scene::Instance& instance : instances) {
        if (instance.model >= level.models.size() || instance.stand_in || instance.lighting_only) {
            continue;
        }
        const scene::Model& model = level.models[instance.model];
        for (const scene::Primitive& primitive : model.primitives) {
            if (!opaque_caster(level, primitive)) {
                continue;
            }
            const std::size_t end =
                static_cast<std::size_t>(primitive.first_index) + primitive.index_count;
            if (end > level.indices.size()) {
                continue;
            }
            for (std::size_t at = primitive.first_index; at < end; ++at) {
                const std::uint32_t index = level.indices[at];
                if (index >= level.vertices.size() || seen[index]) {
                    continue;
                }
                const scene::Vertex& vertex = level.vertices[index];
                const renderer::Vec3 world =
                    renderer::transform_point(instance.matrix, point(vertex));
                renderer::Vec3 n = renderer::normalize(
                    renderer::transform_vector(instance.matrix, normal(vertex))
                );
                if (!finite(world) || !finite(n) || renderer::dot(n, n) < 0.25f) {
                    continue;
                }
                seen[index] = true;
                RayReceiver receiver{};
                receiver.position[0] = world[0];
                receiver.position[1] = world[1];
                receiver.position[2] = world[2];
                receiver.position[3] = 1.0f;
                receiver.normal[0] = n[0];
                receiver.normal[1] = n[1];
                receiver.normal[2] = n[2];
                receiver.normal[3] = 0.0f;
                receivers.push_back(receiver);
                receiver_vertices.push_back(index);
            }
        }
    }
}

}  // namespace

RayTracedSunStats ray_trace_static_sun_metal(
    scene::LevelData& level, renderer::Vec3 sunlight_travel, std::string& error
) {
    RayTracedSunStats stats;
    const platform::RayTracingSupport support = platform::ray_tracing_support();
    stats.backend = platform::ray_tracing_backend_name(support.backend);
    stats.hardware = support.hardware_accelerated;
    if (!support.supported) {
        return stats;
    }

    sunlight_travel = renderer::normalize(sunlight_travel);
    if (!finite(sunlight_travel) || renderer::dot(sunlight_travel, sunlight_travel) < 0.5f) {
        error = "ray-traced sunlight direction is invalid";
        return stats;
    }

    std::vector<Packed3> triangles;
    append_static_layer(level, scene::Layer::Terrain, triangles);
    append_static_layer(level, scene::Layer::Ties, triangles);
    append_static_layer(level, scene::Layer::Shrubs, triangles);

    std::vector<RayReceiver> receivers;
    std::vector<std::uint32_t> receiver_vertices;
    append_terrain_receivers(level, receivers, receiver_vertices);
    stats.triangles = triangles.size() / 3;
    stats.receivers = receivers.size();
    if (triangles.empty() || receivers.empty()) {
        error = "ray-traced sunlight has no eligible static geometry";
        return stats;
    }

    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        id<MTLCommandQueue> queue = [device newCommandQueue];
        id<MTLBuffer> vertices = [device newBufferWithBytes:triangles.data()
                                                     length:triangles.size() * sizeof(Packed3)
                                                    options:MTLResourceStorageModeShared];
        if (device == nil || queue == nil || vertices == nil) {
            error = "could not allocate Metal ray-tracing scene resources";
            return stats;
        }

        MTLAccelerationStructureTriangleGeometryDescriptor* geometry =
            [MTLAccelerationStructureTriangleGeometryDescriptor descriptor];
        geometry.vertexBuffer = vertices;
        geometry.vertexStride = sizeof(Packed3);
        geometry.triangleCount = stats.triangles;
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
        if (structure == nil || scratch == nil) {
            error = "could not allocate Metal sunlight acceleration structure";
            return stats;
        }

        id<MTLCommandBuffer> build = [queue commandBuffer];
        id<MTLAccelerationStructureCommandEncoder> encoder =
            [build accelerationStructureCommandEncoder];
        [encoder buildAccelerationStructure:structure
                                 descriptor:descriptor
                              scratchBuffer:scratch
                        scratchBufferOffset:0];
        [encoder endEncoding];
        [build commit];
        [build waitUntilCompleted];
        if (build.status != MTLCommandBufferStatusCompleted) {
            error = build.error.localizedDescription.UTF8String ?: "Metal BLAS build failed";
            return stats;
        }

        static const char* const kSource = R"metal(
#include <metal_stdlib>
#include <metal_raytracing>
using namespace metal;
using namespace raytracing;

struct Receiver {
    float4 position;
    float4 normal;
};

kernel void openrac_static_sun(
    acceleration_structure<> scene [[buffer(0)]],
    device const Receiver* receivers [[buffer(1)]],
    device float* visibility [[buffer(2)]],
    constant float4& travel [[buffer(3)]],
    uint id [[thread_position_in_grid]])
{
    Receiver r = receivers[id];
    float3 toward_sun = normalize(-travel.xyz);
    float3 n = normalize(r.normal.xyz);
    float3 origin = r.position.xyz + n * 0.03f + toward_sun * 0.01f;
    intersector<> trace;
    ray shadow(origin, toward_sun, 0.001f, 100000.0f);
    const auto hit = trace.intersect(shadow, scene);
    visibility[id] = hit.type == intersection_type::triangle ? 0.0f : 1.0f;
}
)metal";
        NSString* source = [NSString stringWithUTF8String:kSource];
        NSError* library_error = nil;
        id<MTLLibrary> library = [device newLibraryWithSource:source
                                                      options:nil
                                                        error:&library_error];
        id<MTLFunction> function = [library newFunctionWithName:@"openrac_static_sun"];
        NSError* pipeline_error = nil;
        id<MTLComputePipelineState> pipeline =
            function != nil
                ? [device newComputePipelineStateWithFunction:function error:&pipeline_error]
                : nil;
        id<MTLBuffer> receiver_buffer =
            [device newBufferWithBytes:receivers.data()
                                length:receivers.size() * sizeof(RayReceiver)
                               options:MTLResourceStorageModeShared];
        id<MTLBuffer> visibility = [device newBufferWithLength:receivers.size() * sizeof(float)
                                                       options:MTLResourceStorageModeShared];
        if (pipeline == nil || receiver_buffer == nil || visibility == nil) {
            NSString* message = pipeline_error != nil ? pipeline_error.localizedDescription
                                                      : library_error.localizedDescription;
            error = message.UTF8String ?: "could not create Metal sunlight ray-query pipeline";
            return stats;
        }

        const float travel[4] = {sunlight_travel[0], sunlight_travel[1], sunlight_travel[2], 0.0f};
        id<MTLBuffer> travel_buffer = [device newBufferWithBytes:travel
                                                          length:sizeof(travel)
                                                         options:MTLResourceStorageModeShared];
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> compute = [command computeCommandEncoder];
        [compute setComputePipelineState:pipeline];
        [compute setAccelerationStructure:structure atBufferIndex:0];
        [compute setBuffer:receiver_buffer offset:0 atIndex:1];
        [compute setBuffer:visibility offset:0 atIndex:2];
        [compute setBuffer:travel_buffer offset:0 atIndex:3];
        const NSUInteger width = std::min<NSUInteger>(pipeline.maxTotalThreadsPerThreadgroup, 256);
        [compute dispatchThreads:MTLSizeMake(receivers.size(), 1, 1)
            threadsPerThreadgroup:MTLSizeMake(width, 1, 1)];
        [compute endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status != MTLCommandBufferStatusCompleted) {
            error =
                command.error.localizedDescription.UTF8String ?: "Metal sunlight ray query failed";
            return stats;
        }

        level.rt_sun_visibility.assign(level.vertices.size(), 1.0f);
        const float* values = static_cast<const float*>(visibility.contents);
        for (std::size_t i = 0; i < receiver_vertices.size(); ++i) {
            level.rt_sun_visibility[receiver_vertices[i]] = std::clamp(values[i], 0.0f, 1.0f);
        }
        stats.active = true;
        return stats;
    }
}

}  // namespace openrac::enhanced
