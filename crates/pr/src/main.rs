#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

pub(crate) enum WriteStep<E> {
    Advance(usize),
    Flush,
    WriteZero,
    Retry,
    Failed(E),
}

/// File::write never reports more bytes than the supplied suffix.
pub(crate) fn write_step<E>(
    total: usize, written: usize, observation: Result<usize, (bool, E)>,
) -> (step: WriteStep<E>)
    requires
        written <= total,
        match observation { Ok(n) => n <= total - written, Err(_) => true },
    ensures
        match observation {
            Ok(n) => if written + n == total { step is Flush }
                else if n == 0 { step is WriteZero }
                else { step == WriteStep::<E>::Advance((written + n) as usize) },
            Err((interrupted, error)) => if interrupted { step is Retry }
                else { step == WriteStep::<E>::Failed(error) },
        },
        match step { WriteStep::Advance(next) => written < next < total, _ => true },
{
    match observation {
        Ok(n) => {
            let next = written + n;
            if next == total { WriteStep::Flush }
            else if n == 0 { WriteStep::WriteZero }
            else { WriteStep::Advance(next) }
        }
        Err((true, _)) => WriteStep::Retry,
        Err((false, error)) => WriteStep::Failed(error),
    }
}

pub(crate) fn publication_exit(business: u8, published: bool) -> (code: u8)
    ensures code == (if published { business } else { 2u8 }),
{
    if published { business } else { 2 }
}

}

#[cfg(not(verus_keep_ghost))]
fn main() -> std::process::ExitCode {
    use std::fs::File;
    use std::io::{self, Write};
    use std::os::fd::AsFd;
    use std::process::ExitCode;

    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = chrono_pr::run(&args);
    let stdout = io::stdout();
    let descriptor = stdout.as_fd().try_clone_to_owned().map(File::from);
    let bytes = result.stdout.as_bytes();
    let mut written = 0;
    let publication = descriptor.and_then(|mut stdout| {
        loop {
            let observation = stdout
                .write(&bytes[written..])
                .map_err(|error| (error.kind() == io::ErrorKind::Interrupted, error));
            match write_step(bytes.len(), written, observation) {
                WriteStep::Advance(next) => written = next,
                WriteStep::Flush => {
                    written = bytes.len();
                    break stdout.flush();
                }
                WriteStep::WriteZero => break Err(io::ErrorKind::WriteZero.into()),
                WriteStep::Retry => continue,
                WriteStep::Failed(error) => break Err(error),
            }
        }
    });
    let exit = publication_exit(result.exit_code, publication.is_ok());
    if let Err(err) = publication {
        let event = serde_json::json!({"event":"pr.output_publication_failed",
            "stream":"stdout","published_bytes":written,"output_bytes":bytes.len(),
            "operation_exit_code":result.exit_code,"native_code":err.raw_os_error(),"error":err.to_string()});
        let _ = writeln!(io::stderr().lock(), "{event}");
    }
    ExitCode::from(exit)
}
