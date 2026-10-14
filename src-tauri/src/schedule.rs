//! Scheduled backups: while the app is open and unlocked, an encrypted backup goes to a folder the
//! person picked, every day, week or month. Closed or locked means no backup: the passphrase is
//! kept inside the encrypted database and the database is not open then.
//!
//! The newest `KEEP` scheduled files are kept. Only files named `meowny-auto-<date>-<time>.meowbak`
//! are ever deleted, so backups made by hand and anything else in the folder are left alone.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::backup;
use crate::vault::{AppError, Vault};

/// Scheduled backups to keep in the folder.
pub const KEEP: usize = 10;
const PREFIX: &str = "meowny-auto-";
const EVERY_DAYS: [u32; 3] = [1, 7, 30];
/// A failed run is tried again after this long, not at every check.
const RETRY_AFTER_FAILURE: Duration = Duration::hours(1);

/// What the UI may know. The passphrase never leaves the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub folder: String,
    pub every_days: u32,
    /// UTC, ISO 8601.
    pub last_attempt_at: Option<String>,
    pub last_ok: bool,
}

fn load(conn: &Connection) -> Result<Option<(Schedule, String)>, AppError> {
    Ok(conn
        .query_row(
            "SELECT folder, every_days, last_attempt_at, last_ok, passphrase FROM backup_schedule WHERE id = 1",
            [],
            |r| {
                let schedule = Schedule {
                    folder: r.get(0)?,
                    every_days: r.get(1)?,
                    last_attempt_at: r.get(2)?,
                    last_ok: r.get::<_, i64>(3)? == 1,
                };
                Ok((schedule, r.get(4)?))
            },
        )
        .optional()?)
}

pub fn get(conn: &Connection) -> Result<Option<Schedule>, AppError> {
    Ok(load(conn)?.map(|(schedule, _)| schedule))
}

pub fn stop(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM backup_schedule", [])?;
    Ok(())
}

fn stamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn record(conn: &Connection, now: DateTime<Utc>, ok: bool) -> Result<(), AppError> {
    conn.execute(
        "UPDATE backup_schedule SET last_attempt_at = ?1, last_ok = ?2 WHERE id = 1",
        params![stamp(now), ok],
    )?;
    Ok(())
}

pub fn is_due(schedule: &Schedule, now: DateTime<Utc>) -> bool {
    let Some(last) = schedule.last_attempt_at.as_deref().and_then(|t| DateTime::parse_from_rfc3339(t).ok()) else {
        return true;
    };
    let wait = if schedule.last_ok { Duration::days(schedule.every_days.into()) } else { RETRY_AFTER_FAILURE };
    now >= last.with_timezone(&Utc) + wait
}

/// Makes one backup file in `folder` and trims old scheduled ones.
fn write_one(vault: &Vault, folder: &Path, passphrase: &str, now: DateTime<Utc>) -> Result<(), AppError> {
    let snapshot = vault.with_conn(|c| backup::dump(c))?;
    let bytes = backup::seal(&snapshot, passphrase)?;
    let name = format!("{PREFIX}{}.{}", now.format("%Y-%m-%d-%H%M%S"), backup::EXTENSION);
    backup::write_file(&folder.join(name), &bytes)?;
    prune(folder);
    Ok(())
}

fn is_scheduled_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(PREFIX).and_then(|n| n.strip_suffix(&format!(".{}", backup::EXTENSION))) else {
        return false;
    };
    let b = rest.as_bytes();
    b.len() == "2026-10-14-093000".len()
        && b.iter().enumerate().all(|(i, c)| if matches!(i, 4 | 7 | 10) { *c == b'-' } else { c.is_ascii_digit() })
}

/// Deletes all but the newest `KEEP` scheduled backups. Errors are ignored: a file that cannot be
/// deleted is not worth failing a backup that was written.
fn prune(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_scheduled_name(n))
        .collect();
    // The date is in the name, so name order is time order.
    names.sort();
    for old in names.iter().take(names.len().saturating_sub(KEEP)) {
        let _ = fs::remove_file(folder.join(old));
    }
}

