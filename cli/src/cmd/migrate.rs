//! Database schema migration command.
//!
//! Migrates an agentfs SQLite database to the current schema version.

use agentfs_sdk::AgentFSOptions;
use anyhow::{Context, Result as AnyhowResult};
use std::io::Write;
use std::path::Path;
use turso::Builder;

/// Current schema version target for migrations.
const CURRENT_SCHEMA_VERSION: &str = "0.4";

/// Detected schema version based on column introspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    /// Base schema: fs_inode, fs_dentry, fs_data, fs_symlink, fs_config, kv_store, tool_calls
    V0_0,
    /// Added nlink column to fs_inode
    V0_2,
    /// Added atime_nsec, mtime_nsec, ctime_nsec, rdev columns to fs_inode
    V0_4,
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaVersion::V0_0 => write!(f, "0.0"),
            SchemaVersion::V0_2 => write!(f, "0.2"),
            SchemaVersion::V0_4 => write!(f, "0.4"),
        }
    }
}

/// Column information from PRAGMA table_info.
#[derive(Debug)]
struct ColumnInfo {
    name: String,
}

/// Handle the migrate command.
pub async fn handle_migrate_command(
    stdout: &mut impl Write,
    id_or_path: String,
    dry_run: bool,
) -> AnyhowResult<()> {
    let options = AgentFSOptions::resolve(&id_or_path)?;
    let db_path_str = options
        .db_path()
        .context("Failed to resolve database path")?;
    let db_path = Path::new(&db_path_str);

    if !db_path.exists() {
        anyhow::bail!("Database not found: {}", db_path.display());
    }

    writeln!(stdout, "Database: {}", db_path.display())?;

    // Open database directly using turso::Builder (not SDK) to avoid automatic migrations
    let db = Builder::new_local(&db_path_str)
        .build()
        .await
        .context("Failed to open database")?;
    let conn = db.connect().context("Failed to connect to database")?;

    // Detect current schema version
    let current_version = detect_schema_version(&conn).await?;
    writeln!(stdout, "Current schema version: {}", current_version)?;
    writeln!(stdout, "Target schema version: {}", CURRENT_SCHEMA_VERSION)?;

    if current_version == SchemaVersion::V0_4 {
        writeln!(stdout, "Database is already at the latest schema version.")?;
        return Ok(());
    }

    if dry_run {
        writeln!(
            stdout,
            "\n[DRY RUN] The following migrations would be applied:"
        )?;
        print_pending_migrations(stdout, current_version)?;
        writeln!(stdout, "\nRun without --dry-run to apply migrations.")?;
    } else {
        writeln!(stdout, "\nApplying migrations...")?;
        apply_migrations(&conn, current_version, stdout).await?;

        // Store schema version in fs_config for future use
        conn.execute(
            "INSERT OR REPLACE INTO fs_config (key, value) VALUES ('schema_version', ?)",
            [CURRENT_SCHEMA_VERSION],
        )
        .await
        .context("Failed to store schema version")?;

        writeln!(stdout, "\nMigration completed successfully.")?;
    }

    Ok(())
}

/// Detect the current schema version by introspecting fs_inode columns.
async fn detect_schema_version(conn: &turso::Connection) -> AnyhowResult<SchemaVersion> {
    let columns = get_table_columns(conn, "fs_inode").await?;

    let has_nlink = columns.iter().any(|c| c.name == "nlink");
    let has_atime_nsec = columns.iter().any(|c| c.name == "atime_nsec");
    let has_mtime_nsec = columns.iter().any(|c| c.name == "mtime_nsec");
    let has_ctime_nsec = columns.iter().any(|c| c.name == "ctime_nsec");
    let has_rdev = columns.iter().any(|c| c.name == "rdev");

    // v0.4: has all nsec columns and rdev
    if has_atime_nsec && has_mtime_nsec && has_ctime_nsec && has_rdev {
        return Ok(SchemaVersion::V0_4);
    }

    // v0.2: has nlink but not the nsec columns
    if has_nlink {
        return Ok(SchemaVersion::V0_2);
    }

    // v0.0: base schema
    Ok(SchemaVersion::V0_0)
}

/// Get column information for a table using PRAGMA table_info.
async fn get_table_columns(
    conn: &turso::Connection,
    table_name: &str,
) -> AnyhowResult<Vec<ColumnInfo>> {
    let mut rows = conn
        .query(&format!("PRAGMA table_info({})", table_name), ())
        .await
        .context("Failed to get table info")?;

    let mut columns = Vec::new();
    while let Some(row) = rows.next().await? {
        let name: String = row.get(1)?;
        columns.push(ColumnInfo { name });
    }

    Ok(columns)
}

