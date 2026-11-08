// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include <string>
#include <vector>

#include "enhanced/tools/baker/output/quality_heatmap.h"
#include "tests/check.h"

namespace {
using namespace openrac;

lighting::ProbeVolume volume(unsigned nx, unsigned ny, unsigned nz) {
    lighting::ProbeGrid grid;
    grid.dimensions = {nx, ny, nz};
    auto result = lighting::ProbeVolume::allocate(grid);
    for (auto& probe : result.probes) {
        probe.valid = true;
    }
    return result;
}

void ranks_problem_grids_and_escapes_names() {
    auto good = volume(4, 4, 2);
    auto bad = volume(4, 4, 2);
    bad.probes[0].valid = false;
    bad.probes[1].valid = false;
    bad.probes[2].relocated = true;

    const std::vector<lighting::ProbeQualityGrid> grids{
        {"good.orpg", &good},
        {"bad<&>.orpg", &bad},
    };
    const std::string svg = lighting::probe_quality_svg(grids);
    CHECK(svg.find("<svg") != std::string::npos);
    CHECK(svg.find("bad&lt;&amp;&gt;.orpg") < svg.find("good.orpg"));
    CHECK(svg.find("invalid 2/32") != std::string::npos);
    CHECK(svg.find("relocated 1") != std::string::npos);
    CHECK(svg.find("<title>x ") != std::string::npos);
}

void empty_is_still_a_valid_visual() {
    const std::vector<lighting::ProbeQualityGrid> none;
    const std::string svg = lighting::probe_quality_svg(none);
    CHECK(svg.find("No verified probe grids.") != std::string::npos);
    CHECK(svg.ends_with("</svg>\n"));
}
}  // namespace

int main() {
    ranks_problem_grids_and_escapes_names();
    empty_is_still_a_valid_visual();
    return openrac::test::result();
}
