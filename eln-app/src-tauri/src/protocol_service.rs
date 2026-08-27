//! Protocol domain service.
//!
//! Reads the active `Protocols/protocol_versions` rows and writes user-defined
//! Protocol templates plus new template versions. Used by both the Desktop UI
//! (via the existing `save_user_protocol`/`save_protocol_template_version`
//! Tauri commands) and the Agent Interface (via the LabFlow MCP adapter).
//!
//! This service only manipulates Protocol templates — never Records. Records
//! freeze a Protocol snapshot at creation time; later Protocol edits do not
//! flow into saved Records (see `AgentInterface` skill for the contract).

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::{json, Value};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolServiceError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Persistence(String),
}

impl ProtocolServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Persistence(_) => "persistence_error",
        }
    }
}

impl fmt::Display for ProtocolServiceError {
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

impl Error for ProtocolServiceError {}

impl From<rusqlite::Error> for ProtocolServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

fn require_string(value: &Value, key: &str) -> Result<String, ProtocolServiceError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ProtocolServiceError::Validation(format!("Missing {key}")))
}

fn require_nonempty_string(value: &Value, key: &str) -> Result<String, ProtocolServiceError> {
    let value = require_string(value, key)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ProtocolServiceError::Validation(format!(
            "{key} cannot be empty"
        )));
    }
    Ok(trimmed.to_owned())
}

fn validate_protocol_template(template: &str, spec: &Value) -> Result<(), ProtocolServiceError> {
    if template.trim().is_empty() {
        return Err(ProtocolServiceError::Validation(
            "Record template cannot be empty".into(),
        ));
    }
    let mut allowed = vec![
        "date".to_string(),
        "input_sample_summary".to_string(),
        "output_sample_summary".to_string(),
        "plate_layout_summary".to_string(),
    ];
    for field in spec
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(key) = field.get("key").and_then(Value::as_str) {
            allowed.push(key.to_owned());
        }
    }
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or_else(|| {
            ProtocolServiceError::Validation(
                "Record template contains an unclosed placeholder".into(),
            )
        })?;
        let key = after[..end].trim();
        if !allowed.iter().any(|candidate| candidate == key) {
            return Err(ProtocolServiceError::Validation(format!(
                "Unknown Record template placeholder: {key}"
            )));
        }
        rest = &after[end + 2..];
    }
    Ok(())
}

fn canonical_sample_type(value: &str) -> Result<String, ProtocolServiceError> {
    let canonical = value.trim().to_uppercase();
    if canonical.is_empty()
        || canonical.len() > 32
        || !canonical.chars().enumerate().all(|(index, character)| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit() && index > 0
                || character == '_' && index > 0
        })
    {
        return Err(ProtocolServiceError::Validation(
            "Sample type must use 1–32 letters, numbers, or underscores".into(),
        ));
    }
    Ok(canonical)
}

fn register_sample_type(
    connection: &Connection,
    canonical_type: &str,
    display_name: &str,
    registered_at: &str,
) -> Result<(), ProtocolServiceError> {
    connection.execute(
        "INSERT INTO sample_types (canonical_type,display_name,origin,created_at) VALUES (?1,?2,'user',?3) ON CONFLICT(canonical_type) DO UPDATE SET display_name=excluded.display_name WHERE sample_types.origin='user'",
        params![canonical_type, display_name.trim(), registered_at],
    )?;
    Ok(())
}

/// One Protocol row, joined with the schema of its currently-active version.
#[derive(Debug, Clone)]
pub struct ProtocolView {
    pub id: String,
    pub name: String,
    pub category: String,
    pub version: i64,
    pub accent: String,
    pub description: String,
    pub origin: String,
    pub active_version_origin: String,
    pub spec: Value,
}

fn protocol_view_from_row(row: &Row<'_>) -> Result<ProtocolView, rusqlite::Error> {
    let schema: String = row.get(5)?;
    let spec: Value = serde_json::from_str(&schema).unwrap_or(json!({"blocks":[]}));
    Ok(ProtocolView {
        id: row.get(0)?,
        name: row.get(1)?,
        category: row.get(2)?,
        version: row.get(3)?,
        accent: row.get(4)?,
        description: row.get(6)?,
        origin: row.get(7)?,
        active_version_origin: row.get(8)?,
        spec,
    })
}

