#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Moby_GetHitMessage g_get_hit = nullptr;
openrac_rac1_Hero_TakeDamage g_take_damage = nullptr;
openrac_rac1_Weapon_TakeAmmo g_take_ammo = nullptr;

int g_hit_sound = -1;
int g_damage_sound = -1;
int g_dry_sound = -1;
int g_low_sound = -1;
std::uint32_t g_low_voice = 0;
float g_volume = 0.7f;
bool g_hit_confirm = true;
bool g_damage_cue = true;
bool g_dry_fire = true;
bool g_low_health = true;
int g_low_threshold = 2;
int g_dry_cooldown = 0;
bool g_status_panel = false;
openrac_gaddr g_last_hit = 0;
std::uint32_t g_last_hit_tick = 0;

void play_once(int sound, float volume = 1.0f, float pitch = 1.0f) {
    if (sound >= 0 && g_api && g_api->sound_play)
        g_api->sound_play(g_mod, sound, g_volume * volume, pitch, 0);
}

int hero_health() {
    if (!g_api || !g_api->hero_pose) return -1;
    openrac_pose pose{};
    pose.size = sizeof pose;
    return g_api->hero_pose(&pose) == OPENRAC_HOOK_OK ? pose.health : -1;
}

openrac_gaddr get_hit(openrac_gaddr moby, int32_t mask, int32_t keep) {
    const openrac_gaddr hit = g_get_hit(moby, mask, keep);
    if (!hit || !g_hit_confirm || !g_api || !g_api->memory) return hit;

    const openrac_input* input = g_api->input ? g_api->input() : nullptr;
    const std::uint32_t tick = input ? input->tick : 0;
    if (hit == g_last_hit && tick == g_last_hit_tick) return hit;
    g_last_hit = hit;
    g_last_hit_tick = tick;

    openrac_gaddr attacker = 0;
    float damage = 0.0f;
    std::memcpy(&attacker, g_api->memory + hit + 0x20, sizeof attacker);
    std::memcpy(&damage, g_api->memory + hit + 0x2c, sizeof damage);
    const int hero_slot = g_api->figure_slot ? g_api->figure_slot(g_mod, -1) : -1;
    const int attacker_slot = g_api->moby_slot_of ? g_api->moby_slot_of(attacker) : -1;
    if (hero_slot >= 0 && attacker_slot == hero_slot && damage > 0.0f && std::isfinite(damage))
        play_once(g_hit_sound, 0.7f, 1.0f);
    return hit;
}

void take_damage(int32_t amount) {
    const int before = hero_health();
    g_take_damage(amount);
    const int after = hero_health();
    if (g_damage_cue && before >= 0 && after >= 0 && after < before)
        play_once(g_damage_sound, 0.85f, 1.0f);
}

int32_t take_ammo(int32_t item, int32_t amount) {
    const int32_t result = g_take_ammo(item, amount);
    if (!result && amount > 0 && g_dry_fire && g_dry_cooldown == 0) {
        play_once(g_dry_sound, 0.65f, 1.0f);
        g_dry_cooldown = 8;
    }
    return result;
}


std::uint32_t colour(unsigned r, unsigned g, unsigned b, unsigned a) {
    return ((r & 0xffu) << 24u) | ((g & 0xffu) << 16u) | ((b & 0xffu) << 8u) | (a & 0xffu);
}

void rac_text(const openrac_overlay* overlay, float x, float y, float h, std::uint32_t rgba, const char* text) {
    if (!overlay || !overlay->text || !text) return;
    overlay->text(overlay->context, x + 1.0f, y + 1.0f, h, colour(0, 0, 0, 185), text);
    overlay->text(overlay->context, x, y, h, rgba, text);
}

