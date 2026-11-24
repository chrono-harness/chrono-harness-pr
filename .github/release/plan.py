"""Select the verified commit and release using GitHub and the consumer's landed lockfile."""
import base64
import json
import os
import re
import subprocess
from pathlib import Path
from urllib.parse import urlencode


def require(ok, message):
    if not ok:
        raise ValueError(message)


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path]))


def config_at(repository, sha):
    require(re.fullmatch(r"[0-9a-f]{40}", sha), "release config requires a commit SHA")
    doc = api(f"repos/{repository}/contents/.chrono-harness/release.json?ref={sha}")
    return json.loads(base64.b64decode(doc["content"]))


def judge(command, request):
    result = subprocess.run(["chrono-host-release", command], input=json.dumps(request),
                            text=True, capture_output=True)
    if result.returncode:
        raise ValueError(result.stderr.strip() or f"{command} exited {result.returncode}")
    return result.stdout


def check_run(run, repository, branch, workflow, sha):
    judge("template-run-v1", {"run": run, "repository": repository, "branch": branch,
                              "workflow": workflow, "sha": sha})


def successful_run(repository, branch, workflow, sha):
    query = urlencode({"event": "push", "branch": branch, "head_sha": sha, "per_page": 100})
    path = f"repos/{repository}/actions/workflows/{workflow}/runs?{query}"
    for run in api(path)["workflow_runs"]:
        if run["status"] == "completed" and run["conclusion"] == "success":
            check_run(run, repository, branch, workflow, sha)
            return run
    raise ValueError(f"no successful {workflow} push for {repository}@{sha}")


def check_acceptance(repository, run, consumer):
    path = f"repos/{repository}/actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100"
    judge("template-acceptance-v1", {"jobs": api(path)["jobs"], "sha": run["head_sha"],
                                     "required_jobs": consumer["jobs"], "step": consumer["step"]})


def sha_of(repository, ref):
    obj = api(f"repos/{repository}/git/ref/{ref}")["object"]
    require(obj["type"] == "commit" and re.fullmatch(r"[0-9a-f]{40}", obj["sha"]),
            f"{ref} must point directly to a commit")
    return obj["sha"]


def pin_manifest(repository, binary, version, tool):
    tag, checksums = json.loads(judge("template-consumer-pin-v1", {
        "kind": "binary", "repository": repository, "binary": binary, "version": version, "tool": tool}))
    return tag, checksums


def check_release(release, tag, prerelease, draft, assets):
    judge("template-release-v1", {"release": release, "tag": tag, "prerelease": prerelease,
                                  "draft": draft, "assets": assets})


def public_preview(repository, tag, checksums):
    release = api(f"repos/{repository}/releases/tags/{tag}")
    assets = {} if checksums is None else {**checksums, "SHA256SUMS": None}
    check_release(release, tag, True, False, assets)
    return release


def consumer_pin(config, repository, sha):
    consumer = config["consumer"]
    doc = api(f"repos/{consumer['repository']}/contents/{consumer['pin']}?ref={sha}")
    raw = base64.b64decode(doc["content"]).decode()
    if config["kind"] == "copier":
        import yaml
        answers = yaml.safe_load(raw)
        tag, checksums = json.loads(judge("template-consumer-pin-v1", {
            "kind": "copier", "repository": repository, "version": config["version"],
            "rolling": config.get("version_policy") == "next_patch", "answers": answers}))
        return tag, checksums
    import tomllib
    binary = config["binary"]
    tool = tomllib.loads(raw)["tools"]["http:" + binary]
    return pin_manifest(repository, binary, config["version"], tool)


CORE = r"(0|[1-9][0-9]*)[.](0|[1-9][0-9]*)[.](0|[1-9][0-9]*)"


def latest_source_release(repository):
    rows = []
    page = 1
    while True:
        releases = api(f"repos/{repository}/releases?per_page=100&page={page}")
        rows.extend({key: release[key] for key in ("tag_name", "draft", "prerelease", "published_at")}
                    for release in releases)
        if len(releases) < 100:
            break
        page += 1
    tag = json.loads(judge("template-latest-stable-v1", rows))
    if tag is None:
        return None
    sha = sha_of(repository, "tags/" + tag)
    require(api(f"repos/{repository}/compare/{sha}...main")["status"] in ("identical", "ahead"),
            "main does not contain the latest source release")
    return tag, sha


