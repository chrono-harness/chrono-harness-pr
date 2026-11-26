//! A notification wakes a read of the original push run. Only GitHub's current
//! run and job observations can settle the call.
#[cfg(verus_keep_ghost)]
use super::super::{same_text, sha_format, valid_sha_text};
#[cfg(not(verus_keep_ghost))]
use crate::verified_snapshot::{same_text, valid_sha_text};
#[cfg(not(verus_keep_ghost))]
use crate::{Client, branch, notification::Frames, verified_snapshot};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, string::StringSliceAdditionalSpecFns};

#[cfg_attr(
    not(verus_keep_ghost),
    derive(Debug, Deserialize, Serialize, PartialEq, Eq)
)]
#[cfg_attr(not(verus_keep_ghost), serde(rename_all = "camelCase"))]
struct Step {
    number: u64,
    name: String,
    status: String,
    conclusion: String,
}

#[cfg_attr(
    not(verus_keep_ghost),
    derive(Debug, Deserialize, Serialize, PartialEq, Eq)
)]
#[cfg_attr(not(verus_keep_ghost), serde(rename_all = "camelCase"))]
pub(crate) struct Job {
    database_id: u64,
    name: String,
    status: String,
    conclusion: String,
    steps: Vec<Step>,
}

verus! {
impl Job {
    pub(crate) closed spec fn id(&self) -> u64 { self.database_id }
}

/// The native adapter supplies the existing Git branch parser's verdict.
pub(crate) fn valid_push_header(
    requested: u64, observed: u64, workflow: u64, attempt: u64, url: &str,
    expected_url: &str, event: &str, valid_branch: bool, head: &str,
) -> (valid: bool)
    ensures valid == (requested == observed && workflow != 0 && attempt != 0
        && url@ == expected_url@ && event@ == "push"@ && valid_branch && sha_format(head.spec_bytes())),
{
    requested == observed && workflow != 0 && attempt != 0
        && same_text(url, expected_url) && same_text(event, "push")
        && valid_branch && valid_sha_text(head)
}

pub(crate) open spec fn ordered_at(jobs: Seq<Job>, i: int) -> bool
    recommends 0 <= i < jobs.len(),
{
    jobs[i].id() > (if i == 0 { 0 } else { jobs[i - 1].id() })
}

/// The caller sorts once. Every subsequent ID must exceed the previous one.
pub(crate) fn valid_job_ids(jobs: &[Job]) -> (valid: bool)
    ensures valid == (forall|i: int| 0 <= i < jobs.len() ==>
        #[trigger] ordered_at(jobs@, i)),
{
    let mut index = 0;
    let mut previous = 0;
    while index < jobs.len()
        invariant index <= jobs.len(),
            previous == (if index == 0 { 0 } else { jobs@[index-1].id() }),
            forall|i: int| 0 <= i < index ==> #[trigger] ordered_at(jobs@, i),
        decreases jobs.len() - index,
    {
        let id = jobs[index].database_id;
        if id <= previous {
            assert(!ordered_at(jobs@, index as int));
            return false;
        }
        assert(ordered_at(jobs@, index as int));
        previous = id;
        index += 1;
    }
    true
}
}

#[cfg(not(verus_keep_ghost))]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Run {
    database_id: u64,
    workflow_database_id: u64,
    attempt: u64,
    url: String,
    event: String,
    head_branch: String,
    head_sha: String,
    status: String,
    conclusion: String,
    jobs: Vec<Job>,
}

