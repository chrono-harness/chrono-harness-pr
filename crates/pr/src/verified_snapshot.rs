//! Pull request check and snapshot outcome classification with a single Verus owner.
#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::string::StringSliceAdditionalSpecFns;

#[cfg(verus_keep_ghost)]
#[path = "reminders.rs"]
pub mod reminders;

#[cfg(verus_keep_ghost)]
#[path = "main.rs"]
mod publication;

#[cfg(verus_keep_ghost)]
#[path = "lib.rs"]
pub(crate) mod client;
#[cfg(not(verus_keep_ghost))]
pub use crate::{record_pr_observation, snapshot::pull_request_merged};
#[cfg(verus_keep_ghost)]
pub use client::{record_pr_observation, snapshot::pull_request_merged};

#[cfg(verus_keep_ghost)]
#[path = "gh_process.rs"]
mod gh_process;
#[cfg(verus_keep_ghost)]
#[path = "notification.rs"]
mod notification;
#[cfg(verus_keep_ghost)]
pub use self::{
    gh_process::{
        append_output,
        group::{confirmed_absent, observed_pidfd_event},
    },
    notification::{FramePart, frame_part},
};
#[cfg(not(verus_keep_ghost))]
pub use crate::{
    gh_process::{
        append_output,
        group::{confirmed_absent, observed_pidfd_event},
    },
    notification::{FramePart, frame_part},
};

