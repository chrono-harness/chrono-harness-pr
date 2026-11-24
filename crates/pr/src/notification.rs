//! One existing GitHub webhook delivery may wake a pending query. The final
//! state is read independently from GitHub for the original PR and commit.
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufRead, BufReader, IsTerminal},
    os::fd::{AsFd, AsRawFd},
    time::Instant,
};

/// Keep the unread suffix when a single pipe write contains several deliveries.
pub(crate) struct Frames {
    input: BufReader<File>,
    deadline: Instant,
    limit: usize,
}

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
            if bytes.is_empty() {
                return Err("notification channel closed before a complete frame".into());
            }
            let end = bytes.iter().position(|b| *b == b'\n');
            let length = end.unwrap_or(bytes.len());
            if frame.len().saturating_add(length) > self.limit {
                return Err("completion notification exceeds configured byte limit".into());
            }
            frame.extend_from_slice(&bytes[..length]);
            self.input.consume(length + usize::from(end.is_some()));
            if end.is_some() {
                return crate::snapshot::response(&frame);
            }
        }
    }
}

pub(crate) fn wait(
    repo: &str,
    number: u64,
    head: &str,
    required_suites: &BTreeSet<u64>,
    deadline: Instant,
    limit: usize,
) -> Result<Value, String> {
    let body = Frames::stdin(deadline, limit)?.next()?;
    if body
        .pointer("/repository/full_name")
        .and_then(Value::as_str)
        != Some(repo)
    {
        return Err("completion notification repository differs".into());
    }
    let run = &body["workflow_run"];
    let identity = match body["action"].as_str() {
        Some("completed")
            if run["event"] == "pull_request"
                && run["status"] == "completed"
                && run["head_sha"] == head
                && run["pull_requests"].as_array().is_some_and(|items| {
                    let matching_pr = items.iter().any(|item| {
                        item["number"].as_u64() == Some(number) && item["head"]["sha"] == head
                    });
                    let matching_suite = run["check_suite_id"]
                        .as_u64()
                        .is_some_and(|id| required_suites.contains(&id));
                    crate::verified_snapshot::notification_belongs(
                        items.is_empty(),
                        matching_pr,
                        matching_suite,
                    )
                }) =>
        {
            serde_json::json!({"event":"workflow_run.completed","run_id":run["id"].as_u64().filter(|id| *id > 0).ok_or("notification lacks workflow run ID")?})
        }
        Some("closed")
            if body["pull_request"]["state"] == "closed"
                && body["pull_request"]["number"].as_u64() == Some(number)
                && body["pull_request"]["head"]["sha"] == head =>
        {
            serde_json::json!({"event":"pull_request.closed"})
        }
        _ => return Err("completion notification does not match the requested PR and head".into()),
    };
    Ok(identity)
}
