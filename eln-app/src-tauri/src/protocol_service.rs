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
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
};

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

fn validate_schema(spec: &Value) -> Result<(), ProtocolServiceError> {
    crate::protocol_schema::validate_schema(spec).map_err(ProtocolServiceError::Validation)
}

fn optional_nonempty_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn requested_fields(request: &Value) -> Result<Option<Vec<Value>>, ProtocolServiceError> {
    request
        .get("fields")
        .filter(|v| !v.is_null())
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| ProtocolServiceError::Validation("fields must be an array".into()))
        })
        .transpose()
}

fn validate_user_fields(fields: &[Value]) -> Result<(), ProtocolServiceError> {
    for field in fields {
        if !matches!(
            field.get("kind").and_then(Value::as_str),
            Some("text" | "number" | "select")
        ) {
            return Err(ProtocolServiceError::Validation(
                "User fields must use text, number, or select kind".into(),
            ));
        }
    }
    Ok(())
}

fn field_map(spec: &Value) -> HashMap<&str, &Value> {
    spec.get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| Some((field.get("key")?.as_str()?, field)))
        .collect()
}

fn contains_string(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(value) => value == needle,
        Value::Array(values) => values.iter().any(|value| contains_string(value, needle)),
        Value::Object(values) => values.values().any(|value| contains_string(value, needle)),
        _ => false,
    }
}

/// Preserve fields that carry executor structure. When cloning a source, every
/// source field is part of that capability contract. On an ordinary edit,
/// custom scalar fields remain removable, while structural/dependency fields
/// cannot silently invalidate execution.
fn protect_fields(
    base: &Value,
    proposed: &[Value],
    protect_all: bool,
) -> Result<(), ProtocolServiceError> {
    let proposed_by_key: HashMap<_, _> = proposed
        .iter()
        .filter_map(|field| Some((field.get("key")?.as_str()?, field)))
        .collect();
    let execution = base.get("execution").unwrap_or(&Value::Null);
    let selector = base.get("templateSelector").and_then(Value::as_str);
    let persisted_protected: HashSet<_> = base
        .get("protectedFieldKeys")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let legacy_builtin_execution = base
        .get("execution")
        .and_then(|value| value.get("engine"))
        .and_then(Value::as_str)
        != Some("sample_flow_v1")
        && base.get("userDefined").and_then(Value::as_bool) != Some(true);
    let visible_dependencies: HashSet<_> = base
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| field.get("visibleWhen")?.get("key")?.as_str())
        .collect();
    for (key, old) in field_map(base) {
        let kind = old.get("kind").and_then(Value::as_str).unwrap_or("");
        let protected = protect_all
            || persisted_protected.contains(key)
            || legacy_builtin_execution
            || matches!(key, "output_count" | "plate_format" | "condition_groups")
            || !matches!(kind, "text" | "number" | "select")
            || selector == Some(key)
            || visible_dependencies.contains(key)
            || contains_string(execution, key);
        if !protected {
            continue;
        }
        let new = proposed_by_key.get(key).ok_or_else(|| {
            ProtocolServiceError::Validation(format!(
                "Protected Protocol field cannot be removed: {key}"
            ))
        })?;
        if new.get("kind") != old.get("kind") {
            return Err(ProtocolServiceError::Validation(format!(
                "Protected Protocol field kind cannot change: {key}"
            )));
        }
        if kind == "select" && new.get("options") != old.get("options") {
            return Err(ProtocolServiceError::Validation(format!(
                "Protected select options cannot change: {key}"
            )));
        }
        for property in ["required", "visibleWhen", "visibleForInputTypes"] {
            // Scalar defaults and labels are editable; executor dependencies
            // must remain reachable through the Record form.
            let unchanged = if property == "required" {
                new.get(property).and_then(Value::as_bool).unwrap_or(false)
                    == old.get(property).and_then(Value::as_bool).unwrap_or(false)
            } else {
                new.get(property) == old.get(property)
            };
            if !unchanged {
                return Err(ProtocolServiceError::Validation(format!(
                    "Protected Protocol field {property} cannot change: {key}"
                )));
            }
        }
    }
    Ok(())
}

