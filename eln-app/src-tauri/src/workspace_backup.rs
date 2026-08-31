use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const BACKUP_FORMAT: &str = "labflow-workspace-backup";
const BACKUP_FORMAT_VERSION: u32 = 1;
const DATABASE_SCHEMA_VERSION: u32 = 2;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_UNCOMPRESSED_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const REQUIRED_TABLES: &[&str] = &[
    "experiments",
    "tasks",
    "protocols",
    "protocol_versions",
    "records",
    "samples",
    "process_events",
    "attachments",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupCounts {
    pub experiments: u64,
    pub tasks: u64,
    pub records: u64,
    pub samples: u64,
    pub attachments: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub exported_at: String,
    pub database_schema_version: u32,
    pub database_sha256: String,
    pub file_sha256: BTreeMap<String, String>,
    pub counts: BackupCounts,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub app_version: String,
    pub exported_at: String,
    pub database_schema_version: u32,
    pub counts: BackupCounts,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub summary: BackupSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub recovery_backup_path: String,
    pub summary: BackupSummary,
}

struct ExtractedBackup {
    root: PathBuf,
    database: PathBuf,
    files: PathBuf,
    manifest: BackupManifest,
}

fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_database(source: &Connection, destination: &mut Connection) -> Result<(), String> {
    let backup = Backup::new(source, destination).map_err(|error| error.to_string())?;
    backup
        .run_to_completion(64, Duration::from_millis(5), None)
        .map_err(|error| error.to_string())
}

fn snapshot_database(source: &Connection, destination_path: &Path) -> Result<(), String> {
    if let Some(parent) = destination_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if destination_path.exists() {
        fs::remove_file(destination_path).map_err(|error| error.to_string())?;
    }
    let mut destination = Connection::open(destination_path).map_err(|error| error.to_string())?;
    copy_database(source, &mut destination)?;
    validate_sqlite(&destination)?;
    Ok(())
}

fn validate_sqlite(connection: &Connection) -> Result<(), String> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if integrity != "ok" {
        return Err(format!("SQLite integrity check failed: {integrity}"));
    }
    let foreign_key_errors: i64 = connection
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    if foreign_key_errors != 0 {
        return Err(format!(
            "SQLite foreign key check found {foreign_key_errors} error(s)."
        ));
    }
    for table in REQUIRED_TABLES {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !exists {
            return Err(format!(
                "Backup database is missing required table: {table}"
            ));
        }
    }
    Ok(())
}

fn count_table(connection: &Connection, table: &str) -> Result<u64, String> {
    connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, u64>(0)
        })
        .map_err(|error| error.to_string())
}

fn database_counts(connection: &Connection, files: u64) -> Result<BackupCounts, String> {
    Ok(BackupCounts {
        experiments: count_table(connection, "experiments")?,
        tasks: count_table(connection, "tasks")?,
        records: count_table(connection, "records")?,
        samples: count_table(connection, "samples")?,
        attachments: count_table(connection, "attachments")?,
        files,
    })
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
        if !current.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            let path = entry.path();
            if file_type.is_symlink() {
                return Err(format!(
                    "User file directory contains an unsupported symbolic link: {}",
                    path.display()
                ));
            }
            if file_type.is_dir() {
                visit(root, &path, output)?;
            } else if file_type.is_file() {
                path.strip_prefix(root).map_err(|error| error.to_string())?;
                output.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn portable_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if path.is_absolute()
        || !value.starts_with("files/")
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("Non-portable user file locator: {value}"));
    }
    Ok(path.to_path_buf())
}

fn validate_file_locators(connection: &Connection, extracted_root: &Path) -> Result<(), String> {
    let mut queries = vec![
        "SELECT relative_path FROM attachments",
        "SELECT relative_path FROM export_manifests",
    ];
    let has_preview_locator: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('attachments') WHERE name='preview_relative_path')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if has_preview_locator {
        queries.push(
            "SELECT preview_relative_path FROM attachments WHERE preview_relative_path IS NOT NULL",
        );
    }
    for query in queries {
        let mut statement = connection
            .prepare(query)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        for row in rows {
            let locator = row.map_err(|error| error.to_string())?;
            let relative = portable_relative_path(&locator)?;
            if !extracted_root.join(relative).is_file() {
                return Err(format!("Backup is missing referenced user file: {locator}"));
            }
        }
    }
    Ok(())
}

