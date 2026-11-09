#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Moby_GetHitMessage g_get_hit = nullptr;
openrac_rac1_Weapon_TakeAmmo g_take_ammo = nullptr;

struct WeaponInfo {
    int item;
    const char* name;
};

// Combat weapons. Swingshot, Taunter and Hydrodisplacer are utility items and intentionally do not level.
constexpr std::array<WeaponInfo, 15> kWeapons = {{
    {0x08, "OmniWrench"},
    {0x09, "Suck Cannon"},
    {0x0a, "Bomb Glove"},
    {0x0b, "Devastator"},
    {0x0d, "Visibomb Gun"},
    {0x0f, "Blaster"},
    {0x10, "Pyrocitor"},
    {0x11, "Mine Glove"},
    {0x12, "Walloper"},
    {0x13, "Tesla Claw"},
    {0x14, "Glove of Doom"},
    {0x15, "Morph-o-Ray"},
    {0x17, "R.Y.N.O."},
    {0x18, "Drone Device"},
    {0x19, "Decoy Glove"},
}};

constexpr std::array<std::uint32_t, 6> kTierXp = {0, 200, 600, 1400, 3000, 6000};
constexpr std::array<float, 6> kDamageMultiplier = {1.00f, 1.08f, 1.18f, 1.30f, 1.45f, 1.65f};
constexpr int kItemCount = 64;
constexpr int kSaveSlots = 16;
constexpr int kUnsavedSlot = kSaveSlots;
constexpr int kProgressSlots = kSaveSlots + 1;

std::array<std::array<std::uint32_t, kItemCount>, kProgressSlots> g_xp{};
int g_active_slot = kUnsavedSlot;
bool g_unsaved_session = false;
bool g_progress_dirty = false;
std::string g_progress_path;
std::uint32_t g_save_timer = 0;

bool g_hit_marker = true;
bool g_word_marker = false;
bool g_projectile_haze = true;
bool g_weapon_xp = true;
bool g_xp_hud = true;
bool g_damage_upgrades = true;
bool g_level_up_notice = true;
bool g_arsenal_mastery_loaded = false;
std::uint32_t g_mod_scan_timer = 0;
int g_marker_duration = 10;
int g_marker_left = 0;
float g_marker_scale = 0.85f;
float g_haze_strength = 0.08f;
float g_xp_rate = 1.0f;
int g_levelup_left = 0;
int g_levelup_item = -1;
openrac_gaddr g_last_hit = 0;
std::uint32_t g_last_hit_tick = 0;

constexpr std::array<int, 5> kPlayerProjectileClasses = {74, 153, 172, 230, 457};
std::array<openrac_moby, 512> g_mobys{};

const WeaponInfo* weapon_info(int item) {
    for (const auto& weapon : kWeapons) {
        if (weapon.item == item) return &weapon;
    }
    return nullptr;
}

bool loaded_mod(const char* id) {
    if (!g_api || g_api->api_minor < 14 || !g_api->mod_count || !g_api->mod_entry) return false;
    const int count = g_api->mod_count();
    for (int i = 0; i < count; ++i) {
        openrac_mod_entry entry{};
        entry.size = sizeof entry;
        if (g_api->mod_entry(i, &entry) == OPENRAC_HOOK_OK && entry.id
            && std::strcmp(entry.id, id) == 0) {
            return true;
        }
    }
    return false;
}

bool own_progression_enabled() {
    if (!g_arsenal_mastery_loaded && loaded_mod("arsenal-mastery")) {
        g_arsenal_mastery_loaded = true;
    }
    return g_weapon_xp && !g_arsenal_mastery_loaded;
}

const char* tier_name(int tier) {
    static constexpr const char* names[] = {"V1", "V2", "V3", "V4", "V5", "OMEGA"};
    return names[std::clamp(tier, 0, 5)];
}

int tier_for_xp(std::uint32_t xp) {
    int tier = 0;
    for (int i = 1; i < static_cast<int>(kTierXp.size()); ++i) {
        if (xp >= kTierXp[static_cast<std::size_t>(i)]) tier = i;
    }
    return tier;
}

std::uint32_t& xp_for(int item) {
    return g_xp[static_cast<std::size_t>(g_active_slot)][static_cast<std::size_t>(item)];
}

