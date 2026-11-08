# Accessibility Pack

Native OpenRAC accessibility mod for RAC1.

Implemented in v0.1:
- optional invulnerability for normal hits
- incoming-damage scaling from 0-100%
- large high-contrast status overlay using the OpenRAC 1.12 HUD API

This first version does not claim to provide full subtitle, colour-blind or flash-reduction support. Those require renderer/UI/audio hooks that are not currently exposed as mod API controls.

Build:

```sh
cmake -S mods/accessibility-pack -B build/accessibility-pack
cmake --build build/accessibility-pack
```
