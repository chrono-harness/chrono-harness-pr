#[cfg(not(verus_keep_ghost))]
use crate::verified_snapshot;
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(verus_keep_ghost))]
pub(crate) const QUERY: &str = r#"query($owner:String!,$name:String!,$number:Int!,$head:GitObjectID!) {
  repository(owner:$owner,name:$name) {
    nameWithOwner
    pullRequest(number:$number) {
      number url state headRefOid headRefName baseRefName isDraft
      headRepository { nameWithOwner }
      mergeCommit { oid }
    }
    object(oid:$head) { ... on Commit {
      oid statusCheckRollup { contexts(first:100) {
        pageInfo { hasNextPage }
        nodes {
          __typename
          ... on CheckRun {
            databaseId name status conclusion
            checkSuite {
              databaseId
              workflowRun { workflow { databaseId } }
              commit { oid }
              checkRuns(first:100,filterBy:{checkType:LATEST}) {
                pageInfo { hasNextPage }
                nodes { databaseId }
              }
            }
          }
          ... on StatusContext { context state commit { oid } }
        }
      } }
    } }
  }
}"#;

#[cfg(not(verus_keep_ghost))]
pub(crate) fn field<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("invalid remote field {pointer}"))
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn repository(value: &Value, expected: &str) -> Result<(), String> {
    if !value.is_object()
        || value
            .get("errors")
            .is_some_and(|e| !e.is_null() && e.as_array().is_none_or(|a| !a.is_empty()))
        || field(value, "/data/repository/nameWithOwner")? != expected
    {
        return Err("GitHub returned errors or a different repository".into());
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn sha(value: &str) -> bool {
    verified_snapshot::valid_sha_text(value)
}

#[cfg(not(verus_keep_ghost))]
pub(crate) struct Snapshot {
    pub number: u64,
    pub url: String,
    pub head: String,
    pub branch: String,
    pub state: String,
    pub draft: bool,
    pub merge_commit: Option<String>,
    pub checks: Vec<Value>,
    pub required_suite_ids: BTreeSet<u64>,
    pub outcome: &'static str,
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn pull_request(
    value: &Value,
    repo: &str,
    base: &str,
    number: u64,
) -> Result<Snapshot, String> {
    repository(value, repo)?;
    let pr = &value["data"]["repository"]["pullRequest"];
    if pr["number"].as_u64() != Some(number)
        || field(pr, "/baseRefName")? != base
        || field(pr, "/headRepository/nameWithOwner")? != repo
    {
        return Err("PR number, base or head repository mismatch".into());
    }
    let observed = field(pr, "/headRefOid")?;
    if !verified_snapshot::valid_sha_text(observed) {
        return Err("invalid PR head SHA".into());
    }
    let state = field(pr, "/state")?;
    if !verified_snapshot::valid_pr_state(state) {
        return Err("unknown PR state".into());
    }
    let url = field(pr, "/url")?;
    let expected_url = format!("https://github.com/{repo}/pull/{number}");
    if url != expected_url {
        return Err("PR URL mismatch (only github.com is supported)".into());
    }
    let merge_commit = if pull_request_merged(state) {
        let oid = field(pr, "/mergeCommit/oid")?;
        if !verified_snapshot::valid_sha_text(oid) {
            return Err("invalid actual merge commit".into());
        }
        Some(oid.to_owned())
    } else {
        if !pr.get("mergeCommit").is_some_and(Value::is_null) {
            return Err("unmerged PR has a merge commit".into());
        }
        None
    };
    Ok(Snapshot {
        number,
        url: url.into(),
        head: observed.into(),
        branch: field(pr, "/headRefName")?.into(),
        state: state.into(),
        merge_commit,
        draft: pr["isDraft"].as_bool().ok_or("invalid draft state")?,
        checks: vec![],
        required_suite_ids: BTreeSet::new(),
        outcome: "pending",
    })
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn evaluate_checks(
    value: &Value,
    result: &mut Snapshot,
    head: &str,
    required: &[String],
) -> Result<(), String> {
    let observed = result.head.as_str();
    let state = result.state.as_str();
    if verified_snapshot::observed_head_is_stale(observed, head) {
        result.outcome = "stale";
        return Ok(());
    }
    if verified_snapshot::pull_request_closed(state) {
        result.outcome = "closed";
        return Ok(());
    }
    let commit = &value["data"]["repository"]["object"];
    if field(commit, "/oid")? != head {
        return Err("check commit does not match expected head".into());
    }
    let rollup = commit
        .get("statusCheckRollup")
        .ok_or("missing check rollup field")?;
    let mut current = BTreeMap::new();
    let mut required_suites: BTreeMap<&str, (Option<u64>, BTreeSet<u64>)> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut suites = BTreeMap::new();
    if !rollup.is_null() {
        let contexts = &rollup["contexts"];
        complete(contexts)?;
        for node in contexts["nodes"].as_array().ok_or("invalid check nodes")? {
            let (name, state) = match field(node, "/__typename")? {
                "CheckRun" => {
                    let id = node["databaseId"]
                        .as_u64()
                        .filter(|id| *id > 0)
                        .ok_or("invalid check ID")?;
                    if !ids.insert(id) {
                        return Err("duplicate check ID".into());
                    }
                    let name = field(node, "/name")?;
                    let status = field(node, "/status")?;
                    let state = match status {
                        "COMPLETED" => {
                            let conclusion = field(node, "/conclusion")?;
                            if ![
                                "SUCCESS",
                                "FAILURE",
                                "CANCELLED",
                                "TIMED_OUT",
                                "ACTION_REQUIRED",
                                "STARTUP_FAILURE",
                                "STALE",
                                "NEUTRAL",
                                "SKIPPED",
                            ]
                            .contains(&conclusion)
                            {
                                return Err("invalid completed check conclusion".into());
                            }
                            conclusion
                        }
                        "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED" | "PENDING" => {
                            if !node.get("conclusion").is_some_and(Value::is_null) {
                                return Err("pending check has a conclusion".into());
                            }
                            status
                        }
                        _ => return Err("unknown check status".into()),
                    };
                    classify(state)?;
                    if field(node, "/checkSuite/commit/oid")? != head {
                        return Err("check suite commit mismatch".into());
                    }
                    let latest = &node["checkSuite"]["checkRuns"];
                    complete(latest)?;
                    let mut latest_ids = BTreeSet::new();
                    for row in latest["nodes"].as_array().ok_or("invalid latest checks")? {
                        let latest_id = row["databaseId"]
                            .as_u64()
                            .filter(|id| *id > 0)
                            .ok_or("invalid latest check ID")?;
                        if !latest_ids.insert(latest_id) {
                            return Err("duplicate latest check ID".into());
                        }
                    }
                    if latest_ids.is_empty() {
                        return Err("empty latest check set".into());
                    }
                    let suite_id = node["checkSuite"]["databaseId"]
                        .as_u64()
                        .filter(|id| *id > 0)
                        .ok_or("invalid check suite ID")?;
                    if suites
                        .insert(suite_id, latest_ids.clone())
                        .is_some_and(|previous| previous != latest_ids)
                    {
                        return Err("inconsistent latest check set".into());
                    }
                    if !latest_ids.contains(&id) {
                        continue;
                    }
                    (name, state)
                }
                "StatusContext" => {
                    if field(node, "/commit/oid")? != head {
                        return Err("status commit mismatch".into());
                    }
                    let state = field(node, "/state")?;
                    if !["SUCCESS", "FAILURE", "ERROR", "PENDING", "EXPECTED"].contains(&state) {
                        return Err("unknown commit status".into());
                    }
                    (field(node, "/context")?, state)
                }
                _ => return Err("unknown check context type".into()),
            };
            if name.is_empty() {
                return Err("empty or ambiguous check name".into());
            }
            if !required.iter().any(|required_name| required_name == name) {
                continue;
            }
            let suite = &node["checkSuite"];
            let workflow = suite
                .pointer("/workflowRun/workflow/databaseId")
                .and_then(Value::as_u64)
                .filter(|id| *id > 0);
            let (first_workflow, sources) = required_suites.entry(name).or_default();
            let new_suite = suite["databaseId"]
                .as_u64()
                .is_some_and(|id| sources.insert(id));
            if let Some(previous) = current.insert(name, state) {
                if previous != "SUCCESS"
                    || state != "SUCCESS"
                    || workflow.is_none()
                    || *first_workflow != workflow
                    || !new_suite
                {
                    return Err("empty or ambiguous check name".into());
                }
            } else {
                *first_workflow = workflow;
            }
        }
    }
    if suites.values().any(|latest| !latest.is_subset(&ids)) {
        return Err("latest check missing from complete rollup".into());
    }
    let mut failed = false;
    let mut waiting = false;
    for name in required {
        let state = current.get(name.as_str()).copied().unwrap_or("MISSING");
        match classify(state)? {
            "failed" => failed = true,
            "pending" => waiting = true,
            _ => {}
        }
        result.checks.push(json!({"name":name,"state":state}));
    }
    result.required_suite_ids = required_suites
        .into_values()
        .flat_map(|(_, ids)| ids)
        .collect();
    result.outcome =
        verified_snapshot::snapshot_outcome_label(verified_snapshot::snapshot_outcome(
            verified_snapshot::observed_head_is_stale(observed, head),
            verified_snapshot::pull_request_closed(state),
            failed,
            waiting,
        ));
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
fn complete(value: &Value) -> Result<(), String> {
    if value
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err("missing or truncated check page; complete facts required".into());
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
fn classify(state: &str) -> Result<&'static str, String> {
    let class = verified_snapshot::classify_check(state);
    match class {
        verified_snapshot::CheckClass::Unknown => Err(format!("unknown check conclusion {state}")),
        _ => Ok(verified_snapshot::check_class_label(class)),
    }
}

#[cfg(verus_keep_ghost)]
use super::super::same_text;
#[cfg(not(verus_keep_ghost))]
use crate::verified_snapshot::same_text;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::string::StringSliceAdditionalSpecFns;

verus! {
pub fn pull_request_merged(state: &str) -> (merged: bool)
    ensures merged == (state@ == "MERGED"@),
{ same_text(state, "MERGED") }
}

/// Keep serde_json's depth and number limits; inspect object entries before
/// accepting a response, because selecting one of two remote facts is ambiguous.
#[cfg(not(verus_keep_ghost))]
pub(crate) fn response(bytes: &[u8]) -> Result<Value, String> {
    use json_syntax::{FragmentRef, Parse};
    let parsed = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let (syntax, _) = json_syntax::Value::parse_slice(bytes).map_err(|e| e.to_string())?;
    for (_, fragment) in syntax.traverse() {
        if let FragmentRef::Value(json_syntax::Value::Object(object)) = fragment {
            for entry in object.entries() {
                if object.redundant_index_of(&entry.key).is_some() {
                    return Err(format!("duplicate JSON member {}", entry.key));
                }
            }
        }
    }
    Ok(parsed)
}