fn manifest_summary(manifest: &BackupManifest) -> BackupSummary {
    BackupSummary {
        app_version: manifest.app_version.clone(),
        exported_at: manifest.exported_at.clone(),
        database_schema_version: manifest.database_schema_version,
        counts: manifest.counts.clone(),
    }
}

pub fn export_workspace(
    connection: &Connection,
    files_dir: &Path,
    destination: &Path,
    exported_at: &str,
) -> Result<ExportResult, String> {
    let parent = destination
        .parent()
        .ok_or("Backup destination has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let work_dir = parent.join(format!(".labflow-backup-staging-{}", nonce()));
    fs::create_dir_all(&work_dir).map_err(|error| error.to_string())?;
    let result = (|| {
        let database_snapshot = work_dir.join("labflow.sqlite");
        snapshot_database(connection, &database_snapshot)?;
        let files = collect_files(files_dir)?;
        let mut file_sha256 = BTreeMap::new();
        for file in &files {
            let relative = file
                .strip_prefix(files_dir)
                .map_err(|error| error.to_string())?;
            let archive_name = format!("files/{}", relative.to_string_lossy().replace('\\', "/"));
            file_sha256.insert(archive_name, hash_file(file)?);
        }
        let snapshot = Connection::open(&database_snapshot).map_err(|error| error.to_string())?;
        let counts = database_counts(&snapshot, files.len() as u64)?;
        let manifest = BackupManifest {
            format: BACKUP_FORMAT.into(),
            format_version: BACKUP_FORMAT_VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            exported_at: exported_at.into(),
            database_schema_version: DATABASE_SCHEMA_VERSION,
            database_sha256: hash_file(&database_snapshot)?,
            file_sha256,
            counts,
        };
        let partial = parent.join(format!(".labflow-backup-{}.partial", nonce()));
        let archive_file = File::create(&partial).map_err(|error| error.to_string())?;
        let mut archive = ZipWriter::new(archive_file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        archive
            .start_file("manifest.json", options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(&serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        archive
            .start_file("database/labflow.sqlite", options)
            .map_err(|error| error.to_string())?;
        let mut database_file =
            File::open(&database_snapshot).map_err(|error| error.to_string())?;
        std::io::copy(&mut database_file, &mut archive).map_err(|error| error.to_string())?;
        for file in &files {
            let relative = file
                .strip_prefix(files_dir)
                .map_err(|error| error.to_string())?;
            let archive_name = format!("files/{}", relative.to_string_lossy().replace('\\', "/"));
            archive
                .start_file(archive_name, options)
                .map_err(|error| error.to_string())?;
            let mut source = File::open(file).map_err(|error| error.to_string())?;
            std::io::copy(&mut source, &mut archive).map_err(|error| error.to_string())?;
        }
        let finished = archive.finish().map_err(|error| error.to_string())?;
        finished.sync_all().map_err(|error| error.to_string())?;
        if destination.exists() {
            fs::remove_file(destination).map_err(|error| error.to_string())?;
        }
        fs::rename(&partial, destination).map_err(|error| error.to_string())?;
        Ok(ExportResult {
            path: destination.to_string_lossy().into_owned(),
            summary: manifest_summary(&manifest),
        })
    })();
    let _ = fs::remove_dir_all(&work_dir);
    result
}

fn extract_and_validate(
    archive_path: &Path,
    staging_parent: &Path,
) -> Result<ExtractedBackup, String> {
    let root = staging_parent.join(format!("import-{}", nonce()));
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let result = (|| {
        let archive_file = File::open(archive_path).map_err(|error| error.to_string())?;
        let mut archive = ZipArchive::new(archive_file).map_err(|error| error.to_string())?;
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            return Err("Backup contains too many entries.".into());
        }
        let mut total_size = 0_u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            total_size = total_size
                .checked_add(entry.size())
                .ok_or("Backup size overflow")?;
            if total_size > MAX_UNCOMPRESSED_BYTES {
                return Err("Backup is too large to import safely.".into());
            }
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("Backup contains an unsupported symbolic link.".into());
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or("Backup contains an unsafe path")?
                .to_path_buf();
            let allowed = enclosed == Path::new("manifest.json")
                || enclosed == Path::new("database/labflow.sqlite")
                || enclosed.starts_with("files");
            if !allowed {
                return Err(format!(
                    "Backup contains an unexpected entry: {}",
                    enclosed.display()
                ));
            }
            let output = root.join(enclosed);
            if entry.is_dir() {
                fs::create_dir_all(&output).map_err(|error| error.to_string())?;
            } else {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let mut file = File::create(&output).map_err(|error| error.to_string())?;
                std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
            }
        }
        let manifest_path = root.join("manifest.json");
        let manifest: BackupManifest =
            serde_json::from_reader(File::open(&manifest_path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("Invalid backup manifest: {error}"))?;
        if manifest.format != BACKUP_FORMAT || manifest.format_version != BACKUP_FORMAT_VERSION {
            return Err("Unsupported LabFlow backup format or version.".into());
        }
        if manifest.database_schema_version > DATABASE_SCHEMA_VERSION {
            return Err(format!(
                "This backup requires database schema version {}, but this app supports {}.",
                manifest.database_schema_version, DATABASE_SCHEMA_VERSION
            ));
        }
        let database = root.join("database/labflow.sqlite");
        if hash_file(&database)? != manifest.database_sha256 {
            return Err("Backup database checksum does not match the manifest.".into());
        }
        let files = root.join("files");
        fs::create_dir_all(&files).map_err(|error| error.to_string())?;
        let extracted_files = collect_files(&files)?;
        let mut actual_hashes = BTreeMap::new();
        for file in extracted_files {
            let relative = file
                .strip_prefix(&files)
                .map_err(|error| error.to_string())?;
            actual_hashes.insert(
                format!("files/{}", relative.to_string_lossy().replace('\\', "/")),
                hash_file(&file)?,
            );
        }
        if actual_hashes != manifest.file_sha256 {
            return Err("Backup file checksums do not match the manifest.".into());
        }
        let connection = Connection::open(&database).map_err(|error| error.to_string())?;
        validate_sqlite(&connection)?;
        validate_file_locators(&connection, &root)?;
        let counts = database_counts(&connection, actual_hashes.len() as u64)?;
        if counts != manifest.counts {
            return Err("Backup object counts do not match the manifest.".into());
        }
        Ok(ExtractedBackup {
            root: root.clone(),
            database,
            files,
            manifest,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

pub fn inspect_backup(archive_path: &Path, app_data_dir: &Path) -> Result<BackupSummary, String> {
    let staging = app_data_dir.join("import-staging");
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let extracted = extract_and_validate(archive_path, &staging)?;
    let summary = manifest_summary(&extracted.manifest);
    fs::remove_dir_all(extracted.root).map_err(|error| error.to_string())?;
    Ok(summary)
}

fn rollback_restore(
    connection: &mut Connection,
    rollback_database: &Path,
    files_dir: &Path,
    rollback_files: &Path,
) -> Result<(), String> {
    let rollback_source = Connection::open(rollback_database).map_err(|error| error.to_string())?;
    copy_database(&rollback_source, connection)?;
    if files_dir.exists() {
        fs::remove_dir_all(files_dir).map_err(|error| error.to_string())?;
    }
    if rollback_files.exists() {
        fs::rename(rollback_files, files_dir).map_err(|error| error.to_string())?;
    } else {
        fs::create_dir_all(files_dir).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn restore_workspace<F>(
    connection: &mut Connection,
    app_data_dir: &Path,
    archive_path: &Path,
    imported_at: &str,
    post_restore: F,
) -> Result<RestoreResult, String>
where
    F: Fn(&mut Connection) -> Result<(), String>,
{
    let staging_parent = app_data_dir.join("import-staging");
    fs::create_dir_all(&staging_parent).map_err(|error| error.to_string())?;
    let extracted = extract_and_validate(archive_path, &staging_parent)?;
    let result = (|| {
        let backups_dir = app_data_dir.join("backups");
        fs::create_dir_all(&backups_dir).map_err(|error| error.to_string())?;
        let recovery_path = backups_dir.join(format!("before-import-{}.labflow-backup", nonce()));
        export_workspace(
            connection,
            &app_data_dir.join("files"),
            &recovery_path,
            imported_at,
        )?;

        let rollback_database = extracted.root.join("rollback.sqlite");
        snapshot_database(connection, &rollback_database)?;
        let files_dir = app_data_dir.join("files");
        let rollback_files = app_data_dir.join(format!("restore-rollback-files-{}", nonce()));
        if files_dir.exists() {
            fs::rename(&files_dir, &rollback_files).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&extracted.files, &files_dir) {
            if rollback_files.exists() {
                let _ = fs::rename(&rollback_files, &files_dir);
            }
            return Err(error.to_string());
        }

        let restore_attempt = (|| {
            let incoming =
                Connection::open(&extracted.database).map_err(|error| error.to_string())?;
            copy_database(&incoming, connection)?;
            post_restore(connection)?;
            validate_sqlite(connection)?;
            validate_file_locators(connection, app_data_dir)
        })();
        if let Err(error) = restore_attempt {
            let rollback_error =
                rollback_restore(connection, &rollback_database, &files_dir, &rollback_files).err();
            return Err(match rollback_error {
                Some(rollback) => format!(
                    "Import failed ({error}) and automatic rollback also failed ({rollback})."
                ),
                None => format!("Import failed and the previous workspace was restored: {error}"),
            });
        }
        if rollback_files.exists() {
            let _ = fs::remove_dir_all(&rollback_files);
        }
        Ok(RestoreResult {
            recovery_backup_path: recovery_path.to_string_lossy().into_owned(),
            summary: manifest_summary(&extracted.manifest),
        })
    })();
    let _ = fs::remove_dir_all(&extracted.root);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("labflow-backup-{label}-{}", nonce()))
    }

    fn test_database(path: &Path, title: &str) -> Connection {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(include_str!("schema.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('exp','EXP001',?1,'','#000')",
                [title],
            )
            .unwrap();
        connection
    }

    #[test]
    fn backup_round_trip_preserves_database_and_files() {
        let root = temp_root("round-trip");
        let files = root.join("files/attachment");
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("raw.txt"), b"raw-result").unwrap();
        fs::write(files.join("preview.png"), b"preview-image").unwrap();
        let database = root.join("labflow.sqlite");
        let connection = test_database(&database, "Original");
        connection.execute("INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id) VALUES ('task','exp','Task','2026-01-01','2026-01-01','completed','record')", []).unwrap();
        connection.execute("INSERT INTO protocols (id,name,category,active_version,accent) VALUES ('protocol','Protocol','Test',1,'#000')", []).unwrap();
        connection.execute("INSERT INTO protocol_versions (protocol_id,version_number,schema_json) VALUES ('protocol',1,'{}')", []).unwrap();
        connection
            .execute(
                "INSERT INTO records VALUES ('record','task','exp','protocol','{\"version\":1}','{\"renderedContent\":\"Frozen body\"}','now')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO record_changes VALUES ('change','record','renderedContent','\"Old body\"','\"Frozen body\"','local_user','now')", []).unwrap();
        connection.execute("INSERT INTO samples (id,experiment_id,sample_code,sample_type,source_record_id,origin) VALUES ('sample','exp','EXP001-RNA01','RNA','record','internal')", []).unwrap();
        connection
            .execute(
                "INSERT INTO record_samples VALUES ('record','sample','output')",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO process_events VALUES ('event','exp','record','rna_extraction','now','{}','labflow_recorded','now')", []).unwrap();
        connection
            .execute("INSERT INTO event_outputs VALUES ('event','sample')", [])
            .unwrap();
        connection.execute("INSERT INTO attachments (id,record_id,file_name,relative_path,created_at,preview_relative_path) VALUES ('attachment','record','raw.txt','files/attachment/raw.txt','now','files/attachment/preview.png')", []).unwrap();
        let archive = root.join("workspace.labflow-backup");
        let exported = export_workspace(
            &connection,
            &root.join("files"),
            &archive,
            "2026-08-26T10:00:00Z",
        )
        .unwrap();
        assert_eq!(exported.summary.counts.attachments, 1);
        assert_eq!(inspect_backup(&archive, &root).unwrap().counts.files, 2);

        connection
            .execute("UPDATE experiments SET title='Changed'", [])
            .unwrap();
        fs::write(files.join("raw.txt"), b"changed").unwrap();
        let mut connection = connection;
        restore_workspace(
            &mut connection,
            &root,
            &archive,
            "2026-08-26T11:00:00Z",
            |_| Ok(()),
        )
        .unwrap();
        let title: String = connection
            .query_row("SELECT title FROM experiments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Original");
        let frozen_body: String = connection
            .query_row(
                "SELECT json_extract(current_data_json,'$.renderedContent') FROM records WHERE id='record'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(frozen_body, "Frozen body");
        assert_eq!(count_table(&connection, "record_changes").unwrap(), 1);
        assert_eq!(count_table(&connection, "process_events").unwrap(), 1);
        assert_eq!(count_table(&connection, "samples").unwrap(), 1);
        assert_eq!(
            fs::read(root.join("files/attachment/raw.txt")).unwrap(),
            b"raw-result"
        );
        assert_eq!(
            fs::read(root.join("files/attachment/preview.png")).unwrap(),
            b"preview-image"
        );
        assert_eq!(collect_files(&root.join("backups")).unwrap().len(), 1);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupted_backup_is_rejected_without_touching_workspace() {
        let root = temp_root("corrupt");
        fs::create_dir_all(root.join("files")).unwrap();
        let database = root.join("labflow.sqlite");
        let connection = test_database(&database, "Preserved");
        let archive = root.join("workspace.labflow-backup");
        export_workspace(
            &connection,
            &root.join("files"),
            &archive,
            "2026-08-26T10:00:00Z",
        )
        .unwrap();
        fs::write(&archive, b"not a zip").unwrap();
        assert!(inspect_backup(&archive, &root).is_err());
        let title: String = connection
            .query_row("SELECT title FROM experiments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Preserved");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_post_restore_rolls_database_and_files_back() {
        let root = temp_root("rollback");
        fs::create_dir_all(root.join("files")).unwrap();
        fs::write(root.join("files/current.txt"), b"current").unwrap();
        let current_path = root.join("labflow.sqlite");
        let mut current = test_database(&current_path, "Current");

        let source_root = temp_root("rollback-source");
        fs::create_dir_all(source_root.join("files")).unwrap();
        fs::write(source_root.join("files/imported.txt"), b"imported").unwrap();
        let source_path = source_root.join("labflow.sqlite");
        let source = test_database(&source_path, "Imported");
        let archive = source_root.join("workspace.labflow-backup");
        export_workspace(
            &source,
            &source_root.join("files"),
            &archive,
            "2026-08-26T10:00:00Z",
        )
        .unwrap();

        let error = restore_workspace(
            &mut current,
            &root,
            &archive,
            "2026-08-26T11:00:00Z",
            |_| Err("simulated migration failure".into()),
        )
        .unwrap_err();
        assert!(error.contains("previous workspace was restored"));
        let title: String = current
            .query_row("SELECT title FROM experiments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Current");
        assert_eq!(
            fs::read(root.join("files/current.txt")).unwrap(),
            b"current"
        );
        assert!(!root.join("files/imported.txt").exists());
        drop(current);
        drop(source);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(source_root).unwrap();
    }

    #[test]
    fn portable_locator_rejects_absolute_and_traversal_paths() {
        assert_eq!(
            portable_relative_path("files/attachment/raw.xlsx").unwrap(),
            PathBuf::from("files/attachment/raw.xlsx")
        );
        assert!(portable_relative_path("/Users/example/raw.xlsx").is_err());
        assert!(portable_relative_path("files/../labflow.sqlite").is_err());
        assert!(portable_relative_path("data/labflow.sqlite").is_err());
    }
}