/// Print pending migrations without applying them.
fn print_pending_migrations(
    stdout: &mut impl Write,
    from_version: SchemaVersion,
) -> AnyhowResult<()> {
    match from_version {
        SchemaVersion::V0_0 => {
            writeln!(stdout, "  - v0.0 -> v0.2: Add nlink column to fs_inode")?;
            writeln!(stdout, "  - v0.2 -> v0.4: Add atime_nsec, mtime_nsec, ctime_nsec, rdev columns to fs_inode")?;
        }
        SchemaVersion::V0_2 => {
            writeln!(stdout, "  - v0.2 -> v0.4: Add atime_nsec, mtime_nsec, ctime_nsec, rdev columns to fs_inode")?;
        }
        SchemaVersion::V0_4 => {
            // Already at latest
        }
    }
    Ok(())
}

/// Apply migrations from the current version to the target version.
async fn apply_migrations(
    conn: &turso::Connection,
    from_version: SchemaVersion,
    stdout: &mut impl Write,
) -> AnyhowResult<()> {
    match from_version {
        SchemaVersion::V0_0 => {
            // Migrate v0.0 -> v0.2
            migrate_v0_0_to_v0_2(conn, stdout).await?;
            // Then v0.2 -> v0.4
            migrate_v0_2_to_v0_4(conn, stdout).await?;
        }
        SchemaVersion::V0_2 => {
            // Migrate v0.2 -> v0.4
            migrate_v0_2_to_v0_4(conn, stdout).await?;
        }
        SchemaVersion::V0_4 => {
            // Already at latest version
        }
    }
    Ok(())
}

/// Migrate from v0.0 to v0.2: Add nlink column to fs_inode.
async fn migrate_v0_0_to_v0_2(
    conn: &turso::Connection,
    stdout: &mut impl Write,
) -> AnyhowResult<()> {
    writeln!(stdout, "  Migrating v0.0 -> v0.2...")?;

    // Add nlink column (idempotent - ignore if exists)
    let result = conn
        .execute(
            "ALTER TABLE fs_inode ADD COLUMN nlink INTEGER NOT NULL DEFAULT 0",
            (),
        )
        .await;

    match result {
        Ok(_) => writeln!(stdout, "    Added nlink column to fs_inode")?,
        Err(e) => {
            // Check if it's a "duplicate column" error (column already exists)
            let err_msg = e.to_string();
            if err_msg.contains("duplicate column") {
                writeln!(stdout, "    nlink column already exists (skipping)")?;
            } else {
                return Err(e).context("Failed to add nlink column");
            }
        }
    }

    writeln!(stdout, "  v0.0 -> v0.2 migration complete.")?;
    Ok(())
}