fn apply_template_overrides(spec: &mut Value, request: &Value) -> Result<(), ProtocolServiceError> {
    if let Some(variants) = request.get("templateVariants").filter(|v| !v.is_null()) {
        let variants = variants.as_object().ok_or_else(|| {
            ProtocolServiceError::Validation("templateVariants must be an object".into())
        })?;
        spec["templateVariants"] = Value::Object(variants.clone());
    }
    if let Some(template) = request.get("template").filter(|v| !v.is_null()) {
        let template = template
            .as_str()
            .ok_or_else(|| ProtocolServiceError::Validation("template must be text".into()))?;
        spec["template"] = json!(template);
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

fn requested_output_rules(
    request: &Value,
) -> Result<Vec<(String, String, u64)>, ProtocolServiceError> {
    let rules = request
        .get("outputRules")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProtocolServiceError::Validation(
                "Multi-type output requires at least two output rules".into(),
            )
        })?;
    if !(2..=16).contains(&rules.len()) {
        return Err(ProtocolServiceError::Validation(
            "Multi-type output requires 2–16 output rules".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut normalized = Vec::with_capacity(rules.len());
    for rule in rules {
        let sample_type = canonical_sample_type(&require_string(rule, "outputType")?)?;
        if !seen.insert(sample_type.clone()) {
            return Err(ProtocolServiceError::Validation(format!(
                "Duplicate output Sample type: {sample_type}"
            )));
        }
        let display_name = rule
            .get("outputTypeDisplayName")
            .and_then(Value::as_str)
            .unwrap_or(&sample_type)
            .trim()
            .to_string();
        if display_name.is_empty() {
            return Err(ProtocolServiceError::Validation(
                "Output Sample display name is required".into(),
            ));
        }
        let count = rule
            .get("count")
            .and_then(Value::as_u64)
            .filter(|count| (1..=96).contains(count))
            .ok_or_else(|| {
                ProtocolServiceError::Validation("Each multi-type output count must be 1–96".into())
            })?;
        total += count;
        normalized.push((sample_type, display_name, count));
    }
    if total > 96 {
        return Err(ProtocolServiceError::Validation(
            "Multi-type outputs may total at most 96 per input".into(),
        ));
    }
    Ok(normalized)
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

fn requested_input_types(request: &Value) -> Result<Vec<(String, String)>, ProtocolServiceError> {
    let allow_any = request
        .get("allowAnyInputType")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(values) = request.get("inputTypes") {
        let values = values.as_array().ok_or_else(|| {
            ProtocolServiceError::Validation("inputTypes must be an array".into())
        })?;
        if allow_any {
            if !values.is_empty() {
                return Err(ProtocolServiceError::Validation(
                    "An unrestricted Protocol cannot also list input types".into(),
                ));
            }
            return Ok(Vec::new());
        }
        if !(1..=16).contains(&values.len()) {
            return Err(ProtocolServiceError::Validation(
                "Applicable input types must contain 1–16 entries".into(),
            ));
        }
        let mut seen = HashSet::new();
        return values
            .iter()
            .map(|value| {
                let canonical = canonical_sample_type(&require_string(value, "canonicalType")?)?;
                if !seen.insert(canonical.clone()) {
                    return Err(ProtocolServiceError::Validation(
                        "Applicable input types cannot contain duplicates".into(),
                    ));
                }
                let display = require_string(value, "displayName")?;
                if display.chars().count() > 64 {
                    return Err(ProtocolServiceError::Validation(
                        "Input type display name must be 1–64 characters".into(),
                    ));
                }
                Ok((canonical, display))
            })
            .collect();
    }
    if allow_any {
        return Ok(Vec::new());
    }
    let canonical = canonical_sample_type(&require_string(request, "inputType")?)?;
    let display = request
        .get("inputTypeDisplayName")
        .and_then(Value::as_str)
        .unwrap_or(&canonical)
        .trim()
        .to_string();
    Ok(vec![(canonical, display)])
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
    let created_at = require_nonempty_string(&request, "createdAt")?;
    let source_protocol_id = optional_nonempty_string(&request, "sourceProtocolId");

    if let Some(source_protocol_id) = source_protocol_id {
        let source = get_protocol(connection, &source_protocol_id)?
            .ok_or_else(|| ProtocolServiceError::NotFound("Source Protocol not found".into()))?;
        let mut spec = source.spec;
        spec["userDefined"] = json!(true);
        spec["protectedFieldKeys"] =
            Value::Array(field_map(&spec).keys().map(|key| json!(key)).collect());
        if let Some(fields) = requested_fields(&request)? {
            validate_user_fields(
                fields
                    .iter()
                    .filter(|field| {
                        let key = field.get("key").and_then(Value::as_str);
                        !field_map(&spec).contains_key(key.unwrap_or(""))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
                    .as_slice(),
            )?;
            protect_fields(&spec, &fields, true)?;
            spec["fields"] = Value::Array(fields);
        }
        apply_template_overrides(&mut spec, &request)?;
        validate_schema(&spec)?;

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
        tx.execute(
            "INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES (?1,?2,?3,1,?4,?5,'user')",
            params![id, name, request.get("category").and_then(Value::as_str).unwrap_or(&source.category), request.get("accent").and_then(Value::as_str).unwrap_or(&source.accent), description],
        )?;
        tx.execute(
            "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,1,?2,'user',?3)",
            params![id, spec.to_string(), created_at],
        )?;
        tx.commit()?;
        return Ok(SavedProtocol { id, version: 1 });
    }

    let input_types = requested_input_types(&request)?;
    let output_behavior = require_string(&request, "outputBehavior")?;
    let multiple_sample_mode = request
        .get("multipleSampleMode")
        .and_then(Value::as_str)
        .unwrap_or("identical");
    if !matches!(multiple_sample_mode, "identical" | "condition_groups") {
        return Err(ProtocolServiceError::Validation(
            "Unsupported multiple Sample mode".into(),
        ));
    }
    if output_behavior != "derived_multiple" && multiple_sample_mode != "identical" {
        return Err(ProtocolServiceError::Validation(
            "Condition allocation requires multiple output Samples".into(),
        ));
    }
    let plate_mapping = request
        .get("plateMapping")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let condition_container = request
        .get("conditionContainer")
        .and_then(Value::as_str)
        .unwrap_or(if plate_mapping {
            "plate"
        } else {
            "independent"
        });
    if !matches!(condition_container, "independent" | "plate" | "dish") {
        return Err(ProtocolServiceError::Validation(
            "Unsupported condition container".into(),
        ));
    }
    let plate_mapping = condition_container == "plate";
    if plate_mapping
        && !(output_behavior == "derived_multiple" && multiple_sample_mode == "condition_groups")
    {
        return Err(ProtocolServiceError::Validation(
            "Plate mapping requires condition-allocated output Samples".into(),
        ));
    }
    let output_mode = match output_behavior.as_str() {
        "same_sample" => "same_sample",
        "one_to_one" => "record_one",
        "one_to_many" => "record_many",
        "one_to_zero" => "none",
        // Legacy request values remain accepted for existing MCP clients.
        "derived_one" => "per_input",
        "derived_multiple" if multiple_sample_mode == "condition_groups" => "per_input_conditions",
        "derived_multiple" => "per_input_count",
        "derived_multi_type" => "per_input_types",
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
    if matches!(output_behavior.as_str(), "one_to_one" | "one_to_zero")
        && consumption_policy != "consume"
    {
        return Err(ProtocolServiceError::Validation(
            "1→1 and 1→0 Protocols must consume their input Samples".into(),
        ));
    }
    if output_behavior == "same_sample" && consumption_policy != "non_destructive" {
        return Err(ProtocolServiceError::Validation(
            "An unchanged Sample must be retained".into(),
        ));
    }
    if output_mode == "same_sample" && consumption_policy == "consume" {
        return Err(ProtocolServiceError::Validation(
            "A consumed Sample cannot continue as the output".into(),
        ));
    }
    let output_type = if matches!(
        output_mode,
        "per_input" | "per_input_count" | "per_input_conditions"
    ) {
        Some(canonical_sample_type(&require_string(
            &request,
            "outputType",
        )?)?)
    } else {
        None
    };
    let output_rules = if output_mode == "per_input_types" {
        Some(requested_output_rules(&request)?)
    } else {
        None
    };
    let template = require_string(&request, "template")?;
    let mut fields = match output_mode {
        "per_input_count" => json!([
            {"key":"output_count","label":"每个输入产生数量","kind":"number","required":true,"defaultValue":"2"}
        ]),
        "per_input_conditions" if plate_mapping => json!([
            {"key":"plate_format","label":"孔板规格","kind":"select","required":true,"options":["6孔板","12孔板","24孔板","48孔板","96孔板","384孔板"]},
            {"key":"condition_groups","label":"实验条件分配","kind":"condition_groups","required":true}
        ]),
        "per_input_conditions" => json!([
            {"key":"condition_groups","label":"实验条件分配","kind":"condition_groups","required":true}
        ]),
        _ => json!([]),
    };
    if let Some(additional) = requested_fields(&request)? {
        validate_user_fields(&additional)?;
        let fields_array = fields.as_array_mut().unwrap();
        let existing: HashSet<_> = fields_array
            .iter()
            .filter_map(|field| field.get("key")?.as_str())
            .collect();
        if additional
            .iter()
            .filter_map(|field| field.get("key")?.as_str())
            .any(|key| existing.contains(key))
        {
            return Err(ProtocolServiceError::Validation(
                "A user field duplicates a generated field key".into(),
            ));
        }
        fields_array.extend(additional);
    }
    let mut execution = json!({
        "engine":"sample_flow_v1",
        "eventType":format!("custom:{id}"),
        "inputSource":"experiment_samples",
        "inputCardinality":"many",
        "inputTypes":input_types.iter().map(|(canonical, _)| canonical).collect::<Vec<_>>(),
        "inputTypePolicy":"uniform",
        "outputMode":output_mode,
        "consumptionPolicy":consumption_policy,
        "metadataPolicy":"inherit_parent"
    });
    if let Some(output_type) = &output_type {
        execution["outputType"] = json!(output_type);
    }
    if let Some(rules) = &output_rules {
        execution["outputRules"] = Value::Array(
            rules
                .iter()
                .map(|(sample_type, _, count)| json!({"sampleType":sample_type,"count":count}))
                .collect(),
        );
    }
    if output_mode == "per_input_conditions" {
        execution["conditionAllocation"] =
            json!({"plateMapping":plate_mapping,"containerMode":condition_container});
    }
    let spec = json!({
        "schemaVersion":1,
        "userDefined":true,
        "blocks":["选择输入 Sample", "按模板记录实验过程", match output_mode { "same_sample" => "原 Sample 沿用", "record_one" => "逐个登记一个新 Sample", "record_many" => "逐行登记多个新 Sample", "per_input" => "每个输入产生一个新 Sample", "per_input_count" => "每个输入产生多个相同条件的 Sample", "per_input_conditions" => "按实验条件产生多个 Sample", "per_input_types" => "每个输入产生多种类型的 Sample", _ => "消耗输入且不产生 Sample" }],
        "fields":fields,
        "template":template,
        "execution":execution
    });
    validate_schema(&spec)?;
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
    for (canonical, display_name) in &input_types {
        register_sample_type(&tx, canonical, display_name, &created_at)?;
    }
    if let Some(output_type) = &output_type {
        let output_display = request
            .get("outputTypeDisplayName")
            .and_then(Value::as_str)
            .unwrap_or(output_type);
        register_sample_type(&tx, output_type, output_display, &created_at)?;
    }
    if let Some(rules) = &output_rules {
        for (sample_type, display_name, _) in rules {
            register_sample_type(&tx, sample_type, display_name, &created_at)?;
        }
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
    let (active_version, schema): (i64, String) = connection
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
    let old_spec: Value = serde_json::from_str(&schema)
        .map_err(|_| ProtocolServiceError::Persistence("Protocol schema is invalid".into()))?;
    let source_protocol_id = optional_nonempty_string(&request, "sourceProtocolId");
    let mut spec = if let Some(source_protocol_id) = &source_protocol_id {
        let mut source_spec = get_protocol(connection, source_protocol_id)?
            .ok_or_else(|| ProtocolServiceError::NotFound("Source Protocol not found".into()))?
            .spec;
        // The new source is the complete capability contract, including its
        // event type. Historical Records and ProcessEvents already retain the
        // prior version's snapshot/event and are not rewritten here.
        source_spec["userDefined"] = json!(true);
        source_spec["protectedFieldKeys"] = Value::Array(
            field_map(&source_spec)
                .keys()
                .map(|key| json!(key))
                .collect(),
        );
        source_spec
    } else {
        old_spec.clone()
    };
    if let Some(fields) = requested_fields(&request)? {
        if source_protocol_id.is_some() {
            protect_fields(&spec, &fields, true)?;
        } else {
            protect_fields(&old_spec, &fields, false)?;
        }
        let base_fields = field_map(&spec);
        let added = fields
            .iter()
            .filter(|field| {
                field
                    .get("key")
                    .and_then(Value::as_str)
                    .is_none_or(|key| !base_fields.contains_key(key))
            })
            .cloned()
            .collect::<Vec<_>>();
        validate_user_fields(&added)?;
        spec["fields"] = Value::Array(fields);
    }
    apply_template_overrides(&mut spec, &request)?;
    if let Some(defaults) = request
        .get("defaultOutputTypes")
        .filter(|value| !value.is_null())
    {
        let defaults = defaults.as_array().ok_or_else(|| {
            ProtocolServiceError::Validation("defaultOutputTypes must be an array".into())
        })?;
        if !matches!(
            spec.pointer("/execution/outputMode")
                .and_then(Value::as_str),
            Some("record_one" | "record_many")
        ) {
            return Err(ProtocolServiceError::Validation(
                "Output defaults are only supported for Record-defined outputs".into(),
            ));
        }
        let normalized = defaults
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| {
                        ProtocolServiceError::Validation("Default output type must be text".into())
                    })
                    .and_then(canonical_sample_type)
                    .map(Value::String)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if normalized
            .iter()
            .any(|value| matches!(value.as_str(), Some("PLATE" | "DISH" | "WELL" | "OTHER")))
        {
            return Err(ProtocolServiceError::Validation(
                "PLATE, DISH, WELL, and OTHER cannot be default output material types".into(),
            ));
        }
        if normalized.is_empty() || normalized.len() > 96 {
            return Err(ProtocolServiceError::Validation(
                "Default output types must contain 1–96 entries".into(),
            ));
        }
        if spec
            .pointer("/execution/outputMode")
            .and_then(Value::as_str)
            == Some("record_one")
            && normalized.len() != 1
        {
            return Err(ProtocolServiceError::Validation(
                "1→1 Protocols require exactly one default output type".into(),
            ));
        }
        spec["execution"]["defaultOutputTypes"] = Value::Array(normalized);
    }
    validate_schema(&spec)?;

    let tx = connection.transaction()?;
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

    fn seed_builtins(connection: &Connection) {
        for builtin in crate::protocol_catalog::builtins() {
            connection.execute(
                "INSERT INTO protocols (id,name,category,active_version,accent,description,origin) VALUES (?1,?2,?3,?4,?5,'','builtin')",
                params![builtin.id, builtin.name, builtin.category, builtin.schema_version, builtin.accent],
            ).unwrap();
            connection.execute(
                "INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at) VALUES (?1,?2,?3,'builtin','now')",
                params![builtin.id, builtin.schema_version, builtin.schema],
            ).unwrap();
        }
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
                "INSERT INTO sample_types VALUES ('CUSTOM_TISSUE','Custom tissue','user','now',NULL)",
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
                "SELECT count(*) FROM sample_types WHERE canonical_type='CUSTOM_TISSUE'",
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
    fn record_defined_flows_fix_identity_and_defer_output_types() {
        let mut connection = fresh();
        for (id, behavior, policy, expected_mode) in [
            ("continue", "same_sample", "retain", "same_sample"),
            ("convert", "one_to_one", "consume", "record_one"),
            ("split", "one_to_many", "retain", "record_many"),
            ("consume", "one_to_zero", "consume", "none"),
        ] {
            save_user_protocol(
                &mut connection,
                json!({
                    "id":id,"name":id,"description":"d",
                    "inputType":"ANIMAL","inputTypeDisplayName":"动物",
                    "outputBehavior":behavior,"consumptionPolicy":policy,
                    "template":"{{input_sample_summary}} -> {{output_sample_summary}}",
                    "createdAt":"2026-09-18T09:00:00"
                }),
            )
            .unwrap();
            let view = get_protocol(&connection, id).unwrap().unwrap();
            assert_eq!(view.spec["execution"]["outputMode"], expected_mode);
            assert!(view.spec["execution"].get("outputType").is_none());
        }

        let invalid = save_user_protocol(
            &mut connection,
            json!({
                "id":"invalid","name":"invalid","description":"d",
                "inputType":"ANIMAL","outputBehavior":"one_to_one",
                "consumptionPolicy":"retain","template":"body","createdAt":"now"
            }),
        )
        .unwrap_err();
        assert_eq!(invalid.code(), "validation_error");

        let saved = save_protocol_template_version(
            &mut connection,
            json!({
                "protocolId":"split",
                "defaultOutputTypes":["tissue","NUCLEI","TISSUE"],
                "createdAt":"2026-09-18T10:00:00"
            }),
        )
        .unwrap();
        assert_eq!(saved.version, 2);
        let updated = get_protocol(&connection, "split").unwrap().unwrap();
        assert_eq!(
            updated.spec["execution"]["defaultOutputTypes"],
            json!(["TISSUE", "NUCLEI", "TISSUE"])
        );

        save_user_protocol(
            &mut connection,
            json!({
                "id":"multi-input","name":"multi-input","description":"d",
                "inputTypes":[
                    {"canonicalType":"CELL","displayName":"细胞"},
                    {"canonicalType":"ORGANOID","displayName":"类器官"}
                ],
                "allowAnyInputType":false,
                "outputBehavior":"same_sample","consumptionPolicy":"retain",
                "template":"body","createdAt":"now"
            }),
        )
        .unwrap();
        let multi = get_protocol(&connection, "multi-input").unwrap().unwrap();
        assert_eq!(
            multi.spec["execution"]["inputTypes"],
            json!(["CELL", "ORGANOID"])
        );
        assert_eq!(multi.spec["execution"]["inputTypePolicy"], "uniform");

        save_user_protocol(
            &mut connection,
            json!({
                "id":"any-input","name":"any-input","description":"d",
                "inputTypes":[],"allowAnyInputType":true,
                "outputBehavior":"same_sample","consumptionPolicy":"retain",
                "template":"body","createdAt":"now"
            }),
        )
        .unwrap();
        let any = get_protocol(&connection, "any-input").unwrap().unwrap();
        assert_eq!(any.spec["execution"]["inputTypes"], json!([]));
    }

    #[test]
    fn multi_type_protocol_registers_every_output_rule() {
        let mut connection = fresh();
        save_user_protocol(
            &mut connection,
            json!({
                "id":"multi-harvest","name":"Multi harvest","description":"d",
                "inputType":"CELL","inputTypeDisplayName":"Cell",
                "outputBehavior":"derived_multi_type","consumptionPolicy":"retain",
                "outputRules":[
                    {"outputType":"SUP","outputTypeDisplayName":"Supernatant","count":1},
                    {"outputType":"RNA","outputTypeDisplayName":"RNA","count":1},
                    {"outputType":"PROTEIN","outputTypeDisplayName":"Protein","count":2}
                ],
                "template":"{{input_sample_summary}} -> {{output_sample_summary}}",
                "createdAt":"2026-08-26T09:00:00Z"
            }),
        )
        .unwrap();

        let view = get_protocol(&connection, "multi-harvest").unwrap().unwrap();
        assert_eq!(view.spec["execution"]["outputMode"], "per_input_types");
        assert_eq!(
            view.spec["execution"]["outputRules"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let registered: i64 = connection
            .query_row(
                "SELECT count(*) FROM sample_types WHERE canonical_type IN ('SUP','RNA','PROTEIN')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(registered, 3);

        let duplicate = save_user_protocol(
            &mut connection,
            json!({
                "id":"invalid-multi","name":"Invalid","description":"d",
                "inputType":"CELL","outputBehavior":"derived_multi_type",
                "consumptionPolicy":"retain",
                "outputRules":[
                    {"outputType":"RNA","outputTypeDisplayName":"RNA","count":1},
                    {"outputType":"rna","outputTypeDisplayName":"RNA duplicate","count":1}
                ],
                "template":"body","createdAt":"now"
            }),
        )
        .unwrap_err();
        assert_eq!(duplicate.code(), "validation_error");
        assert!(get_protocol(&connection, "invalid-multi")
            .unwrap()
            .is_none());
    }

    #[test]
    fn condition_allocated_protocol_keeps_output_type_independent_from_plate_mapping() {
        let mut connection = fresh();
        save_user_protocol(
            &mut connection,
            json!({
                "id":"p-conditions","name":"Condition split","description":"d",
                "inputType":"CELL","inputTypeDisplayName":"Cell",
                "outputBehavior":"derived_multiple",
                "multipleSampleMode":"condition_groups","plateMapping":true,
                "outputType":"DISH","outputTypeDisplayName":"Dish",
                "consumptionPolicy":"retain",
                "template":"{{condition_groups_summary}}\n{{output_sample_summary}}",
                "createdAt":"2026-08-26T09:00:00Z"
            }),
        )
        .unwrap();
        let view = get_protocol(&connection, "p-conditions").unwrap().unwrap();
        assert_eq!(view.spec["execution"]["outputMode"], "per_input_conditions");
        assert_eq!(view.spec["execution"]["outputType"], "DISH");
        assert_eq!(
            view.spec["execution"]["conditionAllocation"]["plateMapping"],
            true
        );
        assert_eq!(view.spec["fields"][1]["kind"], "condition_groups");
    }

    #[test]
    fn dish_condition_allocation_is_persisted_without_plate_field() {
        let mut connection = fresh();
        save_user_protocol(
            &mut connection,
            json!({
                "id":"p-dishes","name":"Dish split","description":"d",
                "inputType":"CELL","inputTypeDisplayName":"Cell",
                "outputBehavior":"derived_multiple","multipleSampleMode":"condition_groups",
                "conditionContainer":"dish","outputType":"CELL","outputTypeDisplayName":"Cell",
                "consumptionPolicy":"retain","template":"{{condition_groups_summary}}",
                "createdAt":"2026-09-14T09:00:00Z"
            }),
        )
        .unwrap();
        let view = get_protocol(&connection, "p-dishes").unwrap().unwrap();
        assert_eq!(
            view.spec["execution"]["conditionAllocation"]["containerMode"],
            "dish"
        );
        assert_eq!(
            view.spec["execution"]["conditionAllocation"]["plateMapping"],
            false
        );
        assert!(view.spec["fields"]
            .as_array()
            .unwrap()
            .iter()
            .all(|field| field["key"] != "plate_format"));
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

    #[test]
    fn all_builtin_active_schemas_can_be_copied_without_client_execution() {
        let mut connection = fresh();
        seed_builtins(&connection);
        for builtin in crate::protocol_catalog::builtins() {
            let id = format!("copy-{}", builtin.id);
            save_user_protocol(
                &mut connection,
                json!({
                    "id":id,"name":format!("Copy {}", builtin.name),"description":"copy",
                    "sourceProtocolId":builtin.id,"createdAt":"2026-09-12T09:00:00",
                    "execution":{"eventType":"attacker","outputMode":"none"}
                }),
            )
            .unwrap();
            let copied = get_protocol(&connection, &id).unwrap().unwrap().spec;
            let source: Value = serde_json::from_str(builtin.schema).unwrap();
            assert_eq!(copied["execution"], source["execution"], "{}", builtin.id);
            assert_eq!(copied["fields"], source["fields"], "{}", builtin.id);
            assert_eq!(
                copied.get("terminalAssay"),
                source.get("terminalAssay"),
                "{}",
                builtin.id
            );
        }
    }

    #[test]
    fn source_and_field_failures_are_atomic() {
        let mut connection = fresh();
        seed_builtins(&connection);
        let missing = save_user_protocol(&mut connection, json!({
            "id":"missing-copy","name":"Copy","description":"d","sourceProtocolId":"absent","createdAt":"now"
        })).unwrap_err();
        assert_eq!(missing.code(), "not_found");
        let invalid = save_user_protocol(&mut connection, json!({
            "id":"bad-copy","name":"Copy","description":"d","sourceProtocolId":"pro-rna","createdAt":"now",
            "fields":[{"key":"resuspension_volume","label":"x","kind":"text"},{"key":"storage","label":"x","kind":"select","options":["立即逆转录","-80℃ 保存"]}]
        })).unwrap_err();
        assert_eq!(invalid.code(), "validation_error");
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM protocols WHERE id IN ('missing-copy','bad-copy')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn version_fields_are_frozen_and_protected_capability_fields_survive() {
        let mut connection = fresh();
        seed_builtins(&connection);
        save_user_protocol(&mut connection, json!({
            "id":"rna-copy","name":"RNA copy","description":"d","sourceProtocolId":"pro-rna","createdAt":"now"
        })).unwrap();
        let original = get_protocol(&connection, "rna-copy").unwrap().unwrap().spec;
        let mut fields = original["fields"].as_array().unwrap().clone();
        fields[0]["label"] = json!("New label");
        fields.push(json!({"key":"note","label":"Note","kind":"text"}));
        save_protocol_template_version(
            &mut connection,
            json!({
                "protocolId":"rna-copy","fields":fields,"createdAt":"later"
            }),
        )
        .unwrap();
        let (v1, v2): (String, String) = connection.query_row(
            "SELECT a.schema_json,b.schema_json FROM protocol_versions a JOIN protocol_versions b ON b.protocol_id=a.protocol_id WHERE a.protocol_id='rna-copy' AND a.version_number=1 AND b.version_number=2",
            [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_ne!(
            serde_json::from_str::<Value>(&v1).unwrap()["fields"],
            serde_json::from_str::<Value>(&v2).unwrap()["fields"]
        );
        let err = save_protocol_template_version(&mut connection, json!({
            "protocolId":"rna-copy","fields":[{"key":"note","label":"Note","kind":"text"}],"template":"{{note}}","createdAt":"latest"
        })).unwrap_err();
        assert_eq!(err.code(), "validation_error");
        assert_eq!(
            get_protocol(&connection, "rna-copy")
                .unwrap()
                .unwrap()
                .version,
            2
        );
    }

    #[test]
    fn source_version_switches_to_the_new_execution_without_rewriting_old_version() {
        let mut connection = fresh();
        seed_builtins(&connection);
        save_user_protocol(
            &mut connection,
            json!({"id":"switchable","name":"Switchable","description":"d","sourceProtocolId":"pro-rna","createdAt":"now"}),
        )
        .unwrap();
        save_protocol_template_version(
            &mut connection,
            json!({"protocolId":"switchable","sourceProtocolId":"pro-cell-treatment","createdAt":"later"}),
        )
        .unwrap();
        let (v1, v2): (String, String) = connection
            .query_row(
                "SELECT a.schema_json,b.schema_json FROM protocol_versions a JOIN protocol_versions b ON b.protocol_id=a.protocol_id WHERE a.protocol_id='switchable' AND a.version_number=1 AND b.version_number=2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&v1).unwrap()["execution"]["eventType"],
            "rna_extraction"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&v2).unwrap()["execution"]["eventType"],
            "treatment"
        );
    }
}
