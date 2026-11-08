# Custom Skin Studio

An integrated OpenRAC Ratchet skin editor.

Version 0.2.0 changes the tool from a RAC1/RAC2-only ZIP editor into an OpenRAC asset workflow:

- UI follows the compact Enhanced graphics settings style.
- Shows every game in the OpenRAC catalogue: RAC1, Going Commando, Up Your Arsenal, Deadlocked, Size Matters and Secret Agent Clank.
- Uses Ratchet's base class 0 and, with All looks, the exported hero/armour look classes 6000+.
- Builds a normal OpenRAC format-1 overrides pack. The pack is what the native port loads, so edited textures replace Ratchet in game.
- With the Skin Studio host extension, the page can read prepared assets from another selected game and Apply to game installs the generated pack immediately.
- On an unmodified official launcher the current game's editor still works; Export ZIP remains the compatible fallback.

## Supported prepared assets

OpenRAC currently exposes editor-ready port levels for Ratchet & Clank, Going Commando, Up Your Arsenal and Deadlocked. Size Matters and Secret Agent Clank are listed in the selector, but OpenRAC does not yet provide the same prepared editor-level pipeline for them. The Studio reports this instead of producing a fake pack.

## Use

1. Install custom-skins-0.2.0.zip.
2. Open Mods -> Custom Skin Studio -> Open.
3. Pick the game and disc/version.
4. Choose Base only or All looks.
5. Press Load skin.
6. Paint, tint, grayscale or replace a texture PNG.
7. Press Apply to game when the launcher has the host extension, or Export ZIP otherwise.
8. A standard OpenRAC overrides pack replaces the edited Ratchet textures when that game starts with mods enabled.

Nothing modifies the disc image or original prepared levels.

## Cross-game and one-click apply

The stock protocol-1 Mod Tool API deliberately scopes a page to the game that opened it. The companion launcher branch adds two narrow capabilities: read prepared levels for a selected OpenRAC game/version, and install bytes produced by the tool as a validated standard overrides pack. No arbitrary filesystem access or shell execution is given to the mod page.

MIT. Original game assets remain the user's own data and are not included in this repository or release.
