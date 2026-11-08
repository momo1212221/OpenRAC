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


std::uint32_t colour(unsigned r, unsigned g, unsigned b, unsigned a) {
    return ((r & 0xffu) << 24u) | ((g & 0xffu) << 16u) | ((b & 0xffu) << 8u) | (a & 0xffu);
}

void rac_text(const openrac_overlay* overlay, float x, float y, float h, std::uint32_t rgba, const char* text) {
    if (!overlay || !overlay->text || !text) return;
    overlay->text(overlay->context, x + 1.2f, y + 1.2f, h, colour(0, 0, 0, 190), text);
    overlay->text(overlay->context, x, y, h, rgba, text);
}

void centred_text(
    const openrac_overlay* overlay,
    float cx,
    float y,
    float h,
    std::uint32_t rgba,
    const char* text
) {
    if (!overlay || !text) return;
    const float width = overlay->text_width ? overlay->text_width(overlay->context, h, text) : 0.0f;
    rac_text(overlay, cx - width * 0.5f, y, h, rgba, text);
}

void draw_health_card(const openrac_overlay* overlay, int health) {
    if (!g_large_health || health < 0 || !overlay || !overlay->rect) return;

    const float scale = std::clamp(g_health_scale, 0.5f, 2.0f);
    const float x = 18.0f;
    const float y = 18.0f;
    const float w = 116.0f * std::max(0.85f, scale);
    const float h = 48.0f * std::max(0.90f, scale);

    overlay->rect(overlay->context, x + 3.0f, y + 3.0f, w, h, colour(0, 0, 0, 135));
    overlay->rect(overlay->context, x, y, w, h, colour(87, 53, 11, 238));
    overlay->rect(overlay->context, x + 2.0f, y + 2.0f, w - 4.0f, h - 4.0f, colour(21, 17, 12, 238));
    overlay->rect(overlay->context, x + 4.0f, y + 4.0f, 4.0f, h - 8.0f, colour(234, 154, 31, 255));

    rac_text(overlay, x + 15.0f, y + 8.0f, 6.0f * scale, colour(255, 221, 145, 255), "HEALTH");

    char value[32];
    std::snprintf(value, sizeof value, "%d HP", health);
    const bool low = health == 1;
    rac_text(
        overlay, x + 15.0f, y + 23.0f, 10.0f * scale,
        low ? colour(255, 116, 66, 255) : colour(255, 244, 210, 255), value
    );

    // A simple pulse strip retains the game's tech-panel language without introducing a foreign meter.
    const float strip_y = y + h - 8.0f;
    const int cells = std::clamp(health, 0, 8);
    for (int i = 0; i < 8; ++i) {
        const float cx = x + 15.0f + static_cast<float>(i) * 10.0f;
        overlay->rect(
            overlay->context, cx, strip_y, 7.0f, 3.0f,
            i < cells
                ? (low ? colour(242, 91, 49, 255) : colour(229, 143, 24, 245))
                : colour(47, 39, 29, 210)
        );
    }
}

void draw_warning_banner(const openrac_overlay* overlay, int health) {
    if (!overlay || !overlay->rect) return;
    const bool protected_hit = g_protected_notice > 0;
    const bool low = g_low_health_warning && health == 1;
    if (!protected_hit && !low) return;

    const bool bright = ((g_tick / 15u) % 2u) != 0;
    const float x = 146.0f;
    const float y = 70.0f;
    const float w = 220.0f;
    const float h = 46.0f;

    overlay->rect(overlay->context, x + 3.0f, y + 3.0f, w, h, colour(0, 0, 0, 130));
    overlay->rect(
        overlay->context, x, y, w, h,
        protected_hit ? colour(122, 76, 10, 242) : colour(118, 43, 18, 242)
    );
    overlay->rect(overlay->context, x + 2.0f, y + 2.0f, w - 4.0f, h - 4.0f, colour(23, 18, 13, 240));

    const std::uint32_t accent = protected_hit
        ? colour(255, 196, 55, 255)
        : (bright ? colour(255, 107, 62, 255) : colour(207, 72, 39, 255));
    overlay->rect(overlay->context, x + 6.0f, y + 6.0f, w - 12.0f, 4.0f, accent);

    centred_text(
        overlay, 256.0f, y + 17.0f, 8.5f, accent,
        protected_hit ? "HIT PROTECTED" : "LOW HEALTH"
    );
    centred_text(
        overlay, 256.0f, y + 32.0f, 5.0f, colour(229, 214, 184, 235),
        protected_hit ? "LETHAL PROTECTION ACTIVE" : "FIND NANOTECH"
    );
}

void overlay_frame(void*, const openrac_overlay* overlay) {
    const int health = hero_health();
    draw_health_card(overlay, health);
    draw_warning_banner(overlay, health);
}

void frame(void*) {
    ++g_tick;
    if (g_protection_cooldown > 0) --g_protection_cooldown;
    if (g_protected_notice > 0) --g_protected_notice;
}

}  // namespace

OPENRAC_MOD_DEFINE("accessibility-pack", "0.2.0")

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
    if (!api->on_overlay || api->on_overlay(mod, overlay_frame, nullptr) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Enhanced RAC Style requires OpenRAC mod overlay API 1.14.");
        return 1;
    }
    if (api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register the accessibility callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, "loaded");
    return 0;
}
