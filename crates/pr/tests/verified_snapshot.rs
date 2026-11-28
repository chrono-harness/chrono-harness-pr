//! PR snapshot classification boundaries are independently exercised in Rust and Verus.
#![allow(unexpected_cfgs)]

#[cfg(verus_keep_ghost)]
#[path = "workflow_actions.rs"]
mod workflow_actions;

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

#[allow(dead_code)]
#[path = "../src/main.rs"]
mod publication;

verus! {
#[cfg_attr(not(verus_keep_ghost), test)]
fn group_exit_requires_observed_absence_or_the_requested_pidfd_event() {
    if !policy::confirmed_absent(-1, true) { panic!(); }
    if policy::confirmed_absent(-1, false) { panic!(); }
    if policy::confirmed_absent(0, true) { panic!(); }
    proof {
        assert((1i16 & (1i16 | 16i16)) == 1i16) by(bit_vector);
        assert((16i16 & (1i16 | 16i16)) == 16i16) by(bit_vector);
        assert((24i16 & (16i16 | 16i16)) == 16i16) by(bit_vector);
        assert((1i16 & (16i16 | 16i16)) == 0i16) by(bit_vector);
        assert((8i16 & (1i16 | 16i16)) == 0i16) by(bit_vector);
    }
    // IN=1, ERR=8, HUP=16. POLLIN can describe an exited, unreaped child.
    if !policy::observed_pidfd_event(1, 1, 1, 16) { panic!(); }
    if !policy::observed_pidfd_event(1, 16, 1, 16) { panic!(); }
    if !policy::observed_pidfd_event(1, 24, 16, 16) { panic!(); }
    if policy::observed_pidfd_event(1, 1, 16, 16) { panic!(); }
    if policy::observed_pidfd_event(1, 8, 1, 16) { panic!(); }
    if policy::observed_pidfd_event(0, 16, 16, 16) { panic!(); }
    if policy::observed_pidfd_event(-1, 16, 16, 16) { panic!(); }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn adjacent_notifications_keep_the_tail_and_enforce_the_accumulated_limit() {
    use policy::{FramePart, frame_part};
    let input = [255u8, 10u8, 7u8, 10u8];
    match frame_part(&input, 2, 3) {
        FramePart::Bytes { payload, consumed, complete } => {
            if !complete || consumed != 2 || payload.len() != 1 || payload[0] != 255 { panic!(); }
            let (_, tail) = input.split_at(consumed);
            assert(tail.len() == 2 && tail@[0] == 7u8 && tail@[1] == 10u8);
            match frame_part(tail, 0, 1) {
                FramePart::Bytes { payload, consumed, complete } => {
                    if !complete || consumed != 2 || payload.len() != 1 || payload[0] != 7 { panic!(); }
                }
                _ => panic!(),
            }
        }
        _ => panic!(),
    }
    let empty: [u8; 0] = [];
    if !matches!(frame_part(&empty, 0, 1), FramePart::EndOfStream) { panic!(); }
    if !matches!(frame_part(&input, 3, 3), FramePart::AboveLimit) { panic!(); }
    match frame_part(&[10u8], 3, 3) {
        FramePart::Bytes { payload, consumed, complete } => {
            if !complete || !payload.is_empty() || consumed != 1 { panic!(); }
        }
        _ => panic!(),
    }
    match frame_part(&[6u8, 9u8], 1, 3) {
        FramePart::Bytes { payload, consumed, complete } => {
            if complete || consumed != 2 || payload.len() != 2 || payload[1] != 9 { panic!(); }
        }
        _ => panic!(),
    }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn gh_output_keeps_previous_bytes_and_only_the_accepted_prefix() {
    let mut out = vec![3u8];
    if policy::append_output(&mut out, &[0xffu8, 2u8, 0u8], 3) != Some(2) { panic!(); }
    if out.len() != 3 || out[0] != 3 || out[1] != 255 || out[2] != 2 { panic!(); }
    if policy::append_output(&mut out, &[7u8], 3).is_some() { panic!(); }
    if policy::append_output(&mut out, &[], 3) != Some(0) { panic!(); }
    if out.len() != 3 || out[0] != 3 || out[1] != 255 || out[2] != 2 { panic!(); }
}


#[cfg(verus_keep_ghost)]
fn readback_checks_identity_and_the_full_job_list(
    url: &str, head: &str, jobs: &[policy::client::run_watch::Job], id: u64,
)
    requires policy::sha_format(head.spec_bytes()),
{
    use policy::client::run_watch::{valid_push_header, valid_job_ids};
    let ok = valid_push_header(id,id,1,1,url,url,"push",true,head);
    let missing_attempt = valid_push_header(id,id,1,0,url,url,"push",true,head);
    let missing_workflow = valid_push_header(id,id,0,1,url,url,"push",true,head);
    let bad_branch = valid_push_header(id,id,1,1,url,url,"push",false,head);
    assert(ok && !missing_attempt && !missing_workflow && !bad_branch);
    let accepted = valid_job_ids(jobs);
    assert forall|i: int| 0 < i < jobs.len() && accepted implies
        jobs@[i-1].id() < (#[trigger] jobs@[i]).id() by {
        assert(policy::client::run_watch::ordered_at(jobs@, i));
    }
    if accepted && jobs.len() > 0 { assert(policy::client::run_watch::ordered_at(jobs@, 0)); }
    assert(jobs.len() > 0 && jobs@[0].id() == 0 ==> !accepted);
    assert(jobs.len() == 0 ==> accepted);
}


#[cfg(verus_keep_ghost)]
fn local_reminder_opens_only_the_authorized_store(head: &str) {
    use policy::reminders::{opens_database, reminder_change, ReminderChange};
    expect_snapshot_true(!opens_database(false, false));
    expect_snapshot_true(opens_database(false, true));
    expect_snapshot_true(opens_database(true, false));
    expect_snapshot_true(opens_database(true, true));
    let kept = reminder_change(Some(head));
    match kept {
        ReminderChange::RememberFirst(value) => { assert(value@ == head@); },
        ReminderChange::Remove => { expect_snapshot_true(false); },
    }
    expect_snapshot_true(matches!(reminder_change(None), ReminderChange::Remove));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn stdout_write_boundaries() {
    use publication::{WriteStep, write_step, publication_exit};
    expect_snapshot_true(matches!(write_step::<u8>(0, 0, Ok(0)), WriteStep::Flush));
    expect_snapshot_true(matches!(write_step::<u8>(12, 4, Ok(0)), WriteStep::WriteZero));
    expect_snapshot_true(matches!(write_step::<u8>(12, 4, Ok(2)), WriteStep::Advance(6)));
    expect_snapshot_true(matches!(write_step::<u8>(12, 4, Ok(8)), WriteStep::Flush));
    expect_snapshot_true(matches!(write_step::<u8>(usize::MAX, usize::MAX - 1, Ok(1)), WriteStep::Flush));
    expect_snapshot_true(matches!(write_step::<u8>(12, 4, Err((true, 8))), WriteStep::Retry));
    expect_snapshot_true(matches!(write_step::<u8>(12, 4, Err((false, 9))), WriteStep::Failed(9)));
    expect_snapshot_true(publication_exit(4, true) == 4);
    expect_snapshot_true(publication_exit(4, false) == 2);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn gh_completion_retains_errors_and_selects_only_successful_replies() {
    use policy::{CommandReply::*, command_reply};
    expect_snapshot_true(matches!(command_reply(Some(37u16), Some(0), None, true), TransportFailure(error) if error == 37));
    expect_snapshot_true(matches!(command_reply(Some(41u16), None, Some(15), false), TransportFailure(error) if error == 41));
    expect_snapshot_true(matches!(command_reply::<u16>(None, None, Some(9), true), Signal(signal) if signal == 9));
    expect_snapshot_true(matches!(command_reply::<u16>(None, None, None, false), MissingStatus));
    expect_snapshot_true(matches!(command_reply::<u16>(None, Some(1), None, true), Nonzero(code) if code == 1));
    expect_snapshot_true(matches!(command_reply::<u16>(None, Some(u32::MAX), None, false), Nonzero(code) if code == u32::MAX));
    expect_snapshot_true(matches!(command_reply::<u16>(None, Some(0), None, true), CreationUrl));
    expect_snapshot_true(matches!(command_reply::<u16>(None, Some(0), None, false), Json));
}

fn push_notifications_preserve_run_and_job_boundaries(
    repo: &str, head: &str, action: &str, old: u64, next: u64,
) {
    use policy::{PushNotice, push_notice_matches};
    let missing_run = push_notice_matches(repo,repo,8,8,head,head,None,old,action,PushNotice::Run { event: "push" });
    let zero_job = push_notice_matches(repo,repo,8,8,head,head,None,old,action,PushNotice::Job { id: 0 });
    let job = push_notice_matches(repo,repo,8,8,head,head,Some(next),old,action,PushNotice::Job { id: 12 });
    let missing_job = push_notice_matches(repo,repo,8,8,head,head,None,old,action,PushNotice::Job { id: 12 });
    assert(!missing_run && !zero_job);
    if next < old { assert(!job); } else { assert(job == missing_job); }
    proof {
        if action@ == "queued"@ { assert(missing_job); }
    }
}

fn same_run_label(actual: &str, expected: &str) -> (equal: bool)
    ensures equal == (actual@ == expected@),
{ actual.to_string() == expected.to_string() }

// Use arbitrary valid pushed OIDs; native command tests supply the GitHub wire values.
fn pr_branch_notifications_bind_both_ends(
    old: &str, next: &str, base_old: &str,
)
    requires old@ != next@, base_old@ != old@, base_old@ != next@,
        policy::sha_format(next.spec_bytes()),
{
    use policy::{WatchedBranch, PushedBranch};
    proof {
        reveal_strlit("f"); reveal_strlit("r"); reveal_strlit("x"); reveal_strlit("b");
        assert("f"@[0] == 'f'); assert("r"@[0] == 'r');
        assert("x"@[0] == 'x'); assert("b"@[0] == 'b');
    }
    let head = WatchedBranch { repository: "f", branch: "x", oid: old };
    let base = WatchedBranch { repository: "r", branch: "b", oid: base_old };
    let fork = policy::pushed_branch(&head, &base, "f", "x", old, next);
    let base_push = policy::pushed_branch(&head, &base, "r", "b", base_old, next);
    let different_fork = policy::pushed_branch(&head, &base, "r", "x", old, next);
    let wrong_before = policy::pushed_branch(&head, &base, "f", "x", base_old, next);
    let no_change = policy::pushed_branch(&head, &base, "f", "x", old, old);
    assert(matches!(fork, Some(PushedBranch::Head)));
    assert(matches!(base_push, Some(PushedBranch::Base)));
    assert(different_fork.is_none() && wrong_before.is_none() && no_change.is_none());

    let same_ref = WatchedBranch { repository: "f", branch: "x", oid: base_old };
    let head_first = policy::pushed_branch(&head, &same_ref, "f", "x", old, next);
    let no_fallback = policy::pushed_branch(&head, &same_ref, "f", "x", base_old, next);
    assert(matches!(head_first, Some(PushedBranch::Head)) && no_fallback.is_none());

    let sync = policy::synchronized_head(42, 42, &head, &base, "f", "x", "r", "b", old, next, next);
    let wrong_pr = policy::synchronized_head(42, 41, &head, &base, "f", "x", "r", "b", old, next, next);
    let wrong_tip = policy::synchronized_head(42, 42, &head, &base, "f", "x", "r", "b", old, next, old);
    let wrong_base = policy::synchronized_head(42, 42, &head, &base, "f", "x", "r", "x", old, next, next);
    assert(sync && !wrong_pr && !wrong_tip && !wrong_base);
}

fn malformed_after_never_wakes_a_pr(
    head: &policy::WatchedBranch, base: &policy::WatchedBranch, bad: &str,
)
    requires !policy::sha_format(bad.spec_bytes()),
{
    let push = policy::pushed_branch(head, base, head.repository, head.branch, head.oid, bad);
    let sync = policy::synchronized_head(1, 1, head, base, head.repository, head.branch,
        base.repository, base.branch, head.oid, bad, bad);
    assert(push.is_none() && !sync);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn push_run_change_and_terminal_priorities() {
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(true,true,true,true), "stale"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,true,true,true), "passed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,true,false,true), "failed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,false,false,true), "changed"));
    expect_snapshot_true(same_run_label(policy::push_watch_outcome(false,false,false,false), "pending"));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn opening_requires_one_valid_candidate_or_an_empty_list() {
    expect_snapshot_true(matches!(policy::open_candidate(0, None), Ok(policy::OpenCandidate::Create)));
    expect_snapshot_true(matches!(policy::open_candidate(1, Some(1)), Ok(policy::OpenCandidate::Reuse(1))));
    expect_snapshot_true(matches!(policy::open_candidate(1, Some(2147483647)), Ok(policy::OpenCandidate::Reuse(2147483647))));
    expect_snapshot_true(matches!(policy::open_candidate(1, Some(0)), Err(_)));
    expect_snapshot_true(matches!(policy::open_candidate(1, Some(2147483648)), Err(_)));
    expect_snapshot_true(matches!(policy::open_candidate(1, None), Err(_)));
    expect_snapshot_true(matches!(policy::open_candidate(2, Some(42)), Err(_)));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn open_readback_checks_identity_before_preserving_a_stale_snapshot() {
    proof {
        reveal_strlit("a"); reveal_strlit("b");
        reveal_strlit("passed"); reveal_strlit("closed"); reveal_strlit("stale");
        reveal_strlit("OPEN"); reveal_strlit("MERGED");
        assert("a"@[0] == 'a'); assert("b"@[0] == 'b');
        assert("passed"@[0] == 'p'); assert("closed"@[0] == 'c');
        assert("stale"@[0] == 's');
        assert("OPEN"@.len() == 4); assert("MERGED"@.len() == 6);
    }
    expect_snapshot_true(matches!(policy::accept_open_readback("a","b","a","a","stale","MERGED",true,false), Err(_)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","b","a","closed","MERGED",true,false), Err(_)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","a","a","stale","OPEN",true,false), Ok(false)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","a","a","closed","MERGED",true,false), Ok(false)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","a","a","passed","MERGED",true,false), Err(_)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","a","a","passed","OPEN",true,false), Err(_)));
    expect_snapshot_true(matches!(policy::accept_open_readback("a","a","a","a","passed","OPEN",true,true), Ok(true)));
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

fn snapshot_branch_oid_expectations(
    pr_base: &str, pr_head: &str, live_base: &str, live_head: &str, invalid: &str,
)
    requires
        policy::sha_format(live_base.spec_bytes()),
        policy::sha_format(live_head.spec_bytes()),
        !policy::sha_format(invalid.spec_bytes()),
{
    proof {
        reveal_strlit("OPEN");
        reveal_strlit("CLOSED");
        reveal_strlit("MERGED");
        assert("CLOSED"@.len() != "OPEN"@.len());
        assert("MERGED"@.len() != "OPEN"@.len());
    }
    let open = policy::snapshot_branch_oids("OPEN", pr_base, pr_head, Some(live_base), Some(live_head));
    expect_branch_oids(open, live_base, live_head);
    let closed = policy::snapshot_branch_oids("CLOSED", pr_base, pr_head, Some(live_base), Some(live_head));
    expect_branch_oids(closed, pr_base, pr_head);
    let merged = policy::snapshot_branch_oids("MERGED", pr_base, pr_head, None, Some(invalid));
    expect_branch_oids(merged, pr_base, pr_head);

    let no_refs = policy::snapshot_branch_oids("OPEN", pr_base, pr_head, None, None);
    expect_branch_oid_error(no_refs, "invalid remote field /baseRef/target/oid");
    let no_head = policy::snapshot_branch_oids("OPEN", pr_base, pr_head, Some(invalid), None);
    expect_branch_oid_error(no_head, "invalid remote field /headRef/target/oid");
    let malformed_head = policy::snapshot_branch_oids("OPEN", pr_base, pr_head, Some(live_base), Some(invalid));
    expect_branch_oid_error(malformed_head, "invalid live branch SHA");
}

fn expect_branch_oids(actual: Result<(&str, &str), &str>, expected_base: &str, expected_head: &str)
    requires match actual {
        Ok((base, head)) => base@ == expected_base@ && head@ == expected_head@,
        Err(_) => false,
    },
{
    match actual {
        Ok((base, head)) => {
            expect_snapshot_true(!policy::observed_head_is_stale(base, expected_base));
            expect_snapshot_true(!policy::observed_head_is_stale(head, expected_head));
        },
        Err(_) => panic!(),
    }
}

fn expect_branch_oid_error(actual: Result<(&str, &str), &str>, expected: &str)
    requires match actual { Ok(_) => false, Err(error) => error@ == expected@ },
{
    match actual {
        Ok(_) => panic!(),
        Err(error) => expect_snapshot_true(!policy::observed_head_is_stale(error, expected)),
    }
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
fn snapshot_branch_oids_use_the_expected_github_fields() {
    snapshot_branch_oid_expectations(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccccccccccccccccccccccc",
        "dddddddddddddddddddddddddddddddddddddddd",
        "invalid",
    );
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
