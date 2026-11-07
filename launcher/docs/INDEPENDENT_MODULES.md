# Independent launcher modules

The Mods list has three independently enabled entries. They ship with the fork launcher because their controls run in the launcher, outside the game-mod sandbox. Their state is stored in `launcher-modules.json`, separate from renderer settings and save data.

- **Enhanced graphics** owns the Vulkan/MoltenVK renderer and ordinary runtime quality settings. Enabling it never starts the baker.
- **Graphics preparation** owns the light/GI, water and cloud baker, progress, resumable jobs and logs, offline Ultra textures, AI asset preparation, hardware tuning and the Metal sunlight side path. It can prepare data while Enhanced graphics is off. To display prepared GI/clouds and Metal sunlight, enable both entries. The native renderer/Metal bridge remains in the port; this is not a replacement Metal renderer.
- **Backup & Cloud Saves** owns manual backup/restore and automatic folder synchronization around Play. Its switch is independent of graphics and applies to all games. The normal save editor and its protective pre-write backup remain available. A cloud application, Syncthing or a NAS synchronizes the selected folder; no provider login or system iCloud activation is performed by this module.

## Existing data

Existing probe and Ultra cache paths are retained. Explicitly configured preparation switches migrate on first use; an explicit module Off wins thereafter. A configured legacy cloud folder keeps synchronization enabled until the module is switched off. Corrupt module configuration fails closed and is not overwritten.

Disabling a module preserves caches, saves, backups and settings. Disabling Ultra texture use also keeps its cache; **Delete generated texture cache** deletes only the generated Ultra folder after confirmation. Lighting cache deletion remains a separate confirmed action. Running asset writes and the baker lock prevent a mid-write disable.

New AI textures live under the pack's `PreparedAI` subfolder; new mesh results live under `prepared-graphics/meshes/<version>`. The launcher passes their use to the game for each start. When preparation is off, generated textures, new AI meshes, probes and Metal sunlight are excluded without editing saved renderer choices. Older AI files already mixed into a user/editor pack are left in that pack rather than guessed at or removed.

## Development

Preparation logic is `core/src/graphics_preparation/`; module state and migration are `core/src/launcher_addons.rs`. `enhanced.rs` retains compatibility exports for older callers. Backups use the existing archive formats and conflict detection. No game or disc data is distributed in the mod source.

Tests cover settings ownership, migration, malformed configuration, cache preservation, active writers, launch arguments, texture filtering and the independent navigation/switches in the browser preview. Metal acceleration structures and sunlight queries have native regression tests.
