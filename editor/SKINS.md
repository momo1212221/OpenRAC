# Custom Ratchet skins

OpenRAC can load custom Ratchet textures and models as ordinary **overrides packs**. The skin
workbench is a focused wrapper around the current model/texture exporter: it exports only Ratchet
(moby class `0`), keeps the game's skeleton and animation contract, and packs only the files you
actually changed.

Nothing changes the disc image or the extracted levels. The export contains data from your own
disc and must not be redistributed. Share only the finished overrides pack with people who own the
game.

## 1. Export a skin workspace

Get the game's port levels ready first, then run:

```sh
python3 editor/skins.py export assets/port/rac1/pal build/my-ratchet-skin
```

By default every extracted level that contains Ratchet is included. To work from selected levels:

```sh
python3 editor/skins.py export assets/port/rac1/pal build/my-ratchet-skin --level 0 --level 3
```

The workspace contains Ratchet's glTF model, the PNG textures it uses, OpenRAC's normal asset
metadata and a `skin.json` marker. Cross-level matching remains OpenRAC's existing asset logic, so an
edited Ratchet picture is applied to the matching Ratchet textures in other levels when the pack is
built.

## 2. Edit textures

Open the bundled local editor:

```sh
python3 editor/skins.py editor
```

Choose a PNG from `build/my-ratchet-skin/level_NN/textures/`. The editor supports:

- paint and erase;
- brush size and opacity;
- whole-image colour tint;
- grayscale;
- undo and reset;
- PNG export with the original file name.

Save the edited PNG back over the same PNG in the workspace. The editor runs locally in the browser;
it uploads nothing.

You can instead use Krita, GIMP, Photoshop or another image editor. Keep the texture's aspect ratio.
Higher resolutions are allowed as long as the shape stays the same.

## 3. Edit the model (optional)

Ratchet's model is under `level_NN/mobys/moby_0_ratchet/`. Import its `.gltf` into Blender. Keep the
`joint_NNN` skeleton and weight every vertex to valid bones. Export glTF back into that same folder.
The game continues to use its original animations.

## 4. Build the mod

Create a normal OpenRAC overrides pack:

```sh
python3 editor/skins.py pack build/my-ratchet-skin \
  --levels assets/port/rac1/pal \
  --out build/my-ratchet-skin-pack \
  --name "My Ratchet skin" \
  --author "Your name"
```

Or install/update it directly in an overrides folder:

```sh
python3 editor/skins.py pack build/my-ratchet-skin \
  --levels assets/port/rac1/pal \
  --into build/overrides \
  --name "My Ratchet skin"
```

The result uses the same pack format as **Mods → Models and textures** in the current OpenRAC
launcher. No special game loader, binary plugin or fork-only runtime format is required.

## Scope

The first workbench targets Ratchet's base class (`0`). Armour/look variants and Clank can be added as
named presets once their current-game class mappings are stable across the supported games and discs.
