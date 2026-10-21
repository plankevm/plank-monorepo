use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sir_stack_scheduling::{
    BlockFinalization,
    op_graph::{CanonicalBlock, OpGraph},
    stack::{ShuffleConfig, StackOps, gas_cost},
};
use std::{path::Path, time::Duration};

const SCHEMA_VERSION: i64 = 2;
const COLUMNS: &str =
    "canonical_hash, canonical_graph, best_schedule, best_gas_cost, manually_optimized";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBlockRow {
    pub canonical_hash: String,
    pub canonical_graph: String,
    pub best_schedule: String,
    pub best_gas_cost: u64,
    pub manually_optimized: bool,
}

pub struct DatabaseEntry {
    pub canonical_hash: String,
    pub graph: OpGraph,
    pub finalization: BlockFinalization,
    pub schedule: Box<[StackOps]>,
    pub gas_cost: u64,
}

impl TryFrom<CanonicalBlockRow> for DatabaseEntry {
    type Error = String;

    fn try_from(row: CanonicalBlockRow) -> Result<Self, String> {
        let canonical = serde_json::from_str::<CanonicalBlock>(&row.canonical_graph)
            .map_err(|error| format!("canonical graph is invalid: {error}"))?;
        let finalization = canonical.finalization;
        let graph = canonical
            .to_op_graph()
            .map_err(|error| format!("canonical graph is invalid: {error}"))?;
        let schedule = serde_json::from_str(&row.best_schedule)
            .map_err(|error| format!("best schedule is invalid: {error}"))?;
        Ok(Self {
            canonical_hash: row.canonical_hash,
            graph,
            finalization,
            schedule,
            gas_cost: row.best_gas_cost,
        })
    }
}

pub struct CanonicalDatabase {
    connection: Connection,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleUpdate {
    Improved { previous_cost: u64, new_cost: u64 },
    NotImproved { current_cost: u64 },
}

impl CanonicalDatabase {
    pub fn open(database: &Path) -> Result<Self, String> {
        Ok(Self { connection: open(database, OpenFlags::SQLITE_OPEN_READ_ONLY)? })
    }

    pub fn all(&self) -> Result<Box<[CanonicalBlockRow]>, String> {
        let mut statement = self
            .connection
            .prepare(&format!("SELECT {COLUMNS} FROM canonical_blocks ORDER BY canonical_hash"))
            .map_err(|error| error.to_string())?;
        statement
            .query_map([], decode_row)
            .map_err(|error| error.to_string())?
            .collect::<Result<Box<[_]>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn find(&self, hash: &str) -> Result<CanonicalBlockRow, String> {
        let hash = normalize_hash(hash);
        if let Some(row) = self
            .connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM canonical_blocks WHERE canonical_hash = ?1"),
                [&hash],
                decode_row,
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            return Ok(row);
        }

        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM canonical_blocks
                 WHERE substr(canonical_hash, 1, length(?1)) = ?1
                 ORDER BY canonical_hash LIMIT 2"
            ))
            .map_err(|error| error.to_string())?;
        let mut matches =
            statement.query_map([&hash], decode_row).map_err(|error| error.to_string())?;
        let Some(first) = matches.next().transpose().map_err(|error| error.to_string())? else {
            return Err(format!("hash prefix '{hash}' was not found in the canonical database"));
        };
        if matches.next().transpose().map_err(|error| error.to_string())?.is_some() {
            return Err(format!("hash prefix '{hash}' matches more than one canonical block"));
        }
        Ok(first)
    }

    pub fn random(&self) -> Result<CanonicalBlockRow, String> {
        self.connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM canonical_blocks ORDER BY random() LIMIT 1"),
                [],
                decode_row,
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "database contains no canonical blocks".to_owned())
    }
}

pub fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("ssb1:") { hash.to_owned() } else { format!("ssb1:{hash}") }
}

fn open(path: &Path, flags: OpenFlags) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|error| format!("failed to open '{}': {error}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(30)).map_err(|error| error.to_string())?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported canonical database version {version} in '{}'",
            path.display()
        ));
    }
    Ok(connection)
}

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CanonicalBlockRow> {
    Ok(CanonicalBlockRow {
        canonical_hash: row.get(0)?,
        canonical_graph: row.get(1)?,
        best_schedule: row.get(2)?,
        best_gas_cost: read_cost(row, 3)?,
        manually_optimized: row.get(4)?,
    })
}

