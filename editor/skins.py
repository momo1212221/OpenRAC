#!/usr/bin/env python3
"""Ratchet skin workbench for current OpenRAC.

This is a thin, skin-specific wrapper around editor/assets.py. It exports
Ratchet (moby class 0) and only the textures/models used by that class, then
turns edits back into a normal OpenRAC overrides pack.

Examples:
  python3 editor/skins.py export assets/port/rac1/pal build/my-skin
  python3 editor/skins.py editor
  python3 editor/skins.py pack build/my-skin --levels assets/port/rac1/pal --out build/my-skin-pack

The exported files are disc data and must not be redistributed. Share only
the resulting overrides pack, with people who own the game.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile
import webbrowser

import assets
import overrides

RATCHET_CLASS = 0
SKIN_FORMAT = 1
EDITOR = Path(__file__).with_name("skin_editor") / "index.html"


def _manifest(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def ratchet_levels(levels: Path) -> list[int]:
    """Every extracted level that carries Ratchet's class-0 model."""
    result: list[int] = []
    for path in sorted(levels.glob("level_*/manifest.json")):
        try:
            number = int(path.parent.name.removeprefix("level_"))
            manifest = _manifest(path)
            mobys = manifest.get("classes", {}).get("moby", {})
            ratchet = mobys.get("0", mobys.get(0))
            if isinstance(ratchet, dict) and ratchet.get("mesh"):
                result.append(number)
        except (ValueError, OSError, json.JSONDecodeError):
            continue
    if not result:
        raise assets.AssetError(f"{levels}: no extracted level with Ratchet (moby class 0)")
    return result


def export_skin(levels: Path, out: Path, selected: list[int] | None = None, unique: bool = False) -> dict:
    available = ratchet_levels(levels)
    chosen = sorted(set(selected or available))
    missing = [n for n in chosen if n not in available]
    if missing:
        raise assets.AssetError(f"Ratchet is not available in level(s): {', '.join(map(str, missing))}")
    target = assets.unique(out.resolve()) if unique else out
    summary = assets.export(levels, target, chosen, "mobys", [RATCHET_CLASS])
    marker = {
        "format": SKIN_FORMAT,
        "kind": "openrac-ratchet-skin-workspace",
        "game": summary.get("game"),
        "version": summary.get("version"),
        "levels": chosen,
        "class": RATCHET_CLASS,
        "models": summary.get("models", 0),
        "textures": summary.get("textures", 0),
    }
    (target / "skin.json").write_text(json.dumps(marker, indent=2) + "\n", encoding="utf-8")
    return {**summary, "path": target, "levels": chosen}


def validate_workspace(edited: Path) -> dict:
    marker_path = edited / "skin.json"
    if not marker_path.is_file():
        raise assets.AssetError(f"{edited}: not a Ratchet skin workspace (skin.json is missing)")
    marker = _manifest(marker_path)
    if marker.get("format") != SKIN_FORMAT or marker.get("kind") != "openrac-ratchet-skin-workspace":
        raise assets.AssetError(f"{marker_path}: unsupported skin workspace")
    if marker.get("class") != RATCHET_CLASS:
        raise assets.AssetError(f"{marker_path}: this tool edits Ratchet class 0 only")
    for path in edited.glob("level_*/mobys/*/asset.json"):
        asset = _manifest(path)
        if asset.get("kind") != "moby" or asset.get("class") != RATCHET_CLASS:
            raise assets.AssetError(f"{path}: a Ratchet skin workspace may contain only moby class 0")
    return marker


def pack_skin(
    edited: Path,
    levels: Path,
    *,
    out: Path | None,
    into: Path | None,
    name: str,
    author: str,
    description: str,
    strict: bool,
) -> tuple[Path, assets.Report]:
    marker = validate_workspace(edited)
    report = assets.Report()
    pack = assets.import_edits(edited.resolve(), levels, report)
    if not pack.files:
        raise assets.AssetError("nothing changed in the skin workspace")
    if report.refused and strict:
        raise assets.AssetError(f"{report.refused} edit(s) were refused; nothing was written (--strict)")
    source_id = assets.read_json(edited / "export.json").get("id")
    description = description or "Custom Ratchet skin made with the OpenRAC skin workbench."
    if into is not None:
        with tempfile.TemporaryDirectory(prefix=".skin-import-", dir=overrides.ensure(into)) as scratch:
            staged = Path(scratch) / "pack"
            assets.write_pack(pack, staged, name, author, description, source_id, levels)
            pack_id = overrides.install(staged, into, replace_export=source_id)
        written = into.resolve() / pack_id
    else:
        assert out is not None
        assets.write_pack(pack, assets.output_dir(out), name, author, description, source_id, levels)
        written = out.resolve()
    return written, report


def open_editor() -> None:
    if not EDITOR.is_file():
        raise FileNotFoundError(f"skin editor is missing: {EDITOR}")
    if not webbrowser.open(EDITOR.resolve().as_uri()):
        print(EDITOR.resolve())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)

    export = commands.add_parser("export", help="export Ratchet's model and textures")
    export.add_argument("levels", type=Path)
    export.add_argument("out", type=Path)
    export.add_argument("--level", type=int, action="append", help="only this level (repeatable; default: all with Ratchet)")
    export.add_argument("--unique", action="store_true", help="write a new numbered folder when OUT already exists")

    commands.add_parser("editor", help="open the local PNG skin editor")

    pack = commands.add_parser("pack", help="turn an edited skin workspace into a normal OpenRAC overrides pack")
    pack.add_argument("edited", type=Path)
    pack.add_argument("--levels", type=Path, required=True)
    where = pack.add_mutually_exclusive_group(required=True)
    where.add_argument("--out", type=Path)
    where.add_argument("--into", type=Path)
    pack.add_argument("--name", default="Custom Ratchet skin")
    pack.add_argument("--author", default="")
    pack.add_argument("--description", default="")
    pack.add_argument("--strict", action="store_true")

    args = parser.parse_args(argv)
    try:
        if args.command == "export":
            result = export_skin(args.levels, args.out, args.level, args.unique)
            print(
                f"Exported Ratchet from {len(result['levels'])} level(s): "
                f"{result.get('models', 0)} model(s), {result.get('textures', 0)} texture(s)."
            )
            print(f"Edit the PNG files in {result['path']} or run: python3 editor/skins.py editor")
            print(f"openrac-output: {Path(result['path']).resolve()}")
            return 0
        if args.command == "editor":
            open_editor()
            return 0
        written, report = pack_skin(
            args.edited,
            args.levels,
            out=args.out,
            into=args.into,
            name=args.name,
            author=args.author,
            description=args.description,
            strict=args.strict,
        )
        tail = f" ({report.refused} refused edit(s); see the messages above)" if report.refused else ""
        print(f"Skin pack ready: {written}{tail}")
        print(f"openrac-output: {written}")
        return 0
    except (assets.AssetError, OSError, json.JSONDecodeError) as exc:
        print(f"skins: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
