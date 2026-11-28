//! One existing GitHub webhook delivery may wake a pending query. The final
//! state is read independently from GitHub for the original PR and commit.
#[cfg(not(verus_keep_ghost))]
use serde_json::Value;
#[cfg(not(verus_keep_ghost))]
use std::{
    fs::File,
    io::{self, BufRead, BufReader, IsTerminal},
    os::fd::{AsFd, AsRawFd},
    time::Instant,
};

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub enum FramePart<'a> {
    EndOfStream,
    AboveLimit,
    Bytes { payload: &'a [u8], consumed: usize, complete: bool },
}

/// LF terminates the current frame. Bytes after it stay in the input buffer.
pub fn frame_part(bytes: &[u8], retained: usize, limit: usize) -> (part: FramePart<'_>)
    ensures
        (part is EndOfStream) == bytes.is_empty(),
        match part {
            FramePart::EndOfStream => true,
            FramePart::AboveLimit => {
                &&& !bytes.is_empty()
                &&& forall|i: int| 0 <= i < bytes.len() && retained as int + i <= limit ==>
                    #[trigger] bytes@[i] != 10u8
                &&& retained as int + bytes.len() > limit
            },
            FramePart::Bytes { payload, consumed, complete } => {
                &&& !bytes.is_empty()
                &&& retained as int + payload.len() <= limit
                &&& consumed == payload.len() as int + if complete { 1int } else { 0int }
                &&& consumed <= bytes.len()
                &&& payload@ == bytes@.take(payload.len() as int)
                &&& forall|i: int| 0 <= i < payload.len() ==> #[trigger] bytes@[i] != 10u8
                &&& if complete { bytes@[payload.len() as int] == 10u8 } else { consumed == bytes.len() }
            },
        },
{
    if bytes.is_empty() { return FramePart::EndOfStream; }
    if retained > limit { return FramePart::AboveLimit; }
    let room = limit - retained;
    let mut i = 0;
    while i < bytes.len()
        invariant
            0 <= i <= bytes.len(), i <= room, room == limit - retained,
            forall|j: int| 0 <= j < i ==> #[trigger] bytes@[j] != 10u8,
        decreases bytes.len() - i,
    {
        if bytes[i] == 10u8 {
            let (payload, _) = bytes.split_at(i);
            return FramePart::Bytes { payload, consumed: i + 1, complete: true };
        }
        if i == room { return FramePart::AboveLimit; }
        i += 1;
    }
    FramePart::Bytes { payload: bytes, consumed: i, complete: false }
}
}

/// Keep the unread suffix when a single pipe write contains several deliveries.
#[cfg(not(verus_keep_ghost))]
pub(crate) struct Frames {
    input: BufReader<File>,
    deadline: Instant,
    limit: usize,
}