/// Migrate from v0.2 to v0.4: Add nanosecond timestamp columns and rdev.
async fn migrate_v0_2_to_v0_4(
    conn: &turso::Connection,
    stdout: &mut impl Write,
) -> AnyhowResult<()> {
    writeln!(stdout, "  Migrating v0.2 -> v0.4...")?;

    // Add atime_nsec column (idempotent)
    add_column_idempotent(
        conn,
        stdout,
        "atime_nsec",
        "ALTER TABLE fs_inode ADD COLUMN atime_nsec INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // Add mtime_nsec column (idempotent)
    add_column_idempotent(
        conn,
        stdout,
        "mtime_nsec",
        "ALTER TABLE fs_inode ADD COLUMN mtime_nsec INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // Add ctime_nsec column (idempotent)
    add_column_idempotent(
        conn,
        stdout,
        "ctime_nsec",
        "ALTER TABLE fs_inode ADD COLUMN ctime_nsec INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    // Add rdev column (idempotent)
    add_column_idempotent(
        conn,
        stdout,
        "rdev",
        "ALTER TABLE fs_inode ADD COLUMN rdev INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    writeln!(stdout, "  v0.2 -> v0.4 migration complete.")?;
    Ok(())
}

/// Add a column idempotently (ignore duplicate column errors).
async fn add_column_idempotent(
    conn: &turso::Connection,
    stdout: &mut impl Write,
    column_name: &str,
    sql: &str,
) -> AnyhowResult<()> {
    let result = conn.execute(sql, ()).await;

    match result {
        Ok(_) => writeln!(stdout, "    Added {} column to fs_inode", column_name)?,
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("duplicate column") {
                writeln!(
                    stdout,
                    "    {} column already exists (skipping)",
                    column_name
                )?;
            } else {
                return Err(e).context(format!("Failed to add {} column", column_name));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn create_test_db_v0_0() -> (turso::Database, NamedTempFile) {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();
        let db = Builder::new_local(path).build().await.unwrap();
        let conn = db.connect().unwrap();

        // Create v0.0 schema (without nlink, nsec columns, or rdev)
        conn.execute(
            "CREATE TABLE fs_inode (
                ino INTEGER PRIMARY KEY AUTOINCREMENT,
                mode INTEGER NOT NULL,
                uid INTEGER NOT NULL DEFAULT 0,
                gid INTEGER NOT NULL DEFAULT 0,
                size INTEGER NOT NULL DEFAULT 0,
                atime INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                ctime INTEGER NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

        conn.execute(
            "CREATE TABLE fs_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

        (db, file)
    }

    async fn create_test_db_v0_2() -> (turso::Database, NamedTempFile) {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();
        let db = Builder::new_local(path).build().await.unwrap();
        let conn = db.connect().unwrap();

        // Create v0.2 schema (with nlink, but without nsec columns or rdev)
        conn.execute(
            "CREATE TABLE fs_inode (
                ino INTEGER PRIMARY KEY AUTOINCREMENT,
                mode INTEGER NOT NULL,
                nlink INTEGER NOT NULL DEFAULT 0,
                uid INTEGER NOT NULL DEFAULT 0,
                gid INTEGER NOT NULL DEFAULT 0,
                size INTEGER NOT NULL DEFAULT 0,
                atime INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                ctime INTEGER NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

        conn.execute(
            "CREATE TABLE fs_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

        (db, file)
    }

    async fn create_test_db_v0_4() -> (turso::Database, NamedTempFile) {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap();
        let db = Builder::new_local(path).build().await.unwrap();
        let conn = db.connect().unwrap();

        // Create v0.4 schema (with nlink, nsec columns, and rdev)
        conn.execute(
            "CREATE TABLE fs_inode (
                ino INTEGER PRIMARY KEY AUTOINCREMENT,
                mode INTEGER NOT NULL,
                nlink INTEGER NOT NULL DEFAULT 0,
                uid INTEGER NOT NULL DEFAULT 0,
                gid INTEGER NOT NULL DEFAULT 0,
                size INTEGER NOT NULL DEFAULT 0,
                atime INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                ctime INTEGER NOT NULL,
                rdev INTEGER NOT NULL DEFAULT 0,
                atime_nsec INTEGER NOT NULL DEFAULT 0,
                mtime_nsec INTEGER NOT NULL DEFAULT 0,
                ctime_nsec INTEGER NOT NULL DEFAULT 0
            )",
            (),
        )
        .await
        .unwrap();

        conn.execute(
            "CREATE TABLE fs_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

        (db, file)
    }

    #[tokio::test]
    async fn test_detect_schema_version_v0_0() {
        let (db, _file) = create_test_db_v0_0().await;
        let conn = db.connect().unwrap();

        let version = detect_schema_version(&conn).await.unwrap();
        assert_eq!(version, SchemaVersion::V0_0);
    }

    #[tokio::test]
    async fn test_detect_schema_version_v0_2() {
        let (db, _file) = create_test_db_v0_2().await;
        let conn = db.connect().unwrap();

        let version = detect_schema_version(&conn).await.unwrap();
        assert_eq!(version, SchemaVersion::V0_2);
    }

    #[tokio::test]
    async fn test_detect_schema_version_v0_4() {
        let (db, _file) = create_test_db_v0_4().await;
        let conn = db.connect().unwrap();

        let version = detect_schema_version(&conn).await.unwrap();
        assert_eq!(version, SchemaVersion::V0_4);
    }

    #[tokio::test]
    async fn test_migrate_v0_0_to_v0_4() {
        let (db, _file) = create_test_db_v0_0().await;
        let conn = db.connect().unwrap();

        // Verify starting at v0.0
        assert_eq!(
            detect_schema_version(&conn).await.unwrap(),
            SchemaVersion::V0_0
        );

        // Apply migrations
        let mut stdout = Vec::new();
        apply_migrations(&conn, SchemaVersion::V0_0, &mut stdout)
            .await
            .unwrap();

        // Verify now at v0.4
        assert_eq!(
            detect_schema_version(&conn).await.unwrap(),
            SchemaVersion::V0_4
        );
    }

    #[tokio::test]
    async fn test_migrate_v0_2_to_v0_4() {
        let (db, _file) = create_test_db_v0_2().await;
        let conn = db.connect().unwrap();

        // Verify starting at v0.2
        assert_eq!(
            detect_schema_version(&conn).await.unwrap(),
            SchemaVersion::V0_2
        );

        // Apply migrations
        let mut stdout = Vec::new();
        apply_migrations(&conn, SchemaVersion::V0_2, &mut stdout)
            .await
            .unwrap();

        // Verify now at v0.4
        assert_eq!(
            detect_schema_version(&conn).await.unwrap(),
            SchemaVersion::V0_4
        );
    }

    #[tokio::test]
    async fn test_migrations_are_idempotent() {
        let (db, _file) = create_test_db_v0_0().await;
        let conn = db.connect().unwrap();

        // Apply migrations twice - should not error
        let mut stdout = Vec::new();
        apply_migrations(&conn, SchemaVersion::V0_0, &mut stdout)
            .await
            .unwrap();
        apply_migrations(&conn, SchemaVersion::V0_0, &mut stdout)
            .await
            .unwrap();

        // Should still be at v0.4
        assert_eq!(
            detect_schema_version(&conn).await.unwrap(),
            SchemaVersion::V0_4
        );
    }
}