verus! {

/// Only a successfully completed gh invocation supplies bytes for an operation response.
pub enum CommandReply<E> {
    TransportFailure(E),
    Signal(i32),
    MissingStatus,
    Nonzero(u32),
    CreationUrl,
    Json,
}

pub fn command_reply<E>(failure: Option<E>, code: Option<u32>, signal: Option<i32>, creating: bool)
    -> (reply: CommandReply<E>)
    ensures reply == (match failure {
        Some(error) => CommandReply::TransportFailure(error),
        None => match code {
            Some(0) => if creating { CommandReply::CreationUrl } else { CommandReply::Json },
            Some(code) => CommandReply::Nonzero(code),
            None => match signal {
                Some(signal) => CommandReply::Signal(signal),
                None => CommandReply::MissingStatus,
            },
        },
    }),
{
    match failure {
        Some(error) => CommandReply::TransportFailure(error),
        None => match code {
            Some(0) => if creating { CommandReply::CreationUrl } else { CommandReply::Json },
            Some(code) => CommandReply::Nonzero(code),
            None => match signal {
                Some(signal) => CommandReply::Signal(signal),
                None => CommandReply::MissingStatus,
            },
        },
    }
}

/// A single watch call returns on a verified change, even while the run continues.
pub fn push_watch_outcome(stale: bool, completed: bool, success: bool, changed: bool) -> (out: &'static str)
    ensures out@ == (if stale { "stale"@ } else if completed {
        if success { "passed"@ } else { "failed"@ }
    } else if changed { "changed"@ } else { "pending"@ }),
{
    if stale { "stale" } else if completed {
        if success { "passed" } else { "failed" }
    } else if changed { "changed" } else { "pending" }
}

pub open spec fn completed_conclusion(text: Seq<char>) -> bool {
    text == "success"@ || text == "failure"@ || text == "cancelled"@ ||
    text == "timed_out"@ || text == "action_required"@ || text == "neutral"@ ||
    text == "skipped"@ || text == "stale"@ || text == "startup_failure"@
}

pub open spec fn pending_run_status(text: Seq<char>) -> bool {
    text == "queued"@ || text == "requested"@ || text == "pending"@ ||
    text == "waiting"@ || text == "in_progress"@
}

/// Validate the provider's observation before selecting a terminal or change result.
pub fn workflow_outcome(status: &str, conclusion: &str, head: &str, expected: &str, changed: bool)
    -> (out: Result<&'static str, &'static str>)
    ensures
        match out {
            Err(error) => if status@ == "completed"@ {
                !completed_conclusion(conclusion@) && error@ == "unknown completed workflow conclusion"@
            } else {
                (!pending_run_status(status@) || conclusion@ != ""@) &&
                    error@ == "unknown or inconsistent workflow status"@
            },
            Ok(value) => {
                (if status@ == "completed"@ { completed_conclusion(conclusion@) }
                 else { pending_run_status(status@) && conclusion@ == ""@ }) &&
                value@ == (if head@ != expected@ { "stale"@ } else if status@ == "completed"@ {
                    if conclusion@ == "success"@ { "passed"@ } else { "failed"@ }
                } else if changed { "changed"@ } else { "pending"@ })
            },
        },
{
    let completed = same_text(status, "completed");
    if completed {
        if !(same_text(conclusion, "success") || same_text(conclusion, "failure") ||
            same_text(conclusion, "cancelled") || same_text(conclusion, "timed_out") ||
            same_text(conclusion, "action_required") || same_text(conclusion, "neutral") ||
            same_text(conclusion, "skipped") || same_text(conclusion, "stale") ||
            same_text(conclusion, "startup_failure")) {
            return Err("unknown completed workflow conclusion");
        }
    } else if !(same_text(status, "queued") || same_text(status, "requested") ||
        same_text(status, "pending") || same_text(status, "waiting") || same_text(status, "in_progress"))
        || !same_text(conclusion, "") {
        return Err("unknown or inconsistent workflow status");
    }
    Ok(push_watch_outcome(!same_text(head, expected), completed, same_text(conclusion, "success"), changed))
}

pub fn same_workflow_attempt(first_id: u64, id: u64, first_head: &str, head: &str,
    first_branch: &str, branch: &str, first_attempt: u64, attempt: u64) -> (accepted: bool)
    ensures accepted == (id == first_id && head@ == first_head@ && branch@ == first_branch@ && attempt >= first_attempt),
{
    id == first_id && same_text(head, first_head) && same_text(branch, first_branch) && attempt >= first_attempt
}

/// GitHub GraphQL expects a positive signed 32-bit PR number.
pub fn valid_pr_number(number: u64) -> (valid: bool)
    ensures valid == (1 <= number <= 2147483647),
{
    number > 0 && number <= i32::MAX as u64
}

pub enum OpenCandidate {
    Create,
    Reuse(u64),
}

pub fn open_candidate(count: usize, first_number: Option<u64>)
    -> (choice: Result<OpenCandidate, &'static str>)
    ensures
        match choice {
            Ok(OpenCandidate::Create) => count == 0,
            Ok(OpenCandidate::Reuse(n)) => count == 1 && first_number == Some(n) && 1 <= n <= 2147483647,
            Err(error) => if count > 1 {
                error@ == "ambiguous existing PRs; no creation attempted"@
            } else {
                count == 1 && !(exists|n: u64| first_number == Some(n) && 1 <= n <= 2147483647)
                    && error@ == "invalid existing PR number"@
            },
        },
{
    if count == 0 {
        Ok(OpenCandidate::Create)
    } else if count > 1 {
        Err("ambiguous existing PRs; no creation attempted")
    } else {
        match first_number {
            Some(number) if valid_pr_number(number) => Ok(OpenCandidate::Reuse(number)),
            _ => Err("invalid existing PR number"),
        }
    }
}

/// True publishes opened/existing. False preserves the terminal or stale snapshot.
pub fn accept_open_readback(branch: &str, requested_branch: &str, repo: &str, requested_repo: &str,
    outcome: &str, state: &str, requested_draft: bool, draft: bool)
    -> (decision: Result<bool, &'static str>)
    ensures
        match decision {
            Ok(accept) => branch@ == requested_branch@ && repo@ == requested_repo@ &&
                if outcome@ == "stale"@ || outcome@ == "closed"@ { !accept }
                else { accept && state@ == "OPEN"@ && (!requested_draft || draft) },
            Err(message) => if branch@ != requested_branch@ || repo@ != requested_repo@ {
                message@ == "PR head branch mismatch"@
            } else {
                outcome@ != "stale"@ && outcome@ != "closed"@ &&
                if state@ != "OPEN"@ {
                    message@ == "PR merged during open; creation result is no longer open"@
                } else {
                    requested_draft && !draft &&
                    message@ == "requested draft state not present; existing PR was preserved"@
                }
            },
        },
{
    if !same_text(branch, requested_branch) || !same_text(repo, requested_repo) {
        Err("PR head branch mismatch")
    } else if same_text(outcome, "stale") || same_text(outcome, "closed") {
        Ok(false)
    } else if !pull_request_open(state) {
        Err("PR merged during open; creation result is no longer open")
    } else if requested_draft && !draft {
        Err("requested draft state not present; existing PR was preserved")
    } else {
        Ok(true)
    }
}

/// An already failed or conflicting PR takes precedence over base movement.
pub fn report_base_change(state: &str, previous_head: &str, next_head: &str,
    previous_base: &str, next_base: &str, outcome: &str) -> (changed: bool)
    ensures changed == (state@ == "OPEN"@ && previous_head@ == next_head@ && previous_base@ != next_base@
                        && outcome@ != "conflict"@ && outcome@ != "failed"@),
{
    same_text(state, "OPEN") && same_text(previous_head, next_head) && !same_text(previous_base, next_base)
        && !same_text(outcome, "conflict") && !same_text(outcome, "failed")
}

pub fn outcome_exit(outcome: &str) -> (code: u8)
    ensures code == (if outcome@ == "opened"@ || outcome@ == "existing"@ || outcome@ == "passed"@ { 0u8 }
        else if outcome@ == "failed"@ || outcome@ == "closed"@ || outcome@ == "conflict"@ { 1 }
        else if outcome@ == "pending"@ || outcome@ == "changed"@ { 3 }
        else if outcome@ == "stale"@ { 4 } else { 2 }),
{
    if same_text(outcome, "opened") || same_text(outcome, "existing") || same_text(outcome, "passed") { 0 }
    else if same_text(outcome, "failed") || same_text(outcome, "closed") || same_text(outcome, "conflict") { 1 }
    else if same_text(outcome, "pending") || same_text(outcome, "changed") { 3 }
    else if same_text(outcome, "stale") { 4 } else { 2 }
}

/// SQLite supplies signed Unix seconds for the observed creation time.
pub fn reminder_deadline(created: i64) -> (deadline: Option<i64>)
    ensures match deadline {
        Some(value) => value as int == created as int + 30 * 24 * 60 * 60,
        None => created as int + 30 * 24 * 60 * 60 > i64::MAX,
    }
{
    created.checked_add(30 * 24 * 60 * 60)
}

pub fn reminder_expired(deadline: i64, now: i64) -> (expired: bool)
    ensures expired == (deadline <= now),
{
    deadline <= now
}

// The pinned verifier specifies standard String equality on character sequences.
pub(crate) fn same_text(left: &str, right: &str) -> (same: bool)
    ensures same <==> left@ == right@,
{
    left.to_string() == right.to_string()
}

#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, PartialEq, Eq))]
pub enum CheckClass {
    Passed,
    Pending,
    Failed,
    Unknown,
}

#[cfg_attr(not(verus_keep_ghost), derive(Clone, Copy, Debug, PartialEq, Eq))]
pub enum SnapshotOutcome {
    Stale,
    Closed,
    Failed,
    Pending,
    Passed,
    Unknown,
}

pub open spec fn check_class_spec(state: Seq<char>) -> CheckClass {
    if state == "SUCCESS"@ {
        CheckClass::Passed
    } else if state == "MISSING"@ || state == "QUEUED"@
        || state == "IN_PROGRESS"@ || state == "WAITING"@
        || state == "REQUESTED"@ || state == "PENDING"@ || state == "EXPECTED"@ {
        CheckClass::Pending
    } else if state == "FAILURE"@ || state == "ERROR"@
        || state == "CANCELLED"@ || state == "TIMED_OUT"@
        || state == "ACTION_REQUIRED"@ || state == "STARTUP_FAILURE"@
        || state == "STALE"@ || state == "NEUTRAL"@ || state == "SKIPPED"@ {
        CheckClass::Failed
    } else {
        CheckClass::Unknown
    }
}

pub fn classify_check(state: &str) -> (class: CheckClass)
    ensures class == check_class_spec(state@),
{
    if same_text(state, "SUCCESS") {
        CheckClass::Passed
    } else if same_text(state, "MISSING")
        || same_text(state, "QUEUED")
        || same_text(state, "IN_PROGRESS")
        || same_text(state, "WAITING")
        || same_text(state, "REQUESTED")
        || same_text(state, "PENDING")
        || same_text(state, "EXPECTED") {
        CheckClass::Pending
    } else if same_text(state, "FAILURE")
        || same_text(state, "ERROR")
        || same_text(state, "CANCELLED")
        || same_text(state, "TIMED_OUT")
        || same_text(state, "ACTION_REQUIRED")
        || same_text(state, "STARTUP_FAILURE")
        || same_text(state, "STALE")
        || same_text(state, "NEUTRAL")
        || same_text(state, "SKIPPED") {
        CheckClass::Failed
    } else {
        CheckClass::Unknown
    }
}

pub fn check_class_label(class: CheckClass) -> (label: &'static str)
    ensures label@ == match class {
        CheckClass::Passed => "passed"@,
        CheckClass::Pending => "pending"@,
        CheckClass::Failed => "failed"@,
        CheckClass::Unknown => "unknown"@,
    },
{
    match class {
        CheckClass::Passed => "passed",
        CheckClass::Pending => "pending",
        CheckClass::Failed => "failed",
        CheckClass::Unknown => "unknown",
    }
}

/// PR 快照只接受 provider 返回的完整小写 40 位提交 SHA。
#[verifier::inline]
pub open spec fn sha_byte(byte: u8) -> bool {
    48 <= byte <= 57 || 97 <= byte <= 102
}

pub open spec fn sha_format(bytes: Seq<u8>) -> bool {
    bytes.len() == 40
        && forall|i: int| 0 <= i < bytes.len() ==> sha_byte(#[trigger] bytes[i])
}

pub fn valid_sha(bytes: &[u8]) -> (valid: bool)
    ensures valid == sha_format(bytes@),
{
    if bytes.len() != 40 {
        return false;
    }
    let mut i = 0;
    while i < bytes.len()
        invariant
            0 <= i <= bytes.len(),
            bytes.len() == 40,
            forall|j: int| 0 <= j < i ==> sha_byte(#[trigger] bytes[j]),
        decreases bytes.len() - i,
    {
        let byte = bytes[i];
        if !((48 <= byte && byte <= 57) || (97 <= byte && byte <= 102)) {
            return false;
        }
        i += 1;
    }
    true
}

pub fn valid_sha_text(text: &str) -> (valid: bool)
    ensures valid == sha_format(text.spec_bytes()),
{
    valid_sha(text.as_bytes())
}

pub open spec fn pr_state_spec(state: Seq<char>) -> bool {
    state == "OPEN"@ || state == "CLOSED"@ || state == "MERGED"@
}

pub fn valid_pr_state(state: &str) -> (valid: bool)
    ensures valid == pr_state_spec(state@),
{
    same_text(state, "OPEN")
        || same_text(state, "CLOSED")
        || same_text(state, "MERGED")
}

/// GitHub may retain the PR's recorded base after the branch moves. Open PRs
/// use the live refs, and terminal PRs retain their recorded OIDs.
pub fn snapshot_branch_oids<'a>(
    state: &str, pr_base: &'a str, pr_head: &'a str,
    live_base: Option<&'a str>, live_head: Option<&'a str>,
) -> (selected: Result<(&'a str, &'a str), &'static str>)
    ensures match selected {
        Ok((base, head)) => if state@ == "OPEN"@ {
            live_base == Some(base) && live_head == Some(head)
                && sha_format(base.spec_bytes()) && sha_format(head.spec_bytes())
        } else { base == pr_base && head == pr_head },
        Err(error) => state@ == "OPEN"@ && match (live_base, live_head) {
            (None, _) => error@ == "invalid remote field /baseRef/target/oid"@,
            (Some(_), None) => error@ == "invalid remote field /headRef/target/oid"@,
            (Some(base), Some(head)) => error@ == "invalid live branch SHA"@
                && (!sha_format(base.spec_bytes()) || !sha_format(head.spec_bytes())),
        },
    },
{
    if !pull_request_open(state) {
        return Ok((pr_base, pr_head));
    }
    let base = match live_base {
        Some(base) => base,
        None => return Err("invalid remote field /baseRef/target/oid"),
    };
    let head = match live_head {
        Some(head) => head,
        None => return Err("invalid remote field /headRef/target/oid"),
    };
    if !valid_sha_text(base) || !valid_sha_text(head) {
        return Err("invalid live branch SHA");
    }
    Ok((base, head))
}

pub open spec fn snapshot_outcome_spec(
    stale: bool, closed: bool, failed: bool, pending: bool,
) -> SnapshotOutcome {
    if stale {
        SnapshotOutcome::Stale
    } else if closed {
        SnapshotOutcome::Closed
    } else if failed {
        SnapshotOutcome::Failed
    } else if pending {
        SnapshotOutcome::Pending
    } else {
        SnapshotOutcome::Passed
    }
}

pub fn observed_head_is_stale(observed: &str, expected: &str) -> (stale: bool)
    ensures stale == (observed@ != expected@),
{ !same_text(observed, expected) }

pub fn pull_request_closed(state: &str) -> (closed: bool)
    ensures closed == (state@ == "CLOSED"@),
{ same_text(state, "CLOSED") }

pub fn pull_request_open(state: &str) -> (open: bool)
    ensures open == (state@ == "OPEN"@),
{ same_text(state, "OPEN") }

/// GitHub can omit the associated PR list. The selected check suite then
/// binds this completion to the original GitHub snapshot of the requested PR.
pub fn open_conflict(state: &str, mergeable: &str) -> (conflict: bool)
    ensures conflict == (state@ == "OPEN"@ && mergeable@ == "CONFLICTING"@),
{
    same_text(state, "OPEN") && same_text(mergeable, "CONFLICTING")
}

/// A notification identifies which push run to read. Only that read settles the watch.
pub enum PushNotice<'a> {
    Run { event: &'a str },
    Job { id: u64 },
}

pub fn push_notice_matches(
    repo: &str, expected_repo: &str, id: u64, expected_id: u64,
    head: &str, expected_head: &str, attempt: Option<u64>, first_attempt: u64,
    action: &str, notice: PushNotice<'_>,
) -> (accepted: bool)
    ensures accepted == (
        repo@ == expected_repo@ && id == expected_id && head@ == expected_head@
        && match notice {
            PushNotice::Run { event } => event@ == "push"@
                && (action@ == "requested"@ || action@ == "in_progress"@ || action@ == "completed"@)
                && match attempt { Some(n) => n >= first_attempt, None => false },
            PushNotice::Job { id: job } => job > 0
                && (action@ == "waiting"@ || action@ == "queued"@ || action@ == "in_progress"@ || action@ == "completed"@)
                && match attempt { Some(n) => n >= first_attempt, None => true },
        }
    ),
{
    if !same_text(repo,expected_repo) || id != expected_id || !same_text(head,expected_head) { return false; }
    if let Some(n) = attempt { if n < first_attempt { return false; } }
    let active_or_done = same_text(action,"in_progress") || same_text(action,"completed");
    match notice {
        PushNotice::Run { event } => attempt.is_some() && same_text(event,"push")
            && (active_or_done || same_text(action,"requested")),
        PushNotice::Job { id: job } => job > 0 && (active_or_done || same_text(action,"queued") || same_text(action,"waiting")),
    }
}

/// The two registered endpoints of one pending pull request.
pub struct WatchedBranch<'a> {
    pub repository: &'a str,
    pub branch: &'a str,
    pub oid: &'a str,
}

impl<'a> WatchedBranch<'a> {
    pub open spec fn named(self, repository: &str, branch: &str) -> bool {
        self.repository@ == repository@ && self.branch@ == branch@
    }

    fn matches(&self, repository: &str, branch: &str) -> (yes: bool)
        ensures yes == self.named(repository, branch),
    {
        same_text(self.repository, repository) && same_text(self.branch, branch)
    }
}

pub open spec fn advances_tip(tip: &str, before: &str, after: &str) -> bool {
    tip@ == before@ && before@ != after@ && sha_format(after.spec_bytes())
}

fn tip_changed(tip: &str, before: &str, after: &str) -> (changed: bool)
    ensures changed == advances_tip(tip, before, after),
{
    same_text(tip, before) && !same_text(before, after) && valid_sha_text(after)
}

pub enum PushedBranch { Head, Base }

pub fn pushed_branch(
    head: &WatchedBranch, base: &WatchedBranch, repository: &str, branch: &str,
    before: &str, after: &str,
) -> (role: Option<PushedBranch>)
    ensures
        (role matches Some(PushedBranch::Head)) ==
            (head.named(repository, branch) && advances_tip(head.oid, before, after)),
        (role matches Some(PushedBranch::Base)) ==
            (!head.named(repository, branch) && base.named(repository, branch)
             && advances_tip(base.oid, before, after)),
{
    if head.matches(repository, branch) {
        if tip_changed(head.oid, before, after) { Some(PushedBranch::Head) } else { None }
    } else if base.matches(repository, branch) && tip_changed(base.oid, before, after) {
        Some(PushedBranch::Base)
    } else {
        None
    }
}

pub fn synchronized_head(
    number: u64, actual_number: u64, head: &WatchedBranch, base: &WatchedBranch,
    head_repository: &str, head_branch: &str, base_repository: &str, base_branch: &str,
    before: &str, after: &str, actual_head: &str,
) -> (accepted: bool)
    ensures accepted == (number == actual_number && head.named(head_repository, head_branch)
        && base.named(base_repository, base_branch) && advances_tip(head.oid, before, after)
        && actual_head@ == after@),
{
    number == actual_number && head.matches(head_repository, head_branch)
        && base.matches(base_repository, base_branch) && tip_changed(head.oid, before, after)
        && same_text(actual_head, after)
}

pub fn notification_belongs(no_associations: bool, matching_pr: bool, matching_suite: bool) -> (accepted: bool)
    ensures accepted == (if no_associations { matching_suite } else { matching_pr }),
{
    if no_associations { matching_suite } else { matching_pr }
}

pub fn snapshot_outcome(stale: bool, closed: bool, failed: bool, pending: bool)
    -> (outcome: SnapshotOutcome)
    ensures outcome == snapshot_outcome_spec(stale, closed, failed, pending),
{
    if stale {
        SnapshotOutcome::Stale
    } else if closed {
        SnapshotOutcome::Closed
    } else if failed {
        SnapshotOutcome::Failed
    } else if pending {
        SnapshotOutcome::Pending
    } else {
        SnapshotOutcome::Passed
    }
}

pub fn snapshot_outcome_label(outcome: SnapshotOutcome) -> (label: &'static str)
    ensures label@ == match outcome {
        SnapshotOutcome::Stale => "stale"@,
        SnapshotOutcome::Closed => "closed"@,
        SnapshotOutcome::Failed => "failed"@,
        SnapshotOutcome::Pending => "pending"@,
        SnapshotOutcome::Passed => "passed"@,
        SnapshotOutcome::Unknown => "unknown"@,
    },
{
    match outcome {
        SnapshotOutcome::Stale => "stale",
        SnapshotOutcome::Closed => "closed",
        SnapshotOutcome::Failed => "failed",
        SnapshotOutcome::Pending => "pending",
        SnapshotOutcome::Passed => "passed",
        SnapshotOutcome::Unknown => "unknown",
    }
}

pub fn matches_pr_number(observed: Option<u64>, number: u64) -> (matches: bool)
    ensures matches == (observed == Some(number)),
{
    match observed {
        Some(candidate) => candidate == number,
        None => false,
    }
}

pub fn positive_id(observed: Option<u64>) -> (id: Option<u64>)
    ensures id == (match observed { Some(n) if n > 0 => Some(n), _ => None }),
{
    match observed {
        Some(n) if n > 0 => Some(n),
        _ => None,
    }
}

pub fn repository_envelope(object: bool, errors: bool) -> (accepted: bool)
    ensures accepted == (object && !errors),
{
    object && !errors
}

pub fn graphql_errors(null: bool, count: Option<usize>) -> (errors: bool)
    ensures errors == (!null && count != Some(0usize)),
{
    !null && !matches!(count, Some(0))
}

pub fn head_repository_name(bytes: &[u8]) -> (accepted: bool)
    ensures accepted == (exists|i: int| 0 < i && i + 1 < bytes@.len()
        && (#[trigger] bytes@[i]) == 47
        && forall|j: int| 0 <= j < bytes@.len() && i != j ==> (#[trigger] bytes@[j]) != 47),
{
    let mut slash = bytes.len();
    let mut i = 0;
    while i < bytes.len()
        invariant
            i <= bytes.len(),
            slash < i || slash == bytes.len(),
            slash < i ==> bytes@[slash as int] == 47 && 0 < slash && slash + 1 < bytes.len(),
            forall|j: int| 0 <= j < i && j != slash ==> (#[trigger] bytes@[j]) != 47,
        decreases bytes.len() - i,
    {
        if bytes[i] == 47 {
            if slash < i {
                assert(bytes@[slash as int] == 47 && bytes@[i as int] == 47);
                return false;
            }
            if i == 0 || i + 1 == bytes.len() {
                return false;
            }
            slash = i;
        }
        i += 1;
    }
    if slash < bytes.len() {
        assert(bytes@[slash as int] == 47);
        true
    } else {
        false
    }
}

pub fn valid_base_and_mergeability(base: &str, mergeable: &str) -> (accepted: bool)
    ensures accepted == (sha_format(base.spec_bytes()) && (mergeable@ == "MERGEABLE"@
        || mergeable@ == "CONFLICTING"@ || mergeable@ == "UNKNOWN"@)),
{
    valid_sha_text(base) && (same_text(mergeable, "MERGEABLE")
        || same_text(mergeable, "CONFLICTING") || same_text(mergeable, "UNKNOWN"))
}

pub fn complete_check_page(has_next: Option<bool>) -> (complete: bool)
    ensures complete == (has_next == Some(false)),
{
    matches!(has_next, Some(false))
}

pub fn completed_check(status: &str) -> (completed: bool)
    ensures completed == (status@ == "COMPLETED"@),
{
    same_text(status, "COMPLETED")
}

pub fn supported_run_conclusion(conclusion: &str) -> (accepted: bool)
    ensures accepted == (check_class_spec(conclusion@) == CheckClass::Passed
        || (check_class_spec(conclusion@) == CheckClass::Failed && conclusion@ != "ERROR"@)),
{
    match classify_check(conclusion) {
        CheckClass::Passed => true,
        CheckClass::Failed => !same_text(conclusion, "ERROR"),
        _ => false,
    }
}

pub fn pending_check_status(status: &str) -> (accepted: bool)
    ensures accepted == (check_class_spec(status@) == CheckClass::Pending
        && status@ != "MISSING"@ && status@ != "EXPECTED"@),
{
    match classify_check(status) {
        CheckClass::Pending => !same_text(status, "MISSING") && !same_text(status, "EXPECTED"),
        _ => false,
    }
}

pub fn supported_commit_status(state: &str) -> (accepted: bool)
    ensures accepted == (state@ == "SUCCESS"@ || state@ == "FAILURE"@ || state@ == "ERROR"@
        || state@ == "PENDING"@ || state@ == "EXPECTED"@),
{
    same_text(state, "SUCCESS") || same_text(state, "FAILURE") || same_text(state, "ERROR")
        || same_text(state, "PENDING") || same_text(state, "EXPECTED")
}

pub fn canonical_set_equal(left: &[u64], right: &[u64]) -> (equal: bool)
    ensures equal == (left@ == right@),
{
    if left.len() != right.len() {
        return false;
    }
    let mut i = 0;
    while i < left.len()
        invariant
            i <= left.len(), left.len() == right.len(),
            forall|j: int| 0 <= j < i ==> (#[trigger] left@[j]) == right@[j],
        decreases left.len() - i,
    {
        if left[i] != right[i] {
            return false;
        }
        i += 1;
    }
    assert(left@ =~= right@);
    true
}

pub fn contains_check_id(ids: &[u64], id: u64) -> (present: bool)
    ensures present == (exists|i: int| 0 <= i < ids@.len() && (#[trigger] ids@[i]) == id),
{
    let mut i = 0;
    while i < ids.len()
        invariant
            i <= ids.len(),
            forall|j: int| 0 <= j < i ==> (#[trigger] ids@[j]) != id,
        decreases ids.len() - i,
    {
        if ids[i] == id {
            return true;
        }
        i += 1;
    }
    false
}

pub fn latest_members_belong(suite: u64, observed_suites: &[Option<u64>]) -> (complete: bool)
    ensures complete == (forall|i: int| 0 <= i < observed_suites@.len()
        ==> (#[trigger] observed_suites@[i]) == Some(suite)),
{
    let mut i = 0;
    while i < observed_suites.len()
        invariant
            i <= observed_suites.len(),
            forall|j: int| 0 <= j < i ==> (#[trigger] observed_suites@[j]) == Some(suite),
        decreases observed_suites.len() - i,
    {
        if !matches_pr_number(observed_suites[i], suite) {
            return false;
        }
        i += 1;
    }
    true
}

pub fn compatible_required_success(
    previous: &str, next: &str, first_workflow: Option<u64>,
    workflow: Option<u64>, new_suite: bool,
) -> (accepted: bool)
    ensures accepted == (previous@ == "SUCCESS"@ && next@ == "SUCCESS"@
        && workflow.is_some() && first_workflow == workflow && new_suite),
{
    same_text(previous, "SUCCESS") && same_text(next, "SUCCESS") && new_suite
        && match workflow { Some(id) => matches_pr_number(first_workflow, id), None => false }
}

pub fn missing_check_state(observed: Option<&str>) -> (state: &str)
    ensures state@ == match observed { Some(state) => state@, None => "MISSING"@ },
{
    match observed { Some(state) => state, None => "MISSING" }
}

pub fn fold_required_checks(states: &[&str]) -> (outcome: SnapshotOutcome)
    ensures outcome == snapshot_outcome_spec(false, false,
        exists|i: int| 0 <= i < states@.len() && check_class_spec((#[trigger] states@[i])@) == CheckClass::Failed,
        exists|i: int| 0 <= i < states@.len() && check_class_spec((#[trigger] states@[i])@) == CheckClass::Pending),
{
    let mut failed = false;
    let mut pending = false;
    let mut i = 0;
    while i < states.len()
        invariant
            i <= states.len(),
            failed == (exists|j: int| 0 <= j < i && check_class_spec((#[trigger] states@[j])@) == CheckClass::Failed),
            pending == (exists|j: int| 0 <= j < i && check_class_spec((#[trigger] states@[j])@) == CheckClass::Pending),
        decreases states.len() - i,
    {
        match classify_check(states[i]) {
            CheckClass::Failed => failed = true,
            CheckClass::Pending => pending = true,
            _ => {},
        }
        i += 1;
    }
    snapshot_outcome(false, false, failed, pending)
}

pub fn snapshot_mergeability(
    state: &str, mergeable: &str, checks: SnapshotOutcome,
) -> (outcome: SnapshotOutcome)
    ensures outcome == (if state@ == "OPEN"@ && mergeable@ == "UNKNOWN"@
        && checks == SnapshotOutcome::Passed { SnapshotOutcome::Unknown } else { checks }),
{
    if pull_request_open(state) && same_text(mergeable, "UNKNOWN")
        && matches!(checks, SnapshotOutcome::Passed) {
        SnapshotOutcome::Unknown
    } else {
        checks
    }
}

pub fn wait_for_pr(outcome: &str) -> (wait: bool)
    ensures wait == (outcome@ == "pending"@),
{
    same_text(outcome, "pending")
}

pub fn finish_pr_wait(outcome: &str, checks_changed: bool, before: &str, after: &str) -> (finish: bool)
    ensures finish == (outcome@ != "pending"@ || checks_changed || before@ != after@),
{
    !wait_for_pr(outcome) || checks_changed || !same_text(before, after)
}

pub fn remember_pr_event(seen: &mut Vec<String>, key: String) -> (fresh: bool)
    ensures
        fresh == !(exists|i: int| 0 <= i < old(seen)@.len() && (#[trigger] old(seen)@[i])@ == key@),
        seen@ == (if fresh { old(seen)@.push(key) } else { old(seen)@ }),
{
    let mut i = 0;
    while i < seen.len()
        invariant
            seen@ == old(seen)@, i <= seen.len(),
            forall|j: int| 0 <= j < i ==> (#[trigger] seen@[j])@ != key@,
        decreases seen.len() - i,
    {
        if seen[i] == key {
            return false;
        }
        i += 1;
    }
    seen.push(key);
    true
}

}
