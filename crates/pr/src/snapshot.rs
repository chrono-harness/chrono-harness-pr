#[cfg(not(verus_keep_ghost))]
use crate::verified_snapshot::{self, CheckClass, CheckContext, CheckRunState};
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(verus_keep_ghost))]
pub(crate) const QUERY: &str = r#"query($owner:String!,$name:String!,$number:Int!,$head:GitObjectID!) {
  repository(owner:$owner,name:$name) {
    nameWithOwner
    pullRequest(number:$number) {
      number url state headRefOid headRefName baseRefOid baseRefName mergeable isDraft
      headRepository { nameWithOwner }
      baseRef { target { oid } }
      headRef { target { oid } }
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
fn text_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn field<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, String> {
    text_at(value, pointer).ok_or_else(|| format!("invalid remote field {pointer}"))
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn repository(value: &Value, expected: &str) -> Result<(), String> {
    let errors = value.get("errors").is_some_and(|errors| {
        verified_snapshot::graphql_errors(errors.is_null(), errors.as_array().map(Vec::len))
    });
    if !verified_snapshot::repository_envelope(value.is_object(), errors)
        || !verified_snapshot::same_text(field(value, "/data/repository/nameWithOwner")?, expected)
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
    pub head_repository: String,
    pub base_oid: String,
    pub mergeable: String,
    pub pr_base_oid: String,
    pub pr_head_oid: String,
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
    if !verified_snapshot::matches_pr_number(pr["number"].as_u64(), number)
        || !verified_snapshot::same_text(field(pr, "/baseRefName")?, base)
    {
        return Err("PR number, base or head repository mismatch".into());
    }
    let head_repository = field(pr, "/headRepository/nameWithOwner")?;
    if !verified_snapshot::head_repository_name(head_repository.as_bytes()) {
        return Err("invalid PR head repository".into());
    }
    let base_oid = field(pr, "/baseRefOid")?;
    let mergeable = field(pr, "/mergeable")?;
    if !verified_snapshot::valid_base_and_mergeability(base_oid, mergeable) {
        return Err("invalid PR base SHA or mergeability".into());
    }
    let observed = field(pr, "/headRefOid")?;
    if !verified_snapshot::valid_sha_text(observed) {
        return Err("invalid PR head SHA".into());
    }
    let state = field(pr, "/state")?;
    if !verified_snapshot::valid_pr_state(state) {
        return Err("unknown PR state".into());
    }
    let (selected_base, selected_head) = verified_snapshot::snapshot_branch_oids(
        state,
        base_oid,
        observed,
        text_at(pr, "/baseRef/target/oid"),
        text_at(pr, "/headRef/target/oid"),
    )?;
    let url = field(pr, "/url")?;
    let expected_url = format!("https://github.com/{repo}/pull/{number}");
    if !verified_snapshot::same_text(url, &expected_url) {
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
        head: selected_head.into(),
        pr_head_oid: observed.into(),
        pr_base_oid: base_oid.into(),
        branch: field(pr, "/headRefName")?.into(),
        head_repository: head_repository.into(),
        base_oid: selected_base.into(),
        mergeable: mergeable.into(),
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
    if verified_snapshot::open_conflict(state, &result.mergeable) {
        result.outcome = "conflict";
        return Ok(());
    }
    let commit = &value["data"]["repository"]["object"];
    if !verified_snapshot::same_text(field(commit, "/oid")?, head) {
        return Err("check commit does not match expected head".into());
    }
    let rollup = commit
        .get("statusCheckRollup")
        .ok_or("missing check rollup field")?;
    let mut current = BTreeMap::new();
    let mut required_suites: BTreeMap<&str, (Option<u64>, BTreeSet<u64>)> = BTreeMap::new();
    let mut suites_by_check = BTreeMap::new();
    let mut suites: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
    if !rollup.is_null() {
        let contexts = &rollup["contexts"];
        complete(contexts)?;
        for node in contexts["nodes"].as_array().ok_or("invalid check nodes")? {
            let (name, state) = match verified_snapshot::check_context(field(node, "/__typename")?)
            {
                CheckContext::Run => {
                    let id = verified_snapshot::positive_id(node["databaseId"].as_u64())
                        .ok_or("invalid check ID")?;
                    if !verified_snapshot::new_check_id(suites_by_check.get(&id).copied()) {
                        return Err("duplicate check ID".into());
                    }
                    let name = field(node, "/name")?;
                    let status = field(node, "/status")?;
                    let state = match verified_snapshot::check_run_state(
                        status,
                        node.get("conclusion").is_some_and(Value::is_null),
                    ) {
                        CheckRunState::Completed => {
                            let conclusion = field(node, "/conclusion")?;
                            if !verified_snapshot::supported_run_conclusion(conclusion) {
                                return Err("invalid completed check conclusion".into());
                            }
                            conclusion
                        }
                        CheckRunState::Pending => status,
                        CheckRunState::PendingWithConclusion => {
                            return Err("pending check has a conclusion".into());
                        }
                        CheckRunState::Unknown => return Err("unknown check status".into()),
                    };
                    if !verified_snapshot::same_text(field(node, "/checkSuite/commit/oid")?, head) {
                        return Err("check suite commit mismatch".into());
                    }
                    let latest = &node["checkSuite"]["checkRuns"];
                    complete(latest)?;
                    let mut latest_ids = BTreeSet::new();
                    for row in latest["nodes"].as_array().ok_or("invalid latest checks")? {
                        let latest_id = verified_snapshot::positive_id(row["databaseId"].as_u64())
                            .ok_or("invalid latest check ID")?;
                        if !verified_snapshot::new_check_id(latest_ids.get(&latest_id).copied()) {
                            return Err("duplicate latest check ID".into());
                        }
                        latest_ids.insert(latest_id);
                    }
                    let latest_ids: Vec<_> = latest_ids.into_iter().collect();
                    if !verified_snapshot::nonempty_latest_ids(&latest_ids) {
                        return Err("empty latest check set".into());
                    }
                    let suite_id =
                        verified_snapshot::positive_id(node["checkSuite"]["databaseId"].as_u64())
                            .ok_or("invalid check suite ID")?;
                    suites_by_check.insert(id, suite_id);
                    if let Some(previous) = suites.get(&suite_id) {
                        if !verified_snapshot::canonical_set_equal(previous, &latest_ids) {
                            return Err("inconsistent latest check set".into());
                        }
                    }
                    let latest = verified_snapshot::contains_check_id(&latest_ids, id);
                    suites.entry(suite_id).or_insert(latest_ids);
                    if !latest {
                        continue;
                    }
                    (name, state)
                }
                CheckContext::Status => {
                    if !verified_snapshot::same_text(field(node, "/commit/oid")?, head) {
                        return Err("status commit mismatch".into());
                    }
                    let state = field(node, "/state")?;
                    if !verified_snapshot::supported_commit_status(state) {
                        return Err("unknown commit status".into());
                    }
                    (field(node, "/context")?, state)
                }
                CheckContext::Unknown => return Err("unknown check context type".into()),
            };
            if !verified_snapshot::nonempty_check_name(name) {
                return Err("empty or ambiguous check name".into());
            }
            if !required
                .iter()
                .any(|required_name| verified_snapshot::same_text(required_name, name))
            {
                continue;
            }
            let suite = &node["checkSuite"];
            let workflow = verified_snapshot::positive_id(
                suite
                    .pointer("/workflowRun/workflow/databaseId")
                    .and_then(Value::as_u64),
            );
            let (first_workflow, sources) = required_suites.entry(name).or_default();
            let new_suite = suite["databaseId"]
                .as_u64()
                .is_some_and(|id| sources.insert(id));
            if let Some(previous) = current.insert(name, state) {
                if !verified_snapshot::compatible_required_success(
                    previous,
                    state,
                    *first_workflow,
                    workflow,
                    new_suite,
                ) {
                    return Err("empty or ambiguous check name".into());
                }
            } else {
                *first_workflow = workflow;
            }
        }
    }
    for (suite, ids) in suites {
        let observed_suites: Vec<_> = ids
            .iter()
            .map(|id| suites_by_check.get(id).copied())
            .collect();
        if !verified_snapshot::latest_members_belong(suite, &observed_suites) {
            return Err("latest check missing from complete rollup".into());
        }
    }
    let mut states = Vec::with_capacity(required.len());
    for name in required {
        let state = verified_snapshot::missing_check_state(current.get(name.as_str()).copied());
        if let CheckClass::Unknown = verified_snapshot::classify_check(state) {
            return Err(format!("unknown check conclusion {state}"));
        }
        states.push(state);
        result.checks.push(json!({"name":name,"state":state}));
    }
    result.required_suite_ids = required_suites
        .into_values()
        .flat_map(|(_, ids)| ids)
        .collect();
    let checks = verified_snapshot::fold_required_checks(&states);
    result.outcome = verified_snapshot::snapshot_outcome_label(
        verified_snapshot::snapshot_mergeability(state, &result.mergeable, checks),
    );
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
fn complete(value: &Value) -> Result<(), String> {
    let has_next = value
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool);
    if !verified_snapshot::complete_check_page(has_next) {
        return Err("missing or truncated check page; complete facts required".into());
    }
    Ok(())
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
    use json_syntax::{JsonFragment, JsonParse, JsonValue};
    let parsed = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let (syntax, _) = JsonValue::parse_slice(bytes).map_err(|e| e.to_string())?;
    for (_, fragment) in syntax.traverse() {
        if let JsonFragment::Value(JsonValue::Object(object)) = fragment {
            for (key, _) in object.entries() {
                if object.redundant_index_of(key).is_some() {
                    return Err(format!("duplicate JSON member {key}"));
                }
            }
        }
    }
    Ok(parsed)
}
