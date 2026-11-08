# Enhanced Weapons

A cosmetic native mod for **Ratchet & Clank** in OpenRAC.

## Features

- Hit marker for wrench and weapon hits.
- Two marker styles: cross or `HIT`.
- Configurable marker size and duration.
- Optional heat distortion around known player mines, bombs and missiles.
- Cosmetic multiplayer classification: it does not alter damage, ammo, saves or progression.

The mod deliberately does **not** duplicate the Difficulty mod's ammo/damage scaling or the external Damage Numbers mod.

## Compatibility

- Ratchet & Clank PAL
- Ratchet & Clank NTSC-U
- OpenRAC mod interface 1.12 or newer
- Windows x86_64, Linux x86_64/aarch64, macOS arm64/x86_64 and Android arm64 through the standard SDK workflow

## Build

```sh
python3 tools/fetch_sdk.py
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure --no-tests=ignore
```

## Release

Tag `v0.1.0` when this folder is used as a standalone repository, or `enhanced-weapons-v0.1.0` in a multi-mod repository. The included workflow builds and packages all supported platforms.

Author: **Moritz Wallinger**.
