#include "openrac/openrac_mod.h"
#include "openrac/games/rac1.h"

#include <algorithm>
#include <cstdint>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
openrac_rac1_Weapon_TakeAmmo g_take_ammo = nullptr;
bool g_infinite = false;
int g_cost_percent = 100;

int32_t take_ammo(int32_t item, int32_t amount) {
    if (g_infinite) return 1;
    if (amount <= 0) return g_take_ammo(item, amount);
    const int64_t scaled = (static_cast<int64_t>(amount) * g_cost_percent + 99) / 100;
    const int32_t adjusted = static_cast<int32_t>(std::clamp<int64_t>(scaled, 1, INT32_MAX));
    return g_take_ammo(item, adjusted);
}
}

OPENRAC_MOD_DEFINE("enhanced-weapons", "0.1.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Enhanced Weapons currently supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_infinite = api->setting_bool(mod, "infinite_ammo") != 0;
    g_cost_percent = static_cast<int>(api->setting_int(mod, "ammo_cost_percent"));
    g_cost_percent = std::clamp(g_cost_percent, 25, 200);

    const int rc = OPENRAC_RAC1_HOOK(api, mod, Weapon_TakeAmmo, take_ammo, &g_take_ammo);
    if (rc != OPENRAC_HOOK_OK) {
        api->fail(mod, "Weapon_TakeAmmo hook is not available on this port.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO, g_infinite ? "loaded: infinite ammo" : "loaded: configurable ammo cost");
    return 0;
}
