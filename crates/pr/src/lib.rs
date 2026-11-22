#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
mod gh_process;
#[cfg(not(verus_keep_ghost))]
mod reminders;
#[path = "snapshot.rs"]
pub(crate) mod snapshot;
#[cfg(not(verus_keep_ghost))]
pub mod verified_snapshot;

#[cfg(not(verus_keep_ghost))]
pub struct CliOutput {
    pub exit_code: u8,
    pub stdout: String,
    pub stderr: String,
}
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[cfg(not(verus_keep_ghost))]
const HELP: &str = "chrono-pr pr-open --head BRANCH --head-sha SHA --title TITLE --body-file FILE [--draft]\nchrono-pr pr-status --pr NUMBER --head-sha SHA\nchrono-pr pr-watch --pr NUMBER --head-sha SHA\nchrono-pr pr-reminders [--host-root ROOT]\nAll remote commands: [--host-root ROOT] [--config .chrono-harness/pr.json]\nResults: JSON; exits 0 complete, 1 failed/closed, 2 unknown/error, 3 pending, 4 stale.\n";

#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    repository: String,
    base: String,
    gh: String,
    required_checks: Vec<String>,
    api_timeout_seconds: u64,
    watch_timeout_seconds: u64,
    watch_interval_seconds: u64,
    output_limit_bytes: usize,
}

#[cfg(not(verus_keep_ghost))]
impl Config {
    fn validate(&self) -> Result<(), String> {
        let parts = self.repository.split('/').collect::<Vec<_>>();
        if self.schema != "chrono-pr-config/v3"
            || parts.len() != 2
            || parts.iter().any(|s| {
                s.is_empty()
                    || !s.as_bytes()[0].is_ascii_alphanumeric()
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            })
        {
            return Err("invalid config schema or owner/repository".into());
        }
        if !branch(&self.base)
            || self.gh.is_empty()
            || self.gh.contains('\0')
            || self.required_checks.is_empty()
            || self
                .required_checks
                .iter()
                .any(|s| s.trim().is_empty() || s.chars().any(char::is_control))
            || self
                .required_checks
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.required_checks.len()
            || !(1..=3600).contains(&self.api_timeout_seconds)
            || !(1..=86400).contains(&self.watch_timeout_seconds)
            || !(60..=3600).contains(&self.watch_interval_seconds)
            || !(4096..=16777216).contains(&self.output_limit_bytes)
        {
            return Err("invalid PR config: branch, tool, checks or resource bounds (watch interval >=60 seconds)".into());
        }
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
struct Args {
    command: String,
    values: BTreeMap<String, String>,
    draft: bool,
}
#[cfg(not(verus_keep_ghost))]
impl Args {
    fn parse(raw: &[OsString]) -> Result<Self, String> {
        let raw = raw
            .iter()
            .map(|s| s.to_str().ok_or("arguments must be UTF-8"))
            .collect::<Result<Vec<_>, _>>()?;
        let command = *raw.first().ok_or("missing command; use --help")?;
        if !["pr-open", "pr-status", "pr-watch"].contains(&command) {
            return Err("unknown command; use --help".into());
        }
        let mut result = Self {
            command: command.into(),
            values: BTreeMap::new(),
            draft: false,
        };
        let mut index = 1;
        while index < raw.len() {
            let flag = raw[index];
            if flag == "--draft" && command == "pr-open" {
                if result.draft {
                    return Err("duplicate --draft".into());
                }
                result.draft = true;
                index += 1;
                continue;
            }
            let common = ["--host-root", "--config", "--head-sha"];
            let specific = if command == "pr-open" {
                &["--head", "--title", "--body-file"][..]
            } else {
                &["--pr"][..]
            };
            if !common.contains(&flag) && !specific.contains(&flag) {
                return Err(format!("unknown option {flag}"));
            }
            let value = *raw
                .get(index + 1)
                .ok_or_else(|| format!("missing value for {flag}"))?;
            if value.is_empty()
                || value.contains('\0')
                || value.starts_with("--")
                || result.values.insert(flag.into(), value.into()).is_some()
            {
                return Err(format!("invalid or repeated option {flag}"));
            }
            index += 2;
        }
        if !snapshot::sha(result.get("--head-sha")?) {
            return Err("--head-sha must be 40 lowercase hex digits".into());
        }
        if command == "pr-open" {
            if !branch(result.get("--head")?)
                || result.get("--title")?.trim().is_empty()
                || result.get("--title")?.chars().any(char::is_control)
            {
                return Err("invalid head branch or title".into());
            }
            result.get("--body-file")?;
        } else {
            result.number()?;
        }
        Ok(result)
    }
    fn get(&self, key: &str) -> Result<&str, String> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("missing {key}"))
    }
    fn number(&self) -> Result<u64, String> {
        let value = self.get("--pr")?;
        if !value.bytes().all(|b| b.is_ascii_digit()) {
            return Err("invalid PR number".into());
        }
        value
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0 && *n <= i32::MAX as u64)
            .ok_or("invalid PR number".into())
    }
}

