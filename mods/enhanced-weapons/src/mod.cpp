#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Moby_GetHitMessage g_get_hit = nullptr;

bool g_hit_marker = true;
bool g_word_marker = false;
bool g_projectile_haze = true;
int g_marker_duration = 10;
int g_marker_left = 0;
float g_marker_scale = 0.85f;
float g_haze_strength = 0.08f;
openrac_gaddr g_last_hit = 0;
std::uint32_t g_last_hit_tick = 0;

constexpr std::array<int, 5> kPlayerProjectileClasses = {74, 153, 172, 230, 457};
std::array<openrac_moby, 512> g_mobys{};

bool is_player_projectile(int oclass) {
    return std::find(kPlayerProjectileClasses.begin(), kPlayerProjectileClasses.end(), oclass)
           != kPlayerProjectileClasses.end();
}

openrac_gaddr get_hit(openrac_gaddr moby, int32_t mask, int32_t keep) {
    const openrac_gaddr hit = g_get_hit(moby, mask, keep);
    if (!hit || !g_hit_marker || !g_api || !g_api->memory) return hit;

    const openrac_input* input = g_api->input ? g_api->input() : nullptr;
    const std::uint32_t tick = input ? input->tick : 0;
    if (hit == g_last_hit && tick == g_last_hit_tick) return hit;
    g_last_hit = hit;
    g_last_hit_tick = tick;

    openrac_gaddr attacker = 0;
    float damage = 0.0f;
    std::memcpy(&attacker, g_api->memory + hit + 0x20, sizeof attacker);
    std::memcpy(&damage, g_api->memory + hit + 0x2c, sizeof damage);
    if (!(damage > 0.0f) || !std::isfinite(damage)) return hit;

    const int hero_slot = g_api->figure_slot ? g_api->figure_slot(g_mod, -1) : -1;
    const int attacker_slot = g_api->moby_slot_of ? g_api->moby_slot_of(attacker) : -1;
    if (hero_slot >= 0 && attacker_slot == hero_slot) g_marker_left = g_marker_duration;
    return hit;
}

void frame(void*) {
    if (!g_api || !g_mod) return;

    if (g_marker_left > 0 && g_hit_marker && g_api->hud_text_set) {
        openrac_hud_text line{};
        std::strncpy(line.text, g_word_marker ? "HIT" : "+", sizeof(line.text) - 1);
        line.x = 0.5f;
        line.y = 0.5f;
        line.scale = g_marker_scale;
        const float fade = static_cast<float>(g_marker_left) / static_cast<float>(g_marker_duration);
        const auto alpha = static_cast<std::uint8_t>(std::clamp(255.0f * fade, 0.0f, 255.0f));
        line.rgba[0] = 255;
        line.rgba[1] = 255;
        line.rgba[2] = 255;
        line.rgba[3] = alpha;
        line.flags = OPENRAC_HUD_SHADOW;
        g_api->hud_text_set(g_mod, &line, 1);
        --g_marker_left;
    } else if (g_api->hud_text_set) {
        g_api->hud_text_set(g_mod, nullptr, 0);
    }

    if (!g_api->haze_set) return;
    if (!g_projectile_haze || g_haze_strength <= 0.0f || !g_api->moby_list) {
        g_api->haze_set(g_mod, nullptr, 0);
        return;
    }

    const int found = g_api->moby_list(g_mobys.data(), static_cast<int>(g_mobys.size()));
    std::array<openrac_haze, 16> haze{};
    int count = 0;
    for (int i = 0; i < std::min(found, static_cast<int>(g_mobys.size())) && count < static_cast<int>(haze.size()); ++i) {
        const openrac_moby& m = g_mobys[static_cast<std::size_t>(i)];
        if ((m.flags & (OPENRAC_MOBY_DEAD | OPENRAC_MOBY_HIDDEN)) != 0 || !is_player_projectile(m.oclass)) continue;
        haze[static_cast<std::size_t>(count)].position[0] = m.position[0];
        haze[static_cast<std::size_t>(count)].position[1] = m.position[1];
        haze[static_cast<std::size_t>(count)].position[2] = m.position[2];
        haze[static_cast<std::size_t>(count)].radius = (m.oclass == 172 ? 2.2f : 1.4f);
        haze[static_cast<std::size_t>(count)].strength = g_haze_strength;
        ++count;
    }
    g_api->haze_set(g_mod, count ? haze.data() : nullptr, count);
}
}  // namespace

OPENRAC_MOD_DEFINE("enhanced-weapons", "0.1.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Enhanced Weapons supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_hit_marker = api->setting_bool(mod, "hit_marker") != 0;
    g_projectile_haze = api->setting_bool(mod, "projectile_haze") != 0;
    g_marker_duration = std::clamp<int>(static_cast<int>(api->setting_int(mod, "marker_duration")), 4, 30);
    g_marker_scale = std::clamp<float>(static_cast<float>(api->setting_float(mod, "marker_scale")), 0.5f, 1.5f);
    g_haze_strength = std::clamp<float>(static_cast<float>(api->setting_float(mod, "haze_strength")), 0.0f, 0.2f);
    g_word_marker = std::string(api->setting_string(mod, "marker_style")) == "word";

    if (OPENRAC_RAC1_HOOK(api, mod, Moby_GetHitMessage, get_hit, &g_get_hit) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Moby_GetHitMessage hook is unavailable.");
        return 1;
    }
    if (api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register the effects callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, "loaded");
    return 0;
}