int current_item() {
    if (!g_api || !g_api->hero_pose) return -1;
    openrac_pose pose{};
    pose.size = sizeof pose;
    return g_api->hero_pose(&pose) == OPENRAC_HOOK_OK ? pose.item : -1;
}

bool slot_has_progress(int slot) {
    if (slot < 0 || slot >= kProgressSlots) return false;
    for (std::uint32_t value : g_xp[static_cast<std::size_t>(slot)]) {
        if (value != 0) return true;
    }
    return false;
}

void load_progress() {
    if (g_progress_path.empty()) return;
    std::FILE* file = std::fopen(g_progress_path.c_str(), "rb");
    if (!file) return;

    char header[32]{};
    if (!std::fgets(header, sizeof header, file) || std::strncmp(header, "OPENRAC_WEAPON_XP_V2", 20) != 0) {
        std::fclose(file);
        return;
    }

    int slot = 0;
    int item = 0;
    unsigned long value = 0;
    while (std::fscanf(file, "%d %d %lu", &slot, &item, &value) == 3) {
        if (slot >= 0 && slot < kSaveSlots && item >= 0 && item < kItemCount) {
            g_xp[static_cast<std::size_t>(slot)][static_cast<std::size_t>(item)] =
                static_cast<std::uint32_t>(std::min<unsigned long>(value, kTierXp.back()));
        }
    }
    std::fclose(file);
}

void save_progress() {
    if (!g_progress_dirty || g_progress_path.empty()) return;
    const std::string temp = g_progress_path + ".tmp";
    std::FILE* file = std::fopen(temp.c_str(), "wb");
    if (!file) return;

    std::fputs("OPENRAC_WEAPON_XP_V2\n", file);
    for (int slot = 0; slot < kSaveSlots; ++slot) {
        for (const auto& weapon : kWeapons) {
            const std::uint32_t value =
                g_xp[static_cast<std::size_t>(slot)][static_cast<std::size_t>(weapon.item)];
            if (value != 0) std::fprintf(file, "%d %d %u\n", slot, weapon.item, value);
        }
    }
    if (std::fclose(file) != 0) return;

    std::remove(g_progress_path.c_str());
    if (std::rename(temp.c_str(), g_progress_path.c_str()) == 0) g_progress_dirty = false;
}

void update_progress_slot() {
    if (!g_api || !g_api->save_slot_current || !g_api->play_status) return;

    openrac_play_status status{};
    status.size = sizeof status;
    if (g_api->play_status(&status) != OPENRAC_HOOK_OK) return;

    if (status.phase == OPENRAC_PLAY_FRONT_END) {
        if (g_active_slot != kUnsavedSlot) {
            save_progress();
            g_active_slot = kUnsavedSlot;
            g_xp[static_cast<std::size_t>(kUnsavedSlot)].fill(0);
        }
        g_unsaved_session = false;
        return;
    }

    const int slot = g_api->save_slot_current();
    if (slot >= 0 && slot < kSaveSlots && slot != g_active_slot) {
        if (g_active_slot == kUnsavedSlot && g_unsaved_session) {
            // A new, previously unsaved game was just written to this memory-card slot.
            g_xp[static_cast<std::size_t>(slot)] = g_xp[static_cast<std::size_t>(kUnsavedSlot)];
            g_xp[static_cast<std::size_t>(kUnsavedSlot)].fill(0);
            g_progress_dirty = true;
        }
        g_active_slot = slot;
        g_unsaved_session = false;
        return;
    }

    if (slot < 0 && g_active_slot == kUnsavedSlot &&
        (status.phase == OPENRAC_PLAY_PLAYING || status.phase == OPENRAC_PLAY_SCENE ||
         status.phase == OPENRAC_PLAY_MENU)) {
        g_unsaved_session = true;
    }
}

