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
        expected = [
            ("template-promotion-v1", {
                "kind": "event", "repository": repository, "event_name": "workflow_run",
                "event": {"repository": event["repository"], "ref": None, "workflow_run": {"head_branch": "dev"}},
            }, '"dev"'),
            ("template-promotion-v1", {"kind": "version-policy", "config": config}, "false"),
            ("template-run-v1", {
                "run": run, "repository": repository, "branch": "dev", "workflow": workflow, "sha": sha,
            }, ""),
            ("template-promotion-v1", {"kind": "contains", "comparison": {"status": "identical"}}, ""),
            ("template-promotion-v1", {"kind": "tag", "tag": stable, "refs": []}, "false"),
        ]

        def respond(argv, *, input, **kwargs):
            command, request, output = expected.pop(0)
            self.assertEqual(argv, ["chrono-host-release", command])
            self.assertEqual(json.loads(input), request)
            return subprocess.CompletedProcess(argv, 0, output, "")

        with patch.object(plan, "api", side_effect=responses.__getitem__), \
                patch.object(plan.subprocess, "run", side_effect=respond):
            selected = plan.plan(config, repository, event, "workflow_run")
        self.assertEqual(selected, {"mode": "preview", "sha": sha, "tag": stable + "-alpha.15"})
        self.assertEqual(expected, [])


if __name__ == "__main__":
    unittest.main()