/// Sets the schedule up. The first backup is made right away, so a folder that does not work is
/// reported now and not at some later check; nothing is saved if it fails.
pub fn start(vault: &Vault, folder: &Path, passphrase: &str, every_days: u32, now: DateTime<Utc>) -> Result<(), AppError> {
    backup::check_passphrase(passphrase)?;
    if !EVERY_DAYS.contains(&every_days) || !folder.is_absolute() || !folder.is_dir() {
        return Err(AppError::InvalidInput);
    }
    let folder_text = folder.to_str().ok_or(AppError::InvalidInput)?;
    write_one(vault, folder, passphrase, now)?;
    vault.with_conn(|c| {
        c.execute(
            "INSERT INTO backup_schedule (id, folder, every_days, passphrase, last_attempt_at, last_ok)
             VALUES (1, ?1, ?2, ?3, ?4, 1)
             ON CONFLICT (id) DO UPDATE SET folder = ?1, every_days = ?2, passphrase = ?3, last_attempt_at = ?4, last_ok = 1",
            params![folder_text, every_days, passphrase, stamp(now)],
        )?;
        Ok(())
    })
}

/// Makes a backup if one is due. Does nothing while locked, and does not count as activity, so it
/// never keeps the app unlocked. Called from a timer.
pub fn run_if_due(vault: &Vault, now: DateTime<Utc>) {
    let Ok(Some((schedule, passphrase))) = vault.with_conn(|c| load(c)) else { return };
    if !is_due(&schedule, now) {
        return;
    }
    let result = write_one(vault, Path::new(&schedule.folder), &passphrase, now);
    if result == Err(AppError::Locked) {
        return;
    }
    let _ = vault.with_conn(|c| record(c, now, result.is_ok()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::memory::MemoryStore;
    use crate::vault::Settings;

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn vault(dir: &Path) -> Vault {
        let v = Vault::new(dir.to_path_buf(), Box::new(MemoryStore::default()));
        let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "UTC".into(), language: "en".into() };
        v.create(&settings, None).unwrap();
        v.with_conn(|c| crate::categories::create(c, "Food", crate::categories::CategoryType::Expense)).unwrap();
        v
    }

    fn scheduled_files(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with(PREFIX))
            .collect();
        names.sort();
        names
    }

    fn state(v: &Vault) -> Schedule {
        v.with_conn(|c| get(c)).unwrap().unwrap()
    }

    const PASS: &str = "backup passphrase";

    #[test]
    fn due_dates_follow_the_interval_and_a_failure_is_retried_sooner() {
        let s = |every_days, last: Option<&str>, last_ok| Schedule {
            folder: "/x".into(),
            every_days,
            last_attempt_at: last.map(str::to_string),
            last_ok,
        };
        let now = at("2026-10-14T12:00:00Z");
        assert!(is_due(&s(1, None, false), now), "never run");
        assert!(is_due(&s(1, Some("not a time"), true), now), "unreadable stamp");
        assert!(!is_due(&s(1, Some("2026-10-14T00:00:00Z"), true), now));
        assert!(is_due(&s(1, Some("2026-10-13T12:00:00Z"), true), now), "exactly a day");
        assert!(!is_due(&s(7, Some("2026-10-08T13:00:00Z"), true), now));
        assert!(is_due(&s(7, Some("2026-10-07T12:00:00Z"), true), now));
        assert!(!is_due(&s(30, Some("2026-09-15T12:00:00Z"), true), now));
        assert!(is_due(&s(30, Some("2026-09-14T12:00:00Z"), true), now));
        assert!(!is_due(&s(30, Some("2026-10-14T11:30:00Z"), false), now), "failed 30 minutes ago");
        assert!(is_due(&s(30, Some("2026-10-14T11:00:00Z"), false), now), "failed an hour ago");
    }

    #[test]
    fn starting_makes_a_first_backup_that_opens_with_the_passphrase() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, folder.path(), PASS, 7, at("2026-10-14T09:30:00Z")).unwrap();

        assert_eq!(scheduled_files(folder.path()), ["meowny-auto-2026-10-14-093000.meowbak"]);
        let file = fs::read(folder.path().join("meowny-auto-2026-10-14-093000.meowbak")).unwrap();
        let snapshot = backup::open(&file, PASS).unwrap();
        assert_eq!(snapshot.rows("categories").len(), 1);
        assert_eq!(
            state(&v),
            Schedule {
                folder: folder.path().to_str().unwrap().into(),
                every_days: 7,
                last_attempt_at: Some("2026-10-14T09:30:00Z".into()),
                last_ok: true,
            }
        );
    }

    #[test]
    fn a_bad_setup_saves_nothing_and_writes_nothing() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        let now = at("2026-10-14T09:30:00Z");
        let file = folder.path().join("a file");
        fs::write(&file, b"x").unwrap();
        for (path, pass, every) in [
            (folder.path().to_path_buf(), "short", 7),
            (folder.path().to_path_buf(), PASS, 2),
            (folder.path().to_path_buf(), PASS, 0),
            (folder.path().join("missing"), PASS, 7),
            (file, PASS, 7),
            (Path::new("relative/dir").to_path_buf(), PASS, 7),
        ] {
            assert_eq!(start(&v, &path, pass, every, now), Err(AppError::InvalidInput), "{path:?} {pass} {every}");
        }
        assert!(v.with_conn(|c| get(c)).unwrap().is_none());
        assert!(scheduled_files(folder.path()).is_empty());
    }

    #[test]
    fn starting_again_replaces_the_schedule_and_stopping_removes_it() {
        let (data, a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, a.path(), PASS, 7, at("2026-10-14T09:30:00Z")).unwrap();
        start(&v, b.path(), "another passphrase", 1, at("2026-10-15T09:30:00Z")).unwrap();
        let now = state(&v);
        assert_eq!((now.folder.as_str(), now.every_days), (b.path().to_str().unwrap(), 1));
        let file = fs::read(b.path().join("meowny-auto-2026-10-15-093000.meowbak")).unwrap();
        assert!(backup::open(&file, "another passphrase").is_ok());

        v.with_conn(|c| stop(c)).unwrap();
        assert!(v.with_conn(|c| get(c)).unwrap().is_none());
    }

    #[test]
    fn the_timer_backs_up_when_due_and_only_then() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, folder.path(), PASS, 1, at("2026-10-14T09:30:00Z")).unwrap();

        run_if_due(&v, at("2026-10-15T09:00:00Z"));
        assert_eq!(scheduled_files(folder.path()).len(), 1, "not due yet");

        run_if_due(&v, at("2026-10-15T09:30:00Z"));
        assert_eq!(scheduled_files(folder.path()).len(), 2);
        assert_eq!(state(&v).last_attempt_at.as_deref(), Some("2026-10-15T09:30:00Z"));

        run_if_due(&v, at("2026-10-15T09:31:00Z"));
        assert_eq!(scheduled_files(folder.path()).len(), 2, "once per interval");
    }

    #[test]
    fn a_failing_folder_is_recorded_and_retried_later_not_at_every_check() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        let target = folder.path().join("backups");
        fs::create_dir(&target).unwrap();
        start(&v, &target, PASS, 1, at("2026-10-14T09:30:00Z")).unwrap();
        fs::remove_dir_all(&target).unwrap();

        run_if_due(&v, at("2026-10-15T09:30:00Z"));
        let failed = state(&v);
        assert!(!failed.last_ok);
        assert_eq!(failed.last_attempt_at.as_deref(), Some("2026-10-15T09:30:00Z"));

        // The folder comes back; the next check inside the hour leaves it alone, the one after it tries.
        fs::create_dir(&target).unwrap();
        run_if_due(&v, at("2026-10-15T10:00:00Z"));
        assert!(scheduled_files(&target).is_empty());
        run_if_due(&v, at("2026-10-15T10:30:00Z"));
        assert_eq!(scheduled_files(&target).len(), 1);
        assert!(state(&v).last_ok);
    }

    #[test]
    fn a_locked_app_makes_no_backup_and_records_nothing() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, folder.path(), PASS, 1, at("2026-10-14T09:30:00Z")).unwrap();
        v.lock();
        run_if_due(&v, at("2026-10-20T09:30:00Z"));
        assert_eq!(scheduled_files(folder.path()).len(), 1);
        v.unlock(None).unwrap();
        assert_eq!(state(&v).last_attempt_at.as_deref(), Some("2026-10-14T09:30:00Z"));
    }

    #[test]
    fn old_scheduled_backups_are_trimmed_and_nothing_else_is_touched() {
        let folder = tempfile::tempdir().unwrap();
        let name = |day: u32| format!("meowny-auto-2026-09-{day:02}-120000.meowbak");
        for day in 1..=14 {
            fs::write(folder.path().join(name(day)), b"x").unwrap();
        }
        // Not ours: a manual backup, notes, look-alikes, and a folder with a matching name.
        let others = [
            "meowny-backup-2026-09-01.meowbak",
            "notes.txt",
            "meowny-auto-2026-09-01-120000.meowbak.tmp",
            "meowny-auto-latest.meowbak",
            "meowny-auto-2026-09-0a-120000.meowbak",
        ];
        for other in others {
            fs::write(folder.path().join(other), b"x").unwrap();
        }
        fs::create_dir(folder.path().join(name(2).replace("09-02", "08-02"))).unwrap();

        prune(folder.path());

        let kept = scheduled_files(folder.path());
        let ours: Vec<&String> = kept.iter().filter(|n| is_scheduled_name(n) && folder.path().join(n).is_file()).collect();
        assert_eq!(ours.len(), KEEP);
        assert_eq!(*ours[0], name(5), "the oldest four went");
        assert_eq!(*ours[KEEP - 1], name(14));
        for other in others {
            assert!(folder.path().join(other).exists(), "{other} was deleted");
        }
        assert!(folder.path().join("meowny-auto-2026-08-02-120000.meowbak").is_dir());
    }

    #[test]
    fn the_passphrase_stays_inside_the_database() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, folder.path(), "unmistakable passphrase", 7, at("2026-10-14T09:30:00Z")).unwrap();

        let shown = serde_json::to_string(&state(&v)).unwrap();
        assert!(!shown.contains("unmistakable"), "the UI is never told the passphrase");
        assert!(shown.contains("everyDays") && shown.contains("lastOk"));

        let snapshot = v.with_conn(|c| backup::dump(c)).unwrap();
        assert!(!snapshot.tables.contains_key("backup_schedule"));
        assert!(!serde_json::to_string(&snapshot.tables).unwrap().contains("unmistakable"));
        let in_history: i64 = v
            .with_conn(|c| Ok(c.query_row("SELECT count(*) FROM audit_log WHERE new_values LIKE '%unmistakable%'", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(in_history, 0);
    }

    #[test]
    fn restoring_a_backup_leaves_this_devices_schedule_alone() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        let file = backup::seal(&v.with_conn(|c| backup::dump(c)).unwrap(), PASS).unwrap();
        start(&v, folder.path(), PASS, 7, at("2026-10-14T09:30:00Z")).unwrap();
        let before = state(&v);

        v.restore_backup(&file, PASS, "RESTORE").unwrap();
        assert_eq!(state(&v), before);
    }

    #[test]
    fn wiping_removes_the_schedule_and_leaves_the_backup_files() {
        let (data, folder) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let v = vault(data.path());
        start(&v, folder.path(), PASS, 7, at("2026-10-14T09:30:00Z")).unwrap();
        v.wipe("DELETE").unwrap();
        assert_eq!(scheduled_files(folder.path()).len(), 1, "the person's own files stay");
        let settings = Settings { currency: "EUR".into(), locale: "en-IE".into(), timezone: "UTC".into(), language: "en".into() };
        v.create(&settings, None).unwrap();
        assert!(v.with_conn(|c| get(c)).unwrap().is_none());
    }
}
