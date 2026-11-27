#[cfg(not(verus_keep_ghost))]
mod group;

#[cfg(not(verus_keep_ghost))]
use std::collections::BTreeMap;
#[cfg(not(verus_keep_ghost))]
use std::io::{self, Write};
#[cfg(not(verus_keep_ghost))]
use std::path::Path;
#[cfg(not(verus_keep_ghost))]
use std::time::{Duration, Instant};

#[cfg(not(verus_keep_ghost))]
use subprocess::{Exec, ExecExt, JobExt, Redirection};

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Append the portion accepted by Write. A full stream still accepts an empty write.
pub fn append_output(buffer: &mut Vec<u8>, input: &[u8], limit: usize) -> (written: Option<usize>)
    ensures
        written.is_some() == (old(buffer).len() <= limit
            && (input.len() == 0 || old(buffer).len() < limit)),
        match written {
            Some(count) => {
                &&& count == if input.len() <= limit as int - old(buffer).len() { input.len() as int }
                    else { limit as int - old(buffer).len() }
                &&& count <= input.len()
                &&& buffer@ == old(buffer)@ + input@.take(count as int)
                &&& buffer.len() <= limit
            },
            None => buffer@ == old(buffer)@,
        },
{
    if buffer.len() > limit || (buffer.len() == limit && !input.is_empty()) {
        return None;
    }
    let count = input.len().min(limit - buffer.len());
    let (prefix, _) = input.split_at(count);
    buffer.extend_from_slice(prefix);
    Some(count)
}
}

/// Capture one stream without accepting more bytes than the caller can return.
#[cfg(not(verus_keep_ghost))]
struct Stream {
    bytes: Vec<u8>,
    limit: usize,
}

#[cfg(not(verus_keep_ghost))]
impl Write for Stream {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        append_output(&mut self.bytes, input, self.limit)
            .ok_or_else(|| io::Error::new(io::ErrorKind::FileTooLarge, "gh output limit exceeded"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
pub(crate) struct Call {
    pub child_pid: u32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<u32>,
    pub signal: Option<i32>,
    pub failure: Option<String>,
    pub timed_out: bool,
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn run(
    root: &Path,
    executable: &Path,
    args: &[String],
    environment: &BTreeMap<String, String>,
    input: &[u8],
    timeout: Duration,
    limit: usize,
) -> io::Result<Call> {
    let started = Instant::now();
    let mut job = Exec::cmd(executable)
        .args(args)
        .cwd(root)
        .env_clear()
        .env_extend(environment)
        .setpgid()
        .stdin(input.to_vec())
        .stdout(Redirection::Pipe)
        .stderr(Redirection::Pipe)
        .start()?;
    let mut stdout = Stream {
        bytes: Vec::new(),
        limit,
    };
    let mut stderr = Stream {
        bytes: Vec::new(),
        limit,
    };
    let result = (|| {
        job.communicate()?
            .limit_time(timeout.saturating_sub(started.elapsed()))
            .read_to(&mut stdout, &mut stderr)?;
        group::wait(job.pid(), started + timeout)?;
        job.wait()
    })();
    let (status, failure, timed_out) = match result {
        Ok(status) => (
            Some(status),
            group::verify_empty(job.pid())
                .err()
                .map(|error| error.to_string()),
            false,
        ),
        Err(error) => {
            let timed_out = error.kind() == io::ErrorKind::TimedOut;
            // The direct child is still unreaped, so cancellation can only
            // signal the original process group.
            job.send_signal_group(libc::SIGKILL)?;
            (
                Some(job.wait()?),
                Some(if timed_out {
                    "process timed out".into()
                } else {
                    error.to_string()
                }),
                timed_out,
            )
        }
    };
    Ok(Call {
        child_pid: job.pid(),
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        exit_code: status.and_then(|status| status.code()),
        signal: status.and_then(|status| status.signal()),
        failure,
        timed_out,
    })
}
