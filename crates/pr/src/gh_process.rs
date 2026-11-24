mod group;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use subprocess::{Exec, ExecExt, JobExt, Redirection};

/// Capture one stream without accepting more bytes than the caller can return.
struct Stream {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for Stream {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let allowed = self.limit - self.bytes.len();
        if allowed == 0 && !input.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "gh output limit exceeded",
            ));
        }
        let accepted = input.len().min(allowed);
        self.bytes.extend_from_slice(&input[..accepted]);
        Ok(accepted)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) struct Call {
    pub child_pid: u32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: Option<u32>,
    pub signal: Option<i32>,
    pub failure: Option<String>,
    pub timed_out: bool,
}

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
