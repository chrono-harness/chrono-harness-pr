//! PR snapshot classification boundaries are independently exercised in Rust and Verus.
#![allow(unexpected_cfgs)]

#[cfg(verus_keep_ghost)]
#[path = "lifecycle.rs"]
mod lifecycle;

#[cfg(verus_keep_ghost)]
#[path = "../src/verified_snapshot.rs"]
mod policy;
#[cfg(not(verus_keep_ghost))]
use chrono_pr::verified_snapshot as policy;

#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::string::StringSliceAdditionalSpecFns;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {

fn same_run_label(actual: &str, expected: &str) -> (equal: bool)
    ensures equal == (actual@ == expected@),
{ actual.to_string() == expected.to_string() }

#[cfg_attr(not(verus_keep_ghost), test)]
fn push_run_change_and_terminal_priorities() {
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(true,true,true,true), "stale"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,true,true,true), "passed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,true,false,true), "failed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,false,false,true), "changed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,false,false,false), "pending"));
}

/// The same fixed workflow accepts progress in its own attempt or a later one.
#[cfg_attr(not(verus_keep_ghost), test)]
fn reobserved_attempt_boundaries() {
    expect_snapshot_true(policy::same_workflow_attempt(7, 7, "h", "h", "dev", "dev", 2, 2));
    expect_snapshot_true(policy::same_workflow_attempt(7, 7, "h", "h", "dev", "dev", 2, 3));
    expect_snapshot_true(!policy::same_workflow_attempt(7, 7, "h", "h", "dev", "dev", 2, 1));
    expect_snapshot_true(!policy::same_workflow_attempt(7, 8, "h", "h", "dev", "dev", 2, 2));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn conflicting_base_change_keeps_failure_exit() {
    expect_snapshot_true(!policy::report_base_change("OPEN", "h", "h", "base", "next", "conflict"));
    expect_snapshot_true(!policy::report_base_change("OPEN", "h", "h", "base", "next", "failed"));
    proof {
        reveal_strlit("opened"); reveal_strlit("existing"); reveal_strlit("passed");
        reveal_strlit("failed"); reveal_strlit("closed"); reveal_strlit("conflict");
        reveal_strlit("changed"); reveal_strlit("pending"); reveal_strlit("stale");
        assert("conflict"@[0] == 'c'); assert("existing"@[0] == 'e');
        assert("changed"@[0] == 'c'); assert("conflict"@.len() == 8);
        assert("changed"@.len() == 7); assert("opened"@.len() == 6);
        assert("existing"@.len() == 8); assert("passed"@.len() == 6);
        assert("failed"@.len() == 6); assert("closed"@.len() == 6);
        assert("pending"@.len() == 7); assert("stale"@.len() == 5);
    }
    expect_snapshot_true(policy::outcome_exit("conflict") == 1);
    expect_snapshot_true(policy::outcome_exit("changed") == 3);
    expect_snapshot_true(policy::outcome_exit("stale") == 4);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn reminders_end_at_the_recorded_second() {
    let deadline = policy::reminder_deadline(1_000);
    expect_snapshot_true(matches!(deadline, Some(2_593_000)));
    expect_snapshot_true(!policy::reminder_expired(2_593_000, 2_592_999));
    expect_snapshot_true(policy::reminder_expired(2_593_000, 2_593_000));
    expect_snapshot_true(policy::reminder_expired(2_593_000, 2_593_001));
    expect_snapshot_true(matches!(policy::reminder_deadline(i64::MAX), None));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn snapshot_outcome_priority_is_proved() {
    let stale = policy::snapshot_outcome(true, true, true, true);
    let closed = policy::snapshot_outcome(false, true, true, true);
    let failed = policy::snapshot_outcome(false, false, true, true);
    let pending = policy::snapshot_outcome(false, false, false, true);
    let passed = policy::snapshot_outcome(false, false, false, false);
    assert(matches!(stale, policy::SnapshotOutcome::Stale));
    assert(matches!(closed, policy::SnapshotOutcome::Closed));
    assert(matches!(failed, policy::SnapshotOutcome::Failed));
    assert(matches!(pending, policy::SnapshotOutcome::Pending));
    assert(matches!(passed, policy::SnapshotOutcome::Passed));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn snapshot_input_boundaries_are_proved() {
    let valid = [48u8; 40];
    let uppercase = [65u8; 40];
    let short = [48u8; 39];
    expect_snapshot_true(policy::valid_sha(&valid));
    assert(uppercase@[0] == 65);
    assert(!policy::sha_byte(uppercase@[0]));
    expect_snapshot_true(!policy::valid_sha(&uppercase));
    expect_snapshot_true(!policy::valid_sha(&short));
    expect_snapshot_true(policy::valid_pr_state("OPEN"));
    expect_snapshot_true(policy::valid_pr_state("CLOSED"));
    expect_snapshot_true(policy::valid_pr_state("MERGED"));
    proof {
        reveal_strlit("MERGED");
        reveal_strlit("OPEN");
        assert("MERGED"@.len() != "OPEN"@.len());
        assert("MERGED"@ != "OPEN"@);
    }
    expect_snapshot_true(policy::pull_request_merged("MERGED"));
    expect_snapshot_true(!policy::pull_request_merged("OPEN"));
    expect_snapshot_true(policy::record_pr_observation(false, false));
    expect_snapshot_true(policy::record_pr_observation(false, true));
    expect_snapshot_true(policy::record_pr_observation(true, true));
    expect_snapshot_true(!policy::record_pr_observation(true, false));
}

fn expect_snapshot_true(actual: bool)
    requires actual,
{
    if !actual {
        panic!();
    }
}

}

#[cfg(not(verus_keep_ghost))]
#[test]
fn rust_snapshot_classification_covers_all_provider_states() {
    use chrono_pr::verified_snapshot::{
        CheckClass, SnapshotOutcome, check_class_label, classify_check, snapshot_outcome,
        snapshot_outcome_label,
    };

    for state in ["SUCCESS"] {
        assert_eq!(classify_check(state), CheckClass::Passed);
    }
    for state in [
        "MISSING",
        "QUEUED",
        "IN_PROGRESS",
        "WAITING",
        "REQUESTED",
        "PENDING",
        "EXPECTED",
    ] {
        assert_eq!(classify_check(state), CheckClass::Pending, "{state}");
    }
    for state in [
        "FAILURE",
        "ERROR",
        "CANCELLED",
        "TIMED_OUT",
        "ACTION_REQUIRED",
        "STARTUP_FAILURE",
        "STALE",
        "NEUTRAL",
        "SKIPPED",
    ] {
        assert_eq!(classify_check(state), CheckClass::Failed, "{state}");
    }
    for state in ["success", "unknown", ""] {
        assert_eq!(classify_check(state), CheckClass::Unknown, "{state:?}");
    }
    assert_eq!(check_class_label(CheckClass::Passed), "passed");
    assert_eq!(check_class_label(CheckClass::Pending), "pending");
    assert_eq!(check_class_label(CheckClass::Failed), "failed");
    assert_eq!(check_class_label(CheckClass::Unknown), "unknown");
    for (flags, expected) in [
        ((true, true, true, true), SnapshotOutcome::Stale),
        ((false, true, true, true), SnapshotOutcome::Closed),
        ((false, false, true, true), SnapshotOutcome::Failed),
        ((false, false, false, true), SnapshotOutcome::Pending),
        ((false, false, false, false), SnapshotOutcome::Passed),
    ] {
        assert_eq!(
            snapshot_outcome(flags.0, flags.1, flags.2, flags.3),
            expected
        );
    }
    assert_eq!(snapshot_outcome_label(SnapshotOutcome::Stale), "stale");
    assert_eq!(snapshot_outcome_label(SnapshotOutcome::Closed), "closed");
    assert_eq!(snapshot_outcome_label(SnapshotOutcome::Failed), "failed");
    assert_eq!(snapshot_outcome_label(SnapshotOutcome::Pending), "pending");
    assert_eq!(snapshot_outcome_label(SnapshotOutcome::Passed), "passed");
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn rust_snapshot_input_validation_covers_provider_boundaries() {
    use chrono_pr::verified_snapshot::{valid_pr_state, valid_sha_text};

    assert!(valid_sha_text("0123456789abcdef0123456789abcdef01234567"));
    assert!(!valid_sha_text("0123456789ABCDEF0123456789abcdef01234567"));
    assert!(!valid_sha_text("0123456789abcdef0123456789abcdef0123456"));
    for state in ["OPEN", "CLOSED", "MERGED"] {
        assert!(valid_pr_state(state), "{state}");
    }
    for state in ["open", "UNKNOWN", ""] {
        assert!(!valid_pr_state(state), "{state:?}");
    }
}
