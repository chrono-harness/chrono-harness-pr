use serde_json::{Value, json};
use std::{fs, io::Read};

// Keep the quiet worker in the mock gh group while its parent, outside the
// group, deliberately postpones reaping. The parent acknowledges registration
// before the original launcher exits so the group never disappears in between.
#[cfg(target_os = "linux")]
fn delayed_reaper(args: &[String]) {
    use std::{
        io::Write,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["late-marker", &args[1]])
        .process_group(args[3].parse().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::io::stdout().write_all(b"R").unwrap();
    std::io::stdout().flush().unwrap();
    let mut status: libc::siginfo_t = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &mut status,
                libc::WEXITED | libc::WNOWAIT,
            )
        },
        0
    );
    std::thread::sleep(std::time::Duration::from_millis(args[2].parse().unwrap()));
    assert!(child.wait().unwrap().success());
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(target_os = "linux")]
    if args.first().map(String::as_str) == Some("delayed-reaper") {
        delayed_reaper(&args);
        return;
    }
    if args.first().map(String::as_str) == Some("late-marker") {
        std::thread::sleep(std::time::Duration::from_millis(args[1].parse().unwrap()));
        fs::write("child-finished", b"done").unwrap();
        return;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let steps: Vec<Value> = serde_json::from_slice(&fs::read("steps.json").unwrap()).unwrap();
    let index: usize = fs::read_to_string("index")
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let step = steps
        .get(index)
        .unwrap_or_else(|| panic!("unexpected invocation {args:?}"));
    let mut log = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open("calls.jsonl")
        .unwrap();
    use std::io::Write;
    let mut record = serde_json::to_vec(&json!({"args":args,"stdin":input})).unwrap();
    record.push(b'\n');
    log.write_all(&record).unwrap();
    assert_eq!(
        args[0..2],
        serde_json::from_value::<Vec<String>>(step["prefix"].clone()).unwrap()
    );
    if let Some(expected) = step["stdin"].as_str() {
        assert_eq!(input, expected);
    }
    fs::write("index", (index + 1).to_string()).unwrap();
    print!("{}", step["stdout"].as_str().unwrap_or(""));
    eprint!("{}", step["stderr"].as_str().unwrap_or(""));
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    #[cfg(target_os = "linux")]
    if let Some(reap_delay) = step["reap_delay_ms"].as_u64() {
        use std::{
            os::unix::process::CommandExt,
            process::{Command, Stdio},
        };
        let delay = step["quiet_descendant_ms"].as_u64().unwrap();
        let group = unsafe { libc::getpgrp() };
        let mut helper = Command::new(std::env::current_exe().unwrap())
            .args([
                "delayed-reaper",
                &delay.to_string(),
                &reap_delay.to_string(),
                &group.to_string(),
            ])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut ready = [0; 1];
        helper
            .stdout
            .take()
            .unwrap()
            .read_exact(&mut ready)
            .unwrap();
        assert_eq!(ready, *b"R");
        return;
    }
    if let Some(delay) = step["quiet_descendant_ms"].as_u64() {
        use std::process::{Command, Stdio};
        let _child = Command::new(std::env::current_exe().unwrap())
            .arg("late-marker")
            .arg(delay.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
    }
    if let Some(ms) = step["sleep_ms"].as_u64() {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    if step["signal"] == "TERM" {
        unsafe { libc::raise(libc::SIGTERM) };
        panic!("test signal did not terminate gh");
    }
    std::process::exit(step["exit"].as_i64().unwrap_or(0) as i32);
}
