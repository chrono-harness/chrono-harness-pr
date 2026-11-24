//! Per-checkout reminders for PRs created by this tool. GitHub owns PR status.
use crate::{CliOutput, verified_snapshot};
use rusqlite::{Connection, functions::FunctionFlags, params};
use serde_json::{Value, json};
use std::{error::Error, ffi::OsString, path::Path, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn database(root: &Path, create: bool) -> Result<Option<Connection>> {
    let dir = root.join(".chrono-harness");
    let path = dir.join("pr-reminders.sqlite3");
    if !path.try_exists()? && !create {
        return Ok(None);
    }
    if dir.canonicalize()? != dir || path.symlink_metadata().is_ok_and(|m| m.is_symlink()) {
        return Err("reminder storage must belong to this checkout".into());
    }
    let db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.pragma_update(None, "secure_delete", true)?;
    let mode: i64 = db.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
    if mode != 1 {
        // Empty databases adopt the format before their first table. Existing
        // databases require a one-time rebuild to add SQLite's pointer map.
        db.execute_batch("PRAGMA auto_vacuum=FULL")?;
        let applied: i64 = db.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
        if applied != 1 {
            db.execute_batch("VACUUM")?;
        }
    }
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    db.create_scalar_function("reminder_deadline", 1, flags, |ctx| {
        Ok(verified_snapshot::reminder_deadline(ctx.get(0)?))
    })?;
    db.create_scalar_function("reminder_expired", 2, flags, |ctx| {
        Ok(verified_snapshot::reminder_expired(
            ctx.get(0)?,
            ctx.get(1)?,
        ))
    })?;
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS reminder(
          repository TEXT NOT NULL, number INTEGER NOT NULL, head TEXT NOT NULL,
          expires INTEGER NOT NULL, PRIMARY KEY(repository, number)
        ) WITHOUT ROWID;
        DELETE FROM reminder WHERE reminder_expired(expires, unixepoch());",
    )?;
    Ok(Some(db))
}

pub(crate) fn expire(root: &Path) -> Result<()> {
    database(root, false)?;
    Ok(())
}

pub(crate) fn update(root: &Path, repository: &str, number: u64, head: Option<&str>) -> Result<()> {
    let Some(db) = database(root, head.is_some())? else {
        return Ok(());
    };
    if let Some(head) = head {
        db.execute(
            "INSERT INTO reminder VALUES (?1,?2,?3,reminder_deadline(unixepoch()))
                    ON CONFLICT(repository,number) DO NOTHING",
            params![repository, number, head],
        )?;
    } else {
        db.execute(
            "DELETE FROM reminder WHERE repository=?1 AND number=?2",
            params![repository, number],
        )?;
    }
    Ok(())
}

fn read(raw: &[OsString]) -> Result<Vec<Value>> {
    let root = match raw {
        [_] => Path::new("."),
        [_, flag, path] if flag == "--host-root" => Path::new(path),
        _ => return Err("usage: chrono-pr pr-reminders [--host-root ROOT]".into()),
    }
    .canonicalize()?;
    let Some(db) = database(&root, false)? else {
        return Ok(vec![]);
    };
    let mut select = db.prepare(
        "SELECT repository,number,head,expires FROM reminder
                                 WHERE NOT reminder_expired(expires, unixepoch()) ORDER BY repository,number",
    )?;
    let rows = select.query_map([], |r| {
        let repository: String = r.get(0)?;
        let number: u64 = r.get(1)?;
        Ok(json!({"repository":repository,"pr":number,
                  "url":format!("https://github.com/{repository}/pull/{number}"),
                  "head_at_creation":r.get::<_,String>(2)?,"expires_at_unix":r.get::<_,i64>(3)?}))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn list(raw: &[OsString]) -> CliOutput {
    let (exit_code, body) = match read(raw) {
        Ok(prs) => (
            0,
            json!({"schema":"chrono-pr-reminders/v1","pull_requests":prs}),
        ),
        Err(err) => (
            2,
            json!({"schema":"chrono-pr-reminders/v1","error":err.to_string()}),
        ),
    };
    CliOutput {
        exit_code,
        stdout: format!("{body}\n"),
        stderr: String::new(),
    }
}
