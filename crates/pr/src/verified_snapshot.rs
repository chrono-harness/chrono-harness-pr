//! Pull request check and snapshot outcome classification with a single Verus owner.
#![allow(unexpected_cfgs)]

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::string::StringSliceAdditionalSpecFns;

#[cfg(verus_keep_ghost)]
#[path = "lib.rs"]
mod client;
#[cfg(not(verus_keep_ghost))]
pub use crate::{record_pr_observation, snapshot::pull_request_merged};
#[cfg(verus_keep_ghost)]
pub use client::{record_pr_observation, snapshot::pull_request_merged};

verus! {

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
    },
{
    match outcome {
        SnapshotOutcome::Stale => "stale",
        SnapshotOutcome::Closed => "closed",
        SnapshotOutcome::Failed => "failed",
        SnapshotOutcome::Pending => "pending",
        SnapshotOutcome::Passed => "passed",
    }
}

}
