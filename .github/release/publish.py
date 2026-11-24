"""Publish only the exact selected tag and verified bytes; retry incomplete draft uploads."""
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from plan import attempt_jobs, check_release, config_at, judge, require


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
    found = None
    while True:
        releases = request(f"releases?per_page=100&page={page}")
        for release in releases:
            if release["tag_name"] == tag:
                require(found is None, "ambiguous existing release")
                found = release
        if len(releases) < 100:
            return found
        page += 1


def gh(*args):
    subprocess.run(["gh", *map(str, args), "--repo", repo], check=True)


def sums(directory, binary):
    names = [binary + "-macos-aarch64", binary + "-linux-x86_64"]
    expected = {name: hashlib.sha256((directory / name).read_bytes()).hexdigest() for name in names}
    lines = (directory / "SHA256SUMS").read_text().splitlines()
    judge("template-publication-v1", {"kind": "checksums", "lines": lines,
                                     "expected": [digest + "  " + name for name, digest in expected.items()]})
    expected["SHA256SUMS"] = hashlib.sha256((directory / "SHA256SUMS").read_bytes()).hexdigest()
    return expected


def check_ref(name, sha, reference):
    judge("template-publication-v1", {"kind": "ref", "source": sha, "name": "refs/" + name,
                                     "reference": reference})


def notes_match(release, expected):
    return json.loads(judge("template-publication-v1", {"kind": "notes", "release": release,
                                                        "expected": expected}))


def publication_state(tag, sha, prerelease, assets, release):
    return json.loads(judge("template-publication-v1", {"kind": "state", "source": sha, "tag": tag,
                            "reference": optional("git/ref/tags/" + tag), "prerelease": prerelease,
                            "release": release, "assets": assets}))


def binary_origins(config, sha, digests):
    """Tie selected native upload IDs and downloaded bytes to their producing attempts."""
    parent = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())["workflow_run"]
    check_id, check_attempt = parent["id"], parent["run_attempt"]
    run_id = int(os.environ["GITHUB_RUN_ID"])
    query = {
        "repository": repo, "binary": config["binary"], "source": sha, "workflow": config["check"]["file"],
        "check_id": check_id, "check_attempt": check_attempt,
        "check": request(f"actions/runs/{check_id}/attempts/{check_attempt}"),
        "run_id": run_id, "current_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]),
        "workflow_sha": os.environ["GITHUB_SHA"],
    }
    outputs = json.loads(os.environ["BUILD_RECEIPTS"])
    attempts = {}
    for key, platform in (("macos", "macos-aarch64"), ("linux", "linux-x86_64")):
        selected = json.loads(outputs[key])
        attempt, artifact_id = int(selected["attempt"]), int(selected["id"])
        if attempt not in attempts:
            path = f"actions/runs/{run_id}/attempts/{attempt}"
            attempts[attempt] = (request(path), attempt_jobs(repo, run_id, attempt))
        run, jobs = attempts[attempt]
        query[key] = {
            "selected_run": int(selected["run"]), "attempt": attempt, "artifact_id": artifact_id,
            "archive": selected["archive"], "file": selected["file"],
            "downloaded": digests[config["binary"] + "-" + platform], "run": run, "jobs": jobs,
            "artifact": request(f"actions/artifacts/{artifact_id}"),
        }
    return json.loads(judge("template-binary-origins-v1", query))


def main():
    mode, tag, sha = (os.environ[k] for k in ("MODE", "TAG", "SOURCE_SHA"))
    config = config_at(repo, sha)
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
        check_ref("tags/" + parent, sha, request("git/ref/tags/" + parent))
        if binary:
            gh("release", "download", parent, "--dir", dist)
    digests = sums(dist, binary) if binary else {}
    assets = {name: "sha256:" + digest for name, digest in digests.items()}
    origins = binary_origins(config, sha, digests) if binary and not stable else None
    if stable:
        check_release(request("releases/tags/" + parent), parent, True, False, assets)
    decision = publication_state(tag, sha, not stable, digests, release)
    if decision["action"] == "replay":
        if binary:
            with tempfile.TemporaryDirectory() as tmp:
                gh("release", "download", tag, "--dir", tmp)
                downloaded = sums(Path(tmp), binary)
                check_release(release, tag, not stable, False,
                              {name: "sha256:" + digest for name, digest in downloaded.items()})
        if stable:
            check_ref("heads/main", sha, request("git/ref/heads/main"))
        return
    if decision["create_tag"]:
        request("git/refs", method="POST", data=json.dumps({"ref": "refs/tags/" + tag, "sha": sha}).encode())
    check_ref("tags/" + tag, sha, request("git/ref/tags/" + tag))
    notes = ("Verified preview " + os.environ["PREVIEW"] + "\nConsumer acceptance: " + os.environ["PROOF"]
                 if stable else "Preview of " + sha)
    if origins:
        notes += "\n\nBuild origins\n\n```json\n" + json.dumps(origins, sort_keys=True, indent=2) + "\n```"
    if decision["action"] == "create":
        release = request("releases", method="POST", data=json.dumps({
            "tag_name":tag, "target_commitish":sha, "name":tag, "body":notes, "draft":True, "prerelease":not stable
        }).encode())
    address = f"releases/{release['id']}"
    current = request(address)
    decision = publication_state(tag, sha, not stable, digests, current)
    require(decision["action"] == "resume", "expected a verified draft")
    if origins and not notes_match(current, notes):
        request(address, method="PATCH", data=json.dumps({"body": notes}).encode())
    if decision["missing"]:
        gh("release", "upload", tag, *[dist / name for name in decision["missing"]])
    draft = request(address)
    check_release(draft, tag, not stable, True, assets)
    if origins:
        require(notes_match(draft, notes), "different build origins on draft")
    check_ref("tags/" + tag, sha, request("git/ref/tags/" + tag))
    if stable:
        main = request("git/ref/heads/main")
        if main["object"]["sha"] != sha:
            main = request("git/refs/heads/main", method="PATCH", data=json.dumps({"sha": sha, "force": False}).encode())
        check_ref("heads/main", sha, main)
    published = request(address, method="PATCH", data=b'{"draft":false}')
    check_release(published, tag, not stable, False, assets)


if __name__ == "__main__":
    main()
