#include <stdexcept>

#include "enhanced/tools/baker/sources/activation.h"
#include "test.h"

using namespace openrac::lighting;

int main() {
    LevelBakeProfile standard;
    const auto resolved_standard = resolve_gi_bake_quality(standard);
    CHECK(resolved_standard.activation.bake.rays_per_probe == standard.activation.bake.rays_per_probe);
    CHECK(resolved_standard.activation.bake.bounces == standard.activation.bake.bounces);
    CHECK(resolved_standard.map_resolution == standard.map_resolution);

    LevelBakeProfile ultra;
    ultra.gi_quality = GiBakeQuality::Ultra;
    ultra.activation.bake.rays_per_probe = 128;
    ultra.activation.bake.bounces = 2;
    ultra.map_resolution = 6;
    const auto resolved_ultra = resolve_gi_bake_quality(ultra);
    CHECK(resolved_ultra.activation.bake.rays_per_probe == 1024);
    CHECK(resolved_ultra.activation.bake.bounces == 3);
    CHECK(resolved_ultra.map_resolution == 8);

    LevelBakeProfile authored;
    authored.gi_quality = GiBakeQuality::Ultra;
    authored.activation.bake.rays_per_probe = 2048;
    authored.activation.bake.bounces = 5;
    authored.map_resolution = 12;
    const auto resolved_authored = resolve_gi_bake_quality(authored);
    CHECK(resolved_authored.activation.bake.rays_per_probe == 2048);
    CHECK(resolved_authored.activation.bake.bounces == 5);
    CHECK(resolved_authored.map_resolution == 12);
}
