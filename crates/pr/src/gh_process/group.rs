//! Wait on the process group created for this gh invocation. The direct child
//! stays unreaped throughout observation, pinning its PID and group identity.
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Instant;

fn remaining(deadline: Instant) -> io::Result<std::time::Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "gh process group timed out"))
}

pub(super) fn wait(pid: u32, deadline: Instant) -> io::Result<()> {
    let group = pid as libc::pid_t;
    wait_exit(group, group, deadline)?;
    // New children may be forked while an existing member is finishing.
    // Take another inventory only after the kernel reports that member's exit.
    while let Some(member) = live_member(group, deadline)? {
        wait_exit(member, group, deadline)?;
    }
    Ok(())
}

fn belongs_to_group(pid: libc::pid_t, group: libc::pid_t) -> io::Result<bool> {
    let observed = unsafe { libc::getpgid(pid) };
    if observed >= 0 {
        return Ok(observed == group);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(error)
    }
}

/// Once the leader is reaped, an empty process group is definitive. A still
/// existing group could contain a late fork or have a recycled ID. Preserve
/// the original unknown outcome and never signal that now unpinned identity.
pub(super) fn verify_empty(pid: u32) -> io::Result<()> {
    let group = i32::try_from(pid).map_err(io::Error::other)?;
    let result = unsafe { libc::kill(-group, 0) };
    let error = io::Error::last_os_error();
    if result < 0 && error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    if result < 0 {
        return Err(error);
    }
    Err(io::Error::other(
        "cannot confirm that the original gh process group is empty",
    ))
}

#[cfg(target_os = "macos")]
fn live_member(group: libc::pid_t, _deadline: Instant) -> io::Result<Option<libc::pid_t>> {
    // Darwin sys/proc_info.h, omitted by the Rust libc bindings.
    const PROC_PGRP_ONLY: u32 = 2;
    let mut pids = vec![0_i32; 32];
    loop {
        let capacity = i32::try_from(std::mem::size_of_val(&pids[..])).map_err(io::Error::other)?;
        let size = unsafe {
            libc::proc_listpids(
                PROC_PGRP_ONLY,
                group as u32,
                pids.as_mut_ptr().cast(),
                capacity,
            )
        };
        if size <= 0 {
            return Err(io::Error::last_os_error());
        }
        if size == capacity {
            pids.resize(pids.len() * 2, 0);
            continue;
        }
        for &pid in &pids[..size as usize / size_of::<i32>()] {
            if pid == group {
                continue;
            }
            let mut status: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
            let bytes = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    (&raw mut status).cast(),
                    size_of_val(&status) as i32,
                )
            };
            if bytes != size_of_val(&status) as i32 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    continue;
                }
                return Err(error);
            }
            if status.pbi_pgid == group as u32 && status.pbi_status != libc::SZOMB {
                return Ok(Some(pid));
            }
        }
        return Ok(None);
    }
}

#[cfg(target_os = "macos")]
fn wait_exit(pid: libc::pid_t, group: libc::pid_t, deadline: Instant) -> io::Result<()> {
    let raw = unsafe { libc::kqueue() };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let queue = unsafe { OwnedFd::from_raw_fd(raw) };
    let mut event: libc::kevent = unsafe { std::mem::zeroed() };
    event.ident = pid as _;
    event.filter = libc::EVFILT_PROC;
    event.flags = libc::EV_ADD | libc::EV_ONESHOT;
    event.fflags = libc::NOTE_EXIT;
    let started = unsafe {
        libc::kevent(
            queue.as_raw_fd(),
            &event,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    if started < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        };
    }
    // If the original member disappeared during registration, the new process
    // with this PID is outside our group. Never wait on an unrelated one.
    if !belongs_to_group(pid, group)? {
        return Ok(());
    }
    loop {
        let timeout = remaining(deadline)?;
        let ts = libc::timespec {
            tv_sec: timeout.as_secs() as _,
            tv_nsec: timeout.subsec_nanos() as _,
        };
        let n = unsafe { libc::kevent(queue.as_raw_fd(), std::ptr::null(), 0, &mut event, 1, &ts) };
        if n > 0 {
            return Ok(());
        }
        if n == 0 {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

#[cfg(target_os = "linux")]
fn live_member(group: libc::pid_t, deadline: Instant) -> io::Result<Option<libc::pid_t>> {
    use std::fs;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        if pid == group || !belongs_to_group(pid, group)? {
            continue;
        }
        let stat = match fs::read(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        // comm may contain parentheses, spaces and non-UTF-8 bytes. The
        // following state and pgrp fields are defined as ASCII by proc_pid_stat.
        let close = stat
            .iter()
            .rposition(|b| *b == b')')
            .ok_or_else(|| io::Error::other("invalid process stat record"))?;
        let fields = &stat[close..];
        let mut fields = fields.split(|b| *b == b' ').skip(1);
        let state = fields.next();
        let current_group = fields
            .nth(1)
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.parse::<i32>().ok());
        if current_group != Some(group) {
            continue;
        }
        if !matches!(state, Some(b"Z" | b"X" | b"x")) {
            return Ok(Some(pid));
        }
        // An exited member keeps its group alive until its parent reaps it.
        // The leader still pins the group here. Wait for the kernel's reaping
        // event within the original deadline, then apply the final group check.
        match wait_pidfd(pid, group, deadline, libc::POLLHUP) {
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
            result => result?,
        }
    }
    Ok(None)
}

#[cfg(target_os = "linux")]
fn wait_exit(pid: libc::pid_t, group: libc::pid_t, deadline: Instant) -> io::Result<()> {
    wait_pidfd(pid, group, deadline, libc::POLLIN)
}

#[cfg(target_os = "linux")]
fn wait_pidfd(
    pid: libc::pid_t,
    group: libc::pid_t,
    deadline: Instant,
    event: libc::c_short,
) -> io::Result<()> {
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    if raw < 0 {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        };
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    if !belongs_to_group(pid, group)? {
        return Ok(());
    }
    let mut ev = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: event,
        revents: 0,
    };
    loop {
        let timeout = remaining(deadline)?;
        let millis = i32::try_from(timeout.as_millis().saturating_add(1)).unwrap_or(i32::MAX);
        let n = unsafe { libc::poll(&mut ev, 1, millis) };
        if n > 0 && ev.revents & (event | libc::POLLHUP) != 0 {
            return Ok(());
        }
        if n > 0 {
            return Err(io::Error::other("unexpected pidfd event"));
        }
        if n == 0 {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}
