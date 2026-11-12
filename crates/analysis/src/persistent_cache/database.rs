use std::{path::Path, time::Duration};

use document::AnalysisKey;
use katago_protocol::AnalysisResponse;
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};

use super::{CacheError, Operation};

pub(super) struct Database {
    connection: Connection,
    sequence: u64,
    max_bytes: u64,
}

impl Database {
    pub(super) fn open(path: &Path, max_bytes: u64) -> Result<Self, CacheError> {
        // Four pages cover the schema, auto-vacuum pointer map and data root.
        if max_bytes < 4 * 4096 {
            return Err(CacheError::Invalid("size limit must be at least 16 KiB"));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::ZERO)?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > 1 {
            return Err(CacheError::Invalid("unsupported schema version"));
        }
        connection.execute_batch(
            "PRAGMA page_size=4096;
             PRAGMA auto_vacuum=FULL;
             PRAGMA journal_mode=MEMORY;
             PRAGMA temp_store=MEMORY;
             CREATE TABLE IF NOT EXISTS entries (
                 position BLOB NOT NULL,
                 engine TEXT NOT NULL,
                 visits BLOB NOT NULL,
                 last_used BLOB NOT NULL,
                 payload BLOB NOT NULL,
                 PRIMARY KEY(position, engine)
             ) WITHOUT ROWID;
             PRAGMA user_version=1;",
        )?;
        let vacuum: u32 = connection.pragma_query_value(None, "auto_vacuum", |row| row.get(0))?;
        if vacuum != 1 {
            return Err(CacheError::Invalid("database requires full auto vacuum"));
        }
        let integrity: String =
            connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(CacheError::Invalid("integrity check failed"));
        }
        let last: Option<Vec<u8>> =
            connection.query_row("SELECT MAX(last_used) FROM entries", [], |row| row.get(0))?;
        let sequence = match last {
            Some(bytes) => u64::from_be_bytes(
                bytes
                    .try_into()
                    .map_err(|_| CacheError::Invalid("invalid last-used sequence"))?,
            ),
            None => 0,
        };
        let database = Self {
            connection,
            sequence,
            max_bytes,
        };
        let page_size: u64 = database
            .connection
            .pragma_query_value(None, "page_size", |row| row.get(0))?;
        let pages = max_bytes / page_size;
        while database.page_count()? > pages {
            if !database.evict_oldest()? {
                return Err(CacheError::Invalid("size limit is below database overhead"));
            }
        }
        database
            .connection
            .pragma_update(None, "max_page_count", pages)?;
        Ok(database)
    }

    pub(super) fn operate(
        &mut self,
        operation: Operation,
    ) -> Result<Option<AnalysisResponse>, CacheError> {
        match operation {
            Operation::Get(key, engine) => self.get(key, &engine),
            Operation::Put(key, engine, response) => {
                self.put(key, &engine, &response)?;
                Ok(None)
            }
            Operation::ClearPosition(key) => {
                self.connection
                    .execute("DELETE FROM entries WHERE position=?1", [key.0.as_slice()])?;
                Ok(None)
            }
            Operation::ClearAll => {
                self.connection.execute("DELETE FROM entries", [])?;
                Ok(None)
            }
        }
    }

    fn next_sequence(&mut self) -> Result<[u8; 8], CacheError> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(CacheError::Invalid("last-used sequence exhausted"))?;
        Ok(self.sequence.to_be_bytes())
    }

    fn get(
        &mut self,
        key: AnalysisKey,
        engine: &str,
    ) -> Result<Option<AnalysisResponse>, CacheError> {
        let bytes: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT payload FROM entries WHERE position=?1 AND engine=?2",
                params![key.0.as_slice(), engine],
                |row| row.get(0),
            )
            .optional()?;
        let Some(bytes) = bytes else { return Ok(None) };
        let response = serde_json::from_slice(&bytes)?;
        let sequence = self.next_sequence()?;
        self.connection.execute(
            "UPDATE entries SET last_used=?3 WHERE position=?1 AND engine=?2",
            params![key.0.as_slice(), engine, sequence.as_slice()],
        )?;
        Ok(Some(response))
    }

    fn put(
        &mut self,
        key: AnalysisKey,
        engine: &str,
        response: &AnalysisResponse,
    ) -> Result<(), CacheError> {
        let payload = serde_json::to_vec(response)?;
        // A single response larger than the entire budget cannot be retained.
        // Do not evict otherwise useful entries for such a response.
        if payload.len() as u64 + engine.len() as u64 + 16 * 1024 > self.max_bytes {
            return Ok(());
        }
        let visits = response.root_info.visits.to_be_bytes();
        let sequence = self.next_sequence()?;
        loop {
            let result = self.connection.execute(
                "INSERT INTO entries(position, engine, visits, last_used, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(position, engine) DO UPDATE SET
                   visits=excluded.visits, last_used=excluded.last_used, payload=excluded.payload
                 WHERE excluded.visits >= entries.visits",
                params![
                    key.0.as_slice(),
                    engine,
                    visits.as_slice(),
                    sequence.as_slice(),
                    payload
                ],
            );
            match result {
                Ok(_) => return Ok(()),
                Err(rusqlite::Error::SqliteFailure(error, _))
                    if error.code == ErrorCode::DiskFull =>
                {
                    // SQLite rolls the failed statement back. Delete the LRU
                    // entry, reclaim its pages via auto_vacuum, then retry.
                    if !self.evict_oldest()? {
                        return Ok(());
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn evict_oldest(&self) -> Result<bool, CacheError> {
        Ok(self.connection.execute(
            "DELETE FROM entries WHERE (position, engine) =
             (SELECT position, engine FROM entries ORDER BY last_used, position, engine LIMIT 1)",
            [],
        )? != 0)
    }

    fn page_count(&self) -> Result<u64, CacheError> {
        Ok(self
            .connection
            .pragma_query_value(None, "page_count", |row| row.get(0))?)
    }
}
