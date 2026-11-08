import json
import struct
import tempfile
import unittest
from pathlib import Path

import offline_hud


class OfflineHudTests(unittest.TestCase):
    def fixture(self):
        header = bytearray(0xD0)
        struct.pack_into('<HH', header, 0, 1, 1)
        struct.pack_into('<4I', header, 4, 0xB4, 0xBC, 0xC0, 0xC8)
        for i in range(8):
            struct.pack_into('<I', header, 0x14 + i * 4, 1)
            struct.pack_into('<I', header, 0x34 + i * 4, 1)
        struct.pack_into('<I', header, 0x54, 1026)
        struct.pack_into('<H', header, 0xB4, 0xFFFF)
        struct.pack_into('<hh', header, 0xBC, 0, 0)
        struct.pack_into('<I', header, 0xC0, 0)
        struct.pack_into('<I', header, 0xC8, 1024)
        header[0xC8 + 6] = 1  # 2 wide
        header[0xC8 + 7] = 0  # 1 high

        bank = bytearray(1026)
        # CSM1 index 8 reads stored palette entry 16.
        bank[16 * 4:16 * 4 + 4] = bytes((10, 20, 30, 0x80))
        bank[1024:] = bytes((8, 0))
        return bytes(header), [bytes(bank), b'', b'', b'', b'']

    def test_hud_picture_uses_raw_ps2_alpha_and_renderer_hash(self):
        pictures = offline_hud.hud_pictures(*self.fixture())
        rgba = bytes((10, 20, 30, 0x80, 0, 0, 0, 0))
        expected = offline_hud.texture_hash(2, 1, rgba)
        self.assertEqual(set(pictures), {expected})
        image = pictures[expected]
        # tRNS is present and keeps alpha 0x80, rather than display-scaling it to 0xff.
        at = image.index(b'tRNS') + 4
        self.assertEqual(image[at + 8], 0x80)

    def test_pif_scanner_keeps_raw_alpha_and_rejects_bad_headers(self):
        pif = bytearray(0x420 + 4)
        pif[:4] = b"2FIP"
        struct.pack_into('<III', pif, 8, 2, 2, 0x13)
        # CSM1 index 8 comes from stored entry 16.
        pif[0x20 + 16 * 4:0x20 + 16 * 4 + 4] = bytes((1, 2, 3, 0x40))
        pif[0x420:] = bytes((8, 8, 8, 8))
        rgba = bytes((1, 2, 3, 0x40)) * 4
        expected = offline_hud.texture_hash(2, 2, rgba)
        self.assertEqual(set(offline_hud.pif_pictures(bytes(pif))), {expected})
        pif[0x10:0x14] = (0x14).to_bytes(4, 'little')
        self.assertEqual(offline_hud.pif_pictures(bytes(pif)), {})

    def test_stamp_requires_the_exact_generated_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            identity = {'format': 1, 'version': 'pal', 'discBytes': 3, 'executableSha1': 'x'}
            picture = root / '0123456789abcdef.png'
            picture.write_bytes(b'png')
            stamp = {**identity, 'textures': 1, 'bytes': 3}
            (root / offline_hud.STAMP).write_text(json.dumps(stamp))
            self.assertIsNotNone(offline_hud._stamp_ok(root, identity))
            picture.write_bytes(b'changed')
            self.assertIsNone(offline_hud._stamp_ok(root, identity))


if __name__ == '__main__':
    unittest.main()