void grant_xp(int item, int base_amount) {
    if (!own_progression_enabled() || item < 0 || item >= kItemCount || !weapon_info(item) || base_amount <= 0) return;

    std::uint32_t& xp = xp_for(item);
    const int old_tier = tier_for_xp(xp);
    const int scaled = std::max(1, static_cast<int>(std::lround(static_cast<float>(base_amount) * g_xp_rate)));
    xp = std::min<std::uint32_t>(kTierXp.back(), xp + static_cast<std::uint32_t>(scaled));
    const int new_tier = tier_for_xp(xp);
    g_progress_dirty = true;

    if (new_tier > old_tier) {
        g_levelup_item = item;
        g_levelup_left = g_level_up_notice ? 240 : 0;
        save_progress();

        char log_line[160];
        std::snprintf(log_line, sizeof log_line, "%s upgraded to %s (%u XP)",
                      weapon_info(item)->name, tier_name(new_tier), xp);
        if (g_api && g_api->log) g_api->log(g_mod, OPENRAC_LOG_INFO, log_line);
    }
}

bool is_player_projectile(int oclass) {
    return std::find(kPlayerProjectileClasses.begin(), kPlayerProjectileClasses.end(), oclass)
           != kPlayerProjectileClasses.end();
}

openrac_gaddr get_hit(openrac_gaddr moby, int32_t mask, int32_t keep) {
    const openrac_gaddr hit = g_get_hit(moby, mask, keep);
    if (!hit || !g_api || !g_api->memory) return hit;

    const openrac_input* input = g_api->input ? g_api->input() : nullptr;
    const std::uint32_t tick = input ? input->tick : 0;
    if (hit == g_last_hit && tick == g_last_hit_tick) return hit;
    g_last_hit = hit;
    g_last_hit_tick = tick;

    openrac_gaddr attacker = 0;
    float damage = 0.0f;
    std::memcpy(&attacker, g_api->memory + hit + 0x20, sizeof attacker);
    std::memcpy(&damage, g_api->memory + hit + 0x2c, sizeof damage);
    if (!std::isfinite(damage)) return hit;

    const int hero_slot = g_api->figure_slot ? g_api->figure_slot(g_mod, -1) : -1;
    const int attacker_slot = g_api->moby_slot_of ? g_api->moby_slot_of(attacker) : -1;
    if (hero_slot < 0 || attacker_slot != hero_slot) return hit;

    const int item = current_item();
    if (!weapon_info(item)) return hit;

    if (g_hit_marker) g_marker_left = g_marker_duration;

    // Hits give most of the progression. Zero-damage utility hits still give a little XP.
    const int hit_xp = 8 + static_cast<int>(std::clamp(std::fabs(damage) * 2.0f, 0.0f, 32.0f));
    grant_xp(item, hit_xp);

    if (g_damage_upgrades && own_progression_enabled() && damage > 0.0f) {
        const int tier = tier_for_xp(xp_for(item));
        const float upgraded = damage * kDamageMultiplier[static_cast<std::size_t>(tier)];
        std::memcpy(g_api->memory + hit + 0x2c, &upgraded, sizeof upgraded);
    }
    return hit;
}

int32_t take_ammo(int32_t item, int32_t amount) {
    const int resolved = item >= 0 ? item : current_item();
    const int32_t result = g_take_ammo(item, amount);
    if (result && amount > 0 && weapon_info(resolved)) {
        // A real ammo-consuming shot gives a small amount even when it misses.
        grant_xp(resolved, 2);
    }
    return result;
}


std::uint32_t colour(unsigned r, unsigned g, unsigned b, unsigned a) {
    return ((r & 0xffu) << 24u) | ((g & 0xffu) << 16u) | ((b & 0xffu) << 8u) | (a & 0xffu);
}

void panel(const openrac_overlay* overlay, float x, float y, float w, float h, std::uint8_t alpha) {
    if (!overlay || !overlay->rect) return;
    const std::uint8_t shadow_a = static_cast<std::uint8_t>(alpha * 0.55f);
    overlay->rect(overlay->context, x + 3.0f, y + 3.0f, w, h, colour(0, 0, 0, shadow_a));
    overlay->rect(overlay->context, x, y, w, h, colour(91, 55, 10, alpha));
    overlay->rect(overlay->context, x + 2.0f, y + 2.0f, w - 4.0f, h - 4.0f,
                  colour(22, 18, 12, static_cast<std::uint8_t>(alpha * 0.94f)));
    overlay->rect(overlay->context, x + 4.0f, y + 4.0f, 4.0f, h - 8.0f,
                  colour(235, 157, 34, alpha));
    overlay->rect(overlay->context, x + 8.0f, y + 4.0f, w - 12.0f, 2.0f,
                  colour(255, 201, 72, static_cast<std::uint8_t>(alpha * 0.82f)));
}

