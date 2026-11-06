// SPDX-License-Identifier: GPL-3.0-or-later
#include <chrono>
#include <fstream>
#include <limits>

#include "enhanced/lighting/cloud_field.h"
#include "tests/check.h"
using namespace openrac::lighting;

int main() {
    const auto path =
        std::filesystem::temp_directory_path()
        / ("openrac-cloud-test-"
           + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
    CloudParameters p;
    p.resolution = 32;
    const auto field = CloudField::generate(p);
    CHECK(field.cells == CloudField::generate(p).cells);
    CHECK(field.cells.size() == 32 * 32 * 16);
    CHECK(field.sample({0, 0, p.base - 1}) == 0);
    CHECK(field.sample({0, 0, p.base + p.thickness + 1}) == 0);

    for (int i = 0; i < 100; ++i) {
        std::array<float, 3> point{float(i), float(-i), p.base + p.thickness * .5f};
        auto repeat = point;
        repeat[0] += CloudField::tau / p.scale;
        CHECK(std::abs(field.sample(point) - field.sample(repeat)) < .00001f);
    }

    const auto atlas = field.atlas_rgba();
    const auto size = field.atlas_size();
    const unsigned n = p.resolution, columns = field.atlas_columns();

    for (unsigned z = 0; z < n / 2; ++z) {
        for (unsigned y = 0; y < n; ++y) {
            for (unsigned x = 0; x < n; ++x) {
                const auto i = (((z / columns) * n + y) * size[0] + (z % columns) * n + x) * 4;
                CHECK(atlas[i] == field.cells[(z * n + y) * n + x]);
            }
        }
    }

    auto maximum = field;
    maximum.parameters.resolution = 128;
    CHECK(maximum.atlas_size() == (std::array<unsigned, 2>{1024, 1024}));
    auto thin = field;
    thin.parameters.base = 10;
    thin.parameters.thickness = .1f;
    thin.parameters.density = .2f;
    std::fill(thin.cells.begin(), thin.cells.end(), std::uint8_t(255));
    CHECK(std::abs(thin.optical_depth({0, 0, 0}, {0, 0, 1}, 1000) - .02f) < .00001f);
    CHECK(std::abs(thin.optical_depth({0, 0, 100}, {0, 0, -1}, 1000) - .02f) < .00001f);
    CHECK(thin.optical_depth({0, 0, 0}, {1, 0, 0}, 1000) == 0);
    field.save(path);
    CHECK(CloudField::load(path, p).cells == field.cells);
    auto rejected = [&](const CloudParameters& expected) {
        try {
            (void)CloudField::load(path, expected);
            return false;
        } catch (const std::runtime_error&) {
            return true;
        }
    };
    ++p.seed;
    CHECK(rejected(p));
    --p.seed;
    {
        std::fstream corrupt(path, std::ios::binary | std::ios::in | std::ios::out);
        corrupt.seekp(80);
        corrupt.put(char(127));
    }
    CHECK(rejected(p));
    field.save(path);
    std::filesystem::resize_file(path, 10);
    CHECK(rejected(p));
    p.enabled = false;
    const auto off = CloudField::generate(p);
    CHECK(std::all_of(off.cells.begin(), off.cells.end(), [](auto c) { return c == 0; }));
    p.coverage = std::numeric_limits<float>::quiet_NaN();
    bool invalid = false;

    try {
        (void)CloudField::generate(p);
    } catch (const std::runtime_error&) {
        invalid = true;
    }

    CHECK(invalid);
    std::filesystem::remove(path);
}
