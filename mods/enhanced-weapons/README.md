# Enhanced Weapons

Native OpenRAC mod for RAC1.

Implemented in v0.1:
- infinite ammo
- configurable ammo consumption (25-200%)
- PAL and NTSC manifests
- multiplayer classification as gameplay

Build from the OpenRAC checkout:

```sh
cmake -S mods/enhanced-weapons -B build/enhanced-weapons
cmake --build build/enhanced-weapons
```

Planned next: per-weapon profiles, projectile lighting/effects and hit feedback once reusable weapon-shot/effect hooks are available.
