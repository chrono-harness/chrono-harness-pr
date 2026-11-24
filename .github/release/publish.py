"""Publish only the exact selected tag and verified bytes; retry incomplete draft uploads."""
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from plan import check_run, require, sha_of


repo = os.environ["GITHUB_REPOSITORY"]


def request(path, **kwargs):
    req = Request("https://api.github.com/repos/" + repo + "/" + path,
                  headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"],
                           "Content-Type": "application/json", "Accept": "application/vnd.github+json"}, **kwargs)
    with urlopen(req, timeout=45) as reply:
        return json.load(reply)


def optional(path):
    try:
        return request(path)
    except HTTPError as err:
        if err.code == 404:
            return None
        raise


def by_tag(tag):
    public = optional("releases/tags/" + tag)
    if public is not None:
        return public
    page = 1
    while True:
        releases = request(f"releases?per_page=100&page={page}")
        for release in releases:
            if release["tag_name"] == tag:
                return release
        if len(releases) < 100:
            return None
        page += 1


def gh(*args):
    subprocess.run(["gh", *map(str, args), "--repo", repo], check=True)


def sums(directory, binary):
    names = [binary + "-macos-aarch64", binary + "-linux-x86_64"]
    expected = {name: hashlib.sha256((directory / name).read_bytes()).hexdigest() for name in names}
    lines = (directory / "SHA256SUMS").read_text().splitlines()
    require(sorted(lines) == sorted(digest + "  " + name for name, digest in expected.items()),
            "asset checksums differ from SHA256SUMS")
    expected["SHA256SUMS"] = hashlib.sha256((directory / "SHA256SUMS").read_bytes()).hexdigest()
    return expected


def tag_at(tag, sha):
    ref = optional("git/ref/tags/" + tag)
    if ref is None:
        request("git/refs", method="POST", data=json.dumps({"ref": "refs/tags/" + tag, "sha": sha}).encode())
    require(sha_of(repo, "tags/" + tag) == sha, "tag points to different source")


def recorded(release, tag, prerelease):
    require(release["tag_name"] == tag and release["prerelease"] == prerelease, "different release kind")
    require(all(a["state"] == "uploaded" for a in release["assets"]), "incomplete release assets")
    return {a["name"]: a["digest"].removeprefix("sha256:") for a in release["assets"]}


def binary_origins(config, sha, digests):
    """Tie selected native upload IDs and downloaded bytes to their producing attempts."""
    parent = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())["workflow_run"]
    check_id, check_attempt = parent["id"], parent["run_attempt"]
    check = request(f"actions/runs/{check_id}/attempts/{check_attempt}")
    check_run(check, repo, "dev", config["check"]["file"], sha)
    require((check["id"], check["run_attempt"]) == (check_id, check_attempt), "different check attempt")
    run_id = int(os.environ["GITHUB_RUN_ID"])
    current_attempt = int(os.environ["GITHUB_RUN_ATTEMPT"])
    workflow_sha = os.environ["GITHUB_SHA"]
    outputs = json.loads(os.environ["BUILD_RECEIPTS"])
    builds = {}
    jobs = {}
    for key, platform in (("macos", "macos-aarch64"), ("linux", "linux-x86_64")):
        selected = json.loads(outputs[key])
        attempt, artifact_id = int(selected["attempt"]), int(selected["id"])
        require(int(selected["run"]) == run_id and 0 < attempt <= current_attempt and artifact_id > 0,
                f"invalid producing run or attempt for {platform}")
        source = f"actions/runs/{run_id}/attempts/{attempt}"
        if attempt not in jobs:
            run = request(source)
            require(run["id"] == run_id and run["run_attempt"] == attempt
                    and run["repository"]["full_name"] == repo and run["head_repository"]["full_name"] == repo
                    and run["event"] == "workflow_run" and run["path"] == ".github/workflows/release.yml"
                    and run["head_branch"] == "dev" and run["head_sha"] == workflow_sha,
                    "different release workflow or producing attempt")
            jobs[attempt] = {job["name"]: job for job in request(source + "/jobs?per_page=100")["jobs"]}
        job = jobs[attempt].get("Build " + platform, {})
        steps = {step["name"] for step in job.get("steps", []) if step["conclusion"] == "success"}
        require(job.get("conclusion") == "success" and job.get("head_sha") == workflow_sha
                and {"Build and exercise the release binary", "Upload the exercised binary"} <= steps,
                f"missing successful product build or upload for {platform}")
        artifact = request(f"actions/artifacts/{artifact_id}")
        require(artifact["id"] == artifact_id and not artifact["expired"]
                and artifact["name"] == f"{platform}-{sha}-{attempt}"
                and artifact["workflow_run"]["id"] == run_id
                and artifact["workflow_run"]["head_sha"] == workflow_sha
                and artifact["digest"] == "sha256:" + selected["archive"].removeprefix("sha256:"),
                f"different original build artifact for {platform}")
        filename = config["binary"] + "-" + platform
        require(selected["file"] == digests[filename], f"downloaded bytes differ from build for {filename}")
        builds[filename] = {"run": run_id, "attempt": attempt, "job": job["id"],
                            "artifact": artifact_id, "archive_sha256": artifact["digest"],
                            "file_sha256": digests[filename]}
    return {"source": sha, "check": {"run": check_id, "attempt": check_attempt}, "builds": builds}


