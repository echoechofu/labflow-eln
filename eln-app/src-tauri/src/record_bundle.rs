//! Packages an incrementally-rendered merged PDF together with Record attachments.

use serde::Serialize;
use std::{
    collections::HashSet,
    fs::{self, File},
    path::{Path, PathBuf},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    record_attachment_service::portable_file_path,
    record_service::{self, RecordServiceError},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordBundleResult {
    pub path: String,
    pub record_count: usize,
    pub attachment_count: usize,
}

fn safe_archive_name(value: &str) -> String {
    let result: String = value
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\' | ':') {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = result.trim_matches(['.', ' ']);
    if trimmed.is_empty() {
        "attachment.bin".into()
    } else {
        trimmed.chars().take(120).collect()
    }
}

pub fn export_records_bundle(
    connection: &rusqlite::Connection,
    app_data_root: &Path,
    record_ids: &[String],
    pdf_path: &Path,
    destination: &Path,
) -> Result<RecordBundleResult, RecordServiceError> {
    if record_ids.is_empty() || record_ids.len() > 1_000 {
        return Err(RecordServiceError::Validation(
            "Choose between 1 and 1000 Records to export.".into(),
        ));
    }
    let unique: HashSet<_> = record_ids.iter().collect();
    if unique.len() != record_ids.len() {
        return Err(RecordServiceError::Validation(
            "Record selection contains duplicate ids.".into(),
        ));
    }
    if !pdf_path.is_file() {
        return Err(RecordServiceError::NotFound(
            "Merged PDF is missing.".into(),
        ));
    }
    if destination
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("zip")
    {
        return Err(RecordServiceError::Validation(
            "Record bundle must use a .zip extension.".into(),
        ));
    }
    let parent = destination.parent().ok_or_else(|| {
        RecordServiceError::Validation("Export destination has no parent directory.".into())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;

    let mut attachment_entries: Vec<(String, PathBuf)> = Vec::new();
    for id in record_ids {
        let record = record_service::get_record(connection, id)?
            .ok_or_else(|| RecordServiceError::NotFound(format!("Record not found: {id}")))?;
        for attachment in &record.attachments {
            let path =
                portable_file_path(app_data_root, &attachment.relative_path).ok_or_else(|| {
                    RecordServiceError::Persistence("Attachment locator is invalid.".into())
                })?;
            if !path.is_file() {
                return Err(RecordServiceError::NotFound(format!(
                    "Attachment file is missing: {} / {}",
                    record.id, attachment.file_name
                )));
            }
            attachment_entries.push((
                format!(
                    "attachments/{}-{}",
                    attachment.id,
                    safe_archive_name(&attachment.file_name)
                ),
                path,
            ));
        }
    }

    let partial = parent.join(format!(".labflow-records-{}.partial", uuid::Uuid::new_v4()));
    let result = (|| {
        let archive_file = File::create(&partial)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        let mut archive = ZipWriter::new(archive_file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o600);
        archive
            .start_file("LabFlow-Records.pdf", options)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        let mut pdf = File::open(pdf_path)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        std::io::copy(&mut pdf, &mut archive)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        for (name, path) in &attachment_entries {
            archive
                .start_file(name, options)
                .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
            let mut source = File::open(path)
                .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
            std::io::copy(&mut source, &mut archive)
                .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        }
        archive
            .finish()
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?
            .sync_all()
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        }
        fs::rename(&partial, destination)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        Ok(RecordBundleResult {
            path: destination.to_string_lossy().into_owned(),
            record_count: record_ids.len(),
            attachment_count: attachment_entries.len(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_schema;
    use rusqlite::Connection;
    use std::io::Read;
    use zip::ZipArchive;

    #[test]
    fn bundle_contains_only_merged_pdf_and_flat_attachment_collection() {
        let root = std::env::temp_dir().join(format!("labflow-bundle-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("files/a")).unwrap();
        fs::write(root.join("files/a/raw.csv"), b"well,value\nA1,1\n").unwrap();
        let pdf = root.join("merged.pdf");
        fs::write(&pdf, b"%PDF-1.4\n%%EOF\n").unwrap();
        let connection = Connection::open_in_memory().unwrap();
        apply_schema(&connection).unwrap();
        connection.execute_batch(
            "INSERT INTO experiments (id,experiment_code,title,description,color) VALUES ('e','EXP','Main','','#000');
             INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,updated_at) VALUES ('t','e','Task','2026-08-31T09:00','2026-08-31T10:00','completed','now');
             INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES ('r','t','e','p','{}','{\"title\":\"Record\"}','now');
             INSERT INTO attachments (id,record_id,file_name,relative_path,mime_type,size,created_at) VALUES ('a','r','raw.csv','files/a/raw.csv','text/csv',16,'now');",
        ).unwrap();
        let destination = root.join("records.zip");
        let exported =
            export_records_bundle(&connection, &root, &["r".into()], &pdf, &destination).unwrap();
        assert_eq!(exported.record_count, 1);
        assert_eq!(exported.attachment_count, 1);
        let mut archive = ZipArchive::new(File::open(&destination).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        assert!(archive.by_name("LabFlow-Records.pdf").is_ok());
        let mut attachment = archive.by_name("attachments/a-raw.csv").unwrap();
        let mut bytes = Vec::new();
        attachment.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"well,value\nA1,1\n");
        drop(attachment);
        drop(archive);
        fs::remove_dir_all(root).unwrap();
    }
}
