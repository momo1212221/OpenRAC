# Independent launcher modules

Enhanced graphics remains the built-in renderer mod. Two other mods are independently packaged, versioned and installed through **Mods → Add mod → ZIP**:

- [Graphics preparation](https://github.com/momo1212221/openrac-graphics-preparation)
- [Backup & Cloud Saves](https://github.com/momo1212221/openrac-backup-cloud-saves)

These packages require this fork's launcher **1.0.5 or newer**. They have their own sandboxed tool pages. The fork supplies the native baker, renderer and save services; an unmodified upstream launcher cannot run these services. There are no phantom built-in rows for uninstalled packages. Module choices and permission records live in `launcher-modules.json`, separate from renderer settings and save data.

- **Enhanced graphics** owns the Vulkan/MoltenVK renderer and ordinary runtime quality settings. Enabling it never starts the baker.
- **Graphics preparation** owns the light/GI, water and cloud baker, progress, resumable jobs and logs, offline Ultra textures, AI asset preparation, hardware tuning and the Metal sunlight side path. It can prepare data while Enhanced graphics is off. To display prepared GI/clouds and Metal sunlight, enable both entries. The native renderer/Metal bridge remains in the port; this is not a replacement Metal renderer.
- **Backup & Cloud Saves** owns manual backup/restore and automatic folder synchronization around Play. Its switch is independent of graphics and applies to all games. The normal save editor and its protective pre-write backup remain available. A cloud application, Syncthing or a NAS synchronizes the selected folder; no provider login or system iCloud activation is performed by this module.

## Existing data

Existing probe and Ultra cache paths are retained. Explicitly configured preparation switches migrate on first use; an explicit module Off wins thereafter. A configured legacy cloud folder retains its selected state, but automatic synchronization requires an installed, permitted backup mod. Corrupt module configuration fails closed and is not overwritten.

Removing a package makes its runtime hooks inactive. Disabling a module preserves caches, saves, backups and settings. Disabling Ultra texture use also keeps its cache; **Delete generated texture cache** deletes only the generated Ultra folder after confirmation. Lighting cache deletion remains a separate confirmed action. Running asset writes and the baker lock prevent a mid-write disable.

New AI textures live under the pack's `PreparedAI` subfolder; new mesh results live under `prepared-graphics/meshes/<version>`. The launcher passes their use to the game for each start. When preparation is off, generated textures, new AI meshes, probes and Metal sunlight are excluded without editing saved renderer choices. Older AI files already mixed into a user/editor pack are left in that pack rather than guessed at or removed.

## Development

Preparation logic is `core/src/graphics_preparation/`; module state and migration are `core/src/launcher_addons.rs`. `enhanced.rs` retains compatibility exports for older callers. Backups use the existing archive formats and conflict detection. No game or disc data is distributed in the mod source.

Tests cover settings ownership, migration, malformed configuration, cache preservation, active writers, launch arguments, texture filtering and the independent navigation/switches in the browser preview. Metal acceleration structures and sunlight queries have native regression tests.

## Service boundary

A format-2 manifest may declare `tool.service` equal to its own id, either `graphics-preparation` or `save-backup-sync`. Unknown/mismatched service ids are refused. On first Open, a native permission dialog describes that service before granting access. Each service request revalidates installation and permission. The page cannot grant permissions through its message protocol, choose another mod identity, supply a disc serial, invoke a shell or access arbitrary paths.

`{openrac:1,id,call:"service",args:{operation,value}}` receives the ordinary tool RPC reply. The `info` response exposes granted `services` and `hostVersion`. Both services expose `status` and `enable`; graphics adds `setting`, `bake.start/stop/clear`, `textures.prepare/clear`, `ai.textures/meshes`; backup adds `backup.create/restore`, `cloud.choose/disconnect/sync`. Folder selection uses a native picker. Cache deletion, restore and forced conflict resolution require another native confirmation in Rust. A mod can stop only the lighting job recorded for its own game/version, not an externally started process. Logs are bounded to 128 KiB of this version's fixed lighting log.

Developer check: `cargo run -p openrac-launcher-core --example check_launcher_mod -- path/to/mod.zip`. It installs into a temporary directory, grants and toggles there, removes the package, and verifies runtime hooks become inactive; no real saves or game starts are involved.
