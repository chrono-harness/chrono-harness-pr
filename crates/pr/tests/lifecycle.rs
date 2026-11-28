#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {
fn initial_query_requires_unknown(deliberate_timeout: bool, actual_timeout: bool) -> (unknown: bool)
    ensures unknown == (deliberate_timeout || actual_timeout),
{
    deliberate_timeout || actual_timeout
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn timeout_requires_a_successful_initial_observation() {
    let deliberate = initial_query_requires_unknown(true, false);
    let actual = initial_query_requires_unknown(false, true);
    let both = initial_query_requires_unknown(true, true);
    let neither = initial_query_requires_unknown(false, false);
    expect_timeout_boundary(deliberate);
    expect_timeout_boundary(actual);
    expect_timeout_boundary(both);
    expect_timeout_boundary(!neither);
}

fn expect_timeout_boundary(actual: bool)
    requires actual,
{
    if !actual {
        panic!();
    }
}
}

#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{fs, io::Write, process::Command};
#[cfg(not(verus_keep_ghost))]
use tempfile::TempDir;

#[cfg(not(verus_keep_ghost))]
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
#[cfg(not(verus_keep_ghost))]
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
#[cfg(unix)]
#[cfg(not(verus_keep_ghost))]
fn cli_read_only_stdout_returns_native_publication_failure() {
    let expected = chrono_pr::run(&["--help".into()]);
    assert_eq!(expected.exit_code, 0);
    assert!(!expected.stdout.is_empty());
    assert!(expected.stderr.is_empty());
    let mut root = tempfile::tempdir().unwrap();
    root.disable_cleanup(true);
    eprintln!(
        "CLI publication failure originals: {}",
        root.path().display()
    );
    let binary = env!("CARGO_BIN_EXE_chrono-pr");
    let healthy = Command::new(binary)
        .arg("--help")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert_eq!(healthy.status.code(), Some(0));
    assert_eq!(healthy.stdout, expected.stdout.as_bytes());
    assert!(healthy.stderr.is_empty());
    let descriptor = root.path().join("read-only-output");
    let original = b"caller supplied a read-only output descriptor\n";
    fs::write(&descriptor, original).unwrap();
    let actual = Command::new(binary)
        .arg("--help")
        .current_dir(root.path())
        .stdout(fs::File::open(&descriptor).unwrap())
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), Some(2));
    assert!(actual.stdout.is_empty());
    assert_eq!(fs::read(descriptor).unwrap(), original);
    assert!(actual.stderr.ends_with(b"\n"));
    assert_eq!(
        std::str::from_utf8(&actual.stderr).unwrap().lines().count(),
        1
    );
    let event: Value = serde_json::from_slice(&actual.stderr).unwrap();
    assert_eq!(event["event"], "pr.output_publication_failed");
    assert_eq!(event["stream"], "stdout");
    assert_eq!(event["operation_exit_code"], 0);
    assert_eq!(event["output_bytes"], expected.stdout.len());
    assert_eq!(event["published_bytes"], 0);
    assert_eq!(event["native_code"], libc::EBADF);
    root.disable_cleanup(false);
}

#[cfg(not(verus_keep_ghost))]
fn snapshot() -> Value {
    serde_json::from_str(include_str!("fixtures/snapshot.json")).unwrap()
}
#[cfg(not(verus_keep_ghost))]
fn api(value: Value) -> Value {
    json!({"prefix":["api","graphql"],"stdout":value.to_string()})
}
#[cfg(not(verus_keep_ghost))]
fn completed_event() -> Value {
    serde_json::from_str(include_str!("fixtures/workflow_run.json")).unwrap()
}
#[cfg(not(verus_keep_ghost))]
fn pending() -> Value {
    let mut s = snapshot();
    s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"][0]["status"] =
        json!("IN_PROGRESS");
    s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"][0]["conclusion"] =
        Value::Null;
    s
}

