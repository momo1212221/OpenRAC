// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors
#pragma once
#include <algorithm>
#include <array>
#include <bit>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <vector>

#include "enhanced/lighting/cloud_settings.h"

namespace openrac::lighting {

struct CloudField {
    CloudParameters parameters;
    std::vector<std::uint8_t> cells;
    static constexpr std::uint32_t version = 1;
    static constexpr float tau = 6.28318530718f;

    static void validate(const CloudParameters& p) {
        if (p.resolution < 32 || p.resolution > 128 || p.resolution % 16 != 0
            || !std::isfinite(p.coverage) || p.coverage < 0 || p.coverage > .99f
            || !std::isfinite(p.density) || p.density < 0 || p.density > 1 || !std::isfinite(p.base)
            || std::abs(p.base) > 100000 || !std::isfinite(p.thickness) || p.thickness < .01f
            || p.thickness > 2000 || !std::isfinite(p.scale) || p.scale < .00001f || p.scale > 10
            || p.seed > 65535) {
            throw std::runtime_error("invalid cloud preparation settings");
        }

        for (float c : p.colour) {
            if (!std::isfinite(c) || c < 0 || c > 4) {
                throw std::runtime_error("invalid cloud colour");
            }
        }
    }

    static float smooth(float a, float b, float x) {
        const float t = std::clamp((x - a) / (b - a), 0.f, 1.f);
        return t * t * (3 - 2 * t);
    }

    static CloudField generate(const CloudParameters& p) {
        validate(p);
        CloudField field{p, {}};
        const unsigned n = p.resolution, depth = n / 2;
        field.cells.resize(n * n * depth);

        if (!p.enabled || p.density == 0) {
            return field;
        }

        for (unsigned z = 0; z < depth; ++z) {
            const float h = (float(z) + .5f) / float(depth);
            const float vertical = smooth(0, .16f, h) * (1 - smooth(.72f, 1, h));

            for (unsigned y = 0; y < n; ++y) {
                for (unsigned x = 0; x < n; ++x) {
                    const float u = (float(x) + .5f) * tau / float(n);
                    const float v = (float(y) + .5f) * tau / float(n);
                    const float phase = float(p.seed) * .61803398875f;
                    const float a = .5f + .5f * std::sin(u + phase + std::sin(v - phase));
                    const float b = .5f + .5f * std::sin(2 * v + phase + std::sin(u + h * tau));
                    const float shape =
                        smooth(p.coverage, std::min(1.f, p.coverage + .22f), .65f * a + .35f * b);
                    field.cells[(z * n + y) * n + x] =
                        p.enabled ? std::uint8_t(std::lround(255 * vertical * shape)) : 0;
                }
            }
        }

        return field;
    }

    // Exactly the atlas's eight-texel interpolation, including periodic XY and clamped height.
    float sample(const std::array<float, 3>& world) const {
        const auto& p = parameters;
        const int n = int(p.resolution), depth = n / 2;
        const float height = (world[2] - p.base) / p.thickness;

        if (!p.enabled || cells.empty() || height <= 0 || height >= 1) {
            return 0;
        }

        std::array<float, 3> q{world[0] * p.scale / tau, world[1] * p.scale / tau, height};

        for (int axis = 0; axis < 2; ++axis) {
            q[axis] -= std::floor(q[axis]);
        }

        for (int axis = 0; axis < 3; ++axis) {
            q[axis] = q[axis] * float(axis == 2 ? depth : n) - .5f;
        }

        std::array<int, 3> cell{};

        for (int axis = 0; axis < 3; ++axis) {
            cell[axis] = int(std::floor(q[axis]));
        }

        float result = 0;

        for (int z = 0; z < 2; ++z) {
            for (int y = 0; y < 2; ++y) {
                for (int x = 0; x < 2; ++x) {
                    const int ix = (cell[0] + x + n) % n, iy = (cell[1] + y + n) % n;
                    const int iz = std::clamp(cell[2] + z, 0, depth - 1);
                    float weight = 1;
                    const std::array<int, 3> offsets{x, y, z};

                    for (int axis = 0; axis < 3; ++axis) {
                        const float f = q[axis] - float(cell[axis]);
                        weight *= offsets[axis] ? f : 1 - f;
                    }

                    result += weight * float(cells[std::size_t((iz * n + iy) * n + ix)]) / 255.f;
                }
            }
        }

        return result;
    }