void rac_text(
    const openrac_overlay* overlay,
    float x,
    float y,
    float height,
    std::uint32_t rgba,
    const char* text
) {
    if (!overlay || !overlay->text || !text) return;
    overlay->text(overlay->context, x + 1.3f, y + 1.3f, height, colour(0, 0, 0, 190), text);
    overlay->text(overlay->context, x, y, height, rgba, text);
}

void centred_text(
    const openrac_overlay* overlay,
    float centre,
    float y,
    float height,
    std::uint32_t rgba,
    const char* text
) {
    if (!overlay || !text) return;
    const float width = overlay->text_width ? overlay->text_width(overlay->context, height, text) : 0.0f;
    rac_text(overlay, centre - width * 0.5f, y, height, rgba, text);
}

void draw_hit_marker(const openrac_overlay* overlay) {
    if (!g_hit_marker || g_marker_left <= 0 || !overlay || !overlay->rect) return;

    const float fade = static_cast<float>(g_marker_left) / static_cast<float>(g_marker_duration);
    const std::uint8_t alpha = static_cast<std::uint8_t>(std::clamp(255.0f * fade, 0.0f, 255.0f));
    const float scale = g_marker_scale;
    const float cx = 256.0f;
    const float cy = 224.0f;
    const float gap = 4.0f * scale;
    const float length = 8.0f * scale;
    const float thick = std::max(1.5f, 1.8f * scale);

    // Dark undershadow keeps the marker readable against bright planets without looking like a PC FPS HUD.
    const auto shadow = colour(18, 12, 4, static_cast<std::uint8_t>(alpha * 0.80f));
    const auto gold = colour(255, 194, 56, alpha);
    const auto core = colour(255, 244, 190, alpha);

    auto piece = [&](float x, float y, float w, float h) {
        overlay->rect(overlay->context, x + 1.0f, y + 1.0f, w, h, shadow);
        overlay->rect(overlay->context, x, y, w, h, gold);
    };

    piece(cx - gap - length, cy - thick * 0.5f, length, thick);
    piece(cx + gap, cy - thick * 0.5f, length, thick);
    piece(cx - thick * 0.5f, cy - gap - length, thick, length);
    piece(cx - thick * 0.5f, cy + gap, thick, length);
    overlay->rect(overlay->context, cx - 1.0f, cy - 1.0f, 2.0f, 2.0f, core);

    if (g_word_marker) {
        centred_text(overlay, cx, cy + 13.0f * scale, 6.5f * scale, core, "HIT");
    }
}

