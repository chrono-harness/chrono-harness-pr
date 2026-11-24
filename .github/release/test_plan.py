"""Exercise chrono-pr's own release configuration with the installed CLI boundary mocked."""
import json
import os
import subprocess
import unittest
from pathlib import Path
from unittest.mock import patch

import plan


class ReleasePlan(unittest.TestCase):
    def test_dev_check_selects_the_host_binary_preview(self):
        root = Path(__file__).resolve().parents[2]
        config = json.loads((root / ".chrono-harness/release.json").read_text())
        self.assertEqual(config["kind"], "binary")
        self.assertEqual(config["binary"], "chrono-pr")
        self.assertTrue(os.access(root / config["build"], os.X_OK))
        workflow = config["check"]["file"]
        self.assertTrue((root / ".github/workflows" / workflow).is_file())

        repository = "chrono-harness/chrono-harness-pr"
        sha = "a" * 40
        run = {"head_sha": sha, "head_branch": "dev", "run_number": 15}
        event = {"repository": {"full_name": repository}, "workflow_run": run}
        stable = "v" + config["version"]
        responses = {
            f"repos/{repository}/compare/{sha}...dev": {"status": "identical"},
            f"repos/{repository}/git/matching-refs/tags/{stable}": [],
        }
        with patch.object(plan, "api", side_effect=responses.__getitem__), \
                patch.object(plan.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as owner:
            selected = plan.plan(config, repository, event, "workflow_run")
        self.assertEqual(selected, {"mode": "preview", "sha": sha, "tag": stable + "-alpha.15"})
        checked, contained = owner.call_args_list
        self.assertEqual(checked.args, (["chrono-host-release", "template-run-v1"],))
        self.assertEqual(json.loads(checked.kwargs["input"]), {
            "run": run, "repository": repository, "branch": "dev", "workflow": workflow, "sha": sha,
        })
        self.assertEqual(contained.args, (["chrono-host-release", "template-promotion-v1"],))
        self.assertEqual(json.loads(contained.kwargs["input"]), {
            "kind": "contains", "comparison": {"status": "identical"},
        })


if __name__ == "__main__":
    unittest.main()