pub fn list_protocols(connection: &Connection) -> Result<Vec<ProtocolView>, ProtocolServiceError> {
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.category, p.active_version, p.accent, pv.schema_json, p.description, p.origin, pv.origin FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version ORDER BY p.name,p.id",
    )?;
    let rows = statement.query_map([], protocol_view_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_protocol(
    connection: &Connection,
    id: &str,
) -> Result<Option<ProtocolView>, ProtocolServiceError> {
    Ok(connection
        .query_row(
            "SELECT p.id, p.name, p.category, p.active_version, p.accent, pv.schema_json, p.description, p.origin, pv.origin
             FROM protocols p JOIN protocol_versions pv
               ON pv.protocol_id=p.id AND pv.version_number=p.active_version
             WHERE p.id=?1",
            [id],
            protocol_view_from_row,
        )
        .optional()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct DeletedProtocol {
    pub id: String,
    pub deleted_versions: usize,
    pub retained_records: usize,
}

/// Permanently delete a user-defined Protocol and its version history.
///
/// Records keep the historical Protocol id plus a frozen name/version/schema
/// snapshot, so complete snapshots do not block deletion. Built-in Protocols
/// remain catalog-owned and cannot be deleted. Sample types registered while
/// creating the Protocol are deliberately retained because they may be shared
/// by other Protocols or existing Samples.
pub fn delete_protocol(
    connection: &mut Connection,
    protocol_id: &str,
) -> Result<DeletedProtocol, ProtocolServiceError> {
    let protocol_id = protocol_id.trim();
    if protocol_id.is_empty() {
        return Err(ProtocolServiceError::Validation(
            "Protocol id cannot be empty".into(),
        ));
    }
    let tx = connection.transaction()?;
    let origin: Option<String> = tx
        .query_row(
            "SELECT origin FROM protocols WHERE id=?1",
            [protocol_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(origin) = origin else {
        return Err(ProtocolServiceError::NotFound("Protocol not found".into()));
    };
    if origin != "user" {
        return Err(ProtocolServiceError::Conflict(
            "Built-in Protocols cannot be deleted".into(),
        ));
    }

    let retained_records = {
        let mut statement = tx.prepare(
            "SELECT id,protocol_snapshot_json FROM records WHERE protocol_id=?1 ORDER BY id",
        )?;
        let rows = statement.query_map([protocol_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut count = 0;
        for row in rows {
            let (record_id, snapshot_json) = row?;
            let snapshot: Value = serde_json::from_str(&snapshot_json).map_err(|_| {
                ProtocolServiceError::Conflict(format!(
                    "Record {record_id} has an invalid Protocol snapshot"
                ))
            })?;
            let has_name = snapshot
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty());
            let has_schema = snapshot.get("schema").is_some_and(Value::is_object);
            if !has_name || !has_schema {
                return Err(ProtocolServiceError::Conflict(format!(
                    "Record {record_id} has an incomplete Protocol snapshot; repair it before deleting the Protocol"
                )));
            }
            count += 1;
        }
        count
    };

    let deleted_versions = tx.execute(
        "DELETE FROM protocol_versions WHERE protocol_id=?1",
        [protocol_id],
    )?;
    let deleted_protocol = tx.execute("DELETE FROM protocols WHERE id=?1", [protocol_id])?;
    if deleted_protocol != 1 {
        return Err(ProtocolServiceError::Persistence(
            "Protocol deletion changed an unexpected number of rows".into(),
        ));
    }
    tx.commit()?;
    Ok(DeletedProtocol {
        id: protocol_id.to_owned(),
        deleted_versions,
        retained_records,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedProtocol {
    pub id: String,
    pub version: i64,
}

/// Build a user-defined Protocol (always at version 1) — the basis for new
/// Records. Validates the template body and the output vs. input sample
/// relationship before any DB write.
pub fn save_user_protocol(
    connection: &mut Connection,
    request: Value,
) -> Result<SavedProtocol, ProtocolServiceError> {
    let id = require_nonempty_string(&request, "id")?;
    let name = require_nonempty_string(&request, "name")?;
    let description = require_nonempty_string(&request, "description")?;
    let input_type = canonical_sample_type(&require_string(&request, "inputType")?)?;
    let input_display = request
        .get("inputTypeDisplayName")
        .and_then(Value::as_str)
        .unwrap_or(&input_type)
        .trim();
    let output_behavior = require_string(&request, "outputBehavior")?;
    let output_mode = match output_behavior.as_str() {
        "same_sample" => "same_sample",
        "derived_one" => "per_input",
        "derived_multiple" => "per_input_count",
        "measurement_only" => "none",
        _ => {
            return Err(ProtocolServiceError::Validation(
                "Unsupported Sample output behavior".into(),
            ));
        }
    };
    let consumption_policy = match require_string(&request, "consumptionPolicy")?.as_str() {
        "retain" => "non_destructive",
        "consume" => "consume",
        _ => {
            return Err(ProtocolServiceError::Validation(
                "Unsupported input Sample policy".into(),
            ));
        }
    };
    if output_mode == "same_sample" && consumption_policy == "consume" {
        return Err(ProtocolServiceError::Validation(
            "A consumed Sample cannot continue as the output".into(),
        ));
    }
    let output_type = if matches!(output_mode, "per_input" | "per_input_count") {
        Some(canonical_sample_type(&require_string(
            &request,
            "outputType",
        )?)?)
    } else {
        None
    };
    let template = require_string(&request, "template")?;
    let created_at = require_nonempty_string(&request, "createdAt")?;
    let fields = if output_mode == "per_input_count" {
        json!([{"key":"output_count","label":"每个输入产生数量","kind":"number","required":true,"defaultValue":"2"}])
    } else {
        json!([])
    };
    let mut execution = json!({
        "engine":"sample_flow_v1",
        "eventType":format!("custom:{id}"),
        "inputSource":"experiment_samples",
        "inputCardinality":"many",
        "inputTypes":[input_type],
        "outputMode":output_mode,
        "consumptionPolicy":consumption_policy,
        "metadataPolicy":"inherit_parent"
    });
    if let Some(output_type) = &output_type {
        execution["outputType"] = json!(output_type);
    }
    let spec = json!({
        "schemaVersion":1,
        "userDefined":true,
        "blocks":["选择输入 Sample", "按模板记录实验过程", match output_mode { "same_sample" => "原 Sample 继续", "per_input" => "每个输入产生一个新 Sample", "per_input_count" => "每个输入产生多个新 Sample", _ => "仅记录检测，不产生 Sample" }],
        "fields":fields,
        "template":template,
        "execution":execution
    });
    validate_protocol_template(spec["template"].as_str().unwrap_or(""), &spec)?;
    let tx = connection.transaction()?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM protocols WHERE id=?1)",
        [&id],
        |row| row.get(0),
    )?;
    if exists {
        return Err(ProtocolServiceError::Conflict(
            "Protocol id already exists".into(),
        ));
    }
    register_sample_type(&tx, &input_type, input_display, &created_at)?;
    if let Some(output_type) = &output_type {
        let output_display = request
            .get("outputTypeDisplayName")
            .and_then(Value::as_str)
            .unwrap_or(output_type);
        register_sample_type(&tx, output_type, output_display, &created_at)?;
    }
    tx.execute(
        "INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES (?1,?2,?3,1,?4,?5,'user')",
        params![
            id,
            name.trim(),
            request.get("category").and_then(Value::as_str).unwrap_or("自定义"),
            request.get("accent").and_then(Value::as_str).unwrap_or("#6957e8"),
            description.trim()
        ],
    )?;
    tx.execute(
        "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,1,?2,'user',?3)",
        params![id, spec.to_string(), created_at],
    )?;
    tx.commit()?;
    Ok(SavedProtocol { id, version: 1 })
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedProtocolVersion {
    pub id: String,
    pub previous_version: i64,
    pub version: i64,
}

/// Add a new `schema_json` version to an existing Protocol and make it the
/// active version. Validates either the template body (legacy) or each
/// provided template variant (split-template Protocols).
pub fn save_protocol_template_version(
    connection: &mut Connection,
    request: Value,
) -> Result<SavedProtocolVersion, ProtocolServiceError> {
    let protocol_id = require_nonempty_string(&request, "protocolId")?;
    let created_at = require_nonempty_string(&request, "createdAt")?;
    let tx = connection.transaction()?;
    let (active_version, schema): (i64, String) = tx
        .query_row(
            "SELECT p.active_version,pv.schema_json FROM protocols p JOIN protocol_versions pv ON pv.protocol_id=p.id AND pv.version_number=p.active_version WHERE p.id=?1",
            [&protocol_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ProtocolServiceError::NotFound("Protocol not found".into())
            }
            other => ProtocolServiceError::Persistence(other.to_string()),
        })?;
    let mut spec: Value = serde_json::from_str(&schema)
        .map_err(|_| ProtocolServiceError::Persistence("Protocol schema is invalid".into()))?;
    if spec
        .get("templateVariants")
        .and_then(Value::as_object)
        .is_some()
    {
        let variants = request
            .get("templateVariants")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                ProtocolServiceError::Validation(
                    "This Protocol requires all template variants".into(),
                )
            })?;
        let existing = spec
            .get("templateVariants")
            .and_then(Value::as_object)
            .unwrap();
        for key in existing.keys() {
            let value = variants.get(key).and_then(Value::as_str).ok_or_else(|| {
                ProtocolServiceError::Validation(format!("Missing template variant: {key}"))
            })?;
            validate_protocol_template(value, &spec)?;
        }
        spec["templateVariants"] = Value::Object(variants.clone());
    } else {
        let template = require_string(&request, "template")?;
        validate_protocol_template(&template, &spec)?;
        spec["template"] = json!(template);
    }
    let next_version: i64 = tx.query_row(
        "SELECT coalesce(max(version_number),0)+1 FROM protocol_versions WHERE protocol_id=?1",
        [&protocol_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,?2,?3,'user',?4)",
        params![protocol_id, next_version, spec.to_string(), created_at],
    )?;
    tx.execute(
        "UPDATE protocols SET active_version=?2 WHERE id=?1",
        params![protocol_id, next_version],
    )?;
    tx.commit()?;
    Ok(SavedProtocolVersion {
        id: protocol_id,
        previous_version: active_version,
        version: next_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_schema;

    fn fresh() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        // Service-level tests need minimal seeding; turn off FK so the
        // `apply_schema` re-enable doesn't break stand-alone inserts.
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        apply_schema(&connection).unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        connection
    }

    fn seed_user_protocol(connection: &Connection, id: &str) {
        connection
            .execute(
                "INSERT INTO protocols (id,name,category,active_version,accent,description,origin)
                 VALUES (?1,'Custom','Test',1,'#000','Description','user')",
                [id],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at)
                 VALUES (?1,1,'{\"template\":\"body\"}','user','now')",
                [id],
            )
            .unwrap();
    }

    fn seed_record_snapshot(connection: &Connection, protocol_id: &str, snapshot: &Value) {
        connection
            .execute_batch(
                "INSERT INTO experiments VALUES ('e','EXP','Experiment','','#000');
                 INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,created_at,updated_at)
                   VALUES ('t','e','Task','2026-08-27T09:00:00','2026-08-27T10:00:00','completed','now','now');",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at)
                 VALUES ('r','t','e',?1,?2,'{\"renderedContent\":\"frozen body\"}','now')",
                params![protocol_id, snapshot.to_string()],
            )
            .unwrap();
    }

    #[test]
    fn delete_user_protocol_removes_all_versions_but_retains_sample_types() {
        let mut connection = fresh();
        seed_user_protocol(&connection, "custom");
        connection
            .execute(
                "INSERT INTO protocol_versions VALUES ('custom',2,'{}','user','now')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sample_types VALUES ('TISSUE','Tissue','user','now',NULL)",
                [],
            )
            .unwrap();

        let deleted = delete_protocol(&mut connection, "custom").unwrap();

        assert_eq!(deleted.deleted_versions, 2);
        assert_eq!(deleted.retained_records, 0);
        let versions: i64 = connection
            .query_row(
                "SELECT count(*) FROM protocol_versions WHERE protocol_id='custom'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let sample_type: i64 = connection
            .query_row(
                "SELECT count(*) FROM sample_types WHERE canonical_type='TISSUE'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((versions, sample_type), (0, 1));
    }

    #[test]
    fn delete_user_protocol_keeps_records_with_complete_snapshots() {
        let mut connection = fresh();
        seed_user_protocol(&connection, "custom");
        seed_record_snapshot(
            &connection,
            "custom",
            &json!({"name":"Custom","version":1,"schema":{"template":"body"}}),
        );

        let deleted = delete_protocol(&mut connection, "custom").unwrap();

        assert_eq!(deleted.retained_records, 1);
        let record: (String, String) = connection
            .query_row(
                "SELECT protocol_id,protocol_snapshot_json FROM records WHERE id='r'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(record.0, "custom");
        assert!(record.1.contains("Custom"));
    }

    #[test]
    fn delete_protocol_rejects_builtin_and_incomplete_record_snapshots() {
        let mut connection = fresh();
        connection
            .execute(
                "INSERT INTO protocols (id,name,category,active_version,accent,origin) VALUES ('builtin','Builtin','Test',1,'#000','builtin')",
                [],
            )
            .unwrap();
        let builtin_error = delete_protocol(&mut connection, "builtin").unwrap_err();
        assert_eq!(builtin_error.code(), "conflict");

        seed_user_protocol(&connection, "custom");
        seed_record_snapshot(&connection, "custom", &json!({"version":1}));
        let snapshot_error = delete_protocol(&mut connection, "custom").unwrap_err();
        assert_eq!(snapshot_error.code(), "conflict");
        assert!(get_protocol(&connection, "custom").unwrap().is_some());
    }

    #[test]
    fn delete_protocol_reports_not_found_and_rolls_back_on_failure() {
        let mut connection = fresh();
        assert_eq!(
            delete_protocol(&mut connection, "missing")
                .unwrap_err()
                .code(),
            "not_found"
        );
        seed_user_protocol(&connection, "custom");
        connection
            .execute_batch(
                "CREATE TRIGGER reject_protocol_delete BEFORE DELETE ON protocols
                 BEGIN SELECT RAISE(ABORT, 'delete unavailable'); END;",
            )
            .unwrap();

        let error = delete_protocol(&mut connection, "custom").unwrap_err();

        assert_eq!(error.code(), "persistence_error");
        let versions: i64 = connection
            .query_row(
                "SELECT count(*) FROM protocol_versions WHERE protocol_id='custom'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(versions, 1);
        assert!(get_protocol(&connection, "custom").unwrap().is_some());
    }

    #[test]
    fn save_user_protocol_round_trip() {
        let mut connection = fresh();
        let saved = save_user_protocol(
            &mut connection,
            json!({
                "id":"p1","name":"My protocol","description":"d",
                "inputType":"RNA","inputTypeDisplayName":"RNA",
                "outputBehavior":"derived_one","consumptionPolicy":"retain",
                "outputType":"CDNA","outputTypeDisplayName":"cDNA",
                "template":"hello","createdAt":"2026-08-26T09:00:00Z"
            }),
        )
        .unwrap();
        assert_eq!(saved.id, "p1");
        assert_eq!(saved.version, 1);
        let views = list_protocols(&connection).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, "p1");
    }

    #[test]
    fn save_user_protocol_rejects_duplicate_id() {
        let mut connection = fresh();
        let request = json!({
            "id":"p1","name":"p","description":"d",
            "inputType":"RNA","outputBehavior":"measurement_only",
            "consumptionPolicy":"retain","template":"body","createdAt":"2026-08-26T09:00:00Z"
        });
        save_user_protocol(&mut connection, request.clone()).unwrap();
        let err = save_user_protocol(&mut connection, request).unwrap_err();
        assert_eq!(err.code(), "conflict");
    }

    #[test]
    fn save_user_protocol_rejects_blank_domain_fields_and_unknown_placeholders() {
        let mut connection = fresh();
        let base = json!({
            "id":"p1","name":"Protocol","description":"Description",
            "inputType":"RNA","outputBehavior":"measurement_only",
            "consumptionPolicy":"retain","template":"body",
            "createdAt":"2026-08-26T09:00:00Z"
        });
        for field in ["id", "name", "description", "createdAt"] {
            let mut request = base.clone();
            request[field] = json!("   ");
            assert_eq!(
                save_user_protocol(&mut connection, request)
                    .unwrap_err()
                    .code(),
                "validation_error"
            );
        }
        let mut request = base;
        request["template"] = json!("{{unknown_placeholder}}");
        assert_eq!(
            save_user_protocol(&mut connection, request)
                .unwrap_err()
                .code(),
            "validation_error"
        );
    }

    #[test]
    fn consumed_with_same_sample_is_rejected() {
        let mut connection = fresh();
        let err = save_user_protocol(
            &mut connection,
            json!({
                "id":"p1","name":"p","description":"d",
                "inputType":"RNA","outputBehavior":"same_sample",
                "consumptionPolicy":"consume","template":"body","createdAt":"2026-08-26T09:00:00Z"
            }),
        )
        .unwrap_err();
        assert_eq!(err.code(), "validation_error");
    }

    #[test]
    fn save_template_version_for_missing_protocol_yields_not_found() {
        let mut connection = fresh();
        let err = save_protocol_template_version(
            &mut connection,
            json!({"protocolId":"missing","template":"x","createdAt":"2026-08-26T09:00:00Z"}),
        )
        .unwrap_err();
        assert_eq!(err.code(), "not_found");
    }
}
