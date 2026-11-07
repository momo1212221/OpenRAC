"""The lighting log viewer only observes the running bake."""
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace
from follow_bake import follow

class FollowBake(unittest.TestCase):
    def test_reads_existing_log_and_only_queries_status(self):
        with tempfile.TemporaryDirectory() as folder:
            cache = Path(folder)
            (cache / "probe-bake.log").write_text("Prepared lighting\n")
            output = io.StringIO()
            with patch("follow_bake.subprocess.run", return_value=SimpleNamespace(stdout="idle\n")) as run:
                follow(cache, Path("baker"), output=output)
            self.assertIn("Prepared lighting", output.getvalue())
            self.assertIn("no longer running", output.getvalue())
            self.assertEqual(run.call_args.args[0], ["baker", "--cache-status", str(cache)])
            self.assertTrue(run.call_args.kwargs["check"])

    def test_rejects_unknown_status(self):
        with tempfile.TemporaryDirectory() as folder:
            cache = Path(folder)
            (cache / "probe-bake.log").write_text("")
            with patch("follow_bake.subprocess.run", return_value=SimpleNamespace(stdout="unknown")):
                with self.assertRaisesRegex(RuntimeError, "unknown cache status"):
                    follow(cache, Path("baker"), output=io.StringIO())
