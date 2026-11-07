#!/usr/bin/env python3
"""Decode Ratchet & Clank HUD/interface textures from the player's disc before Play.

The renderer's texture-pack identity is FNV-1a over width, height and RGBA texels. HUD palettes
are PS2 CSM1 and, for this identity, keep the GS alpha byte as stored (0x80 is opaque to the GS);
the runtime converts that same raw image before texture-pack replacement. The generated PNGs stay
local in the extracted port data and are input to the launcher's persistent Ultra4K bake.
"""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import struct
import sys

from disc import Disc, SECTOR
from formats import FormatError, png, png_chunk, span, unpack
from level import SECTION_NAMES, decoded

FORMAT = 2
STAMP = "openrac-offline-hud.json"
FNV_OFFSET = 1469598103934665603
FNV_PRIME = 1099511628211
BANKS = 5
PALETTE_BYTES = 256 * 4
# Global lumps whose validated 2FIP images the game streams into menus, maps, help/mission pages,
# galleries, transitions, ships and other front-end screens. Some names are still historical TOC
# names; the PIF validator, not the name, decides what becomes an image.
PIF_GROUPS = {
    "debug_font", "help_controls", "help_moves", "help_weapons", "help_gadgets", "help_ss",
    "options_ss", "mission_ss", "planets", "unknown_stuff2", "goodies_images",
    "character_sketches", "character_renders", "skill_images", "epilogue_images", "sketchbook",
    "commercials", "item_images", "spaceships", "space_plates", "transition", "unknown_wads",
    "credits_images_ntsc", "credits_images_pal",
}


