# Accessibility Pack

A native accessibility and assist mod for **Ratchet & Clank** in OpenRAC.

## Features

- Deterministic normal-hit reduction from 0-100%.
- Optional lethal-hit protection at one health with a three-second cooldown.
- Optional normal-hit invulnerability.
- Large high-contrast health display.
- Prominent low-health warning.
- Visual-only, Assist and Relaxed presets.

Only normal hits routed through the game's damage function are changed. Falls, crushers and other instant-kill hazards retain the game's original behaviour.

## Multiplayer

The mod is classified as gameplay because damage assists affect play. HUD-only settings are marked as each player's own.

## Compatibility

- Ratchet & Clank PAL
- Ratchet & Clank NTSC-U
- OpenRAC mod interface 1.12 or newer

## Build

```sh
python3 tools/fetch_sdk.py
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure --no-tests=ignore
```

## Release

Tag `v0.1.0` for a standalone repository, or `accessibility-pack-v0.1.0` in a multi-mod repository.

Author: **Moritz Wallinger**.