def main():
    mode, tag, sha = (os.environ[k] for k in ("MODE", "TAG", "SOURCE_SHA"))
    config = json.loads(Path(".chrono-harness/release.json").read_text())
    require(mode in ("preview", "stable"), "unsupported publication mode")
    require(config["kind"] in ("binary", "copier"), "unsupported product")
    binary = config["binary"] if config["kind"] == "binary" else None
    dist = Path(os.environ["DIST"])
    dist.mkdir(exist_ok=True)
    stable = mode == "stable"
    release = by_tag(tag)
    if not stable and binary:
        names = [binary + "-macos-aarch64", binary + "-linux-x86_64"]
        (dist / "SHA256SUMS").write_text("".join(hashlib.sha256((dist/name).read_bytes()).hexdigest()
                                                  + "  " + name + "\n" for name in names))
    if stable:
        parent = os.environ["PREVIEW"]
        require(sha_of(repo, "tags/" + parent) == sha, "preview tag moved")
        if binary:
            gh("release", "download", parent, "--dir", dist)
    digests = sums(dist, binary) if binary else {}
    origins = binary_origins(config, sha, digests) if binary and not stable else None
    if not stable and release and not release["draft"]:
        require(sha_of(repo, "tags/" + tag) == sha, "preview tag moved")
        with tempfile.TemporaryDirectory() as tmp:
            if binary:
                gh("release", "download", tag, "--dir", tmp)
            expected = sums(Path(tmp), binary) if binary else {}
            require(recorded(release, tag, True) == expected, "invalid published preview")
            require(digests == expected, "retry assets differ from the published preview")
        return
    if stable:
        preview = request("releases/tags/" + parent)
        require(not preview["draft"] and recorded(preview, parent, True) == digests, "preview bytes changed")
        if release:
            require(sha_of(repo, "tags/" + tag) == sha, "stable tag points to a different source")
            if not release["draft"]:
                require(recorded(release, tag, False) == digests, "stable assets differ from verified preview")
                require(sha_of(repo, "heads/main") == sha, "main differs from published stable")
                return
        # GitHub rejects a non-fast-forward update. Check again at the point of publication.
        if sha_of(repo, "heads/main") != sha:
            request("git/refs/heads/main", method="PATCH", data=json.dumps({"sha": sha, "force": False}).encode())
        require(sha_of(repo, "heads/main") == sha, "main differs from preview")
    tag_at(tag, sha)
    notes = ("Verified preview " + os.environ["PREVIEW"] + "\nConsumer acceptance: " + os.environ["PROOF"]
                 if stable else "Preview of " + sha)
    if origins:
        notes += "\n\nBuild origins\n\n```json\n" + json.dumps(origins, sort_keys=True, indent=2) + "\n```"
    if release is None:
        release = request("releases", method="POST", data=json.dumps({
            "tag_name":tag, "name":tag, "body":notes, "draft":True, "prerelease":not stable
        }).encode())
    else:
        require(release["draft"], "expected an incomplete draft")
        if origins and release.get("body") != notes:
            request(f"releases/{release['id']}", method="PATCH", data=json.dumps({"body": notes}).encode())
    if digests:
        gh("release", "upload", tag, *[dist / n for n in digests], "--clobber")
    address = f"releases/{release['id']}"
    draft = request(address)
    require(draft["draft"] and recorded(draft, tag, not stable) == digests, "draft differs from checked assets")
    if origins:
        require(draft.get("body") == notes, "different build origins on draft")
    published = request(address, method="PATCH", data=b'{"draft":false}')
    require(not published["draft"] and recorded(published, tag, not stable) == digests, "publication differs")


if __name__ == "__main__":
    main()
