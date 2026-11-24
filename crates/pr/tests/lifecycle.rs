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
        assert_eq!(result["schema"], "chrono-pr-result/v3");
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
        fresh["data"]["repository"]["pullRequest"]["baseRefOid"] = json!(next);
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
        let host = Host::new(vec![api(initial), api(advanced)]);
        let (code, result) = host.watch(&event);
        assert_eq!((code, result["observed_head"].as_str()), (4, Some(OTHER)));
        assert_eq!(result["head_repository"], "fork/host");
    }
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
    let mut ambiguous = api(snapshot());
    ambiguous["stdout"] = json!(ambiguous["stdout"].as_str().unwrap().replacen(
        "\"number\":42",
        "\"number\":1,\"number\":42",
        1
    ));
    let h = Host::new(vec![api(snapshot()), ambiguous]);
    assert_eq!(h.status().0, 0);
    assert_eq!(h.status().0, 2);
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
