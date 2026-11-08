# Enhanced Weapons

A native gameplay mod for **Ratchet & Clank** in OpenRAC.

## Weapon progression

Each supported combat weapon earns persistent XP and advances through:

| Tier | XP required | Damage |
| --- | ---: | ---: |
| V1 | 0 | 100% |
| V2 | 200 | 108% |
| V3 | 600 | 118% |
| V4 | 1,400 | 130% |
| V5 | 3,000 | 145% |
| OMEGA | 6,000 | 165% |

XP is earned from successful ammo-consuming shots and, more importantly, from hits. The equipped weapon's tier and progress can be shown in the HUD. A level-up notification appears when a threshold is crossed.

Progress is stored by memory-card save slot in the mod's private OpenRAC data directory. A new unsaved game keeps its in-session XP and transfers it to the slot when that game is first saved.

Supported progression weapons:

- OmniWrench
- Suck Cannon
- Bomb Glove
- Devastator
- Visibomb Gun
- Blaster
- Pyrocitor
- Mine Glove
- Walloper
- Tesla Claw
- Glove of Doom
- Morph-o-Ray
- R.Y.N.O.
- Drone Device
- Decoy Glove

Swingshot, Taunter and Hydrodisplacer remain utility items and do not level.

## Other features

- Hit marker for player weapon/wrench hits.
- Cross or `HIT` marker styles.
- Configurable marker size and duration.
- Optional heat distortion around known player mines, bombs and missiles.
- Higher weapon tiers slightly intensify projectile heat haze.
- Adjustable XP rate from 0.25x to 4x.
- Damage upgrades can be disabled while keeping XP/progression visual only.

## Multiplayer

Because tier damage upgrades can change combat, the mod is classified as **gameplay**. Use the Visual only preset when you want progression/HUD without the damage bonuses.

## Compatibility

- Ratchet & Clank PAL
- Ratchet & Clank NTSC-U
- OpenRAC mod interface 1.12 or newer
- Windows x86_64
- Linux x86_64/aarch64
- macOS arm64/x86_64
- Android arm64

## Build

```sh
python3 tools/fetch_sdk.py
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure --no-tests=ignore
```

Author: **Moritz Wallinger**.
