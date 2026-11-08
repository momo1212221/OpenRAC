#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Hero_TakeDamage g_take_damage = nullptr;

int g_damage_percent = 100;
int g_damage_accumulator = 0;
bool g_lethal_protection = false;
bool g_invulnerable = false;
bool g_large_health = true;
bool g_low_health_warning = true;
float g_health_scale = 0.9f;
int g_protection_cooldown = 0;
int g_protected_notice = 0;
std::uint32_t g_tick = 0;

int hero_health() {
    if (!g_api || !g_api->hero_pose) return -1;
    openrac_pose pose{};
    pose.size = sizeof pose;
    return g_api->hero_pose(&pose) == OPENRAC_HOOK_OK ? pose.health : -1;
}

void take_damage(int32_t amount) {
    if (amount <= 0) {
        g_take_damage(amount);
        return;
    }
    if (g_invulnerable || g_damage_percent <= 0) return;

    const int health = hero_health();
    if (g_lethal_protection && health == 1 && g_protection_cooldown == 0) {
        g_protection_cooldown = 180;
        g_protected_notice = 90;
        return;
    }

    g_damage_accumulator += g_damage_percent;
    if (g_damage_accumulator < 100) return;
    g_damage_accumulator -= 100;
    g_take_damage(amount);
}

void frame(void*) {
    ++g_tick;
    if (g_protection_cooldown > 0) --g_protection_cooldown;
    if (g_protected_notice > 0) --g_protected_notice;
    if (!g_api || !g_api->hud_text_set) return;

    const int health = hero_health();
    if (health < 0 || (!g_large_health && !g_low_health_warning && g_protected_notice == 0)) {
        g_api->hud_text_set(g_mod, nullptr, 0);
        return;
    }

    openrac_hud_text lines[2]{};
    int count = 0;
    if (g_large_health) {
        std::snprintf(lines[count].text, sizeof(lines[count].text), "HP %d", health);
        lines[count].x = 0.11f;
        lines[count].y = 0.09f;
        lines[count].scale = g_health_scale;
        lines[count].rgba[0] = 255;
        lines[count].rgba[1] = 255;
        lines[count].rgba[2] = 255;
        lines[count].rgba[3] = 255;
        lines[count].flags = OPENRAC_HUD_SHADOW;
        ++count;
    }

    if (g_protected_notice > 0 || (g_low_health_warning && health == 1)) {
        std::strncpy(lines[count].text, g_protected_notice > 0 ? "HIT PROTECTED" : "LOW HEALTH",
                     sizeof(lines[count].text) - 1);
        lines[count].x = 0.5f;
        lines[count].y = 0.16f;
        lines[count].scale = 0.9f;
        const bool bright = ((g_tick / 15u) % 2u) != 0;
        lines[count].rgba[0] = 255;
        lines[count].rgba[1] = bright ? 255 : 128;
        lines[count].rgba[2] = bright ? 255 : 128;
        lines[count].rgba[3] = 255;
        lines[count].flags = OPENRAC_HUD_SHADOW;
        ++count;
    }
    g_api->hud_text_set(g_mod, count ? lines : nullptr, count);
}
}  // namespace

OPENRAC_MOD_DEFINE("accessibility-pack", "0.1.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Accessibility Pack supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_damage_percent = std::clamp<int>(static_cast<int>(api->setting_int(mod, "damage_percent")), 0, 100);
    g_lethal_protection = api->setting_bool(mod, "lethal_protection") != 0;
    g_invulnerable = api->setting_bool(mod, "invulnerable") != 0;
    g_large_health = api->setting_bool(mod, "large_health") != 0;
    g_low_health_warning = api->setting_bool(mod, "low_health_warning") != 0;
    g_health_scale = std::clamp<float>(static_cast<float>(api->setting_float(mod, "health_scale")), 0.5f, 2.0f);

    if (OPENRAC_RAC1_HOOK(api, mod, Hero_TakeDamage, take_damage, &g_take_damage) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Hero_TakeDamage hook is unavailable.");
        return 1;
    }
    if (api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register the accessibility HUD callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, "loaded");
    return 0;
}
