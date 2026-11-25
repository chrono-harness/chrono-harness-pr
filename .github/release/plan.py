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
            return run
    raise ValueError(f"no successful {workflow} push for {repository}@{sha}")


def attempt_jobs(repository, run_id, attempt):
    path = f"repos/{repository}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100"
    pages = json.loads(subprocess.check_output(["gh", "api", "--paginate", "--slurp", path]))
    return [job for page in pages for job in page["jobs"]]


def check_acceptance(repository, run, consumer):
    jobs = attempt_jobs(repository, run["id"], run["run_attempt"])
    judge("template-acceptance-v1", {"jobs": jobs, "sha": run["head_sha"],
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


def compare(repository, base, head):
    return api(f"repos/{repository}/compare/{base}...{head}")


def contains(repository, base, head):
    judge("template-promotion-v1", {"kind": "contains", "comparison": compare(repository, base, head)})


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
    contains(repository, sha, "main")
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
        covered = judge("template-promotion-v1", {"kind": "candidate", "comparison": compare(repository, released, sha)})
        if json.loads(covered):
            return {"mode": "complete", "sha": released, "tag": tag}
    return None


def event_route(repository, event, event_name):
    return json.loads(judge("template-promotion-v1", {
        "kind": "event", "repository": repository, "event_name": event_name,
        "event": {"repository": event.get("repository"), "ref": event.get("ref"),
                  "workflow_run": {"head_branch": event.get("workflow_run", {}).get("head_branch")}}
    }))


def plan(config, repository, event, event_name, *, route=None):
    if route is None:
        route = event_route(repository, event, event_name)
    require(config["kind"] in ("copier", "binary"), "unsupported release product")
    check_workflow = config["check"]["file"]
    require(re.fullmatch(CORE, config["version"]), "invalid release version")
    source_config = config
    config, latest = next_target(config, repository)
    version = config["version"]
    if route in ("dev", "main"):
        run = event["workflow_run"]
        sha, branch = run["head_sha"], run["head_branch"]
        check_run(run, repository, branch, check_workflow, sha)
        completed = already_released(repository, latest, sha)
        if completed:
            return completed
        if route == "dev":
            require(re.fullmatch(r"[a-f0-9]{40}", sha), "invalid source commit")
            contains(repository, sha, "dev")
            stable_tag = "v" + version
            path = f"repos/{repository}/git/matching-refs/tags/{stable_tag}"
            if json.loads(judge("template-promotion-v1", {"kind": "tag", "tag": stable_tag, "refs": api(path)})):
                stable = api(f"repos/{repository}/releases/tags/{stable_tag}")
                complete = judge("template-promotion-v1", {"kind": "explicit", "release": stable, "tag": stable_tag})
                if json.loads(complete):
                    released = sha_of(repository, "tags/" + stable_tag)
                    contains(repository, released, "main")
                    return {"mode": "complete", "sha": released, "tag": stable_tag}
            return {"mode": "preview", "sha": sha, "tag": f"v{version}-alpha.{run['run_number']}"}
    else:
        sha = None

    consumer = config["consumer"]
    downstream, branch = consumer["repository"], consumer["branch"]
    adopted = sha_of(downstream, "heads/" + branch)
    run = successful_run(downstream, branch, consumer["workflow"], adopted)
    check_run(run, downstream, branch, consumer["workflow"], adopted)
    check_acceptance(downstream, run, consumer)
    tag, checksums = consumer_pin(config, repository, adopted)
    selected = sha_of(repository, "tags/" + tag)
    completed = already_released(repository, latest, selected)
    if completed:
        return completed
    if route == "dispatch":
        require(config_at(repository, selected) == source_config, "adopted preview has a different release config")
    origin = successful_run(repository, "dev", check_workflow, selected)
    judge("template-promotion-v1", {
        "kind": "adopted", "checked_main": sha, "selected": selected, "tag": tag, "version": version,
        "repository": repository, "workflow": check_workflow, "origin": origin,
        "development": compare(repository, selected, "dev"), "main": compare(repository, "main", selected),
    })
    public_preview(repository, tag, checksums)
    return {"mode": "stable", "sha": selected, "tag": "v" + version, "preview": tag,
            "consumer": downstream, "adopted": adopted, "proof": run["html_url"]}


def event_plan(repository, event, event_name, workflow_sha):
    route = event_route(repository, event, event_name)
    source = workflow_sha if route == "dispatch" else event["workflow_run"]["head_sha"]
    return plan(config_at(repository, source), repository, event, event_name, route=route)


if __name__ == "__main__":
    result = event_plan(os.environ["GITHUB_REPOSITORY"], json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()),
                        os.environ["GITHUB_EVENT_NAME"], os.environ["GITHUB_SHA"])
    print(json.dumps(result))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        for key, value in result.items():
            print(f"{key}={value}", file=output)
