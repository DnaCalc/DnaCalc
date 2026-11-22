import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("usage_ledger.py")
SWEEP = """# DNA Calc sweep 2026-11-22 07:58
## Attention
- ENDED alpha/reused (2 min ago)
## alpha
- **reused** running loop123; output 0 min ago; loop: run 1 start (usage 20.0, model gpt-6.2-sol high)
- **ended** ended; output 2 min ago; loop: done
- CODEX 2026-11-22T05:57:41.415Z 40.0 1795774021 54237.3941980000
## beta
- **reused** running 123/09:38; output 0 min ago; loop:
- **unmapped** running 124/09:38; output 0 min ago; loop:
- CODEX 2026-11-22T05:58:23.601Z 39.0 1795774021 54237.3941980000
## local (example)
- Gemini remaining (weekly, five-hour, Claude/GPT group): 69.83, 97.00, 100.00
- Claude usage: read it in the Opus session (get_usage)
"""


class UsageLedgerTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.directory = Path(folder.name)

    def write_sweep(self, name, content):
        (self.directory / name).write_text(content, encoding="utf-8")

    def run_ledger(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), str(self.directory), *args],
            capture_output=True, text=True, check=False,
        )

    def test_records_readings_in_order_with_source_provenance_and_project(self):
        self.write_sweep("status-20261122-0758.md", SWEEP)
        self.write_sweep(
            "status-20261122-0658.md",
            SWEEP.replace("07:58", "06:58")
            .replace("69.83, 97.00, 100.00", "unread")
            .replace(
                "Claude usage: read it in the Opus session (get_usage)",
                "Claude used (five-hour, weekly, Fable): 10, 32.5, 0",
            ),
        )
        self.write_sweep("unrelated.md", "ignored")
        projects = self.directory / "projects.json"
        projects.write_text(
            json.dumps({"alpha/reused": "DNA Calc", "beta/reused": "CalcVBA"}),
            encoding="utf-8",
        )
        result = self.run_ledger("--projects", str(projects))
        self.assertEqual(result.returncode, 0, result.stderr)
        old, latest = map(json.loads, result.stdout.splitlines())
        self.assertEqual(old["sweep_local"], "2026-11-22T06:58")
        self.assertIsNone(old["gemini"])
        self.assertEqual(
            old["claude"]["used_pct"],
            {"five_hour": 10, "weekly": 32.5, "fable": 0},
        )
        self.assertEqual(latest["source"], "status-20261122-0758.md")
        self.assertIsNone(latest["claude"])
        self.assertEqual(
            latest["gemini"]["used_pct"],
            {"weekly": 30.17, "five_hour": 3, "claude_gpt_group": 0},
        )
        self.assertEqual(latest["codex"]["latest"]["host"], "beta")
        self.assertEqual(latest["codex"]["latest"]["weekly_used_pct"], 39)
        self.assertEqual(
            [(row["host"], row["weekly_used_pct"]) for row in latest["codex"]["readings"]],
            [("alpha", 40), ("beta", 39)],
        )
        self.assertEqual(latest["codex"]["readings"][1]["reset_epoch"], 1795774021)
        self.assertEqual(
            [(row["folder"], row["project"], row["model"]) for row in latest["running_lanes"]],
            [("alpha/reused", "DNA Calc", "gpt-6.2-sol high"),
             ("beta/reused", "CalcVBA", None),
             ("beta/unmapped", None, None)],
        )

    def test_gap_keeps_readings_unavailable(self):
        self.write_sweep("status-20261122-0758.md", SWEEP.split("## Attention")[0])
        result = self.run_ledger()
        self.assertEqual(result.returncode, 0, result.stderr)
        row = json.loads(result.stdout)
        for provider in ("codex", "claude", "gemini"):
            self.assertIsNone(row[provider])

    def test_rejects_malformed_recognized_reading_without_partial_output(self):
        self.write_sweep("status-20261122-0757.md", SWEEP.replace("07:58", "07:57"))
        self.write_sweep("status-20261122-0758.md", SWEEP.replace("69.83", "169.83"))
        result = self.run_ledger()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("status-20261122-0758.md", result.stderr)
        self.assertIn("Gemini", result.stderr)

    def test_conflicting_latest_codex_readings_are_ambiguous(self):
        self.write_sweep(
            "status-20261122-0758.md",
            SWEEP.replace("05:57:41.415Z", "05:58:23.601Z"),
        )
        result = self.run_ledger()
        self.assertEqual(result.returncode, 0, result.stderr)
        row = json.loads(result.stdout)
        self.assertIsNone(row["codex"]["latest"])
        self.assertEqual(len(row["codex"]["readings"]), 2)


if __name__ == "__main__":
    unittest.main()