def next_target(config, repository):
    policy = config.get("version_policy", "explicit")
    require(policy in ("explicit", "next_patch"), "unsupported version policy")
    if policy == "explicit":
        return config, None
    require(config["kind"] == "copier", "next_patch requires a Copier source product")
    latest = latest_source_release(repository)
    result = judge("template-next-version-v1", {"floor": config["version"],
                                                "latest": latest[0][1:] if latest else None})
    return dict(config, version=json.loads(result)), latest


def already_released(repository, latest, sha):
    if latest:
        tag, released = latest
        relation = api(f"repos/{repository}/compare/{released}...{sha}")["status"]
        require(relation != "diverged", "candidate diverges from the latest stable release")
        if relation in ("identical", "behind"):
            return {"mode": "complete", "sha": released, "tag": tag}
        require(relation == "ahead", "invalid release ancestry")
    return None


def plan(config, repository, event, event_name):
    require(config["kind"] in ("copier", "binary"), "unsupported release product")
    check_workflow = config["check"]["file"]
    require(re.fullmatch(CORE, config["version"]), "invalid release version")
    require(event["repository"]["full_name"] == repository, "wrong event repository")
    source_config = config
    config, latest = next_target(config, repository)
    version = config["version"]
    if event_name == "workflow_run":
        run = event["workflow_run"]
        sha, branch = run["head_sha"], run["head_branch"]
        require(branch in ("dev", "main"), "unsupported release branch")
        check_run(run, repository, branch, check_workflow, sha)
        completed = already_released(repository, latest, sha)
        if completed:
            return completed
        if branch == "dev":
            require(re.fullmatch(r"[a-f0-9]{40}", sha), "invalid source commit")
            relation = api(f"repos/{repository}/compare/{sha}...dev")["status"]
            require(relation in ("identical", "ahead"), "development branch does not contain this check")
            stable_tag = "v" + version
            path = f"repos/{repository}/git/matching-refs/tags/{stable_tag}"
            if any(ref["ref"] == "refs/tags/" + stable_tag for ref in api(path)):
                stable = api(f"repos/{repository}/releases/tags/{stable_tag}")
                if not stable["draft"] and not stable["prerelease"]:
                    require(stable["tag_name"] == stable_tag, "different stable release")
                    released = sha_of(repository, "tags/" + stable_tag)
                    require(api(f"repos/{repository}/compare/{released}...main")["status"] in ("identical", "ahead"),
                            "main does not contain the published stable release")
                    return {"mode": "complete", "sha": released, "tag": stable_tag}
            return {"mode": "preview", "sha": sha, "tag": f"v{version}-alpha.{run['run_number']}"}
    else:
        require(event_name == "workflow_dispatch" and event["ref"] == "refs/heads/dev", "dispatch from dev")
        sha = None

    consumer = config["consumer"]
    downstream, branch = consumer["repository"], consumer["branch"]
    adopted = sha_of(downstream, "heads/" + branch)
    run = successful_run(downstream, branch, consumer["workflow"], adopted)
    check_acceptance(downstream, run, consumer)
    tag, checksums = consumer_pin(config, repository, adopted)
    selected = sha_of(repository, "tags/" + tag)
    completed = already_released(repository, latest, selected)
    if completed:
        return completed
    require(sha is None or sha == selected, "main differs from the adopted preview")
    if event_name == "workflow_dispatch":
        require(config_at(repository, selected) == source_config, "adopted preview has a different release config")
    origin = successful_run(repository, "dev", check_workflow, selected)
    require(tag == f"v{version}-alpha.{origin['run_number']}", "tag differs from the verified dev run")
    require(api(f"repos/{repository}/compare/{selected}...dev")["status"] in ("identical", "ahead"),
            "preview is outside the development branch")
    require(api(f"repos/{repository}/compare/main...{selected}")["status"] in ("identical", "ahead"),
            "main cannot fast-forward to the preview")
    public_preview(repository, tag, checksums)
    return {"mode": "stable", "sha": selected, "tag": "v" + version, "preview": tag,
            "consumer": downstream, "adopted": adopted, "proof": run["html_url"]}


def event_plan(repository, event, event_name, workflow_sha):
    source = event["workflow_run"]["head_sha"] if event_name == "workflow_run" else workflow_sha
    return plan(config_at(repository, source), repository, event, event_name)


if __name__ == "__main__":
    result = event_plan(os.environ["GITHUB_REPOSITORY"], json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()),
                        os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_SHA"])
    print(json.dumps(result))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        for key, value in result.items():
            print(f"{key}={value}", file=output)
