#include "openrac/openrac_mod.h"

#include <cstdio>
#include <cstring>
#include <string>

namespace {
const openrac_api* g_api = nullptr;
openrac_mod* g_mod = nullptr;
bool g_show = false;

void frame(void*) {
    if (!g_show || !g_api || !g_api->hud_text_set) return;
    openrac_hud_text line{};
    std::snprintf(line.text, sizeof(line.text), "ENHANCED AUDIO: PACK LAYER ACTIVE");
    line.x = 0.5f;
    line.y = 0.94f;
    line.scale = 0.55f;
    line.rgba[0] = 220;
    line.rgba[1] = 220;
    line.rgba[2] = 220;
    line.rgba[3] = 230;
    line.flags = OPENRAC_HUD_SMALL | OPENRAC_HUD_SHADOW;
    g_api->hud_text_set(g_mod, &line, 1);
}
}

OPENRAC_MOD_DEFINE("enhanced-audio", "0.1.0")

OPENRAC_MOD_EXPORT int openrac_mod_load(const openrac_api* api, openrac_mod* mod) {
    if (!api || !mod || !api->game || std::string(api->game) != "rac1") {
        if (api && mod) api->fail(mod, "Enhanced Audio currently supports Ratchet & Clank only.");
        return 1;
    }
    g_api = api;
    g_mod = mod;
    g_show = api->setting_bool(mod, "show_status") != 0;
    if (g_show && api->on_frame(mod, frame, nullptr) != 0) {
        api->fail(mod, "Could not register audio status callback.");
        return 1;
    }
    api->log(mod, OPENRAC_LOG_INFO,
      "loaded; OpenRAC audio/<key>.wav replacement packs are supported. Spatial/HRTF mixer control requires a future mod API extension.");
    return 0;
}
