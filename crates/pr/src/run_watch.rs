//! A notification wakes a read of the original push run. Only GitHub's current
//! run and job observations can settle the call.
use crate::{Client, branch, notification::Frames, snapshot, verified_snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Step {
    number: u64,
    name: String,
    status: String,
    conclusion: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Job {
    database_id: u64,
    name: String,
    status: String,
    conclusion: String,
    steps: Vec<Step>,
}

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
    if run.database_id != id
        || run.workflow_database_id == 0
        || run.attempt == 0
        || run.url != expected_url
        || run.event != "push"
        || !branch(&run.head_branch)
        || !snapshot::sha(&run.head_sha)
    {
        return Err("GitHub returned a different or invalid push run".into());
    }
    run.jobs.sort_by_key(|job| job.database_id);
    if run.jobs.iter().any(|job| job.database_id == 0)
        || run
            .jobs
            .windows(2)
            .any(|pair| pair[0].database_id == pair[1].database_id)
    {
        return Err("invalid workflow job identity".into());
    }
    Ok(run)
}

fn notification(repo: &str, run: &Run, body: &Value) -> Option<Value> {
    if body.pointer("/repository/full_name")?.as_str()? != repo {
        return None;
    }
    let action = body.get("action")?.as_str()?;
    if let Some(item) = body.get("workflow_run") {
        if !matches!(action, "requested" | "in_progress" | "completed")
            || item["event"] != "push"
            || item["id"].as_u64() != Some(run.database_id)
            || item["head_sha"] != run.head_sha
            || item["run_attempt"].as_u64()? < run.attempt
        {
            return None;
        }
        return Some(json!({"event":format!("workflow_run.{action}"),"run_id":run.database_id}));
    }
    let job = body.get("workflow_job")?;
    let job_id = job["id"].as_u64().filter(|id| *id > 0)?;
    if !matches!(action, "waiting" | "queued" | "in_progress" | "completed")
        || job["run_id"].as_u64() != Some(run.database_id)
        || job["head_sha"] != run.head_sha
        || job["run_attempt"]
            .as_u64()
            .is_some_and(|attempt| attempt < run.attempt)
    {
        return None;
    }
    Some(json!({"event":format!("workflow_job.{action}"),"run_id":run.database_id,"job_id":job_id}))
}

fn outcome(run: &Run, expected: &str, changed: bool) -> Result<&'static str, String> {
    let completed = run.status == "completed";
    if completed {
        if !matches!(
            run.conclusion.as_str(),
            "success"
                | "failure"
                | "cancelled"
                | "timed_out"
                | "action_required"
                | "neutral"
                | "skipped"
                | "stale"
                | "startup_failure"
        ) {
            return Err("unknown completed workflow conclusion".into());
        }
    } else if !matches!(
        run.status.as_str(),
        "queued" | "requested" | "pending" | "waiting" | "in_progress"
    ) || !run.conclusion.is_empty()
    {
        return Err("unknown or inconsistent workflow status".into());
    }
    Ok(verified_snapshot::push_watch_outcome(
        expected != run.head_sha,
        completed,
        run.conclusion == "success",
        changed,
    ))
}

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
        if now.workflow_database_id != first.workflow_database_id
            || now.head_sha != first.head_sha
            || now.head_branch != first.head_branch
            || now.attempt < first.attempt
        {
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