#[cfg(not(verus_keep_ghost))]
fn read(client: &mut Client, id: u64) -> Result<Run, String> {
    let raw = client.call(vec![
        "run".into(), "view".into(), id.to_string(), "--repo".into(),
        client.config.repository.clone(), "--json".into(),
        "databaseId,workflowDatabaseId,attempt,url,event,headBranch,headSha,status,conclusion,jobs".into(),
    ], &[])?;
    let mut run: Run =
        serde_json::from_value(raw).map_err(|e| format!("invalid workflow run: {e}"))?;
    let expected_url = format!(
        "https://github.com/{}/actions/runs/{id}",
        client.config.repository
    );
    if !valid_push_header(
        id,
        run.database_id,
        run.workflow_database_id,
        run.attempt,
        &run.url,
        &expected_url,
        &run.event,
        branch(&run.head_branch),
        &run.head_sha,
    ) {
        return Err("GitHub returned a different or invalid push run".into());
    }
    run.jobs.sort_by_key(|job| job.database_id);
    if !valid_job_ids(&run.jobs) {
        return Err("invalid workflow job identity".into());
    }
    Ok(run)
}

#[cfg(not(verus_keep_ghost))]
fn notification(repo: &str, run: &Run, body: &Value) -> Option<Value> {
    use verified_snapshot::{PushNotice, push_notice_matches};

    let repository = body.pointer("/repository/full_name")?.as_str()?;
    let action = body.get("action")?.as_str()?;
    let (record, run_id, notice, job_id) = if let Some(item) = body.get("workflow_run") {
        (
            item,
            item["id"].as_u64()?,
            PushNotice::Run {
                event: item["event"].as_str()?,
            },
            None,
        )
    } else {
        let job = body.get("workflow_job")?;
        let id = job["id"].as_u64()?;
        (
            job,
            job["run_id"].as_u64()?,
            PushNotice::Job { id },
            Some(id),
        )
    };
    let attempt = match record.get("run_attempt") {
        Some(value) => Some(value.as_u64()?),
        None => None,
    };
    if !push_notice_matches(
        repository,
        repo,
        run_id,
        run.database_id,
        record["head_sha"].as_str()?,
        &run.head_sha,
        attempt,
        run.attempt,
        action,
        notice,
    ) {
        return None;
    }
    Some(match job_id {
        Some(id) => json!({"event":format!("workflow_job.{action}"),"run_id":run_id,"job_id":id}),
        None => json!({"event":format!("workflow_run.{action}"),"run_id":run_id}),
    })
}

#[cfg(not(verus_keep_ghost))]
fn outcome(run: &Run, expected: &str, changed: bool) -> Result<&'static str, String> {
    verified_snapshot::workflow_outcome(
        &run.status,
        &run.conclusion,
        &run.head_sha,
        expected,
        changed,
    )
    .map_err(str::to_owned)
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn watch(client: &mut Client, id: u64, expected: Option<&str>) -> Result<(), String> {
    client.result["run_id"] = json!(id);
    let first = read(client, id)?;
    let head = expected.unwrap_or(&first.head_sha);
    client.result["expected_head"] = json!(head);
    client.result["observed_head"] = json!(first.head_sha);
    client.result["run"] = json!(first);
    let initial = outcome(&first, head, false)?;
    if initial != "pending" {
        client.result["outcome"] = json!(initial);
        return Ok(());
    }
    let mut frames = Frames::stdin(
        client.started + client.budget,
        client.config.output_limit_bytes,
    )?;
    let mut reads = 0u64;
    loop {
        let body = frames.next()?;
        let Some(event) = notification(&client.config.repository, &first, &body) else {
            continue;
        };
        let receipts = client.result["receipts"].as_array_mut().unwrap();
        receipts.truncate(1);
        reads += 1;
        client.result["event_queries"] = json!(reads);
        client.result["notification"] = event;
        let now = read(client, id)?;
        if !verified_snapshot::same_workflow_attempt(
            first.workflow_database_id,
            now.workflow_database_id,
            &first.head_sha,
            &now.head_sha,
            &first.head_branch,
            &now.head_branch,
            first.attempt,
            now.attempt,
        ) {
            return Err(
                "fixed workflow identity changed or GitHub returned an older attempt".into(),
            );
        }
        let state = outcome(&now, head, now != first)?;
        if state != "pending" {
            client.result["run"] = json!(now);
            client.result["outcome"] = json!(state);
            return Ok(());
        }
    }
}