    // Integrate only the ray's intersection with the layer, so thin clouds cannot fall
    // between the coarse fog quadrature samples during sunlight/GI preparation.
    float optical_depth(
        const std::array<float, 3>& origin, const std::array<float, 3>& direction, float length
    ) const {
        const auto& p = parameters;

        if (!p.enabled || p.density <= 0 || !std::isfinite(length) || length <= 0) {
            return 0;
        }

        float lo = 0, hi = length;

        if (std::abs(direction[2]) < 1e-8f) {
            if (origin[2] <= p.base || origin[2] >= p.base + p.thickness) {
                return 0;
            }
        } else {
            const float a = (p.base - origin[2]) / direction[2];
            const float b = (p.base + p.thickness - origin[2]) / direction[2];
            lo = std::max(lo, std::min(a, b));
            hi = std::min(hi, std::max(a, b));
        }

        if (hi <= lo) {
            return 0;
        }

        constexpr unsigned steps = 16;
        const float ds = (hi - lo) / float(steps);
        float result = 0;

        for (unsigned i = 0; i < steps; ++i) {
            const float distance = lo + (float(i) + .5f) * ds;
            std::array<float, 3> point{};

            for (unsigned axis = 0; axis < 3; ++axis) {
                point[axis] = origin[axis] + direction[axis] * distance;
            }

            result += sample(point) * ds;
        }

        return result * p.density;
    }

    unsigned atlas_columns() const {
        return unsigned(std::ceil(std::sqrt(float(parameters.resolution / 2))));
    }

    std::array<unsigned, 2> atlas_size() const {
        const unsigned n = parameters.resolution, depth = n / 2, columns = atlas_columns();
        return {n * columns, n * ((depth + columns - 1) / columns)};
    }

    std::vector<std::uint8_t> atlas_rgba() const {
        const unsigned n = parameters.resolution, depth = n / 2, columns = atlas_columns();

        if (cells.size() != n * n * depth) {
            throw std::runtime_error("invalid prepared cloud cell count");
        }

        const auto size = atlas_size();
        std::vector<std::uint8_t> atlas(size[0] * size[1] * 4, 0);

        for (unsigned z = 0; z < depth; ++z) {
            for (unsigned y = 0; y < n; ++y) {
                for (unsigned x = 0; x < n; ++x) {
                    const unsigned px = (z % columns) * n + x, py = (z / columns) * n + y;
                    atlas[(py * size[0] + px) * 4] = cells[(z * n + y) * n + x];
                }
            }
        }

        return atlas;
    }

    std::uint32_t checksum() const {
        std::uint32_t hash = 2166136261u;

        for (auto c : cells) {
            hash = (hash ^ c) * 16777619u;
        }

        return hash;
    }

    // Explicit little-endian words; never serialize compiler padding or native endianness.
    std::vector<std::uint32_t> header() const {
        const auto& p = parameters;
        std::vector<std::uint32_t> words{
            0x4350524fu, version, p.enabled ? 1u : 0u, p.resolution, p.seed
        };

        for (float f :
             {p.coverage,
              p.density,
              p.base,
              p.thickness,
              p.scale,
              p.colour[0],
              p.colour[1],
              p.colour[2]}) {
            words.push_back(std::bit_cast<std::uint32_t>(f));
        }

        return words;
    }

    void save(const std::filesystem::path& path) const {
        std::ofstream out(path, std::ios::binary | std::ios::trunc);
        auto words = header();
        words.push_back(checksum());

        for (auto word : words) {
            for (unsigned shift = 0; shift < 32; shift += 8) {
                out.put(char((word >> shift) & 255));
            }
        }

        out.write(reinterpret_cast<const char*>(cells.data()), std::streamsize(cells.size()));

        out.flush();

        if (!out) {
            throw std::runtime_error("could not save prepared clouds");
        }
    }

    static CloudField load(const std::filesystem::path& path, const CloudParameters& expected) {
        validate(expected);
        CloudField field{expected, {}};
        std::ifstream in(path, std::ios::binary);
        auto word = [&]() {
            std::uint32_t value = 0;

            for (unsigned shift = 0; shift < 32; shift += 8) {
                const int byte = in.get();

                if (byte == EOF) {
                    throw std::runtime_error(
                        "prepared clouds are missing or truncated; prepare lighting again"
                    );
                }

                value |= std::uint32_t(byte) << shift;
            }

            return value;
        };

        for (auto expected_word : field.header()) {
            if (word() != expected_word) {
                throw std::runtime_error("cloud settings changed; prepare lighting again");
            }
        }

        const auto checksum = word();
        field.cells.resize(expected.resolution * expected.resolution * (expected.resolution / 2));
        in.read(reinterpret_cast<char*>(field.cells.data()), std::streamsize(field.cells.size()));

        if (!in || field.checksum() != checksum || in.peek() != EOF) {
            throw std::runtime_error("prepared cloud cache is damaged; prepare lighting again");
        }

        return field;
    }
};
}  // namespace openrac::lighting