void draw_weapon_progress(const openrac_overlay* overlay) {
    if (!own_progression_enabled() || !g_xp_hud || !overlay) return;
    const int item = current_item();
    const WeaponInfo* active = weapon_info(item);
    if (!active) return;

    const std::uint32_t xp = xp_for(item);
    const int tier = tier_for_xp(xp);
    constexpr float x = 18.0f;
    constexpr float y = 362.0f;
    constexpr float w = 218.0f;
    constexpr float h = 68.0f;
    constexpr std::uint8_t a = 232;
    panel(overlay, x, y, w, h, a);

    rac_text(overlay, x + 15.0f, y + 10.0f, 8.0f, colour(255, 232, 168, 255), active->name);

    const bool omega = tier >= 5;
    const float badge_w = omega ? 58.0f : 38.0f;
    const float badge_x = x + w - badge_w - 10.0f;
    overlay->rect(overlay->context, badge_x, y + 8.0f, badge_w, 17.0f,
                  omega ? colour(126, 70, 8, 245) : colour(71, 47, 18, 235));
    overlay->rect(overlay->context, badge_x + 1.0f, y + 9.0f, badge_w - 2.0f, 2.0f,
                  omega ? colour(255, 214, 71, 255) : colour(225, 151, 36, 255));
    centred_text(
        overlay, badge_x + badge_w * 0.5f, y + 13.0f, 6.5f,
        omega ? colour(255, 231, 112, 255) : colour(255, 206, 92, 255), tier_name(tier)
    );

    char xp_line[64];
    if (omega) {
        std::snprintf(xp_line, sizeof xp_line, "WEAPON XP  %u  MAX", xp);
    } else {
        std::snprintf(xp_line, sizeof xp_line, "WEAPON XP  %u / %u", xp,
                      kTierXp[static_cast<std::size_t>(tier + 1)]);
    }
    rac_text(overlay, x + 15.0f, y + 29.0f, 5.5f, colour(226, 211, 180, 240), xp_line);

    constexpr float bar_x = x + 15.0f;
    constexpr float bar_y = y + 42.0f;
    constexpr float bar_w = w - 30.0f;
    constexpr float bar_h = 8.0f;
    overlay->rect(overlay->context, bar_x, bar_y, bar_w, bar_h, colour(8, 7, 5, 220));
    overlay->rect(overlay->context, bar_x + 1.0f, bar_y + 1.0f, bar_w - 2.0f, bar_h - 2.0f,
                  colour(56, 44, 27, 235));

    float progress = 1.0f;
    if (!omega) {
        const std::uint32_t start = kTierXp[static_cast<std::size_t>(tier)];
        const std::uint32_t end = kTierXp[static_cast<std::size_t>(tier + 1)];
        progress = end > start ? static_cast<float>(xp - start) / static_cast<float>(end - start) : 0.0f;
        progress = std::clamp(progress, 0.0f, 1.0f);
    }
    if (progress > 0.0f) {
        overlay->rect(
            overlay->context, bar_x + 2.0f, bar_y + 2.0f, (bar_w - 4.0f) * progress, bar_h - 4.0f,
            omega ? colour(255, 213, 61, 255) : colour(231, 145, 25, 255)
        );
    }

    // Six small illuminated cells make the V1 -> V5 -> OMEGA hierarchy readable at a glance.
    constexpr float cell_gap = 3.0f;
    constexpr float tier_y = y + 55.0f;
    const float cell_w = (bar_w - cell_gap * 5.0f) / 6.0f;
    for (int i = 0; i < 6; ++i) {
        const float cell_x = bar_x + static_cast<float>(i) * (cell_w + cell_gap);
        const bool reached = i <= tier;
        overlay->rect(
            overlay->context, cell_x, tier_y, cell_w, 5.0f,
            reached
                ? (i == 5 ? colour(255, 212, 52, 255) : colour(206, 121, 20, 245))
                : colour(46, 38, 28, 210)
        );
    }
}

void draw_level_up(const openrac_overlay* overlay) {
    if (!g_level_up_notice || g_levelup_left <= 0 || g_levelup_item < 0 || !overlay) return;
    const WeaponInfo* upgraded = weapon_info(g_levelup_item);
    if (!upgraded) return;

    float fade = 1.0f;
    if (g_levelup_left > 220) fade = static_cast<float>(240 - g_levelup_left) / 20.0f;
    if (g_levelup_left < 35) fade = static_cast<float>(g_levelup_left) / 35.0f;
    fade = std::clamp(fade, 0.0f, 1.0f);
    const auto alpha = static_cast<std::uint8_t>(255.0f * fade);

    constexpr float x = 111.0f;
    constexpr float y = 68.0f;
    constexpr float w = 290.0f;
    constexpr float h = 64.0f;
    panel(overlay, x, y, w, h, alpha);
    overlay->rect(overlay->context, x + 8.0f, y + 8.0f, w - 16.0f, 4.0f,
                  colour(255, 190, 40, alpha));

    centred_text(
        overlay, 256.0f, y + 17.0f, 7.0f,
        colour(255, 224, 142, alpha), "WEAPON UPGRADE"
    );

    char line[96];
    const int tier = tier_for_xp(xp_for(g_levelup_item));
    std::snprintf(line, sizeof line, "%s  %s", upgraded->name, tier_name(tier));
    centred_text(
        overlay, 256.0f, y + 34.0f, 10.0f,
        tier >= 5 ? colour(255, 218, 67, alpha) : colour(255, 246, 211, alpha), line
    );

    // A short gold underline gives the banner the same tech-card language as the weapon panel.
    overlay->rect(overlay->context, 208.0f, y + 52.0f, 96.0f, 3.0f,
                  colour(240, 153, 28, alpha));
}

void overlay_frame(void*, const openrac_overlay* overlay) {
    draw_weapon_progress(overlay);
    draw_hit_marker(overlay);
    draw_level_up(overlay);
}

