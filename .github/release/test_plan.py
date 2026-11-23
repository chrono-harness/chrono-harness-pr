import copy
import unittest
from unittest.mock import patch

import plan


repository = "chrono-harness/chrono-harness-pr"
sha = "a"*40
version = "0.1.0"
config = {"version":version, "binary":"chrono-pr"}
run = {"repository":{"full_name":repository}, "head_repository":{"full_name":repository},
       "path":".github/workflows/check.yml", "event":"push", "status":"completed", "conclusion":"success",
       "head_sha":sha, "head_branch":"dev", "run_number":15}
event = {"repository":{"full_name":repository}, "workflow_run":run}


class ReleasePlan(unittest.TestCase):
    @patch.object(plan, "api", side_effect=lambda path: {"status":"identical"} if "/compare/" in path else [])
    def test_preview_rejects_wrong_events(self, github):
        selected = plan.plan(config, repository, event, "workflow_run")
        self.assertEqual(selected, {"mode":"preview", "sha":sha, "tag":"v0.1.0-alpha.15"})
        for key, value in (("repository", {"full_name":"fork/pr"}), ("head_repository", {"full_name":"fork/pr"}),
                           ("event","pull_request"), ("status","in_progress"), ("conclusion","failure"),
                           ("path",".github/workflows/unrelated.yml"), ("head_branch","feature")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                plan.plan(config, repository, dict(event, workflow_run=dict(run, **{key:value})), "workflow_run")
        github.side_effect = lambda path: {"status":"diverged"}
        with self.assertRaisesRegex(ValueError, "does not contain"):
            plan.plan(config, repository, event, "workflow_run")

    def test_real_draft_api_shape_uses_the_release_id(self):
        import os
        with patch.dict(os.environ, {"GITHUB_REPOSITORY":repository}):
            import publish
        draft={"id":455959087,"tag_name":"v0.1.0-alpha.29","draft":True,"assets":[]}
        with patch.object(publish, "optional", return_value=None), patch.object(publish, "request",
                side_effect=[[{"tag_name":"other"}]*100,[draft]]) as pages:
            self.assertEqual(publish.by_tag(draft["tag_name"]), draft)
            self.assertEqual(pages.call_args.args, ("releases?per_page=100&page=2",))

    def test_published_stable_finishes_the_plan(self):
        with patch.object(plan, "api", side_effect=[{"status":"ahead"}, [{"ref":"refs/tags/v0.1.0"}],
                                                    {"draft":False,"prerelease":False}]):
            self.assertEqual(plan.plan(config, repository, event, "workflow_run")["mode"], "complete")

    def test_validates_both_downloads(self):
        digest = "sha256:"+"c"*64
        prefix = f"https://github.com/{repository}/releases/download/v0.1.0-alpha.15/chrono-pr-"
        tools = {"bin":"chrono-pr", "version":"0.1.0-alpha.15", "platforms":{
            "linux-x64":{"url":prefix+"linux-x86_64","checksum":digest},
            "macos-arm64":{"url":prefix+"macos-aarch64","checksum":digest}}}
        tag, checks = plan.pin_manifest(repository, "chrono-pr", version, tools)
        self.assertEqual(tag, "v0.1.0-alpha.15")
        for platform in tools["platforms"]:
            wrong = copy.deepcopy(tools)
            wrong["platforms"][platform]["url"] += "other"
            with self.assertRaises(ValueError):
                plan.pin_manifest(repository, "chrono-pr", version, wrong)
        release = {"tag_name":tag,"draft":False,"prerelease":True,
                   "assets":[{"name":name,"state":"uploaded","digest":digest}
                             for name in [*checks,"SHA256SUMS"]]}
        with patch.object(plan, "api", return_value=release):
            self.assertEqual(plan.public_preview(repository, tag, checks), release)
            original_assets = release["assets"]
            release["assets"] = []
            with self.assertRaisesRegex(ValueError, "missing"):
                plan.public_preview(repository, tag, checks)
            release["assets"] = original_assets
            release["assets"][0]["digest"]="sha256:"+"0"*64
            with self.assertRaisesRegex(ValueError, "checksums"):
                plan.public_preview(repository, tag, checks)

    def test_skipped_or_wrong_platform_does_not_prove_adoption(self):
        consumer={"jobs":["mac","linux"], "step":"exercise"}
        job={"name":"mac","conclusion":"success","head_sha":sha,
             "steps":[{"name":"exercise","conclusion":"success"}]}
        checked=dict(run,id=1,run_attempt=1)
        jobs=[job,dict(job,name="linux")]
        with patch.object(plan,"api",return_value={"jobs":jobs}):
            plan.check_acceptance("example/org",checked,consumer)
            jobs[1] = dict(job,name="linux",steps=[{"name":"exercise","conclusion":"skipped"}])
            with self.assertRaisesRegex(ValueError, "missing exercised"):
                plan.check_acceptance("example/org",checked,consumer)
            jobs.pop()
            with self.assertRaisesRegex(ValueError,"missing successful"):
                plan.check_acceptance("example/org",checked,consumer)

    def test_dispatch_requires_landed_consumer(self):
        consumer = {"branch":"dev","workflow":"pr-adoption.yml","repository":"example/org","pin":"lock.toml"}
        with patch.object(plan,"sha_of",return_value=sha), patch.object(plan,"api",return_value={"workflow_runs":[]}):
            with self.assertRaisesRegex(ValueError, "no successful"):
                plan.plan(dict(config, consumer=consumer), repository,
                          {"ref":"refs/heads/dev","repository":{"full_name":repository}}, "workflow_dispatch")

    def test_successful_other_candidate_cannot_release_main(self):
        consumer = {"branch":"dev","workflow":"pr-adoption.yml","repository":"example/org","pin":"lock.toml"}
        main = dict(event, workflow_run=dict(run, head_branch="main"))
        with patch.object(plan,"sha_of",return_value="b"*40), patch.object(plan,"successful_run",return_value=run), \
                patch.object(plan,"installed_tool",return_value={}), \
                patch.object(plan,"check_acceptance"), patch.object(plan,"pin_manifest",return_value=("v0.1.0-alpha.15", {})):
            with self.assertRaisesRegex(ValueError, "main differs"):
                plan.plan(dict(config, consumer=consumer), repository, main, "workflow_run")


if __name__ == "__main__":
    unittest.main()
