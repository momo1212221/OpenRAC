# Enhanced Audio

A native OpenRAC mod that adds optional, non-gameplay audio feedback to **Ratchet & Clank**.

## Features

- Hit-confirmation cue when the player's attack connects.
- Damage cue only when Ratchet actually loses health.
- Dry-fire cue when a weapon cannot spend the ammo required for a shot.
- Optional low-health heartbeat with configurable threshold.
- Global cue volume setting and presets.
- Uses small original procedural audio cues included with the mod; no game audio is redistributed.

Enhanced Audio does not claim HRTF, reverb replacement or mixer control that OpenRAC's current mod API does not expose.

## Compatibility

- Ratchet & Clank PAL
- Ratchet & Clank NTSC-U
- OpenRAC mod interface 1.12 or newer
- Standard OpenRAC launcher and native mod loader

## Build

```sh
python3 tools/fetch_sdk.py
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure --no-tests=ignore
```

## Release

Tag `v0.1.0` for a standalone repository, or `enhanced-audio-v0.1.0` in a multi-mod repository.

Author: **Moritz Wallinger**.
