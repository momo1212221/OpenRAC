"""The Ratchet skin wrapper, using assets.py's synthetic no-disc fixtures."""

import json
from pathlib import Path
import tempfile
import unittest

import skins
from test_assets import write_levels, rgb_png


class SkinTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.levels = write_levels(self.root)

    def tearDown(self):
        self.tmp.cleanup()

    def test_finds_ratchet_levels(self):
        self.assertEqual(skins.ratchet_levels(self.levels), [0, 1])

    def test_export_is_ratchet_only(self):
        out = self.root / "skin"
        result = skins.export_skin(self.levels, out)
        self.assertEqual(result["levels"], [0, 1])
        marker = json.loads((out / "skin.json").read_text())
        self.assertEqual((marker["kind"], marker["class"]), ("openrac-ratchet-skin-workspace", 0))
        self.assertTrue((out / "level_00/mobys/moby_0_ratchet/asset.json").is_file())
        self.assertFalse((out / "level_00/ties").exists())

    def test_changed_texture_builds_a_normal_pack(self):
        work = self.root / "skin"
        skins.export_skin(self.levels, work, [0])
        texture = work / "level_00/textures/moby_0003.png"
        texture.write_bytes(rgb_png(16, 8, (255, 120, 0)))
        out = self.root / "pack"
        written, report = skins.pack_skin(
            work,
            self.levels,
            out=out,
            into=None,
            name="Orange Ratchet",
            author="test",
            description="",
            strict=True,
        )
        self.assertEqual(written, out.resolve())
        self.assertEqual(report.refused, 0)
        manifest = json.loads((out / "openrac-mod.json").read_text())
        self.assertEqual((manifest["format"], manifest["game"], manifest["name"]), (1, "rac1", "Orange Ratchet"))
        self.assertTrue(any(out.glob("level_*/textures/*.png")))

    def test_rejects_non_skin_workspace(self):
        empty = self.root / "empty"
        empty.mkdir()
        with self.assertRaisesRegex(Exception, "skin.json"):
            skins.validate_workspace(empty)


if __name__ == "__main__":
    unittest.main()
