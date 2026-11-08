// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
//
// Safe render-normal repair for imported triangle meshes. Existing finite normals are preserved;
// only missing/invalid normals are reconstructed from the actual indexed surface.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <span>
#include <vector>

#include "renderer/math.h"
#include "scene/level.h"

namespace openrac::scene {

inline bool usable_normal(const Vertex& vertex) {
    const renderer::Vec3 n{vertex.normal[0], vertex.normal[1], vertex.normal[2]};
    return std::isfinite(n[0]) && std::isfinite(n[1]) && std::isfinite(n[2])
           && renderer::dot(n, n) > 1e-10f;
}

inline float corner_angle(const renderer::Vec3& a, const renderer::Vec3& b) {
    const float aa = renderer::dot(a, a);
    const float bb = renderer::dot(b, b);
    if (aa <= 1e-16f || bb <= 1e-16f) {
        return 0.0f;
    }
    const float cosine = renderer::dot(a, b) / std::sqrt(aa * bb);
    return std::acos(std::clamp(cosine, -1.0f, 1.0f));
}

// Reconstructs only unusable normals inside one imported primitive. Face directions are weighted
// by each corner angle, so triangulation density does not pull a smooth vertex toward tiny faces.
// UVs, positions, indices, skin weights, materials and collision data are never changed.
inline void repair_missing_normals(
    std::vector<Vertex>& vertices,
    std::span<const std::uint32_t> indices,
    std::uint32_t base_vertex,
    std::uint32_t vertex_count
) {
    if (vertex_count == 0) {
        return;
    }

    const std::uint64_t end = static_cast<std::uint64_t>(base_vertex) + vertex_count;
    std::vector<renderer::Vec3> accumulated(vertex_count);
    std::vector<bool> missing(vertex_count, false);
    for (std::uint32_t i = 0; i < vertex_count; ++i) {
        if (static_cast<std::uint64_t>(base_vertex) + i >= vertices.size()) {
            break;
        }
        missing[i] = !usable_normal(vertices[base_vertex + i]);
    }

    auto position = [&](std::uint32_t global) {
        const auto& v = vertices[global];
        return renderer::Vec3{v.position[0], v.position[1], v.position[2]};
    };
    auto delta = [](const renderer::Vec3& a, const renderer::Vec3& b) {
        return renderer::Vec3{a[0] - b[0], a[1] - b[1], a[2] - b[2]};
    };

    for (std::size_t at = 0; at + 2 < indices.size(); at += 3) {
        const std::array<std::uint32_t, 3> tri{indices[at], indices[at + 1], indices[at + 2]};
        if (std::any_of(tri.begin(), tri.end(), [&](std::uint32_t i) {
                return i < base_vertex || i >= end || i >= vertices.size();
            })) {
            continue;
        }

        const auto p0 = position(tri[0]);
        const auto p1 = position(tri[1]);
        const auto p2 = position(tri[2]);
        const auto e01 = delta(p1, p0);
        const auto e02 = delta(p2, p0);
        const auto cross = renderer::cross(e01, e02);
        const float length2 = renderer::dot(cross, cross);
        if (!(length2 > 1e-16f) || !std::isfinite(length2)) {
            continue;
        }
        const auto face = renderer::normalize(cross);

        const renderer::Vec3 points[3]{p0, p1, p2};
        for (unsigned corner = 0; corner < 3; ++corner) {
            const auto global = tri[corner];
            const auto local = global - base_vertex;
            if (!missing[local]) {
                continue;
            }
            const auto a = delta(points[(corner + 1) % 3], points[corner]);
            const auto b = delta(points[(corner + 2) % 3], points[corner]);
            const float weight = corner_angle(a, b);
            for (unsigned axis = 0; axis < 3; ++axis) {
                accumulated[local][axis] += face[axis] * weight;
            }
        }
    }

    for (std::uint32_t i = 0; i < vertex_count; ++i) {
        if (!missing[i] || static_cast<std::uint64_t>(base_vertex) + i >= vertices.size()) {
            continue;
        }
        auto normal = renderer::normalize(accumulated[i]);
        if (!std::isfinite(normal[0]) || !std::isfinite(normal[1]) || !std::isfinite(normal[2])
            || renderer::dot(normal, normal) < 0.5f) {
            normal = {0.0f, 0.0f, 1.0f};
        }
        std::copy(normal.begin(), normal.end(), vertices[base_vertex + i].normal);
    }
}

}  // namespace openrac::scene