fn read_cost(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value = row.get::<_, i64>(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

/// The caller must replay and validate the candidate against the immutable graph first.
pub fn improve_schedule(
    database: &Path,
    hash: &str,
    schedule: &[StackOps],
) -> Result<ScheduleUpdate, String> {
    let hash = normalize_hash(hash);
    let schedule_cost = gas_cost(schedule, ShuffleConfig::PRE_AMSTERDAM);
    let sql_cost = i64::try_from(schedule_cost).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_string(schedule).map_err(|error| error.to_string())?;
    let mut connection = open(database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.pragma_update(None, "synchronous", "FULL").map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let previous_cost: u64 = transaction
        .query_row(
            "SELECT best_gas_cost FROM canonical_blocks WHERE canonical_hash = ?1",
            [&hash],
            |row| read_cost(row, 0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("hash '{hash}' was not found in the canonical database"))?;
    if schedule_cost >= previous_cost {
        return Ok(ScheduleUpdate::NotImproved { current_cost: previous_cost });
    }
    transaction.execute(
        "UPDATE canonical_blocks SET best_schedule = ?1, best_gas_cost = ?2 WHERE canonical_hash = ?3 AND best_gas_cost > ?2",
        params![encoded, sql_cost, hash],
    ).map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(ScheduleUpdate::Improved { previous_cost, new_cost: schedule_cost })
}

#[cfg(test)]
pub fn create_test_database(path: &Path, rows: &[CanonicalBlockRow]) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE canonical_blocks (
                canonical_hash TEXT PRIMARY KEY,
                canonical_graph TEXT NOT NULL,
                best_schedule TEXT NOT NULL,
                best_gas_cost INTEGER NOT NULL CHECK (best_gas_cost >= 0),
                manually_optimized INTEGER NOT NULL DEFAULT FALSE
                    CHECK (manually_optimized IN (FALSE, TRUE))
            ) WITHOUT ROWID;
            PRAGMA user_version = 2;",
        )
        .unwrap();
    for row in rows {
        connection
            .execute(
                &format!("INSERT INTO canonical_blocks ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5)"),
                params![
                    row.canonical_hash,
                    row.canonical_graph,
                    row.best_schedule,
                    i64::try_from(row.best_gas_cost).unwrap(),
                    row.manually_optimized
                ],
            )
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_core::Idx;
    use sir_stack_scheduling::op_graph::CanonicalValueId;

    fn row(hash: &str, pairs: usize) -> CanonicalBlockRow {
        let graph = CanonicalBlock::new(
            BlockFinalization::ShuffleToOutputs,
            1,
            Box::new([]),
            Box::new([CanonicalValueId::ZERO]),
        );
        let schedule =
            (0..pairs).flat_map(|_| [StackOps::Dup(0), StackOps::Pop]).collect::<Box<[_]>>();
        CanonicalBlockRow {
            canonical_hash: hash.to_owned(),
            canonical_graph: serde_json::to_string(&graph).unwrap(),
            best_schedule: serde_json::to_string(&schedule).unwrap(),
            best_gas_cost: gas_cost(&schedule, ShuffleConfig::PRE_AMSTERDAM),
            manually_optimized: false,
        }
    }

    #[test]
    fn improvements_keep_only_cheaper_schedules_and_manual_status() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        let mut manual = row("ssb1:a", 3);
        manual.manually_optimized = true;
        create_test_database(&path, &[manual, row("ssb1:b", 1)]);

        let cheaper = [StackOps::Dup(0), StackOps::Pop];
        assert_eq!(
            improve_schedule(&path, "a", &cheaper).unwrap(),
            ScheduleUpdate::Improved { previous_cost: 18, new_cost: 6 }
        );
        assert_eq!(
            improve_schedule(&path, "ssb1:b", &cheaper).unwrap(),
            ScheduleUpdate::NotImproved { current_cost: 6 }
        );

        let mut improved = row("ssb1:a", 1);
        improved.manually_optimized = true;
        assert_eq!(
            CanonicalDatabase::open(&path).unwrap().all().unwrap().as_ref(),
            &[improved, row("ssb1:b", 1)]
        );
    }

    #[test]
    fn partial_hashes_must_be_unique() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        create_test_database(&path, &[row("ssb1:aa", 1), row("ssb1:ab", 2)]);
        let database = CanonicalDatabase::open(&path).unwrap();
        assert_eq!(database.find("aa").unwrap().canonical_hash, "ssb1:aa");
        assert_eq!(database.find("ab").unwrap().canonical_hash, "ssb1:ab");
        assert_eq!(
            database.find("a").unwrap_err(),
            "hash prefix 'ssb1:a' matches more than one canonical block"
        );
        assert_eq!(
            database.find("missing").unwrap_err(),
            "hash prefix 'ssb1:missing' was not found in the canonical database"
        );
    }

    #[test]
    fn missing_database_is_not_created_by_read_or_update() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.sqlite3");
        assert!(CanonicalDatabase::open(&path).is_err());
        assert!(improve_schedule(&path, "ssb1:a", &[]).is_err());
        assert!(!path.exists());
    }
}
