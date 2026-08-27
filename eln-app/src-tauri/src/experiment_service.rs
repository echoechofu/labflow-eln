//! Experiment domain service.
//!
//! Persists and queries Experiment definitions. These functions back both the
//! Tauri command surface (`save_experiment`, `delete_experiment`) and the Agent
//! Interface (`labflow_*_experiment` tools). They share an explicit error type
//! — [`ExperimentServiceError`] — so neither UI nor MCP adapter duplicates
//! validation or transaction logic.
//!
//! The `Experiment` struct is re-exported from [`crate::task_service`] for
//! continuity with the original Task/Calendar module.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{error::Error, fmt};
use uuid::Uuid;

use crate::lineage;
use crate::task_service::{Experiment, TaskServiceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperimentServiceError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Persistence(String),
}

impl ExperimentServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Persistence(_) => "persistence_error",
        }
    }
}

impl fmt::Display for ExperimentServiceError {
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

impl Error for ExperimentServiceError {}

impl From<rusqlite::Error> for ExperimentServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Persistence(error.to_string())
    }
}

impl From<String> for ExperimentServiceError {
    fn from(message: String) -> Self {
        Self::Persistence(message)
    }
}

impl From<TaskServiceError> for ExperimentServiceError {
    fn from(error: TaskServiceError) -> Self {
        match error {
            TaskServiceError::Validation(message) => Self::Validation(message),
            TaskServiceError::NotFound(message) => Self::NotFound(message),
            TaskServiceError::Conflict(message) => Self::Conflict(message),
            TaskServiceError::Persistence(message) => Self::Persistence(message),
        }
    }
}

/// Read a single Experiment by ID, returning `None` when absent.
pub fn get_experiment(
    connection: &Connection,
    id: &str,
) -> Result<Option<Experiment>, ExperimentServiceError> {
    let row = connection
        .query_row(
            "SELECT id,experiment_code,title,description,color FROM experiments WHERE id=?1",
            [id],
            |row| {
                Ok(Experiment {
                    id: row.get(0)?,
                    code: row.get(1)?,
                    title: row.get(2)?,
                    description: row.get(3)?,
                    color: row.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

fn require_string(value: &Value, key: &str) -> Result<String, ExperimentServiceError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ExperimentServiceError::Validation(format!("Missing {key}")))
}

/// Insert or update an Experiment, recording the change in the lineage audit
/// log. Validation rejects empty required fields but does **not** require any
/// particular code/title format — that policy belongs to the UI/business layer.
pub fn save_experiment(
    connection: &mut Connection,
    experiment: Value,
    changed_at: &str,
) -> Result<Experiment, ExperimentServiceError> {
    let id = require_string(&experiment, "id")?;
    let code = require_string(&experiment, "code")?;
    let title = require_string(&experiment, "title")?;
    if id.trim().is_empty() {
        return Err(ExperimentServiceError::Validation(
            "Missing experiment id".into(),
        ));
    }
    if code.trim().is_empty() {
        return Err(ExperimentServiceError::Validation(
            "Experiment code cannot be empty".into(),
        ));
    }
    if title.trim().is_empty() {
        return Err(ExperimentServiceError::Validation(
            "Experiment title cannot be empty".into(),
        ));
    }
    let description = experiment
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let color = experiment
        .get("color")
        .and_then(Value::as_str)
        .unwrap_or("#6957e8");
    let transaction = connection.transaction()?;
    let existing: Option<Value> = transaction
        .query_row(
            "SELECT experiment_code,title,description,color FROM experiments WHERE id=?1",
            [&id],
            |row| {
                Ok(json!({
                    "code": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "description": row.get::<_, String>(2)?,
                    "color": row.get::<_, String>(3)?,
                }))
            },
        )
        .optional()?;
    transaction.execute(
        "INSERT INTO experiments (id,experiment_code,title,description,color) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET experiment_code=excluded.experiment_code,title=excluded.title,description=excluded.description,color=excluded.color",
        params![id, code, title, description, color],
    )?;
    lineage::audit(
        &transaction,
        &format!("change-{}", Uuid::new_v4()),
        "experiment",
        &id,
        "$",
        existing.unwrap_or(json!(null)),
        experiment.clone(),
        changed_at,
    )?;
    transaction.commit()?;
    Ok(Experiment {
        id,
        code,
        title,
        description: description.to_owned(),
        color: color.to_owned(),
    })
}

/// Delete an Experiment that has no tasks, samples, or lineage history.
/// Otherwise the call rejects with a Conflict so the caller can either archive
/// or move dependent rows first.
pub fn delete_experiment(connection: &Connection, id: &str) -> Result<(), ExperimentServiceError> {
    let dependent: i64 = connection.query_row(
        "SELECT (SELECT count(*) FROM tasks WHERE experiment_id=?1)+(SELECT count(*) FROM samples WHERE experiment_id=?1)+(SELECT count(*) FROM process_events WHERE experiment_id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if dependent > 0 {
        return Err(ExperimentServiceError::Conflict(
            "Cannot delete an experiment with tasks, samples, or lineage history".into(),
        ));
    }
    let affected = connection.execute("DELETE FROM experiments WHERE id=?1", [id])?;
    if affected == 0 {
        return Err(ExperimentServiceError::NotFound(
            "Experiment not found".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_schema;

    fn fresh() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        apply_schema(&connection).unwrap();
        connection
    }

    #[test]
    fn save_then_get_round_trip() {
        let mut connection = fresh();
        let saved = save_experiment(
            &mut connection,
            json!({"id": "e1", "code": "EXP-001", "title": "Main", "description": "desc", "color": "#abc"}),
            "2026-08-26T09:00:00Z",
        )
        .unwrap();
        assert_eq!(saved.id, "e1");
        let fetched = get_experiment(&connection, "e1").unwrap().unwrap();
        assert_eq!(fetched.title, "Main");
        assert_eq!(fetched.color, "#abc");
    }

    #[test]
    fn save_rejects_empty_code() {
        let mut connection = fresh();
        let err = save_experiment(
            &mut connection,
            json!({"id": "e1", "code": "", "title": "Main"}),
            "2026-08-26T09:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err.code(), "validation_error");
    }

    #[test]
    fn save_rolls_back_when_audit_write_fails() {
        let mut connection = fresh();
        connection
            .execute_batch(
                "CREATE TRIGGER reject_entity_audit BEFORE INSERT ON entity_changes
                 BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END;",
            )
            .unwrap();
        let err = save_experiment(
            &mut connection,
            json!({"id": "e1", "code": "EXP", "title": "Main"}),
            "2026-08-27T09:00:00",
        )
        .unwrap_err();
        assert_eq!(err.code(), "persistence_error");
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM experiments WHERE id='e1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn delete_blocked_when_tasks_exist() {
        let mut connection = fresh();
        save_experiment(
            &mut connection,
            json!({"id": "e1", "code": "EXP", "title": "Main"}),
            "2026-08-26T09:00:00Z",
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,updated_at) VALUES ('t','e1','T','2026-08-26T09:00','2026-08-26T10:00','planned','2026-08-26T09:00')",
                [],
            )
            .unwrap();
        let err = delete_experiment(&connection, "e1").unwrap_err();
        assert_eq!(err.code(), "conflict");
    }
}
