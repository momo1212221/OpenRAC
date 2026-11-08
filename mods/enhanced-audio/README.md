# Enhanced Audio

This is the native foundation for the Enhanced Audio project.

OpenRAC already supports legal user-provided audio replacement packs through the stacked mods folder:

```
audio/<sound-key>.wav
```

The current public mod API can load/play a mod's own sounds, but it cannot yet intercept every game sound or control spatialisation/HRTF/reverb for the game's mixer. Therefore v0.1 deliberately does **not** fake those features.

Implemented:
- native mod manifest and launcher settings
- cosmetic multiplayer classification
- optional in-game capability/status line
- compatibility with OpenRAC's existing audio replacement pack layer

Next port/API work:
- expose per-group mixer gain to mods
- expose game sound play events with position/category
- optional HRTF/spatial backend
- reverb/room profile control
- speech/music/effects category controls