#[cfg(not(verus_keep_ghost))]
struct Host {
    root: TempDir,
}
#[cfg(not(verus_keep_ghost))]
impl Drop for Host {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.root.disable_cleanup(true);
            let _ = writeln!(
                std::io::stderr(),
                "PR test fixture retained for failure diagnosis and regression acceptance: {}",
                self.root.path().display()
            );
        }
    }
}
#[cfg(not(verus_keep_ghost))]
impl Host {
    fn new(steps: Vec<Value>) -> Self {
        let host = Self {
            root: tempfile::tempdir().unwrap(),
        };
        fs::create_dir(host.root.path().join(".chrono-harness")).unwrap();
        host.write_config(json!({"schema":"chrono-pr-config/v3","repository":"example/host","base":"dev",
            "gh":env!("CARGO_BIN_EXE_chrono-pr-test-gh"),"required_checks":["chrono / collection"],
            "api_timeout_seconds":5,"watch_timeout_seconds":5,"watch_interval_seconds":60,"output_limit_bytes":1048576}));
        fs::write(
            host.root.path().join("steps.json"),
            json!(steps).to_string(),
        )
        .unwrap();
        fs::write(
            host.root.path().join("body.md"),
            "literal `echo no` $(touch nope)\n中文\n",
        )
        .unwrap();
        host
    }
    fn write_config(&self, value: Value) {
        fs::write(
            self.root.path().join(".chrono-harness/pr.json"),
            value.to_string(),
        )
        .unwrap();
    }
    fn run(&self, args: &[&str]) -> (i32, Value) {
        self.run_with_input(args, &[], false)
    }
    fn run_with_input(&self, args: &[&str], bytes: &[u8], keep_open: bool) -> (i32, Value) {
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_chrono-pr"))
            .args(args)
            .arg("--host-root")
            .arg(self.root.path())
            .current_dir("/")
            .env("GH_TOKEN", "fixture-secret-never-retain")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(bytes).unwrap();
        let _held_until_result = keep_open.then_some(input);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let schema = if args.first() == Some(&"run-watch") {
            "chrono-run-result/v1"
        } else {
            "chrono-pr-result/v3"
        };
        assert_eq!(result["schema"], schema);
        (output.status.code().unwrap(), result)
    }
    fn watch(&self, event: &Value) -> (i32, Value) {
        self.run_with_input(
            &["pr-watch", "--pr", "42", "--head-sha", HEAD],
            format!("{event}\n").as_bytes(),
            false,
        )
    }
    fn status(&self) -> (i32, Value) {
        self.run(&["pr-status", "--pr", "42", "--head-sha", HEAD])
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.root.path().join("calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn status_passes_only_for_the_bound_head_and_required_success() {
    let h = Host::new(vec![api(snapshot())]);
    let (code, r) = h.status();
    assert_eq!(code, 0);
    assert_eq!(r["outcome"], "passed");
    assert_eq!(r["expected_head"], HEAD);
    assert_eq!(r["observed_head"], HEAD);
    assert_eq!(r["checks"][0]["state"], "SUCCESS");
    assert_eq!(h.calls().len(), 1);
    assert_eq!(r["receipts"][0]["exit_code"], 0);
    assert!(r["receipts"][0].get("environment").is_none());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn failure_skipped_and_missing_checks_cannot_pass() {
    for state in [
        "FAILURE",
        "CANCELLED",
        "TIMED_OUT",
        "ACTION_REQUIRED",
        "STARTUP_FAILURE",
        "STALE",
        "NEUTRAL",
        "SKIPPED",
    ] {
        let mut s = snapshot();
        s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"][0]["conclusion"] =
            json!(state);
        let (code, r) = Host::new(vec![api(s)]).status();
        assert_eq!(code, 1, "{state}: {r}");
        assert_eq!(r["outcome"], "failed");
    }
    let mut s = snapshot();
    s["data"]["repository"]["object"]["statusCheckRollup"] = Value::Null;
    let (code, r) = Host::new(vec![api(s)]).status();
    assert_eq!(code, 3);
    assert_eq!(r["checks"][0]["state"], "MISSING");
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn stale_and_closed_are_distinct_from_check_failure() {
    let mut s = snapshot();
    s["data"]["repository"]["pullRequest"]["headRefOid"] = json!(OTHER);
    s["data"]["repository"]["pullRequest"]["headRef"]["target"]["oid"] = json!(OTHER);
    let (code, r) = Host::new(vec![api(s)]).status();
    assert_eq!(code, 4);
    assert_eq!(r["outcome"], "stale");
    let mut s = snapshot();
    s["data"]["repository"]["pullRequest"]["state"] = json!("CLOSED");
    let (code, r) = Host::new(vec![api(s)]).status();
    assert_eq!(code, 1);
    assert_eq!(r["outcome"], "closed");
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn rejects_incomplete_or_ambiguous_remote_facts() {
    for pointer in [
        "/data/repository/nameWithOwner",
        "/data/repository/pullRequest/number",
        "/data/repository/object/oid",
        "/data/repository/pullRequest/baseRefName",
        "/data/repository/pullRequest/headRepository/nameWithOwner",
    ] {
        let mut s = snapshot();
        *s.pointer_mut(pointer).unwrap() = json!("wrong");
        let (code, _) = Host::new(vec![api(s)]).status();
        assert_eq!(code, 2, "{pointer}");
    }
    for pointer in [
        "/data/repository/object/statusCheckRollup/contexts/pageInfo/hasNextPage",
        "/data/repository/object/statusCheckRollup/contexts/nodes/0/checkSuite/checkRuns/pageInfo/hasNextPage",
    ] {
        let mut s = snapshot();
        *s.pointer_mut(pointer).unwrap() = json!(true);
        assert_eq!(Host::new(vec![api(s)]).status().0, 2);
    }
    let mut s = snapshot();
    let nodes = s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"]
        .as_array_mut()
        .unwrap();
    nodes.push(nodes[0].clone());
    assert_eq!(Host::new(vec![api(s)]).status().0, 2);
    let mut s = snapshot();
    s["errors"] = json!([{"message":"partial result"}]);
    assert_eq!(Host::new(vec![api(s)]).status().0, 2);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn repeated_success_requires_distinct_suites_of_the_same_workflow() {
    for (workflow, second_suite, conclusion, expected_exit) in [
        (Some(7), 201, "SUCCESS", 0),
        (Some(8), 201, "SUCCESS", 2),
        (None, 201, "SUCCESS", 2),
        (Some(7), 201, "FAILURE", 2),
        (Some(7), 200, "SUCCESS", 2),
    ] {
        let mut value = snapshot();
        let checks = value
            .pointer_mut("/data/repository/object/statusCheckRollup/contexts/nodes")
            .unwrap()
            .as_array_mut()
            .unwrap();
        checks[0]["checkSuite"]["workflowRun"] = json!({"workflow":{"databaseId":7}});
        let mut second = checks[0].clone();
        second["databaseId"] = json!(101);
        second["conclusion"] = json!(conclusion);
        second["checkSuite"]["databaseId"] = json!(second_suite);
        second["checkSuite"]["workflowRun"] = json!({"workflow":{"databaseId":workflow}});
        second["checkSuite"]["checkRuns"]["nodes"] = json!([{"databaseId":101}]);
        if second_suite == 200 {
            let ids = json!([{"databaseId":100},{"databaseId":101}]);
            checks[0]["checkSuite"]["checkRuns"]["nodes"] = ids.clone();
            second["checkSuite"]["checkRuns"]["nodes"] = ids;
        }
        checks.push(second);
        assert_eq!(Host::new(vec![api(value)]).status().0, expected_exit);
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn latest_rerun_and_legacy_status_are_supported() {
    let mut s = snapshot();
    let nodes = s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"]
        .as_array_mut()
        .unwrap();
    let mut old = nodes[0].clone();
    old["databaseId"] = json!(99);
    old["conclusion"] = json!("FAILURE");
    nodes.push(old);
    assert_eq!(Host::new(vec![api(s)]).status().0, 0);
    let mut s = snapshot();
    s["data"]["repository"]["object"]["statusCheckRollup"]["contexts"]["nodes"] = json!([
        {"__typename":"StatusContext","context":"chrono / collection","state":"SUCCESS","commit":{"oid":HEAD}}
    ]);
    assert_eq!(Host::new(vec![api(s)]).status().0, 0);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn unknown_states_wrong_commits_and_incomplete_latest_sets_are_errors() {
    for (pointer, value) in [
        (
            "/data/repository/object/statusCheckRollup/contexts/nodes/0/conclusion",
            json!("FUTURE_STATE"),
        ),
        (
            "/data/repository/object/statusCheckRollup/contexts/nodes/0/conclusion",
            json!("PENDING"),
        ),
        (
            "/data/repository/object/statusCheckRollup/contexts/nodes/0/checkSuite/commit/oid",
            json!(OTHER),
        ),
        (
            "/data/repository/object/statusCheckRollup/contexts/nodes/0/checkSuite/checkRuns/nodes",
            json!([{"databaseId":100},{"databaseId":101}]),
        ),
        (
            "/data/repository/object/statusCheckRollup/contexts/nodes/0/status",
            json!("UNKNOWN"),
        ),
    ] {
        let mut s = snapshot();
        *s.pointer_mut(pointer).unwrap() = value;
        assert_eq!(Host::new(vec![api(s)]).status().0, 2, "{pointer}");
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn latest_check_must_belong_to_the_suite_that_lists_it() {
    let mut value = snapshot();
    let checks = value
        .pointer_mut("/data/repository/object/statusCheckRollup/contexts/nodes")
        .unwrap()
        .as_array_mut()
        .unwrap();
    let mut second = checks[0].clone();
    second["name"] = json!("other check");
    second["databaseId"] = json!(101);
    second["checkSuite"]["databaseId"] = json!(201);
    second["checkSuite"]["checkRuns"]["nodes"] = json!([{"databaseId":101}]);
    checks[0]["checkSuite"]["checkRuns"]["nodes"] = json!([{"databaseId":100},{"databaseId":101}]);
    checks.push(second);
    let (code, result) = Host::new(vec![api(value)]).status();
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"], "latest check missing from complete rollup");
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn duplicate_and_pending_check_errors_keep_their_order() {
    let path = "/data/repository/object/statusCheckRollup/contexts/nodes";
    let mut value = snapshot();
    let checks = value.pointer_mut(path).unwrap().as_array_mut().unwrap();
    let mut duplicate = checks[0].clone();
    duplicate["status"] = json!("QUEUED");
    checks.push(duplicate);
    let (code, result) = Host::new(vec![api(value)]).status();
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"], "duplicate check ID", "{result}");

    for (status, latest, expected) in [
        ("QUEUED", json!([]), "pending check has a conclusion"),
        ("COMPLETED", json!([]), "empty latest check set"),
        (
            "COMPLETED",
            json!([{"databaseId":100},{"databaseId":100}]),
            "duplicate latest check ID",
        ),
    ] {
        let mut value = snapshot();
        let check = &mut value.pointer_mut(path).unwrap()[0];
        check["status"] = json!(status);
        check["checkSuite"]["checkRuns"]["nodes"] = latest;
        let (code, result) = Host::new(vec![api(value)]).status();
        assert_eq!(code, 2, "{result}");
        assert_eq!(result["error"], expected, "{result}");
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn draft_creation_is_explicit_and_readback_failure_preserves_the_pr_number() {
    let mut draft = snapshot();
    draft["data"]["repository"]["pullRequest"]["isDraft"] = json!(true);
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42\n"}),
        api(draft),
    ]);
    let mut args = open_args();
    args.push("--draft");
    assert_eq!(h.run(&args).0, 0);
    assert!(
        h.calls()[2]["args"]
            .as_array()
            .unwrap()
            .contains(&json!("--draft"))
    );
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42\n"}),
        json!({"prefix":["api","graphql"],"stderr":"HTTP 502","exit":1}),
    ]);
    let (code, result) = h.run(&open_args());
    assert_eq!(code, 2);
    assert_eq!(result["pr"], 42);
    assert_eq!(result["outcome"], "unknown");
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn quiet_descendant_finishes_before_query_result() {
    let mut step = api(snapshot());
    step["quiet_descendant_ms"] = json!(300);
    #[cfg(target_os = "linux")]
    {
        step["reap_delay_ms"] = json!(250);
    }
    let host = Host::new(vec![step]);
    let (code, result) = host.status();
    assert_eq!(code, 0, "{result}");
    let marker = fs::read(host.root.path().join("child-finished"));
    assert_eq!(marker.unwrap(), b"done");
    assert_eq!(host.calls().len(), 1);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn quiet_descendant_exceeding_the_deadline_keeps_the_launchers_actual_exit() {
    let mut step = api(snapshot());
    step["quiet_descendant_ms"] = json!(10_000);
    let host = Host::new(vec![step]);
    let path = host.root.path().join(".chrono-harness/pr.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["api_timeout_seconds"] = json!(1);
    fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
    let (code, result) = host.status();
    assert_eq!(code, 2, "{result}");
    let receipt = &result["receipts"][0];
    assert_eq!(receipt["timed_out"], true);
    assert_eq!(receipt["exit_code"], 0);
    assert_eq!(
        receipt["stdout_bytes"],
        json!(snapshot().to_string().as_bytes())
    );
    assert_eq!(host.calls().len(), 1);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn signal_keeps_the_original_streams_and_has_no_exit_code() {
    let host = Host::new(vec![json!({
        "prefix": ["api", "graphql"], "stdout": "partial", "stderr": "closed", "signal": "TERM"
    })]);
    let (code, result) = host.status();
    assert_eq!(code, 2);
    assert_eq!(result["outcome"], "unknown");
    assert_eq!(host.calls().len(), 1);
    let receipt = &result["receipts"][0];
    assert_eq!(receipt["exit_signal"], libc::SIGTERM);
    assert_eq!(receipt["exit_code"], Value::Null);
    assert_eq!(receipt["stdout_bytes"], json!(b"partial"));
    assert_eq!(receipt["stderr_bytes"], json!(b"closed"));
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn failed_readback_and_missing_or_mismatched_notification_remain_unknown() {
    let host = Host::new(vec![
        api(pending()),
        json!({"prefix":["api","graphql"],"exit":4,"stderr":"auth"}),
    ]);
    let (code, result) = host.watch(&completed_event());
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["outcome"], "unknown");
    assert_eq!(host.calls().len(), 2);
    let host = Host::new(vec![api(pending())]);
    let (code, result) = host.run(&["pr-watch", "--pr", "42", "--head-sha", HEAD]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["outcome"], "unknown");
    assert_eq!(host.calls().len(), 1);
    for (pointer, value) in [
        ("/action", json!("requested")),
        ("/repository/full_name", json!("other/host")),
        ("/workflow_run/status", json!("in_progress")),
        ("/workflow_run/head_sha", json!(OTHER)),
        ("/workflow_run/pull_requests/0/number", json!(43)),
        ("/workflow_run/pull_requests/0/head/sha", json!(OTHER)),
    ] {
        let host = Host::new(vec![api(pending())]);
        let mut event = completed_event();
        *event.pointer_mut(pointer).unwrap() = value;
        let (code, result) = host.watch(&event);
        assert_eq!(code, 2, "{pointer}: {result}");
        assert_eq!(result["outcome"], "unknown");
        assert_eq!(host.calls().len(), 1);
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn one_matching_event_wakes_one_factual_readback() {
    let host = Host::new(vec![api(pending()), api(snapshot())]);
    let (code, result) = host.watch(&completed_event());
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["outcome"], "passed");
    assert_eq!(result["notification"]["run_id"], 857);
    assert_eq!(host.calls().len(), 2);
    assert!(host.calls().iter().all(|r| r["args"][0] == "api"));
    let mut no_pr = completed_event();
    no_pr["workflow_run"]["pull_requests"] = json!([]);
    no_pr["workflow_run"]["check_suite_id"] = json!(200);
    let host = Host::new(vec![api(pending()), api(snapshot())]);
    assert_eq!(host.watch(&no_pr).0, 0);
    no_pr["workflow_run"]["check_suite_id"] = json!(201);
    let host = Host::new(vec![api(pending())]);
    assert_eq!(host.watch(&no_pr).0, 2);
    assert_eq!(host.calls().len(), 1);
    let mut changed = snapshot();
    changed["data"]["repository"]["pullRequest"]["headRefOid"] = json!(OTHER);
    changed["data"]["repository"]["pullRequest"]["headRef"]["target"]["oid"] = json!(OTHER);
    let host = Host::new(vec![api(pending()), api(changed)]);
    assert_eq!(host.watch(&completed_event()).0, 4);
    let host = Host::new(vec![api(pending()), api(pending())]);
    assert_eq!(host.watch(&completed_event()).0, 2);
    let host = Host::new(vec![api(snapshot())]);
    assert_eq!(
        host.run(&["pr-watch", "--pr", "42", "--head-sha", HEAD]).0,
        0
    );
    assert_eq!(host.calls().len(), 1);
    let host = Host::new(vec![api(pending()), api(snapshot())]);
    let closed = json!({"action":"closed","repository":{"full_name":"example/host"},"pull_request":{"state":"closed","number":42,"head":{"sha":HEAD}}});
    assert_eq!(host.watch(&closed).0, 0);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn repeated_completion_is_ignored_but_a_new_attempt_wakes_the_pr() {
    let mut completed = completed_event();
    completed["workflow_run"]["run_attempt"] = json!(1);
    let mut events = vec![completed.clone()];
    for id in 1000..1065 {
        let mut event = completed.clone();
        event["workflow_run"]["id"] = json!(id);
        events.push(event);
    }
    let mut responses = vec![api(pending()); events.len() + 1];
    events.push(completed.clone());
    completed["workflow_run"]["run_attempt"] = json!(2);
    events.push(completed);
    responses.push(api(snapshot()));
    let host = Host::new(responses);
    let input = events
        .iter()
        .map(|event| format!("{event}\n"))
        .collect::<String>();
    let (code, result) = host.run_with_input(
        &["pr-watch", "--pr", "42", "--head-sha", HEAD],
        input.as_bytes(),
        false,
    );
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["notification"]["run_id"], 857);
    assert_eq!(result["notification"]["run_attempt"], 2);
    assert_eq!(host.calls().len(), events.len());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn conflict_wins_over_checks_and_unknown_mergeability_is_exposed() {
    let mut conflicting = pending();
    conflicting["data"]["repository"]["pullRequest"]["mergeable"] = json!("CONFLICTING");
    let host = Host::new(vec![api(conflicting)]);
    let (code, result) = host.run(&["pr-watch", "--pr", "42", "--head-sha", HEAD]);
    assert_eq!((code, result["outcome"].as_str()), (1, Some("conflict")));
    assert_eq!(host.calls().len(), 1);
    let mut unknown = snapshot();
    unknown["data"]["repository"]["pullRequest"]["mergeable"] = json!("UNKNOWN");
    let (code, result) = Host::new(vec![api(unknown)]).status();
    assert_eq!((code, result["mergeable"].as_str()), (2, Some("UNKNOWN")));
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn open_pr_uses_the_live_head_before_reporting_conflict() {
    let watch = |base, head| {
        let mut reply = pending();
        let pr = &mut reply["data"]["repository"]["pullRequest"];
        pr["mergeable"] = json!("CONFLICTING");
        pr["baseRef"]["target"]["oid"] = json!(base);
        pr["headRef"]["target"]["oid"] = json!(head);
        Host::new(vec![api(reply)]).run(&["pr-watch", "--pr", "42", "--head-sha", HEAD])
    };
    let base = "cccccccccccccccccccccccccccccccccccccccc";
    for (head, exit, outcome) in [(OTHER, 4, "stale"), (HEAD, 1, "conflict")] {
        let (code, result) = watch(Some(base), Some(head));
        assert_eq!(
            (code, result["outcome"].as_str()),
            (exit, Some(outcome)),
            "{result}"
        );
        assert_eq!(result["observed_head"], head);
    }
    for (base, head, error) in [
        (None, None, "invalid remote field /baseRef/target/oid"),
        (
            Some("invalid"),
            None,
            "invalid remote field /headRef/target/oid",
        ),
        (Some(base), None, "invalid remote field /headRef/target/oid"),
        (Some(base), Some("invalid"), "invalid live branch SHA"),
    ] {
        let (code, result) = watch(base, head);
        assert_eq!(
            (code, result["outcome"].as_str()),
            (2, Some("unknown")),
            "{result}"
        );
        assert_eq!(result["error"], error);
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn base_push_rechecks_both_tips_and_only_reports_an_observed_change() {
    let base = "cccccccccccccccccccccccccccccccccccccccc";
    let next = "dddddddddddddddddddddddddddddddddddddddd";
    let changed = json!({"repository":{"full_name":"example/host"},"ref":"refs/heads/dev","before":base,"after":next});
    let mut unrelated = changed.clone();
    unrelated["ref"] = json!("refs/heads/other");
    let mut old = changed.clone();
    old["before"] = json!(OTHER);
    let input = format!("{old}\n{unrelated}\n{changed}\n");
    for (mergeable, outcome, exit) in [
        ("CONFLICTING", "conflict", 1),
        ("UNKNOWN", "changed", 3),
        ("MERGEABLE", "changed", 3),
    ] {
        let mut fresh = pending();
        fresh["data"]["repository"]["pullRequest"]["mergeable"] = json!(mergeable);
        fresh["data"]["repository"]["pullRequest"]["baseRef"]["target"]["oid"] = json!(next);
        let host = Host::new(vec![api(pending()), api(fresh)]);
        let (code, result) = host.run_with_input(
            &["pr-watch", "--pr", "42", "--head-sha", HEAD],
            input.as_bytes(),
            false,
        );
        assert_eq!((code, result["outcome"].as_str()), (exit, Some(outcome)));
        assert_eq!(host.calls().len(), 2);
        assert_eq!(result["observed_base"], next);
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn fork_head_push_or_synchronize_wakes_the_original_pr() {
    let branch = "integration/change";
    for event in [
        json!({"repository":{"full_name":"fork/host"},"ref":"refs/heads/integration/change","before":HEAD,"after":OTHER}),
        json!({"action":"synchronize","repository":{"full_name":"example/host"},"before":HEAD,"after":OTHER,
            "pull_request":{"number":42,"base":{"ref":"dev","repo":{"full_name":"example/host"}},
            "head":{"ref":branch,"sha":OTHER,"repo":{"full_name":"fork/host"}}}}),
    ] {
        let mut initial = pending();
        initial["data"]["repository"]["pullRequest"]["headRepository"]["nameWithOwner"] =
            json!("fork/host");
        let mut advanced = initial.clone();
        advanced["data"]["repository"]["pullRequest"]["headRefOid"] = json!(OTHER);
        advanced["data"]["repository"]["pullRequest"]["headRef"]["target"]["oid"] = json!(OTHER);
        let host = Host::new(vec![api(initial), api(advanced)]);
        let (code, result) = host.watch(&event);
        assert_eq!((code, result["observed_head"].as_str()), (4, Some(OTHER)));
        assert_eq!(result["head_repository"], "fork/host");
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn synchronize_checks_pr_identity_and_both_refs_before_querying_github() {
    let valid = json!({"action":"synchronize","repository":{"full_name":"example/host"},
        "before":HEAD,"after":OTHER,"pull_request":{"number":42,
        "base":{"ref":"dev","repo":{"full_name":"example/host"}},
        "head":{"ref":"integration/change","sha":OTHER,"repo":{"full_name":"example/host"}}}});
    let mut input = String::new();
    for (pointer, value) in [
        ("/repository/full_name", json!("different/host")),
        ("/pull_request/number", json!(43)),
        ("/pull_request/base/ref", json!("main")),
        ("/pull_request/base/repo/full_name", json!("fork/host")),
        ("/pull_request/head/ref", json!("other")),
        ("/pull_request/head/repo/full_name", json!("fork/host")),
        ("/pull_request/head/sha", json!(HEAD)),
        ("/before", json!(OTHER)),
        ("/after", json!("invalid")),
    ] {
        let mut unrelated = valid.clone();
        *unrelated.pointer_mut(pointer).unwrap() = value;
        input.push_str(&unrelated.to_string());
        input.push('\n');
    }
    let host = Host::new(vec![api(pending())]);
    let (code, result) = host.run_with_input(
        &["pr-watch", "--pr", "42", "--head-sha", HEAD],
        input.as_bytes(),
        false,
    );
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["outcome"], "unknown");
    assert_eq!(host.calls().len(), 1);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn api_failure_retains_original_output_without_retry() {
    let h = Host::new(vec![
        json!({"prefix":["api","graphql"],"stdout":"partial\n","stderr":"HTTP 403\n","exit":1}),
    ]);
    let (code, r) = h.status();
    assert_eq!(code, 2);
    assert_eq!(r["outcome"], "unknown");
    assert_eq!(h.calls().len(), 1);
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(r["receipts"][0]["stderr_bytes"].clone()).unwrap(),
        b"HTTP 403\n"
    );
    let h = Host::new(vec![json!({"prefix":["api","graphql"],"stdout":"{broken"})]);
    assert_eq!(h.status().0, 2);
}

#[cfg(not(verus_keep_ghost))]
fn resolve() -> Value {
    api(
        json!({"data":{"repository":{"nameWithOwner":"example/host","ref":{"target":{"oid":HEAD}}}}}),
    )
}
#[cfg(not(verus_keep_ghost))]
fn list(items: Value) -> Value {
    json!({"prefix":["pr","list"],"stdout":items.to_string()})
}
#[cfg(not(verus_keep_ghost))]
fn open_args() -> Vec<&'static str> {
    vec![
        "pr-open",
        "--head",
        "integration/change",
        "--head-sha",
        HEAD,
        "--title",
        "literal $(no) `no` 中文",
        "--body-file",
        "body.md",
    ]
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn opens_once_with_literal_text_and_verifies_the_created_pr() {
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdin":"literal `echo no` $(touch nope)\n中文\n","stdout":"https://github.com/example/host/pull/42\n"}),
        api(snapshot()),
    ]);
    let (code, r) = h.run(&open_args());
    assert_eq!(code, 0);
    assert_eq!(r["outcome"], "opened");
    assert_eq!(r["pr"], 42);
    let calls = h.calls();
    assert_eq!(calls.len(), 4);
    let args = calls[2]["args"].as_array().unwrap();
    assert!(
        args.windows(2)
            .any(|w| w == [json!("--title"), json!("literal $(no) `no` 中文")])
    );
    assert!(
        args.windows(2)
            .any(|w| w == [json!("--body-file"), json!("-")])
    );
    assert!(!h.root.path().join("nope").exists());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn rerunning_open_reuses_existing_pr_without_mutation() {
    let h = Host::new(vec![
        resolve(),
        list(json!([{"number":42}])),
        api(snapshot()),
    ]);
    let (code, r) = h.run(&open_args());
    assert_eq!(code, 0);
    assert_eq!(r["outcome"], "existing");
    assert_eq!(h.calls().len(), 3);
    let mut wrong = resolve();
    wrong["stdout"]=json!(json!({"data":{"repository":{"nameWithOwner":"example/host","ref":{"target":{"oid":OTHER}}}}}).to_string());
    let h = Host::new(vec![wrong]);
    assert_eq!(h.run(&open_args()).0, 4);
    assert_eq!(h.calls().len(), 1);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn ambiguous_creation_failures_and_changed_readback_keep_identity() {
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stderr":"connection reset after POST","exit":1}),
    ]);
    let (code, r) = h.run(&open_args());
    assert_eq!(code, 2);
    assert_eq!(r["outcome"], "unknown");
    assert_eq!(r["expected_head"], HEAD);
    assert_eq!(h.calls().len(), 3);
    let mut s = snapshot();
    s["data"]["repository"]["pullRequest"]["headRefName"] = json!("wrong");
    let h = Host::new(vec![resolve(), list(json!([{"number":42}])), api(s)]);
    assert_eq!(h.run(&open_args()).0, 2);
    let h = Host::new(vec![resolve(), list(json!([{"number":42},{"number":43}]))]);
    assert_eq!(h.run(&open_args()).0, 2);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn invalid_or_ambiguous_existing_candidates_stop_before_creation() {
    for (items, error) in [
        (
            json!([{"number":42},{"number":42}]),
            "ambiguous existing PRs; no creation attempted",
        ),
        (json!([{"number":0}]), "invalid existing PR number"),
        (
            json!([{"number":2147483648_u64}]),
            "invalid existing PR number",
        ),
        (json!([{"number":"42"}]), "invalid existing PR number"),
    ] {
        let h = Host::new(vec![resolve(), list(items)]);
        let (code, result) = h.run(&open_args());
        assert_eq!((code, result["error"].as_str()), (2, Some(error)));
        assert_eq!(h.calls().len(), 2);
        assert!(reminders(&h).is_empty());
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn create_receipt_binds_repository_and_a_decimal_graphql_number() {
    for url in [
        "https://github.com/example/fork/pull/42",
        "https://github.com/example/host/pull/0",
        "https://github.com/example/host/pull/2147483648",
        "https://github.com/example/host/pull/+42",
        "https://github.com/example/host/pull/４２",
        "https://github.com/example/host/pull/42?query",
        "https://github.com/example/host/pull/",
    ] {
        let h = Host::new(vec![
            resolve(),
            list(json!([])),
            json!({"prefix":["pr","create"],"stdout":url}),
        ]);
        let (code, result) = h.run(&open_args());
        assert_eq!(code, 2, "{url} {result}");
        assert_eq!(
            result["error"],
            "creation returned an invalid PR URL; query existing PR before retrying"
        );
        assert_eq!(h.calls().len(), 3);
        assert!(reminders(&h).is_empty());
    }
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"\u{2003}https://github.com/example/host/pull/00042\r\n"}),
        api(snapshot()),
    ]);
    let (code, result) = h.run(&open_args());
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["pr"], 42);
    assert_eq!(reminders(&h).len(), 1);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn invalid_create_reply_follows_command_status_and_precedes_reminders() {
    let invalid = chrono_pr::verified_snapshot::decode_creation_receipt(&[255], "example/host");
    assert_eq!(invalid, Err("non-UTF-8 creation output"));
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42","exit":1}),
    ]);
    let (code, result) = h.run(&open_args());
    assert_eq!(code, 2, "{result}");
    assert_eq!(
        result["error"],
        "GitHub command exited 1; see original receipt"
    );
    assert_eq!(h.calls().len(), 3);
    assert!(reminders(&h).is_empty());
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn open_readback_preserves_closed_and_rejects_missing_requested_draft() {
    let mut args = open_args();
    args.push("--draft");
    for (state, code, outcome, error) in [
        ("CLOSED", 1, "closed", None),
        (
            "OPEN",
            2,
            "unknown",
            Some("requested draft state not present; existing PR was preserved"),
        ),
        (
            "MERGED",
            2,
            "unknown",
            Some("PR merged during open; creation result is no longer open"),
        ),
    ] {
        let mut body = snapshot();
        let pr = &mut body["data"]["repository"]["pullRequest"];
        pr["state"] = json!(state);
        if state == "MERGED" {
            pr["mergeCommit"] = json!({"oid":OTHER});
        }
        let h = Host::new(vec![resolve(), list(json!([{"number":42}])), api(body)]);
        let (actual, result) = h.run(&args);
        assert_eq!(actual, code, "{result}");
        assert_eq!(result["outcome"], outcome);
        assert_eq!(result["error"].as_str(), error);
        assert_eq!(h.calls().len(), 3);
        assert!(reminders(&h).is_empty());
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn invalid_arguments_and_config_do_no_remote_work() {
    for args in [
        vec!["pr-status", "--pr", "0", "--head-sha", HEAD],
        vec!["pr-status", "--pr", "42", "--head-sha", "bad"],
        vec!["pr-status", "--pr", "42", "--head-sha", HEAD, "--unknown"],
        vec!["pr-status", "--pr", "42", "--pr", "43", "--head-sha", HEAD],
        vec!["pr-open", "--head", "--base", "--head-sha", HEAD],
        vec!["pr-watch", "--pr", "42", "--head-sha", HEAD, "--draft"],
    ] {
        let h = Host::new(vec![]);
        assert_eq!(h.run(&args).0, 2);
        assert!(h.calls().is_empty());
    }
    for (key, value) in [
        ("required_checks", json!([])),
        ("api_timeout_seconds", json!(0)),
        ("watch_interval_seconds", json!(1)),
        ("unexpected", json!(true)),
    ] {
        let h = Host::new(vec![]);
        let mut c: Value = serde_json::from_slice(
            &fs::read(h.root.path().join(".chrono-harness/pr.json")).unwrap(),
        )
        .unwrap();
        c[key] = value;
        h.write_config(c);
        assert_eq!(h.status().0, 2, "{key}");
        assert!(h.calls().is_empty());
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn timeout_and_output_limits_preserve_the_original_local_receipt() {
    for initial_timeout in [false, true] {
        let mut request = api(pending());
        if initial_timeout {
            request["sleep_ms"] = json!(1800);
        }
        let host = Host::new(vec![request]);
        let mut config: Value = serde_json::from_slice(
            &fs::read(host.root.path().join(".chrono-harness/pr.json")).unwrap(),
        )
        .unwrap();
        config["watch_timeout_seconds"] = json!(1);
        host.write_config(config);
        let (code, r) =
            host.run_with_input(&["pr-watch", "--pr", "42", "--head-sha", HEAD], b"{", true);
        assert_eq!(code, 2, "{r}");
        assert_eq!(r["outcome"], "unknown");
        assert_eq!(r["receipts"].as_array().unwrap().len(), 1, "{r}");
        let first = &r["receipts"][0];
        if initial_query_requires_unknown(initial_timeout, first["failure"] == "process timed out")
        {
            assert_eq!(r["observed_head"], Value::Null, "{r}");
            assert_eq!(first["failure"], "process timed out");
        } else {
            assert_eq!(r["observed_head"], HEAD);
            assert_eq!(first["exit_code"], 0);
            assert_eq!(r["error"], "completion notification deadline reached");
        }
    }
    let h = Host::new(vec![
        json!({"prefix":["api","graphql"],"stdout":"x".repeat(100000)}),
    ]);
    let mut c: Value =
        serde_json::from_slice(&fs::read(h.root.path().join(".chrono-harness/pr.json")).unwrap())
            .unwrap();
    c["output_limit_bytes"] = json!(4096);
    h.write_config(c);
    assert_eq!(h.status().0, 2);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn merged_state_requires_and_retains_the_actual_merge_identity() {
    let mut merged = snapshot();
    merged["data"]["repository"]["pullRequest"]["state"] = json!("MERGED");
    merged["data"]["repository"]["pullRequest"]["mergeCommit"] = json!({"oid":OTHER});
    let h = Host::new(vec![api(merged.clone())]);
    let (code, result) = h.status();
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["pr_state"], "MERGED");
    assert_eq!(result["merge_commit"], OTHER);
    for value in [Value::Null, json!({"oid":"invalid"})] {
        merged["data"]["repository"]["pullRequest"]["mergeCommit"] = value;
        assert_eq!(Host::new(vec![api(merged.clone())]).status().0, 2);
    }
}

#[cfg(not(verus_keep_ghost))]
fn reminders(h: &Host) -> Vec<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_chrono-pr"))
        .arg("pr-reminders")
        .arg("--host-root")
        .arg(h.root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["pull_requests"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn tracks_its_created_pr_through_a_failed_readback_until_verified_terminal() {
    let mut closed = snapshot();
    closed["data"]["repository"]["pullRequest"]["state"] = json!("CLOSED");
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42\n"}),
        json!({"prefix":["api","graphql"],"exit":1}),
        api(snapshot()),
        api(closed),
    ]);
    assert!(reminders(&h).is_empty());
    let (code, result) = h.run(&open_args());
    assert_eq!(code, 2, "{result}");
    assert!(result.get("reminder_error").is_none(), "{result}");
    let pending = reminders(&h);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["pr"], 42);
    assert_eq!(pending[0]["head_at_creation"], HEAD);
    assert_eq!(h.status().0, 0);
    assert_eq!(reminders(&h), pending);
    assert_eq!(h.status().0, 1);
    assert!(reminders(&h).is_empty());
    assert_eq!(h.calls().len(), 6);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn ordinary_queries_make_no_state_and_duplicate_remote_json_never_passes() {
    let good = api(snapshot());
    let raw = good["stdout"].as_str().unwrap();
    let cases = [
        (r#""number":42"#, r#""number":1,"number":42"#, "number"),
        (r#""number":42"#, r#""nu\u006dber":1,"number":42"#, "number"),
        (r#""data":"#, r#""data":null,"data":"#, "data"),
    ];
    let mut replies = vec![good.clone()];
    for (key, repeated, _) in cases {
        assert!(raw.contains(key));
        let mut reply = good.clone();
        reply["stdout"] = json!(raw.replacen(key, repeated, 1));
        replies.push(reply);
    }
    let h = Host::new(replies);
    assert_eq!(h.status().0, 0);
    for (_, _, key) in cases {
        let (code, report) = h.status();
        assert_eq!(code, 2);
        assert!(
            report
                .to_string()
                .contains(&format!("duplicate JSON member {key}")),
            "{report}"
        );
    }
    assert!(reminders(&h).is_empty());
    assert!(
        !h.root
            .path()
            .join(".chrono-harness/pr-reminders.sqlite3")
            .exists()
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn reminder_conflicts_keep_the_first_row_and_close_only_the_observed_repository() {
    let mut closed = snapshot();
    closed["data"]["repository"]["pullRequest"]["state"] = json!("CLOSED");
    let host = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42"}),
        api(snapshot()),
        api(closed),
    ]);
    let isolated = Host::new(vec![]);
    let path = host
        .root
        .path()
        .join(".chrono-harness/pr-reminders.sqlite3");
    assert!(reminders(&host).is_empty());
    assert!(!path.exists());

    // Simulate the original registration being retried for the same GitHub number.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE reminder(repository TEXT NOT NULL, number INTEGER NOT NULL, head TEXT NOT NULL,
         expires INTEGER NOT NULL, PRIMARY KEY(repository,number)) WITHOUT ROWID;",
    ).unwrap();
    let deadline: i64 = db
        .query_row("SELECT unixepoch()+600", [], |row| row.get(0))
        .unwrap();
    for repo in ["example/host", "example/other"] {
        db.execute(
            "INSERT INTO reminder VALUES (?1,42,?2,?3)",
            rusqlite::params![repo, OTHER, deadline],
        )
        .unwrap();
    }
    assert_eq!(host.run(&open_args()).0, 0);
    let rows = reminders(&host);
    assert_eq!(rows.len(), 2);
    for (row, repo) in rows.iter().zip(["example/host", "example/other"]) {
        assert_eq!(row["repository"], repo);
        assert_eq!(row["head_at_creation"], OTHER);
        assert_eq!(row["expires_at_unix"], deadline);
    }
    assert!(reminders(&isolated).is_empty());
    assert_eq!(host.status().0, 1);
    assert_eq!(reminders(&host), vec![rows[1].clone()]);
    assert!(reminders(&isolated).is_empty());
    assert!(
        !isolated
            .root
            .path()
            .join(".chrono-harness/pr-reminders.sqlite3")
            .exists()
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn expired_reminders_are_removed_from_local_storage_without_a_remote_query() {
    let h = Host::new(vec![
        resolve(),
        list(json!([])),
        json!({"prefix":["pr","create"],"stdout":"https://github.com/example/host/pull/42\n"}),
        api(snapshot()),
    ]);
    let clock = rusqlite::Connection::open_in_memory().unwrap();
    let now = || {
        clock
            .query_row("SELECT unixepoch()", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    let earliest = now();
    assert_eq!(h.run(&open_args()).0, 0);
    let latest = now();
    let db = rusqlite::Connection::open(h.root.path().join(".chrono-harness/pr-reminders.sqlite3"))
        .unwrap();
    let deadline: i64 = db
        .query_row("SELECT expires FROM reminder", [], |r| r.get(0))
        .unwrap();
    assert!((earliest..=latest).contains(&(deadline - 2_592_000)));
    db.execute("UPDATE reminder SET expires=unixepoch()", [])
        .unwrap();
    assert!(reminders(&h).is_empty());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM reminder", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(h.calls().len(), 4);
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn expired_reminders_release_pages_in_existing_database_and_keep_live_rows() {
    let h = Host::new(vec![]);
    let path = h.root.path().join(".chrono-harness/pr-reminders.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA auto_vacuum=NONE;
         CREATE TABLE reminder(
           repository TEXT NOT NULL, number INTEGER NOT NULL, head TEXT NOT NULL,
           expires INTEGER NOT NULL, PRIMARY KEY(repository,number)
         ) WITHOUT ROWID;
         WITH RECURSIVE ids(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM ids WHERE n<3000)
         INSERT INTO reminder SELECT 'example/host', n, hex(zeroblob(20)), unixepoch()-1 FROM ids;
         INSERT INTO reminder VALUES ('example/host',4000,hex(zeroblob(20)),unixepoch()+3600);",
    )
    .unwrap();
    drop(db);
    let before = path.metadata().unwrap().len();
    let live = reminders(&h);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0]["pr"], 4000);
    assert!(path.metadata().unwrap().len() < before / 2);
    let db = rusqlite::Connection::open(path).unwrap();
    let free: i64 = db
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .unwrap();
    assert_eq!(free, 0);
}

#[cfg(not(verus_keep_ghost))]
fn push_run() -> Value {
    serde_json::from_str(include_str!("fixtures/push_run.json")).unwrap()
}

#[cfg(not(verus_keep_ghost))]
fn run_view(value: &Value) -> Value {
    json!({"prefix":["run","view"],"stdout":value.to_string()})
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn push_run_returns_verified_progress_and_ignores_unrelated_or_duplicate_events() {
    let initial = push_run();
    let mut progressed = initial.clone();
    progressed["jobs"][0]["status"] = json!("in_progress");
    let host = Host::new(vec![
        run_view(&initial),
        run_view(&initial),
        run_view(&progressed),
    ]);
    let job = json!({"action":"in_progress","repository":{"full_name":"example/host"},
        "workflow_job":{"run_id":857,"head_sha":initial["headSha"],"id":912,"run_attempt":1}});
    let mut other = job.clone();
    other["workflow_job"]["run_id"] = json!(205);
    let mut frames = Vec::new();
    let mut malformed = job.clone();
    malformed["workflow_job"]["run_attempt"] = json!("1");
    let mut no_attempt = job.clone();
    no_attempt["workflow_job"]
        .as_object_mut()
        .unwrap()
        .remove("run_attempt");
    for event in [&other, &malformed, &no_attempt, &job] {
        writeln!(frames, "{event}").unwrap();
    }
    let (exit, result) = host.run_with_input(&["run-watch", "--run", "857"], &frames, false);
    assert_eq!(exit, 3, "{result}");
    assert_eq!(result["outcome"], "changed");
    assert_eq!(result["event_queries"], 2);
    assert_eq!(result["run"]["jobs"][0]["status"], "in_progress");
    let receipts = result["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 2);
    let args = receipts[0]["argv"].as_array().unwrap();
    assert_eq!(
        args[1..7],
        [
            json!("run"),
            json!("view"),
            json!("857"),
            json!("--repo"),
            json!("example/host"),
            json!("--json")
        ]
    );
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn push_run_checks_terminal_outcomes_commit_attempt_and_channel() {
    let initial = push_run();
    for (conclusion, exit) in [
        ("success", 0),
        ("failure", 1),
        ("cancelled", 1),
        ("timed_out", 1),
        ("unexpected", 2),
    ] {
        let mut ended = initial.clone();
        ended["status"] = json!("completed");
        ended["conclusion"] = json!(conclusion);
        let host = Host::new(vec![run_view(&ended)]);
        let (actual, result) = host.run(&["run-watch", "--run", "857"]);
        assert_eq!(actual, exit, "{result}");
        assert_eq!(result["receipts"].as_array().unwrap().len(), 1);
    }
    let stale = Host::new(vec![run_view(&initial)]).run(&[
        "run-watch",
        "--run",
        "857",
        "--head-sha",
        "2222222222222222222222222222222222222222",
    ]);
    assert_eq!(stale.0, 4);
    let host = Host::new(vec![run_view(&initial)]);
    assert_eq!(host.run(&["run-watch", "--run", "857"]).0, 2);
    let mut retry = initial.clone();
    retry["attempt"] = json!(2);
    let run_event = json!({"action":"requested","repository":{"full_name":"example/host"},
        "workflow_run":{"event":"push","id":857,"run_attempt":2,"head_sha":initial["headSha"]}});
    let host = Host::new(vec![run_view(&initial), run_view(&retry)]);
    let (exit, result) = host.run_with_input(
        &["run-watch", "--run", "857"],
        format!("{run_event}\n").as_bytes(),
        false,
    );
    assert_eq!(exit, 3, "{result}");
    assert_eq!(result["run"]["attempt"], 2);
    // A new notification cannot replace the original workflow or move its attempt backwards.
    for (field, value) in [
        ("workflowDatabaseId", json!(9999)),
        ("headBranch", json!("other")),
        ("headSha", json!("2222222222222222222222222222222222222222")),
        ("attempt", json!(1)),
    ] {
        let mut wrong = retry.clone();
        wrong[field] = value;
        let host = Host::new(vec![run_view(&retry), run_view(&wrong)]);
        let (exit, result) = host.run_with_input(
            &["run-watch", "--run", "857"],
            format!("{run_event}\n").as_bytes(),
            false,
        );
        assert_eq!(exit, 2, "{result}");
        assert_eq!(result["outcome"], "unknown");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("fixed workflow identity")
        );
    }
    let mut inconsistent = initial.clone();
    inconsistent["conclusion"] = json!("success");
    let host = Host::new(vec![run_view(&inconsistent)]);
    let (exit, result) = host.run(&[
        "run-watch",
        "--run",
        "857",
        "--head-sha",
        "2222222222222222222222222222222222222222",
    ]);
    assert_eq!(exit, 2, "{result}");
    assert_eq!(result["outcome"], "unknown");
    for (key, wrong) in [
        ("event", json!("pull_request")),
        ("databaseId", json!(3)),
        (
            "url",
            json!("https://github.com/example/other/actions/runs/857"),
        ),
    ] {
        let mut invalid = initial.clone();
        invalid[key] = wrong;
        let host = Host::new(vec![run_view(&invalid)]);
        assert_eq!(host.run(&["run-watch", "--run", "857"]).0, 2);
    }
}

#[test]
#[cfg(not(verus_keep_ghost))]
fn push_run_readback_rejects_invalid_headers_before_jobs_and_orders_valid_jobs() {
    let args = ["run-watch", "--run", "857"];
    let mut run = push_run();
    run["status"] = json!("completed");
    run["conclusion"] = json!("success");
    let mut second = run["jobs"][0].clone();
    second["databaseId"] = json!(41);
    run["jobs"].as_array_mut().unwrap().push(second);
    for bad in [0, 912] {
        let mut wrong = run.clone();
        wrong["jobs"][1]["databaseId"] = json!(bad);
        let host = Host::new(vec![run_view(&wrong)]);
        let (exit, result) = host.run(&args);
        assert_eq!(exit, 2);
        assert_eq!(result["error"], "invalid workflow job identity");
        assert_eq!(host.calls().len(), 1);
        for (key, value) in [
            ("databaseId", json!(1)),
            ("workflowDatabaseId", json!(0)),
            ("attempt", json!(0)),
            ("url", json!("other")),
            ("event", json!("workflow_dispatch")),
            ("headBranch", json!("invalid..ref")),
            ("headSha", json!("invalid")),
        ] {
            let mut invalid = wrong.clone();
            invalid[key] = value;
            let host = Host::new(vec![run_view(&invalid)]);
            let (exit, result) = host.run(&args);
            assert_eq!(exit, 2);
            assert_eq!(
                result["error"],
                "GitHub returned a different or invalid push run"
            );
            assert_eq!(host.calls().len(), 1);
        }
    }
    let host = Host::new(vec![run_view(&run)]);
    let (exit, result) = host.run(&args);
    assert_eq!(exit, 0);
    assert_eq!(result["run"]["jobs"][0]["databaseId"], 41);
    assert_eq!(result["run"]["jobs"][1]["databaseId"], 912);
}
