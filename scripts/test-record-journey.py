import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class JourneyRecording(unittest.TestCase):
    def test_exit_status_and_private_command_data(self):
        recorder = Path(__file__).with_name("record-journey.py")
        with tempfile.TemporaryDirectory() as temporary:
            record = Path(temporary) / "events.jsonl"
            for code in [0, 7]:
                result = subprocess.run([sys.executable, str(recorder), "--stage", "import", "--record", str(record), "--help-needed", "--", sys.executable, "-c", f"print('private-command-output'); raise SystemExit({code})", "private-argument"], env=dict(os.environ, PRIVATE_JOURNEY_ENV="private-env"), capture_output=True)
                self.assertEqual(result.returncode, code)
            raw = record.read_text()
            self.assertNotIn("private", raw)
            rows = [json.loads(line) for line in raw.splitlines()]
            self.assertEqual([row["exit_code"] for row in rows], [0, 7])
            self.assertEqual(rows[1]["reason"], "unknown")
            self.assertTrue(rows[1]["help_needed"])
            self.assertGreaterEqual(rows[0]["elapsed_seconds"], 0)


if __name__ == "__main__":
    unittest.main()