#[cfg(not(verus_keep_ghost))]
fn branch(value: &str) -> bool {
    !value.is_empty()
        && value != "@"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.contains("//")
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && value
            .split('/')
            .all(|p| !p.starts_with('.') && !p.ends_with('.') && !p.ends_with(".lock"))
}

#[cfg(not(verus_keep_ghost))]
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    fs::File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > limit {
        return Err(format!("{} exceeds byte limit", path.display()));
    }
    Ok(data)
}

#[cfg(not(verus_keep_ghost))]
struct Client {
    root: PathBuf,
    config: Config,
    executable: String,
    env: BTreeMap<String, String>,
    started: Instant,
    budget: Duration,
    pending_deadline: bool,
    result: Value,
}

#[cfg(not(verus_keep_ghost))]
impl Client {
    fn new(args: &Args, result: Value) -> Result<Self, String> {
        let root = fs::canonicalize(
            args.values
                .get("--host-root")
                .map(String::as_str)
                .unwrap_or("."),
        )
        .map_err(|e| e.to_string())?;
        let config_path = args
            .values
            .get("--config")
            .map(String::as_str)
            .unwrap_or(".chrono-harness/pr.json");
        if !Path::new(config_path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err("config must be relative to the host".into());
        }
        let path = root.join(config_path);
        if fs::canonicalize(&path).map_err(|e| e.to_string())? != path {
            return Err("config path must not contain symlinks".into());
        }
        let config_bytes = read_bounded(&path, 1048576)?;
        let config: Config = serde_json::from_slice(&config_bytes).map_err(|e| e.to_string())?;
        config.validate()?;
        let mut env = BTreeMap::new();
        for key in [
            "PATH",
            "HOME",
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_CONFIG_DIR",
            "SSH_AUTH_SOCK",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "NO_PROXY",
            "TMPDIR",
        ] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.into(), value);
            }
        }
        env.extend([
            ("GH_PROMPT_DISABLED".into(), "1".into()),
            ("GH_HOST".into(), "github.com".into()),
            ("GH_PAGER".into(), "cat".into()),
            ("NO_COLOR".into(), "1".into()),
        ]);
        let executable = which::which_in(&config.gh, env.get("PATH"), &root)
            .map_err(|e| format!("cannot resolve {}: {e}", config.gh))?;
        let budget = Duration::from_secs(if args.command == "pr-watch" {
            config.watch_timeout_seconds
        } else {
            config.api_timeout_seconds.saturating_mul(4)
        });
        let mut client = Self {
            root,
            config,
            executable: executable.to_str().ok_or("gh path must be UTF-8")?.into(),
            env,
            started: Instant::now(),
            budget,
            pending_deadline: false,
            result,
        };
        if let Err(err) = reminders::expire(&client.root) {
            client.result["reminder_error"] = json!(err.to_string());
        }
        client.result["repository"] = json!(client.config.repository);
        client.result["base"] = json!(client.config.base);
        Ok(client)
    }

    fn call(
        &mut self,
        argv: Vec<String>,
        input: &[u8],
        wait: bool,
    ) -> Result<(i32, Value), String> {
        let remaining = self.budget.saturating_sub(self.started.elapsed());
        if remaining.is_zero() {
            self.pending_deadline = self.result["outcome"] == "pending";
            return Err("operation deadline reached; remote result remains unverified".into());
        }
        let timeout = if wait {
            remaining
        } else {
            remaining.min(Duration::from_secs(self.config.api_timeout_seconds))
        };
        let process = gh_process::run(
            &self.root,
            Path::new(&self.executable),
            &argv,
            &self.env,
            input,
            timeout,
            self.config.output_limit_bytes,
        )
        .map_err(|err| format!("gh process: {err}"))?;
        let code = process.exit_code.map(|v| v as i32).unwrap_or(-1);
        let creating = argv.get(1).map(String::as_str) == Some("create");
        let argv = std::iter::once(&self.executable)
            .chain(argv.iter())
            .collect::<Vec<_>>();
        self.result["receipts"].as_array_mut().unwrap().push(json!({
            "argv":argv, "child_pid":process.child_pid, "exit_code":code,"exit_signal":process.signal,
            "failure":process.failure, "stdout_bytes":process.stdout,"stderr_bytes":process.stderr
        }));
        if let Some(failure) = process.failure {
            self.pending_deadline = wait && process.timed_out;
            return Err(failure);
        }
        if wait {
            return Ok((code, Value::Null));
        }
        if code != 0 {
            return Err(format!(
                "GitHub command exited {code}; see original receipt"
            ));
        }
        let value = if creating {
            json!(String::from_utf8(process.stdout).map_err(|_| "non-UTF-8 creation output")?)
        } else {
            snapshot::response(&process.stdout)?
        };
        Ok((code, value))
    }

    fn update_reminder(&mut self, number: u64, head: Option<&str>) {
        if let Err(err) = reminders::update(&self.root, &self.config.repository, number, head) {
            self.result["reminder_error"] = json!(err.to_string());
        }
    }

    fn graphql(&mut self, query: &str, variables: Vec<(&str, String)>) -> Result<Value, String> {
        let (owner, name) = self.config.repository.split_once('/').unwrap();
        let mut argv = vec![
            "api".into(),
            "graphql".into(),
            "-f".into(),
            format!("query={query}"),
            "-f".into(),
            format!("owner={owner}"),
            "-f".into(),
            format!("name={name}"),
        ];
        for (key, value) in variables {
            argv.push(if key == "number" {
                "-F".into()
            } else {
                "-f".into()
            });
            argv.push(format!("{key}={value}"));
        }
        self.call(argv, &[], false).map(|(_, v)| v)
    }

    fn snapshot(&mut self, number: u64, head: &str) -> Result<snapshot::Snapshot, String> {
        let raw = self.graphql(
            snapshot::QUERY,
            vec![("number", number.to_string()), ("head", head.into())],
        )?;
        let mut parsed =
            snapshot::pull_request(&raw, &self.config.repository, &self.config.base, number)?;
        let active = verified_snapshot::pull_request_open(&parsed.state);
        let checks =
            snapshot::evaluate_checks(&raw, &mut parsed, head, &self.config.required_checks);
        if record_pr_observation(active, checks.is_ok()) {
            // A verified terminal PR ends object custody even if this commit's
            // checks cannot be interpreted. The check result still fails below.
            if !active {
                self.update_reminder(number, None);
            }
            self.result["pr"] = json!(parsed.number);
            self.result["url"] = json!(parsed.url);
            self.result["observed_head"] = json!(parsed.head);
            self.result["pr_state"] = json!(parsed.state);
            self.result["merge_commit"] = json!(parsed.merge_commit);
            self.result["draft"] = json!(parsed.draft);
            self.result["checks"] = json!([]);
        }
        checks?;
        self.result["checks"] = json!(parsed.checks);
        self.result["outcome"] = json!(parsed.outcome);
        Ok(parsed)
    }

    fn open(&mut self, args: &Args) -> Result<(), String> {
        let branch = args.get("--head")?;
        let head = args.get("--head-sha")?;
        let body = read_bounded(
            &self.root.join(args.get("--body-file")?),
            self.config.output_limit_bytes,
        )?;
        std::str::from_utf8(&body).map_err(|_| "body must be UTF-8")?;
        let raw=self.graphql("query($owner:String!,$name:String!,$ref:String!){repository(owner:$owner,name:$name){nameWithOwner ref(qualifiedName:$ref){target{... on Commit{oid}}}}}",vec![("ref",format!("refs/heads/{branch}"))])?;
        snapshot::repository(&raw, &self.config.repository)?;
        let observed = snapshot::field(&raw, "/data/repository/ref/target/oid")?;
        if !snapshot::sha(observed) {
            return Err("invalid remote branch SHA".into());
        }
        self.result["observed_head"] = json!(observed);
        if observed != head {
            self.result["outcome"] = json!("stale");
            return Ok(());
        }
        let (_, list) = self.call(
            vec![
                "pr".into(),
                "list".into(),
                "--repo".into(),
                self.config.repository.clone(),
                "--head".into(),
                branch.into(),
                "--base".into(),
                self.config.base.clone(),
                "--state".into(),
                "open".into(),
                "--json".into(),
                "number".into(),
            ],
            &[],
            false,
        )?;
        let items = list.as_array().ok_or("invalid PR list")?;
        let (number, outcome) = match items.as_slice() {
            [] => {
                let mut argv = vec![
                    "pr".into(),
                    "create".into(),
                    "--repo".into(),
                    self.config.repository.clone(),
                    "--base".into(),
                    self.config.base.clone(),
                    "--head".into(),
                    branch.into(),
                    "--title".into(),
                    args.get("--title")?.into(),
                    "--body-file".into(),
                    "-".into(),
                ];
                if args.draft {
                    argv.push("--draft".into());
                }
                let (_, created) = self.call(argv, &body, false)?;
                let url = created.as_str().ok_or("invalid create response")?.trim();
                let prefix = format!("https://github.com/{}/pull/", self.config.repository);
                let number = url
                    .strip_prefix(&prefix)
                    .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|s| s.parse::<u64>().ok())
                    .filter(|n| *n > 0 && *n <= i32::MAX as u64)
                    .ok_or(
                        "creation returned an invalid PR URL; query existing PR before retrying",
                    )?;
                self.update_reminder(number, Some(head));
                (number, "opened")
            }
            [item] => (
                item["number"]
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= i32::MAX as u64)
                    .ok_or("invalid existing PR number")?,
                "existing",
            ),
            _ => return Err("ambiguous existing PRs; no creation attempted".into()),
        };
        self.result["pr"] = json!(number);
        let parsed = self.snapshot(number, head)?;
        if parsed.branch != branch {
            return Err("PR head branch mismatch".into());
        }
        if parsed.outcome == "stale" || parsed.outcome == "closed" {
            return Ok(());
        }
        if !verified_snapshot::pull_request_open(&parsed.state) {
            return Err("PR merged during open; creation result is no longer open".into());
        }
        if args.draft && !parsed.draft {
            return Err("requested draft state not present; existing PR was preserved".into());
        }
        self.result["outcome"] = json!(outcome);
        Ok(())
    }

    fn execute(&mut self, args: &Args) -> Result<(), String> {
        if args.command == "pr-open" {
            return self.open(args);
        }
        let number = args.number()?;
        let head = args.get("--head-sha")?;
        let snapshot = self.snapshot(number, head)?;
        if args.command == "pr-status" || snapshot.outcome != "pending" {
            return Ok(());
        }
        // One native blocking channel owns all remote waiting. No harness polling.
        let wait = self.call(
            vec![
                "pr".into(),
                "checks".into(),
                number.to_string(),
                "--repo".into(),
                self.config.repository.clone(),
                "--watch".into(),
                "--interval".into(),
                self.config.watch_interval_seconds.to_string(),
            ],
            &[],
            true,
        );
        let (code, _) = match wait {
            Ok(result) => result,
            Err(error) => {
                if self.result["outcome"] != "pending" {
                    self.result["outcome"] = json!("unknown");
                }
                return Err(error);
            }
        };
        // Transport/auth failures are unknown; ordinary check failure (1) and
        // still pending (8) still permit one factual readback.
        if ![0, 1, 8].contains(&code) {
            self.result["outcome"] = json!("unknown");
            return Err(format!("native wait exited {code}; see original receipt"));
        }
        self.snapshot(number, head)?;
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
pub fn run(raw: &[OsString]) -> CliOutput {
    if raw.len() == 1 && (raw[0] == "--help" || raw[0] == "-h") {
        return CliOutput {
            exit_code: 0,
            stdout: HELP.into(),
            stderr: String::new(),
        };
    }
    if raw.first().is_some_and(|arg| arg == "pr-reminders") {
        return reminders::list(raw);
    }
    let mut result = json!({"schema":"chrono-pr-result/v3","pr_state":null,"merge_commit":null,"command":raw.first().and_then(|s|s.to_str()),"repository":null,"base":null,"pr":null,"expected_head":null,"observed_head":null,"outcome":"unknown","checks":[],"error":null,"receipts":[]});
    match Args::parse(raw) {
        Err(error) => result["error"] = json!(error),
        Ok(args) => {
            result["expected_head"] = json!(args.get("--head-sha").unwrap());
            if let Ok(number) = args.number() {
                result["pr"] = json!(number);
            }
            match Client::new(&args, result.clone()) {
                Err(error) => result["error"] = json!(error),
                Ok(mut client) => {
                    if let Err(error) = client.execute(&args) {
                        // Only an actual watch deadline retains pending. All
                        // other errors invalidate an earlier snapshot outcome.
                        if !client.pending_deadline {
                            client.result["outcome"] = json!("unknown");
                        }
                        client.result["error"] = json!(error);
                    }
                    result = client.result;
                }
            }
        }
    }
    let exit_code = exit_code(&result);
    result["command_exit_code"] = json!(exit_code);
    CliOutput {
        exit_code,
        stdout: format!("{}\n", serde_json::to_string(&result).unwrap()),
        stderr: String::new(),
    }
}

#[cfg(not(verus_keep_ghost))]
fn exit_code(result: &Value) -> u8 {
    match result["outcome"].as_str().unwrap_or("unknown") {
        "opened" | "existing" | "passed" => 0,
        "failed" | "closed" => 1,
        "pending" => 3,
        "stale" => 4,
        _ => 2,
    }
}

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// An already validated terminal PR can settle its object even when the
/// separate check rollup is ambiguous. An OPEN snapshot needs valid checks.
pub fn record_pr_observation(active: bool, checks_valid: bool) -> (record: bool)
    ensures record == (!active || checks_valid),
{
    !active || checks_valid
}
}