void overlay_frame(void*, const openrac_overlay* overlay) {
    if (!g_status_panel || !overlay || !overlay->rect) return;

    constexpr float x = 350.0f;
    constexpr float y = 18.0f;
    constexpr float w = 144.0f;
    constexpr float h = 42.0f;

    overlay->rect(overlay->context, x + 3.0f, y + 3.0f, w, h, colour(0, 0, 0, 130));
    overlay->rect(overlay->context, x, y, w, h, colour(86, 53, 12, 235));
    overlay->rect(overlay->context, x + 2.0f, y + 2.0f, w - 4.0f, h - 4.0f, colour(21, 18, 13, 235));
    overlay->rect(overlay->context, x + 4.0f, y + 4.0f, 4.0f, h - 8.0f, colour(232, 153, 31, 255));

    rac_text(overlay, x + 14.0f, y + 9.0f, 7.0f, colour(255, 226, 154, 255), "ENHANCED AUDIO");

    char flags[96];
    std::snprintf(
        flags, sizeof flags, "%s  %s  %s",
        g_hit_confirm ? "HIT" : "--",
        g_damage_cue ? "DMG" : "--",
        g_low_health ? "LOW HP" : "--"
    );
    rac_text(overlay, x + 14.0f, y + 24.0f, 5.5f, colour(222, 207, 175, 235), flags);
}

void frame(void*) {
    if (g_dry_cooldown > 0) --g_dry_cooldown;
    if (!g_api || !g_api->sound_play || !g_api->sound_stop) return;

    const int health = hero_health();
    const bool should_loop = g_low_health && g_low_sound >= 0 && health > 0 && health <= g_low_threshold;
    if (should_loop && g_low_voice == 0)
        g_low_voice = g_api->sound_play(g_mod, g_low_sound, g_volume * 0.45f, 1.0f, 1);
    else if (!should_loop && g_low_voice != 0) {
        g_api->sound_stop(g_mod, g_low_voice);
        g_low_voice = 0;
    }
}
}  // namespace

OPENRAC_MOD_DEFINE("enhanced-audio", "0.2.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Enhanced Audio supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_volume = std::clamp<float>(static_cast<float>(api->setting_float(mod, "volume")), 0.1f, 1.5f);
    g_hit_confirm = api->setting_bool(mod, "hit_confirm") != 0;
    g_damage_cue = api->setting_bool(mod, "damage_cue") != 0;
    g_dry_fire = api->setting_bool(mod, "dry_fire") != 0;
    g_low_health = api->setting_bool(mod, "low_health") != 0;
    g_low_threshold = std::clamp<int>(static_cast<int>(api->setting_int(mod, "low_health_threshold")), 1, 8);
    g_status_panel = api->setting_bool(mod, "status_panel") != 0;

    if (!api->sound_load || !api->sound_play || !api->sound_stop) {
        api->fail(mod, "This OpenRAC build has no mod sound API.");
        return 1;
    }
    g_hit_sound = api->sound_load(mod, "assets/audio-cues/hit.wav");
    g_damage_sound = api->sound_load(mod, "assets/audio-cues/damage.wav");
    g_dry_sound = api->sound_load(mod, "assets/audio-cues/dry-fire.wav");
    g_low_sound = api->sound_load(mod, "assets/audio-cues/low-health.wav");
    if (g_hit_sound < 0 || g_damage_sound < 0 || g_dry_sound < 0 || g_low_sound < 0) {
        api->fail(mod, "One or more Enhanced Audio cue files could not be loaded.");
        return 1;
    }

    if (OPENRAC_RAC1_HOOK(api, mod, Moby_GetHitMessage, get_hit, &g_get_hit) != OPENRAC_HOOK_OK ||
        OPENRAC_RAC1_HOOK(api, mod, Hero_TakeDamage, take_damage, &g_take_damage) != OPENRAC_HOOK_OK ||
        OPENRAC_RAC1_HOOK(api, mod, Weapon_TakeAmmo, take_ammo, &g_take_ammo) != OPENRAC_HOOK_OK) {
        api->fail(mod, "A required audio feedback hook is unavailable.");
        return 1;
    }
    if (!api->on_overlay || api->on_overlay(mod, overlay_frame, nullptr) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Enhanced RAC Style requires OpenRAC mod overlay API 1.14.");
        return 1;
    }
    if (api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register the audio callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, "loaded");
    return 0;
}

OPENRAC_MOD_EXPORT void openrac_mod_unload(void) {
    if (g_low_voice && g_api && g_api->sound_stop) g_api->sound_stop(g_mod, g_low_voice);
    g_low_voice = 0;
}