void frame(void*) {
    if (!g_api || !g_mod) return;

    if (++g_mod_scan_timer >= 60 || !g_arsenal_mastery_loaded) {
        g_mod_scan_timer = 0;
        const bool found = loaded_mod("arsenal-mastery");
        if (found && !g_arsenal_mastery_loaded && g_api->log) {
            g_api->log(
                g_mod,
                OPENRAC_LOG_INFO,
                "Arsenal Mastery detected: Enhanced Weapons keeps visual effects but disables its own XP/damage progression"
            );
        }
        g_arsenal_mastery_loaded = found;
    }

    update_progress_slot();

    if (++g_save_timer >= 300) {
        g_save_timer = 0;
        save_progress();
    }

    if (g_marker_left > 0) --g_marker_left;
    if (g_levelup_left > 0) --g_levelup_left;

    const int item = current_item();
    const WeaponInfo* active = weapon_info(item);

    if (!g_api->haze_set) return;
    if (!g_projectile_haze || g_haze_strength <= 0.0f || !g_api->moby_list) {
        g_api->haze_set(g_mod, nullptr, 0);
        return;
    }

    float tier_haze = 1.0f;
    if (active && own_progression_enabled()) tier_haze += static_cast<float>(tier_for_xp(xp_for(item))) * 0.08f;

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
        haze[static_cast<std::size_t>(count)].strength = std::min(0.25f, g_haze_strength * tier_haze);
        ++count;
    }
    g_api->haze_set(g_mod, count ? haze.data() : nullptr, count);
}

}  // namespace

OPENRAC_MOD_DEFINE("enhanced-weapons", "0.3.1")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Enhanced Weapons supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_hit_marker = api->setting_bool(mod, "hit_marker") != 0;
    g_projectile_haze = api->setting_bool(mod, "projectile_haze") != 0;
    g_weapon_xp = api->setting_bool(mod, "weapon_xp") != 0;
    g_xp_hud = api->setting_bool(mod, "xp_hud") != 0;
    g_damage_upgrades = api->setting_bool(mod, "damage_upgrades") != 0;
    g_level_up_notice = api->setting_bool(mod, "level_up_notice") != 0;
    g_marker_duration = std::clamp<int>(static_cast<int>(api->setting_int(mod, "marker_duration")), 4, 30);
    g_marker_scale = std::clamp<float>(static_cast<float>(api->setting_float(mod, "marker_scale")), 0.5f, 1.5f);
    g_haze_strength = std::clamp<float>(static_cast<float>(api->setting_float(mod, "haze_strength")), 0.0f, 0.2f);
    g_xp_rate = std::clamp<float>(static_cast<float>(api->setting_float(mod, "xp_rate")), 0.25f, 4.0f);
    g_word_marker = std::string(api->setting_string(mod, "marker_style")) == "word";

    if (api->data_dir) {
        if (const char* dir = api->data_dir(mod); dir && *dir) {
            g_progress_path = std::string(dir) + "/weapon-xp-v2.dat";
            load_progress();
        }
    }

    if (OPENRAC_RAC1_HOOK(api, mod, Moby_GetHitMessage, get_hit, &g_get_hit) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Moby_GetHitMessage hook is unavailable.");
        return 1;
    }
    if (OPENRAC_RAC1_HOOK(api, mod, Weapon_TakeAmmo, take_ammo, &g_take_ammo) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Weapon_TakeAmmo hook is unavailable.");
        return 1;
    }
    if (!api->on_overlay || api->on_overlay(mod, overlay_frame, nullptr) != OPENRAC_HOOK_OK) {
        api->fail(mod, "Enhanced RAC Style requires OpenRAC mod overlay API 1.14.");
        return 1;
    }
    if (api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register the Enhanced Weapons callback.");
        return 1;
    }

    g_arsenal_mastery_loaded = loaded_mod("arsenal-mastery");
    api->log(
        mod,
        OPENRAC_LOG_INFO,
        g_arsenal_mastery_loaded
            ? "loaded: Arsenal Mastery owns progression; Enhanced Weapons visual effects remain enabled"
            : "loaded: Enhanced RAC Style weapon XP and V1-V5/Omega progression enabled"
    );
    return 0;
}

OPENRAC_MOD_EXPORT void openrac_mod_unload(void) {
    save_progress();
}