#[cfg(not(verus_keep_ghost))]
impl Frames {
    pub(crate) fn stdin(deadline: Instant, limit: usize) -> Result<Self, String> {
        let stdin = io::stdin();
        if stdin.is_terminal() {
            return Err("watch requires a GitHub notification stream on stdin".into());
        }
        let fd = stdin
            .as_fd()
            .try_clone_to_owned()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            input: BufReader::new(File::from(fd)),
            deadline,
            limit,
        })
    }

    pub(crate) fn next(&mut self) -> Result<Value, String> {
        let mut frame = Vec::new();
        loop {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("completion notification deadline reached".into());
            }
            if self.input.buffer().is_empty() {
                let mut fd = libc::pollfd {
                    fd: self.input.get_ref().as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                let millis = remaining
                    .as_millis()
                    .saturating_add(1)
                    .min(i32::MAX as u128) as i32;
                match unsafe { libc::poll(&mut fd, 1, millis) } {
                    -1 => {
                        let error = io::Error::last_os_error();
                        if error.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(error.to_string());
                    }
                    0 => return Err("completion notification deadline reached".into()),
                    _ => {}
                }
            }
            let bytes = match self.input.fill_buf() {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.to_string()),
            };
            let (consumed, complete) = match frame_part(bytes, frame.len(), self.limit) {
                FramePart::EndOfStream => {
                    return Err("notification channel closed before a complete frame".into());
                }
                FramePart::AboveLimit => {
                    return Err("completion notification exceeds configured byte limit".into());
                }
                FramePart::Bytes {
                    payload,
                    consumed,
                    complete,
                } => {
                    frame.extend_from_slice(payload);
                    (consumed, complete)
                }
            };
            self.input.consume(consumed);
            if complete {
                return crate::snapshot::response(&frame);
            }
        }
    }
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn pr_event(
    body: &Value,
    repository: &str,
    base: &str,
    pr: &crate::snapshot::Snapshot,
) -> Option<Value> {
    let event_repo = body.pointer("/repository/full_name")?.as_str()?;
    let head = pr.head.as_str();
    let number = pr.number;
    let required_suites = &pr.required_suite_ids;
    use crate::verified_snapshot::{self, PrNotice, PushedBranch, WatchedBranch};
    let head_ref = WatchedBranch {
        repository: &pr.head_repository,
        branch: &pr.branch,
        oid: head,
    };
    let base_ref = WatchedBranch {
        repository,
        branch: base,
        oid: &pr.base_oid,
    };
    let branch = body["ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("refs/heads/"));
    let identity = match verified_snapshot::pr_notice(
        branch.is_some(),
        event_repo,
        repository,
        body["action"].as_str().unwrap_or(""),
    ) {
        PrNotice::Push => {
            let before = body["before"].as_str()?;
            let after = body["after"].as_str()?;
            let role = match verified_snapshot::pushed_branch(
                &head_ref, &base_ref, event_repo, branch?, before, after,
            )? {
                PushedBranch::Head => "head",
                PushedBranch::Base => "base",
            };
            serde_json::json!({"event":"push","role":role,"repository":event_repo,"ref":body["ref"],"before":before,"after":after})
        }
        PrNotice::WorkflowCompleted => {
            let run = &body["workflow_run"];
            if !verified_snapshot::pr_workflow_completed(
                run["event"].as_str()?,
                run["status"].as_str()?,
                run["head_sha"].as_str()?,
                head,
            ) {
                return None;
            }
            let items = run["pull_requests"].as_array()?;
            let matching_pr = items.iter().any(|item| {
                verified_snapshot::matches_pr_number(item["number"].as_u64(), number)
                    && item["head"]["sha"]
                        .as_str()
                        .is_some_and(|observed| verified_snapshot::same_text(observed, head))
            });
            let matching_suite = run["check_suite_id"]
                .as_u64()
                .is_some_and(|id| required_suites.contains(&id));
            if !verified_snapshot::notification_belongs(
                items.is_empty(),
                matching_pr,
                matching_suite,
            ) {
                return None;
            }
            let id = verified_snapshot::positive_id(run["id"].as_u64())?;
            let mut completion = serde_json::json!({"event":"workflow_run.completed","run_id":id});
            if let Some(attempt) = run.get("run_attempt") {
                completion["run_attempt"] =
                    verified_snapshot::positive_id(attempt.as_u64())?.into();
            }
            completion
        }
        PrNotice::Synchronized => {
            let notice = &body["pull_request"];
            let after = body["after"].as_str()?;
            if !verified_snapshot::synchronized_head(
                number,
                notice["number"].as_u64()?,
                &head_ref,
                &base_ref,
                notice.pointer("/head/repo/full_name")?.as_str()?,
                notice["head"]["ref"].as_str()?,
                notice.pointer("/base/repo/full_name")?.as_str()?,
                notice["base"]["ref"].as_str()?,
                body["before"].as_str()?,
                after,
                notice["head"]["sha"].as_str()?,
            ) {
                return None;
            }
            serde_json::json!({"event":"pull_request.synchronize","role":"head","before":head,"after":after})
        }
        PrNotice::Closed => {
            let pr = &body["pull_request"];
            if !verified_snapshot::pr_closed_notice(
                pr["state"].as_str()?,
                pr["number"].as_u64(),
                number,
                pr["head"]["sha"].as_str()?,
                head,
            ) {
                return None;
            }
            serde_json::json!({"event":"pull_request.closed"})
        }
        PrNotice::Ignored => return None,
    };
    Some(identity)
}