def _mix(h: int, byte: int) -> int:
    return ((h ^ byte) * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF


def texture_hash(width: int, height: int, rgba: bytes) -> str:
    h = FNV_OFFSET
    for side in (width, height):
        for byte in side.to_bytes(4, "little"):
            h = _mix(h, byte)
    for byte in rgba:
        h = _mix(h, byte)
    return f"{h:016x}"


def _palette(raw: bytes) -> list[tuple[int, int, int, int]]:
    if len(raw) < PALETTE_BYTES:
        raise FormatError("HUD palette is shorter than 1024 bytes")
    colours = []
    for i in range(256):
        # GS CSM1 swaps palette-index bits 3 and 4. Keep the raw PS2 alpha byte here: the
        # texture-pack hash is made before the renderer maps 0x80 to display alpha 1.0.
        j = (i & ~0x18) | ((i & 0x08) << 1) | ((i & 0x10) >> 1)
        colours.append(tuple(raw[j * 4:j * 4 + 4]))
    return colours


def _picture(width: int, height: int, indices: bytes, palette: bytes) -> tuple[str, bytes]:
    if len(indices) != width * height:
        raise FormatError("HUD texture has the wrong number of texels")
    colours = _palette(palette)
    rgba = bytearray()
    for index in indices:
        rgba.extend(colours[index])
    name = texture_hash(width, height, bytes(rgba))
    picture = png(
        width,
        height,
        3,
        indices,
        png_chunk(b"PLTE", b"".join(bytes(c[:3]) for c in colours)),
        png_chunk(b"tRNS", bytes(c[3] for c in colours)),
    )
    return name, picture



def pif_pictures(blob: bytes) -> dict[str, bytes]:
    """Every validated 2FIP image embedded in a decompressed global lump."""
    out: dict[str, bytes] = {}
    at = 0
    while True:
        at = blob.find(b"2FIP", at)
        if at < 0:
            return out
        if at + 0x420 <= len(blob):
            width, height, fmt = unpack("<III", blob, at + 8)
            valid_side = lambda side: 0 < side <= 1024 and (side & (side - 1)) == 0
            end = at + 0x420 + width * height
            if fmt == 0x13 and valid_side(width) and valid_side(height) and end <= len(blob):
                palette = span(blob, at + 0x20, PALETTE_BYTES)
                pixels = span(blob, at + 0x420, width * height)
                name, picture = _picture(width, height, pixels, palette)
                out.setdefault(name, picture)
        at += 4


def _owner(totals: tuple[int, ...], index: int) -> int | None:
    for bank in range(BANKS):
        if index < totals[bank]:
            return bank
    return None


def hud_pictures(header: bytes, banks: list[bytes]) -> dict[str, bytes]:
    """Every distinct picture referenced by a HUD frame, keyed by texture-pack hash."""
    if len(header) < 0xB4 or len(banks) != BANKS:
        raise FormatError("HUD header or bank set is incomplete")
    icon_count, frame_count = unpack("<HH", header, 0)
    icon_offset, frame_offset, palette_offset, texture_offset = unpack("<4I", header, 4)
    palette_total = unpack("<8I", header, 0x14)
    texture_total = unpack("<8I", header, 0x34)
    bank_size = unpack("<8I", header, 0x54)
    if not 0 < icon_count <= 0x1000 or frame_count > 0x4000:
        raise FormatError("HUD header has implausible counts")
    if palette_total[BANKS - 1] > 0x1000 or texture_total[BANKS - 1] > 0x1000:
        raise FormatError("HUD header has implausible palette/texture counts")
    span(header, icon_offset, icon_count * 8)
    span(header, frame_offset, frame_count * 4)
    span(header, palette_offset, palette_total[BANKS - 1] * 8)
    span(header, texture_offset, texture_total[BANKS - 1] * 8)
    for i in range(BANKS):
        if len(banks[i]) < bank_size[i]:
            raise FormatError(f"HUD bank {i} is shorter than its declared size")

    out: dict[str, bytes] = {}
    seen_pairs: set[tuple[int, int]] = set()
    for frame in range(frame_count):
        palette_index, texture_index = unpack("<hh", header, frame_offset + frame * 4)
        if palette_index < 0 or texture_index < 0:
            continue
        pair = (palette_index, texture_index)
        if pair in seen_pairs:
            continue
        seen_pairs.add(pair)
        if palette_index >= palette_total[BANKS - 1] or texture_index >= texture_total[BANKS - 1]:
            raise FormatError(f"HUD frame {frame} has an out-of-range palette or texture")
        pb = _owner(palette_total, palette_index)
        tb = _owner(texture_total, texture_index)
        if pb is None or tb is None:
            raise FormatError(f"HUD frame {frame} belongs to no bank")
        po = unpack("<I", header, palette_offset + palette_index * 8)[0] & 0x7FFFFFFF
        to = unpack("<I", header, texture_offset + texture_index * 8)[0] & 0x7FFFFFFF
        log_w = header[texture_offset + texture_index * 8 + 6]
        log_h = header[texture_offset + texture_index * 8 + 7]
        width, height = 1 << (log_w & 0x1F), 1 << (log_h & 0x1F)
        pixels = span(banks[tb], to, width * height)
        palette = span(banks[pb], po, PALETTE_BYTES)
        name, picture = _picture(width, height, pixels, palette)
        out.setdefault(name, picture)
    return out


def _level_hud(disc: Disc, info: dict) -> tuple[bytes, list[bytes]]:
    ranges = info["ranges"]
    data = disc.sectors(ranges["data"]["lba"], ranges["data"]["sectors"])
    wanted = {"hud_header", *(f"hud_bank_{i}" for i in range(BANKS))}
    sections: dict[str, bytes] = {}
    for i, name in enumerate(SECTION_NAMES):
        offset, size = unpack("<ii", data, i * 8)
        if size > 0 and name in wanted:
            sections[name] = decoded(span(data, offset, size))
    if "hud_header" not in sections:
        raise FormatError(f"level {info['id']} has no HUD header")
    banks = [sections.get(f"hud_bank_{i}", b"") for i in range(BANKS)]
    return sections["hud_header"], banks


def _global_hud(disc: Disc, survey: dict) -> tuple[bytes, list[bytes]] | None:
    refs = survey.get("global_references", [])
    headers = [r for r in refs if r.get("group") == "hud_header"]
    banks = sorted((r for r in refs if r.get("group") == "hud_banks"), key=lambda r: r.get("index", -1))
    if not headers or len(banks) < BANKS:
        return None
    h = headers[0]
    header = decoded(disc.read(h["lba"] * SECTOR, h["bytes"]))
    images = [decoded(disc.read(r["lba"] * SECTOR, r["bytes"])) for r in banks[:BANKS]]
    return header, images


def _stamp_ok(out: Path, identity: dict) -> dict | None:
    try:
        stamp = json.loads((out / STAMP).read_text())
    except (OSError, ValueError):
        return None
    for key, value in identity.items():
        if stamp.get(key) != value:
            return None
    pictures = list(out.glob("*.png"))
    if len(pictures) != stamp.get("textures"):
        return None
    if sum(p.stat().st_size for p in pictures) != stamp.get("bytes"):
        return None
    return stamp


def extract(iso: Path, out: Path, expected_version: str | None = None) -> dict:
    with Disc(iso) as disc:
        survey = disc.survey()
        if expected_version and survey["version"] != expected_version:
            raise FormatError(f"disc is {survey['version']}, expected {expected_version}")
        identity = {
            "format": FORMAT,
            "version": survey["version"],
            "discBytes": survey["disc_bytes"],
            "executableSha1": survey["executable_sha1"],
        }
        if cached := _stamp_ok(out, identity):
            return {**cached, "cached": True}

        pictures: dict[str, bytes] = {}
        for ref in survey.get("global_references", []):
            if ref.get("group") not in PIF_GROUPS or not ref.get("bytes"):
                continue
            blob = decoded(disc.read(ref["lba"] * SECTOR, ref["bytes"]))
            pictures.update(pif_pictures(blob))

        global_hud = _global_hud(disc, survey)
        if global_hud is not None:
            pictures.update(hud_pictures(*global_hud))
        for info in survey["levels"]:
            pictures.update(hud_pictures(*_level_hud(disc, info)))

    staging = out.with_name(f".{out.name}.building")
    shutil.rmtree(staging, ignore_errors=True)
    staging.mkdir(parents=True, exist_ok=True)
    for name, data in sorted(pictures.items()):
        (staging / f"{name}.png").write_bytes(data)
    byte_count = sum(p.stat().st_size for p in staging.glob("*.png"))
    stamp = {**identity, "textures": len(pictures), "bytes": byte_count}
    (staging / STAMP).write_text(json.dumps(stamp, indent=2) + "\n")
    shutil.rmtree(out, ignore_errors=True)
    staging.rename(out)
    return {**stamp, "cached": False}


def main(argv: list[str]) -> int:
    if len(argv) not in (3, 4):
        print("usage: offline_hud.py DISC.ISO OUTPUT [EXPECTED_VERSION]", file=sys.stderr)
        return 2
    try:
        result = extract(Path(argv[1]), Path(argv[2]), argv[3] if len(argv) == 4 else None)
    except (OSError, FormatError, ValueError) as e:
        print(f"offline HUD extraction failed: {e}", file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":")), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
