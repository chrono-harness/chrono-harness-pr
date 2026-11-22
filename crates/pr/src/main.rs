use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = chrono_pr::run(&args);
    let stdout = io::stdout();
    let descriptor = stdout.as_fd().try_clone_to_owned().map(File::from);
    let bytes = result.stdout.as_bytes();
    let mut written = 0;
    let publication = descriptor.and_then(|mut stdout| {
        loop {
            match stdout.write(&bytes[written..]) {
                Ok(0) if written < bytes.len() => break Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    written += count;
                    if written == bytes.len() {
                        break stdout.flush();
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => break Err(err),
            }
        }
    });
    if let Err(err) = publication {
        let event = serde_json::json!({"event":"pr.output_publication_failed",
            "stream":"stdout","published_bytes":written,"output_bytes":bytes.len(),
            "operation_exit_code":result.exit_code,"native_code":err.raw_os_error(),"error":err.to_string()});
        let _ = writeln!(io::stderr().lock(), "{event}");
        return ExitCode::from(2);
    }
    ExitCode::from(result.exit_code)
}
