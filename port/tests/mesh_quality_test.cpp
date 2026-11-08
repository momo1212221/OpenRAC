// SPDX-License-Identifier: GPL-3.0-or-later
#include <array>
#include <cmath>
#include <cstdint>
#include <vector>

#include "scene/mesh_quality.h"
#include "tests/check.h"

namespace {
using openrac::scene::Vertex;

Vertex vertex(float x, float y, float z) {
    Vertex v{};
    v.position[0] = x;
    v.position[1] = y;
    v.position[2] = z;
    return v;
}

void repairs_missing_normals() {
    std::vector<Vertex> vertices{
        vertex(0,0,0), vertex(1,0,0), vertex(1,1,0), vertex(0,1,0)
    };
    const std::vector<std::uint32_t> indices{0,1,2, 0,2,3};
    openrac::scene::repair_missing_normals(vertices, indices, 0, 4);
    for (const auto& v : vertices) {
        CHECK(std::fabs(v.normal[0]) < 1e-5f);
        CHECK(std::fabs(v.normal[1]) < 1e-5f);
        CHECK(v.normal[2] > .999f);
    }
}

void preserves_authored_normals() {
    std::vector<Vertex> vertices{
        vertex(0,0,0), vertex(1,0,0), vertex(0,1,0)
    };
    vertices[0].normal[0] = 1;
    const std::vector<std::uint32_t> indices{0,1,2};
    openrac::scene::repair_missing_normals(vertices, indices, 0, 3);
    CHECK(vertices[0].normal[0] == 1);
    CHECK(vertices[0].normal[1] == 0);
    CHECK(vertices[0].normal[2] == 0);
    CHECK(vertices[1].normal[2] > .999f);
    CHECK(vertices[2].normal[2] > .999f);
}

void ignores_degenerate_faces() {
    std::vector<Vertex> vertices{vertex(0,0,0), vertex(0,0,0), vertex(0,0,0)};
    const std::vector<std::uint32_t> indices{0,1,2};
    openrac::scene::repair_missing_normals(vertices, indices, 0, 3);
    for (const auto& v : vertices) {
        CHECK(v.normal[2] == 1);
    }
}
}

int main() {
    repairs_missing_normals();
    preserves_authored_normals();
    ignores_degenerate_faces();
    return openrac::test::result();
}
