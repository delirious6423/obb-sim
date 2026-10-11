//! SQLite layer for one `.sav` file: writing, reading and migrating.
//!
//! Only this module touches SQL. Everything is written in one transaction.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::FORMAT_VERSION;
use crate::error::{Result, StoreError};
use crate::model::SaveData;

const SCHEMA: &str = "
    CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE snapshot (id INTEGER PRIMARY KEY CHECK (id = 1), seq INTEGER NOT NULL, bytes BLOB NOT NULL);
    CREATE TABLE log (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
    CREATE TABLE results (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);
";

/// Writes a new `.sav` at `path` (which must not exist) at [`FORMAT_VERSION`].
///
/// The connection is closed before this returns and the file is fsynced, so
/// the file is complete on disk and has no `-journal` or `-wal` companion.
pub(crate) fn write_new(path: &Path, data: &SaveData) -> Result<()> {
    write_new_with_version(path, data, FORMAT_VERSION)
}

/// Like [`write_new`] but stamps `version` into `user_version`. Tests use it
/// to create newer files.
pub(crate) fn write_new_with_version(path: &Path, data: &SaveData, version: u32) -> Result<()> {
    let mut conn = Connection::open(path)?;
    conn.query_row("PRAGMA journal_mode = DELETE", [], |_| Ok(()))?;
    {
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        tx.execute(
            "INSERT INTO snapshot (id, seq, bytes) VALUES (1, ?1, ?2)",
            params![to_i64(data.snapshot_seq)?, data.snapshot.as_slice()],
        )?;
        for (key, value) in &data.meta {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }
        {
            let mut stmt = tx.prepare("INSERT INTO log (seq, bytes) VALUES (?1, ?2)")?;
            for (index, entry) in data.log.iter().enumerate() {
                let seq = seq_at(data.snapshot_seq, index)?;
                stmt.execute(params![seq, entry.as_slice()])?;
            }
        }
        {
            let mut stmt = tx.prepare("INSERT INTO results (seq, bytes) VALUES (?1, ?2)")?;
            for (index, entry) in data.results.iter().enumerate() {
                stmt.execute(params![to_i64(index as u64)?, entry.as_slice()])?;
            }
        }
        tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
        tx.commit()?;
    }
    conn.close().map_err(|(_, err)| StoreError::Sqlite(err))?;
    sync_file(path)
}

/// Reads the `.sav` at `path` without modifying it.
///
/// A file older than [`FORMAT_VERSION`] is migrated in a scratch copy at
/// `scratch`, which is removed afterwards. The original file is never changed.
///
/// # Errors
///
/// Returns [`StoreError::NewerVersion`] for files from a newer format, and
/// SQLite or [`StoreError::Corrupt`] errors for unreadable or malformed files.
pub(crate) fn read_file(path: &Path, scratch: &Path) -> Result<SaveData> {
    let found = {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let found = user_version(&conn)?;
        check_version(found)?;
        if found == FORMAT_VERSION {
            return read_data(&conn);
        }
        found
    };
    fs::copy(path, scratch)?;
    let result = migrate_scratch(scratch, found);
    // The scratch copy is only a work file. Removing it must not hide the result.
    let _ = fs::remove_file(scratch);
    result
}

fn migrate_scratch(scratch: &Path, from: u32) -> Result<SaveData> {
    let mut conn = Connection::open(scratch)?;
    migrate(&mut conn, from)?;
    read_data(&conn)
}

/// Upgrades a database from `from` to [`FORMAT_VERSION`], one step at a time.
///
/// Each step runs inside the same transaction, which also records the new
/// `user_version`. Add a `match` arm for every new step.
pub(crate) fn migrate(conn: &mut Connection, from: u32) -> Result<()> {
    let tx = conn.transaction()?;
    let mut version = from;
    while version < FORMAT_VERSION {
        match version {
            0 => migrate_0_to_1(&tx)?,
            other => {
                return Err(StoreError::Corrupt(format!(
                    "no migration from format version {other}"
                )));
            }
        }
        version += 1;
        tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
    }
    tx.commit()?;
    Ok(())
}

/// Version 0 had no `results` table.
fn migrate_0_to_1(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS results (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);",
    )?;
    Ok(())
}

/// Reads every table of an open connection at the current layout.
pub(crate) fn read_data(conn: &Connection) -> Result<SaveData> {
    let mut meta = BTreeMap::new();
    {
        let mut stmt = conn.prepare("SELECT key, value FROM meta")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            meta.insert(key, value);
        }
    }

    let (seq, snapshot) = conn
        .query_row("SELECT seq, bytes FROM snapshot WHERE id = 1", [], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .optional()?
        .ok_or_else(|| StoreError::Corrupt("snapshot row is missing".into()))?;
    let snapshot_seq = u64::try_from(seq)
        .map_err(|_| StoreError::Corrupt(format!("negative snapshot seq {seq}")))?;

    let mut log = Vec::new();
    for (index, (seq, bytes)) in read_blobs(conn, "log")?.into_iter().enumerate() {
        let expected = seq_at(snapshot_seq, index)?;
        if seq != expected {
            return Err(StoreError::Corrupt(format!(
                "log sequence gap: expected {expected}, found {seq}"
            )));
        }
        log.push(bytes);
    }

    let mut results = Vec::new();
    for (index, (seq, bytes)) in read_blobs(conn, "results")?.into_iter().enumerate() {
        if seq != to_i64(index as u64)? {
            return Err(StoreError::Corrupt(format!(
                "results sequence gap at index {index}, found {seq}"
            )));
        }
        results.push(bytes);
    }

    Ok(SaveData {
        snapshot,
        snapshot_seq,
        log,
        results,
        meta,
    })
}

