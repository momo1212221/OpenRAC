#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <cstdint>\n#include <limits>
#include <cstdio>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Hero_TakeDamage g_take_damage = nullptr;
bool g_invulnerable = false;
bool g_overlay = true;
int g_damage_percent = 100;

void take_damage(int32_t amount) {
    if (g_invulnerable || g_damage_percent <= 0 || amount <= 0) return;
    const int64_t scaled = (static_cast<int64_t>(amount) * g_damage_percent + 99) / 100;
    const int32_t adjusted = static_cast<int32_t>(std::clamp<int64_t>(scaled, 1, std::numeric_limits<int32_t>::max()));
    g_take_damage(adjusted);
}

void frame(void*) {
    if (!g_overlay || !g_api || !g_api->hud_text_set) return;
    openrac_hud_text line{};
    std::snprintf(line.text, sizeof(line.text), "ACCESSIBILITY: DAMAGE %d%%  INVUL %s",
                  g_damage_percent, g_invulnerable ? "ON" : "OFF");
    line.x = 0.5f;
    line.y = 0.06f;
    line.scale = 0.75f;
    line.rgba[0] = 255;
    line.rgba[1] = 255;
    line.rgba[2] = 255;
    line.rgba[3] = 255;
    line.flags = OPENRAC_HUD_SHADOW;
    g_api->hud_text_set(g_mod, &line, 1);
}
}

OPENRAC_MOD_DEFINE("accessibility-pack", "0.1.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Accessibility Pack currently supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_invulnerable = api->setting_bool(mod, "invulnerable") != 0;
    g_overlay = api->setting_bool(mod, "status_overlay") != 0;
    g_damage_percent = static_cast<int>(api->setting_int(mod, "damage_percent"));
    g_damage_percent = std::clamp(g_damage_percent, 0, 100);

    const int rc = OPENRAC_RAC1_HOOK(api, mod, Hero_TakeDamage, take_damage, &g_take_damage);
    if (rc != OPENRAC_HOOK_OK) {
        api->fail(mod, "Hero_TakeDamage hook is not available on this port.");
        return 1;
    }
    if (g_overlay && api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register accessibility HUD callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, "loaded");
    return 0;
}
