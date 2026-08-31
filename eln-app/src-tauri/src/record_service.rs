//! Record domain service.
//!
//! A `Record` freezes a Protocol snapshot at execution time, owns its own
//! `current_data_json` (renamed fields, attachments, results, etc.), and
//! therefore never depends on subsequent Protocol edits. This module backs
//! both:
//!
//! - Desktop UI Tauri commands (`update_record_body`, `delete_record`,
//!   `start_task_record`), and
//! - The Agent Interface (`labflow_*_record` tools) via the MCP adapter.
//!
//! Record creation also runs the Protocol engine; see [`crate::protocol_execution`].

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, error::Error, fmt};
use uuid::Uuid;

use crate::protocol_execution;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordServiceError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Persistence(String),
}

impl RecordServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Persistence(_) => "persistence_error",
        }
    }
}

impl fmt::Display for RecordServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Validation(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Persistence(message) => message,
        };
        formatter.write_str(message)
    }
}

impl Error for RecordServiceError {}

impl From<rusqlite::Error> for RecordServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSampleRef {
    pub sample_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordAttachment {
    pub id: String,
    pub file_name: String,
    pub relative_path: String,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub content_sha256: Option<String>,
    pub preview_relative_path: Option<String>,
    pub width_px: Option<i64>,
    pub height_px: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordResult {
    pub id: String,
    pub r#type: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordChange {
    pub id: String,
    pub field: String,
    pub from: Value,
    pub to: Value,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSummary {
    pub id: String,
    pub task_id: String,
    pub experiment_id: String,
    pub protocol_id: String,
    pub protocol_name: Option<String>,
    pub protocol_snapshot: Value,
    pub title: Option<String>,
    pub updated: String,
    pub notes: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub results: Vec<RecordResult>,
    pub attachments: Vec<RecordAttachment>,
    pub history: Vec<RecordChange>,
    pub rendered_content: Option<Value>,
    pub analysis_sections: Option<Value>,
    pub values: Option<Value>,
    pub protocol_version: Option<Value>,
}

fn record_sample_ids(
    connection: &Connection,
    id: &str,
    role: &str,
) -> Result<Vec<String>, RecordServiceError> {
    let mut statement = connection.prepare(
        "SELECT sample_id FROM record_samples WHERE record_id=?1 AND role=?2 ORDER BY sample_id",
    )?;
    let result = statement
        .query_map(params![id, role], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    Ok(result)
}

pub fn get_record(
    connection: &Connection,
    id: &str,
) -> Result<Option<RecordSummary>, RecordServiceError> {
    let row: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = connection
        .query_row(
            "SELECT id, task_id, experiment_id, protocol_id, current_data_json, updated_at, protocol_snapshot_json FROM records WHERE id=?1",
            [id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((id, task_id, experiment_id, protocol_id, data, updated, snapshot_json)) = row else {
        return Ok(None);
    };
    let current: Value = serde_json::from_str(&data)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    let snapshot: Value = serde_json::from_str(&snapshot_json).unwrap_or(json!({}));
    let mut history = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id, field_path, old_value_json, new_value_json, changed_at FROM record_changes WHERE record_id=?1 ORDER BY changed_at,id",
    )?;
    let rows = statement.query_map([&id], |row| {
        Ok(RecordChange {
            id: row.get(0)?,
            field: row.get(1)?,
            from: serde_json::from_str::<Value>(&row.get::<_, String>(2)?).unwrap_or(json!(null)),
            to: serde_json::from_str::<Value>(&row.get::<_, String>(3)?).unwrap_or(json!(null)),
            at: row.get(4)?,
        })
    })?;
    for change in rows {
        history.push(change?);
    }
    let inputs = record_sample_ids(connection, &id, "input")?;
    let outputs = record_sample_ids(connection, &id, "output")?;
    let mut results = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,result_type,structured_data_json FROM results WHERE record_id=?1 ORDER BY created_at,id",
    )?;
    let rows = statement.query_map([&id], |row| {
        let data: String = row.get(2)?;
        Ok(RecordResult {
            id: row.get(0)?,
            r#type: row.get(1)?,
            data: serde_json::from_str(&data).unwrap_or(json!({})),
        })
    })?;
    for result in rows {
        results.push(result?);
    }
    let mut attachments = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id,file_name,relative_path,mime_type,size,content_sha256,preview_relative_path,width_px,height_px FROM attachments WHERE record_id=?1 ORDER BY created_at,id",
    )?;
    let rows = statement.query_map([&id], |row| {
        Ok(RecordAttachment {
            id: row.get(0)?,
            file_name: row.get(1)?,
            relative_path: row.get(2)?,
            mime_type: row.get(3)?,
            size: row.get(4)?,
            content_sha256: row.get(5)?,
            preview_relative_path: row.get(6)?,
            width_px: row.get(7)?,
            height_px: row.get(8)?,
        })
    })?;
    for attachment in rows {
        attachments.push(attachment?);
    }
    Ok(Some(RecordSummary {
        id,
        task_id,
        experiment_id,
        protocol_id,
        protocol_name: snapshot
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        protocol_snapshot: snapshot.clone(),
        title: current
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned),
        updated,
        notes: current
            .get("notes")
            .and_then(Value::as_str)
            .map(str::to_owned),
        inputs,
        outputs,
        results,
        attachments,
        history,
        rendered_content: current.get("renderedContent").cloned(),
        analysis_sections: current.get("analysisSections").cloned(),
        values: current.get("values").cloned(),
        protocol_version: snapshot.get("version").cloned(),
    }))
}

pub fn list_records(
    connection: &Connection,
    experiment_id: Option<&str>,
) -> Result<Vec<RecordSummary>, RecordServiceError> {
    let mut statement = connection.prepare(
        "SELECT id,task_id,experiment_id,protocol_id,current_data_json,updated_at,protocol_snapshot_json
         FROM records WHERE (?1 IS NULL OR experiment_id=?1) ORDER BY updated_at DESC,id",
    )?;
    let rows = statement.query_map([experiment_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut summaries = Vec::new();
    for row in rows {
        let (id, task_id, experiment_id, protocol_id, data, updated, snapshot_json) = row?;
        let current: Value = serde_json::from_str(&data)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        let snapshot: Value = serde_json::from_str(&snapshot_json).unwrap_or(json!({}));
        summaries.push(RecordSummary {
            id,
            task_id,
            experiment_id,
            protocol_id,
            protocol_name: snapshot
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            protocol_snapshot: snapshot.clone(),
            title: current
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned),
            updated,
            notes: current
                .get("notes")
                .and_then(Value::as_str)
                .map(str::to_owned),
            inputs: Vec::new(),
            outputs: Vec::new(),
            results: Vec::new(),
            attachments: Vec::new(),
            history: Vec::new(),
            rendered_content: current.get("renderedContent").cloned(),
            analysis_sections: current.get("analysisSections").cloned(),
            values: current.get("values").cloned(),
            protocol_version: snapshot.get("version").cloned(),
        });
    }
    if summaries.is_empty() {
        return Ok(summaries);
    }
    let positions = summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| (summary.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let ids_json = serde_json::to_string(
        &summaries
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;

    let mut samples = connection.prepare(
        "SELECT record_id,sample_id,role FROM record_samples
         WHERE record_id IN (SELECT value FROM json_each(?1))
         ORDER BY record_id,role,sample_id",
    )?;
    let rows = samples.query_map([&ids_json], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (record_id, sample_id, role) = row?;
        if let Some(index) = positions.get(&record_id) {
            if role == "input" {
                summaries[*index].inputs.push(sample_id);
            } else if role == "output" {
                summaries[*index].outputs.push(sample_id);
            }
        }
    }

    let mut changes = connection.prepare(
        "SELECT record_id,id,field_path,old_value_json,new_value_json,changed_at
         FROM record_changes WHERE record_id IN (SELECT value FROM json_each(?1))
         ORDER BY record_id,changed_at,id",
    )?;
    let rows = changes.query_map([&ids_json], |row| {
        Ok((
            row.get::<_, String>(0)?,
            RecordChange {
                id: row.get(1)?,
                field: row.get(2)?,
                from: serde_json::from_str::<Value>(&row.get::<_, String>(3)?)
                    .unwrap_or(json!(null)),
                to: serde_json::from_str::<Value>(&row.get::<_, String>(4)?).unwrap_or(json!(null)),
                at: row.get(5)?,
            },
        ))
    })?;
    for row in rows {
        let (record_id, change) = row?;
        if let Some(index) = positions.get(&record_id) {
            summaries[*index].history.push(change);
        }
    }

    let mut results = connection.prepare(
        "SELECT record_id,id,result_type,structured_data_json FROM results
         WHERE record_id IN (SELECT value FROM json_each(?1)) ORDER BY record_id,created_at,id",
    )?;
    let rows = results.query_map([&ids_json], |row| {
        let data: String = row.get(3)?;
        Ok((
            row.get::<_, String>(0)?,
            RecordResult {
                id: row.get(1)?,
                r#type: row.get(2)?,
                data: serde_json::from_str(&data).unwrap_or(json!({})),
            },
        ))
    })?;
    for row in rows {
        let (record_id, result) = row?;
        if let Some(index) = positions.get(&record_id) {
            summaries[*index].results.push(result);
        }
    }

    let mut attachments = connection.prepare(
        "SELECT record_id,id,file_name,relative_path,mime_type,size,content_sha256,preview_relative_path,width_px,height_px FROM attachments
         WHERE record_id IN (SELECT value FROM json_each(?1)) ORDER BY record_id,created_at,id",
    )?;
    let rows = attachments.query_map([&ids_json], |row| {
        Ok((
            row.get::<_, String>(0)?,
            RecordAttachment {
                id: row.get(1)?,
                file_name: row.get(2)?,
                relative_path: row.get(3)?,
                mime_type: row.get(4)?,
                size: row.get(5)?,
                content_sha256: row.get(6)?,
                preview_relative_path: row.get(7)?,
                width_px: row.get(8)?,
                height_px: row.get(9)?,
            },
        ))
    })?;
    for row in rows {
        let (record_id, attachment) = row?;
        if let Some(index) = positions.get(&record_id) {
            summaries[*index].attachments.push(attachment);
        }
    }
    Ok(summaries)
}

pub fn update_record_body(
    connection: &mut Connection,
    id: &str,
    rendered_content: &str,
    change_id: &str,
    changed_at: &str,
) -> Result<(), RecordServiceError> {
    if rendered_content.trim().is_empty() {
        return Err(RecordServiceError::Validation(
            "Record body cannot be empty.".into(),
        ));
    }
    let transaction = connection.transaction()?;
    let current_json: String = transaction
        .query_row(
            "SELECT current_data_json FROM records WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| RecordServiceError::NotFound("Record not found".into()))?;
    let mut current: Value = serde_json::from_str(&current_json)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    if !current.is_object() {
        return Err(RecordServiceError::Persistence(
            "Record data is invalid.".into(),
        ));
    }
    let old_content = current
        .get("renderedContent")
        .cloned()
        .unwrap_or(Value::Null);
    let new_content = json!(rendered_content);
    if old_content == new_content {
        return Ok(());
    }
    current["renderedContent"] = new_content.clone();
    transaction.execute(
        "UPDATE records SET current_data_json=?2,updated_at=?3 WHERE id=?1",
        params![id, current.to_string(), changed_at],
    )?;
    transaction.execute(
        "INSERT INTO record_changes (id,record_id,field_path,old_value_json,new_value_json,actor_id,changed_at) VALUES (?1,?2,'renderedContent',?3,?4,'local_user',?5)",
        params![
            change_id,
            id,
            old_content.to_string(),
            new_content.to_string(),
            changed_at
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Delete a Record plus its assay/qPCR/lineage dependents and attachment
/// directories. Refuses when included in any export manifest or when output
/// samples are reused downstream.
pub fn delete_record(
    connection: &mut Connection,
    attachments_root: &std::path::Path,
    id: &str,
) -> Result<(), RecordServiceError> {
    let exported: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM export_manifests manifest, json_each(manifest.record_ids_json) item WHERE item.value=?1)",
        [id],
        |row| row.get(0),
    )?;
    if exported {
        return Err(RecordServiceError::Conflict(
            "This Record is included in an export manifest and cannot be deleted.".into(),
        ));
    }
    let has_downstream: bool = connection.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM samples output
           WHERE output.source_record_id=?1 AND (
             EXISTS(SELECT 1 FROM record_samples link WHERE link.sample_id=output.id AND link.record_id<>?1)
             OR EXISTS(SELECT 1 FROM event_inputs input JOIN process_events event ON event.id=input.event_id WHERE input.sample_id=output.id AND (event.record_id IS NULL OR event.record_id<>?1))
             OR EXISTS(SELECT 1 FROM sample_relations relation JOIN samples child ON child.id=relation.child_sample_id WHERE relation.parent_sample_id=output.id AND (child.source_record_id IS NULL OR child.source_record_id<>?1))
             OR EXISTS(SELECT 1 FROM assay_well_mappings mapping JOIN assay_plates plate ON plate.id=mapping.plate_id WHERE mapping.sample_id=output.id AND plate.record_id<>?1)
             OR EXISTS(SELECT 1 FROM qpcr_plate_wells legacy WHERE legacy.source_cdna_sample_id=output.id)
           )
         )",
        [id],
        |row| row.get(0),
    )?;
    if has_downstream {
        return Err(RecordServiceError::Conflict(
            "This Record has output Samples used by downstream data and cannot be deleted.".into(),
        ));
    }
    let attachment_ids: Vec<String> = {
        let mut statement = connection.prepare("SELECT id FROM attachments WHERE record_id=?1")?;
        let rows = statement.query_map([id], |row| row.get::<_, String>(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        ids
    };
    let transaction = connection.transaction()?;
    let task_id: String = transaction
        .query_row("SELECT task_id FROM records WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(|_| RecordServiceError::NotFound("Record not found".into()))?;
    transaction.execute(
        "DELETE FROM qpcr_delta_delta_ct_analyses WHERE record_id=?1",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM qpcr_delta_ct_analyses WHERE record_id=?1",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM assay_raw_measurements WHERE import_id IN (SELECT id FROM assay_raw_imports WHERE record_id=?1)",
        [id],
    )?;
    transaction.execute("DELETE FROM assay_raw_imports WHERE record_id=?1", [id])?;
    transaction.execute(
        "DELETE FROM assay_well_mappings WHERE plate_id IN (SELECT id FROM assay_plates WHERE record_id=?1)",
        [id],
    )?;
    transaction.execute("DELETE FROM assay_plates WHERE record_id=?1", [id])?;
    transaction.execute("DELETE FROM assay_items WHERE record_id=?1", [id])?;
    transaction.execute(
        "DELETE FROM sample_usages WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM event_inputs WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM event_outputs WHERE event_id IN (SELECT id FROM process_events WHERE record_id=?1)",
        [id],
    )?;
    transaction.execute("DELETE FROM process_events WHERE record_id=?1", [id])?;
    transaction.execute(
        "DELETE FROM sample_aliases WHERE sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM sample_locations WHERE sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM sample_relations WHERE parent_sample_id IN (SELECT id FROM samples WHERE source_record_id=?1) OR child_sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)",
        [id],
    )?;
    transaction.execute(
        "DELETE FROM record_samples WHERE record_id=?1 OR sample_id IN (SELECT id FROM samples WHERE source_record_id=?1)",
        [id],
    )?;
    transaction.execute("DELETE FROM samples WHERE source_record_id=?1", [id])?;
    transaction.execute("DELETE FROM results WHERE record_id=?1", [id])?;
    transaction.execute("DELETE FROM attachments WHERE record_id=?1", [id])?;
    transaction.execute("DELETE FROM record_changes WHERE record_id=?1", [id])?;
    transaction.execute(
        "UPDATE tasks SET record_id=NULL,status='planned',updated_at=datetime('now') WHERE id=?1 AND record_id=?2",
        params![task_id, id],
    )?;
    transaction.execute("DELETE FROM records WHERE id=?1", [id])?;
    transaction.commit()?;
    for attachment_id in attachment_ids {
        let directory = attachments_root.join(attachment_id);
        if directory.exists() {
            if let Err(error) = std::fs::remove_dir_all(&directory) {
                return Err(RecordServiceError::Persistence(format!(
                    "Record was deleted, but an attachment directory could not be removed: {error}"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartedRecord {
    pub task: Value,
}

/// Start a Record against a Task using a Protocol template (and optional
/// external inputs). This is the only path that creates a Record — it runs
/// the Protocol engine in `protocol_execution` and writes `Record` plus all
/// dependents (process events, sample lineage, results, attachments) in a
/// single transaction.
pub fn start_task_record(
    connection: &mut Connection,
    task_id: &str,
    protocol_id: &str,
    record_id: &str,
    values: Value,
    input_sample_ids: Vec<String>,
    external_inputs: Vec<Value>,
) -> Result<StartedRecord, RecordServiceError> {
    let result = protocol_execution::execute_with_external(
        connection,
        task_id,
        protocol_id,
        record_id,
        values,
        input_sample_ids,
        external_inputs,
    )
    .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    Ok(StartedRecord { task: result.task })
}

/// Generate a fresh UUID for client-supplied resources (Records, change IDs).
pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_schema;

    fn fresh() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        // Service-level tests exercise business rules; turn off FK so callers
        // can seed minimal rows without reproducing the full protocol engine.
        // `apply_schema` later runs `PRAGMA foreign_keys=ON;` again, so disable
        // FK once more after the schema is applied.
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        apply_schema(&connection).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        connection
    }

    fn seed_record(connection: &Connection) {
        connection.execute_batch("
            INSERT INTO experiments VALUES ('e','EXP','Main','','#000');
            INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,updated_at) VALUES ('t','e','T','2026-08-26T09:00','2026-08-26T10:00','planned','2026-08-26T09:00');
            INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES ('r','t','e','p','{\"version\":1}', '{\"renderedContent\":\"initial body\"}', '2026-08-26T09:00');
        ").unwrap();
    }

    #[test]
    fn update_record_body_rejects_empty() {
        let mut connection = fresh();
        seed_record(&connection);
        let err = update_record_body(&mut connection, "r", "   ", "c1", "2026-08-26T09:00:00Z")
            .unwrap_err();
        assert_eq!(err.code(), "validation_error");
    }

    #[test]
    fn update_record_body_is_noop_when_unchanged() {
        let mut connection = fresh();
        seed_record(&connection);
        // Re-submitting the same non-empty body should write no audit row.
        update_record_body(
            &mut connection,
            "r",
            "initial body",
            "c1",
            "2026-08-26T09:00:00Z",
        )
        .unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM record_changes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn delete_record_rejects_unknown() {
        let mut connection = fresh();
        seed_record(&connection);
        let temp = std::env::temp_dir();
        let err = delete_record(&mut connection, &temp, "missing").unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn list_records_returns_seeded_record() {
        let connection = fresh();
        seed_record(&connection);
        connection.execute_batch(
            "INSERT INTO samples (id,workspace_id,experiment_id,sample_code,sample_type,origin)
               VALUES ('s','local','e','EXP-RNA01','RNA','external');
             INSERT INTO record_samples VALUES ('r','s','input');
             INSERT INTO results VALUES ('result','r','measurement','{\"value\":1}','now');
             INSERT INTO attachments (id,record_id,file_name,relative_path,created_at)
               VALUES ('attachment','r','raw.csv','files/attachment/raw.csv','now');
             INSERT INTO record_changes VALUES ('change','r','renderedContent','\"old\"','\"new\"','local_user','now');",
        ).unwrap();
        let records = list_records(&connection, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "r");
        assert_eq!(records[0].inputs, ["s"]);
        assert_eq!(records[0].results.len(), 1);
        assert_eq!(records[0].attachments.len(), 1);
        assert_eq!(records[0].history.len(), 1);
    }
}