/// Reads `(seq, bytes)` rows of a table ordered by `seq`. `table` is one of
/// the crate's own table names, never user input.
fn read_blobs(conn: &Connection, table: &str) -> Result<Vec<(i64, Vec<u8>)>> {
    let mut stmt = conn.prepare(&format!("SELECT seq, bytes FROM {table} ORDER BY seq"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Reads `PRAGMA user_version`.
pub(crate) fn user_version(conn: &Connection) -> Result<u32> {
    let raw: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    u32::try_from(raw).map_err(|_| StoreError::Corrupt(format!("bad user_version {raw}")))
}

fn check_version(found: u32) -> Result<()> {
    if found > FORMAT_VERSION {
        Err(StoreError::NewerVersion {
            found,
            supported: FORMAT_VERSION,
        })
    } else {
        Ok(())
    }
}

/// Log entry `index` after a snapshot covering `snapshot_seq` entries.
fn seq_at(snapshot_seq: u64, index: usize) -> Result<i64> {
    let offset = u64::try_from(index)
        .map_err(|_| StoreError::InvalidInput("too many log entries".into()))?;
    let seq = snapshot_seq
        .checked_add(offset)
        .ok_or_else(|| StoreError::InvalidInput("sequence number overflow".into()))?;
    to_i64(seq)
}

fn to_i64(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| StoreError::InvalidInput(format!("{value} exceeds i64 range")))
}

/// Flushes a file to disk.
pub(crate) fn sync_file(path: &Path) -> Result<()> {
    OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveData {
        let mut meta = BTreeMap::new();
        meta.insert("k".to_string(), "v".to_string());
        SaveData {
            snapshot: vec![1, 2, 3],
            snapshot_seq: 5,
            log: vec![vec![9], vec![8, 8]],
            results: vec![vec![7]],
            meta,
        }
    }

    /// Creates the version 0 layout: no `results` table, `user_version` 0.
    fn write_v0(path: &Path, data: &SaveData) -> Result<()> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE snapshot (id INTEGER PRIMARY KEY CHECK (id = 1), seq INTEGER NOT NULL, bytes BLOB NOT NULL);
             CREATE TABLE log (seq INTEGER PRIMARY KEY, bytes BLOB NOT NULL);",
        )?;
        conn.execute(
            "INSERT INTO snapshot (id, seq, bytes) VALUES (1, ?1, ?2)",
            params![to_i64(data.snapshot_seq)?, data.snapshot.as_slice()],
        )?;
        for (index, entry) in data.log.iter().enumerate() {
            conn.execute(
                "INSERT INTO log (seq, bytes) VALUES (?1, ?2)",
                params![seq_at(data.snapshot_seq, index)?, entry.as_slice()],
            )?;
        }
        conn.close().map_err(|(_, err)| StoreError::Sqlite(err))?;
        Ok(())
    }

    #[test]
    fn v0_file_is_migrated_in_scratch_and_original_is_untouched() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("old.sav");
        let scratch = dir.path().join("old.sav.load.tmp");
        let data = sample();
        write_v0(&path, &data)?;
        let before = fs::read(&path)?;

        let read = read_file(&path, &scratch)?;

        assert_eq!(read.snapshot, data.snapshot);
        assert_eq!(read.log, data.log);
        assert!(read.results.is_empty(), "v0 has no results");
        assert_eq!(fs::read(&path)?, before, "original must not change");
        assert!(!scratch.exists(), "scratch copy must be removed");
        Ok(())
    }

    #[test]
    fn migrate_upgrades_user_version_and_adds_results_table() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("old.sav");
        write_v0(&path, &sample())?;
        let mut conn = Connection::open(&path)?;
        assert_eq!(user_version(&conn)?, 0);
        migrate(&mut conn, 0)?;
        assert_eq!(user_version(&conn)?, FORMAT_VERSION);
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'results'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(tables, 1);
        Ok(())
    }

    #[test]
    fn newer_file_is_refused_and_not_modified() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("new.sav");
        let scratch = dir.path().join("new.sav.load.tmp");
        write_new_with_version(&path, &sample(), FORMAT_VERSION + 1)?;
        let before = fs::read(&path)?;

        let err = read_file(&path, &scratch).expect_err("newer must be refused");

        assert!(matches!(
            err,
            StoreError::NewerVersion { found, supported }
                if found == FORMAT_VERSION + 1 && supported == FORMAT_VERSION
        ));
        assert_eq!(fs::read(&path)?, before);
        Ok(())
    }

    #[test]
    fn write_then_read_round_trips() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("a.sav");
        let scratch = dir.path().join("a.sav.load.tmp");
        let data = sample();
        write_new(&path, &data)?;
        assert_eq!(read_file(&path, &scratch)?, data);
        Ok(())
    }
}
