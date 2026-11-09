// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 the OpenRAC contributors

#include "enhanced/tools/baker/output/quality_heatmap.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <format>
#include <vector>

namespace openrac::lighting {
namespace {
struct RankedGrid {
    const ProbeQualityGrid* source = nullptr;
    std::size_t invalid = 0;
    std::size_t relocated = 0;
    std::size_t total = 0;
};

RankedGrid rank(const ProbeQualityGrid& source) {
    RankedGrid result{&source};
    if (!source.volume) {
        return result;
    }
    result.total = source.volume->probes.size();
    for (const auto& probe : source.volume->probes) {
        result.invalid += probe.valid ? 0 : 1;
        result.relocated += probe.relocated ? 1 : 0;
    }
    return result;
}

double invalid_ratio(const RankedGrid& grid) {
    return grid.total ? double(grid.invalid) / double(grid.total) : 0.0;
}

std::string xml_text(std::string_view text) {
    std::string escaped;
    escaped.reserve(text.size());
    for (const char ch : text) {
        switch (ch) {
            case '&':
                escaped += "&amp;";
                break;
            case '<':
                escaped += "&lt;";
                break;
            case '>':
                escaped += "&gt;";
                break;
            case '"':
                escaped += "&quot;";
                break;
            default:
                escaped += ch;
                break;
        }
    }
    return escaped;
}

std::array<int, 3> cell_colour(double invalid, double relocated) {
    invalid = std::clamp(invalid, 0.0, 1.0);
    relocated = std::clamp(relocated, 0.0, 1.0);
    if (invalid > 0.0) {
        return {
            int(std::lround(55.0 + 200.0 * invalid)),
            int(std::lround(205.0 * (1.0 - invalid))),
            52
        };
    }
    if (relocated > 0.0) {
        return {
            int(std::lround(75.0 + 170.0 * relocated)),
            int(std::lround(185.0 + 45.0 * (1.0 - relocated))),
            45
        };
    }
    return {45, 180, 85};
}
}  // namespace

std::string probe_quality_svg(std::span<const ProbeQualityGrid> grids) {
    std::vector<RankedGrid> ranked;
    ranked.reserve(grids.size());
    for (const auto& grid : grids) {
        if (grid.volume && !grid.volume->probes.empty()) {
            ranked.push_back(rank(grid));
        }
    }
    std::stable_sort(ranked.begin(), ranked.end(), [](const auto& a, const auto& b) {
        if (invalid_ratio(a) != invalid_ratio(b)) {
            return invalid_ratio(a) > invalid_ratio(b);
        }
        return a.source->name < b.source->name;
    });
    if (ranked.size() > 32) {
        ranked.resize(32);
    }

    constexpr int panel = 248;
    constexpr int plot = 216;
    constexpr int margin = 16;
    constexpr int title_height = 34;
    constexpr int columns = 4;
    const int rows = std::max(1, int((ranked.size() + columns - 1) / columns));
    const int width = columns * panel;
    const int height = rows * (panel + title_height) + 44;

    std::string svg = std::format(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" "
        "viewBox=\"0 0 {} {}\">\n"
        "<rect width=\"100%\" height=\"100%\" fill=\"#16181d\"/>\n"
        "<style>text{{font:12px sans-serif;fill:#e9edf3}} .small{{font-size:10px;fill:#b9c0cc}}</style>\n"
        "<text x=\"16\" y=\"24\">OpenRAC verified probe quality · worst grids first</text>\n",
        width,
        height,
        width,
        height
    );

    for (std::size_t index = 0; index < ranked.size(); ++index) {
        const auto& item = ranked[index];
        const auto& volume = *item.source->volume;
        const auto nx = std::max<std::uint32_t>(1, volume.grid.dimensions[0]);
        const auto ny = std::max<std::uint32_t>(1, volume.grid.dimensions[1]);
        const auto nz = std::max<std::uint32_t>(1, volume.grid.dimensions[2]);
        const unsigned cells_x = std::min<unsigned>(64, nx);
        const unsigned cells_y = std::min<unsigned>(64, ny);
        const int column = int(index % columns), row = int(index / columns);
        const double origin_x = column * panel + margin;
        const double origin_y = 44 + row * (panel + title_height) + title_height;
        const double cell_w = double(plot) / cells_x;
        const double cell_h = double(plot) / cells_y;

        svg += std::format(
            "<text x=\"{:.2f}\" y=\"{:.2f}\">{}</text>\n"
            "<text class=\"small\" x=\"{:.2f}\" y=\"{:.2f}\">invalid {}/{} ({:.1f}%), relocated {}</text>\n",
            origin_x,
            origin_y - 20,
            xml_text(item.source->name),
            origin_x,
            origin_y - 6,
            item.invalid,
            item.total,
            invalid_ratio(item) * 100.0,
            item.relocated
        );

        for (unsigned cy = 0; cy < cells_y; ++cy) {
            const unsigned y0 = unsigned((std::uint64_t(cy) * ny) / cells_y);
            const unsigned y1 = unsigned((std::uint64_t(cy + 1) * ny + cells_y - 1) / cells_y);
            for (unsigned cx = 0; cx < cells_x; ++cx) {
                const unsigned x0 = unsigned((std::uint64_t(cx) * nx) / cells_x);
                const unsigned x1 =
                    unsigned((std::uint64_t(cx + 1) * nx + cells_x - 1) / cells_x);
                std::size_t total = 0, invalid = 0, relocated = 0;
                for (unsigned z = 0; z < nz; ++z) {
                    for (unsigned y = y0; y < std::min<unsigned>(y1, ny); ++y) {
                        for (unsigned x = x0; x < std::min<unsigned>(x1, nx); ++x) {
                            const std::size_t at = x + std::size_t(nx) * (y + std::size_t(ny) * z);
                            if (at >= volume.probes.size()) {
                                continue;
                            }
                            const auto& probe = volume.probes[at];
                            ++total;
                            invalid += probe.valid ? 0 : 1;
                            relocated += probe.relocated ? 1 : 0;
                        }
                    }
                }
                const double bad = total ? double(invalid) / double(total) : 0.0;
                const double moved = total ? double(relocated) / double(total) : 0.0;
                const auto colour = cell_colour(bad, moved);
                svg += std::format(
                    "<rect x=\"{:.2f}\" y=\"{:.2f}\" width=\"{:.2f}\" height=\"{:.2f}\" "
                    "fill=\"rgb({},{},{})\" stroke=\"#111318\" stroke-width=\"0.25\">"
                    "<title>x {}..{}, y {}..{} · invalid {}/{} · relocated {}</title></rect>\n",
                    origin_x + cx * cell_w,
                    origin_y + (cells_y - 1 - cy) * cell_h,
                    cell_w,
                    cell_h,
                    colour[0],
                    colour[1],
                    colour[2],
                    x0,
                    std::min<unsigned>(x1, nx) - 1,
                    y0,
                    std::min<unsigned>(y1, ny) - 1,
                    invalid,
                    total,
                    relocated
                );
            }
        }
    }

    if (ranked.empty()) {
        svg += "<text x=\"16\" y=\"56\">No verified probe grids.</text>\n";
    }
    svg += "</svg>\n";
    return svg;
}

}  // namespace openrac::lighting
