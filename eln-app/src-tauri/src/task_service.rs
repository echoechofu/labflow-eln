use chrono::NaiveDateTime;
use rmcp::schemars;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use uuid::Uuid;

use crate::task_graph;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskServiceError {
    Validation(String),
    NotFound(String),
    Conflict(String),
    Persistence(String),
}

impl TaskServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "validation_error",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Persistence(_) => "persistence_error",
        }
    }
}

impl fmt::Display for TaskServiceError {
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

impl Error for TaskServiceError {}

fn persistence(error: impl fmt::Display) -> TaskServiceError {
    TaskServiceError::Persistence(error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Experiment {
    pub id: String,
    pub code: String,
    pub title: String,
    pub description: String,
    pub color: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, rmcp::schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Planned,
    InProgress,
    Completed,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Result<Self, TaskServiceError> {
        match value {
            "planned" => Ok(Self::Planned),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            _ => Err(TaskServiceError::Validation("Invalid task status".into())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub experiment_id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub status: TaskStatus,
    pub record_id: Option<String>,
    pub parent_task_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NewExperiment {
    pub id: String,
    pub code: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct SaveTask {
    pub id: String,
    pub experiment_id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub status: Option<TaskStatus>,
    pub parent_task_ids: Vec<String>,
    pub replace_parent_relations: bool,
    pub validate_temporal_relations: bool,
    pub changed_at: String,
    pub new_experiment: Option<NewExperiment>,
}

#[derive(Debug, Clone)]
pub struct CreateTask {
    pub experiment_id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub parent_task_ids: Vec<String>,
    pub changed_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateTask {
    pub task_id: String,
    pub title: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub status: Option<TaskStatus>,
    pub parent_task_ids: Option<Vec<String>>,
    pub changed_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub experiment_id: Option<String>,
    pub range_start: Option<String>,
    pub range_end: Option<String>,
}

fn normalize_datetime(value: &str, field: &str) -> Result<String, TaskServiceError> {
    let trimmed = value.trim();
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(trimmed, format) {
            return Ok(parsed.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }
    Err(TaskServiceError::Validation(format!(
        "{field} must use YYYY-MM-DDTHH:mm or YYYY-MM-DDTHH:mm:ss"
    )))
}

pub(crate) fn validate_task(title: &str, start: &str, end: &str) -> Result<(), TaskServiceError> {
    if title.trim().is_empty() {
        return Err(TaskServiceError::Validation("Task name is required".into()));
    }
    if end <= start {
        return Err(TaskServiceError::Validation(
            "End time must be after start time".into(),
        ));
    }
    Ok(())
}

fn task_from_row(row: &Row<'_>) -> Result<Task, rusqlite::Error> {
    let id: String = row.get(0)?;
    let status: String = row.get(5)?;
    let parents_json: String = row.get(7)?;
    let parents = serde_json::from_str(&parents_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let status = TaskStatus::parse(&status).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Task {
        id,
        experiment_id: row.get(1)?,
        title: row.get(2)?,
        start: row.get(3)?,
        end: row.get(4)?,
        status,
        record_id: row.get(6)?,
        parent_task_ids: parents,
    })
}

pub fn list_experiments(connection: &Connection) -> Result<Vec<Experiment>, TaskServiceError> {
    let mut statement = connection
        .prepare(
            "SELECT id,experiment_code,title,description,color FROM experiments ORDER BY experiment_code,title,id",
        )
        .map_err(persistence)?;
    let result = statement
        .query_map([], |row| {
            Ok(Experiment {
                id: row.get(0)?,
                code: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                color: row.get(4)?,
            })
        })
        .map_err(persistence)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(persistence);
    result
}

pub fn list_tasks(
    connection: &Connection,
    filter: TaskFilter,
) -> Result<Vec<Task>, TaskServiceError> {
    let range_start = filter
        .range_start
        .as_deref()
        .map(|value| normalize_datetime(value, "range_start"))
        .transpose()?;
    let range_end = filter
        .range_end
        .as_deref()
        .map(|value| normalize_datetime(value, "range_end"))
        .transpose()?;
    if range_start
        .as_ref()
        .zip(range_end.as_ref())
        .is_some_and(|(start, end)| end <= start)
    {
        return Err(TaskServiceError::Validation(
            "range_end must be after range_start".into(),
        ));
    }
    let mut statement = connection
        .prepare(
            "SELECT task.id,task.experiment_id,task.title,task.start_time,task.end_time,task.status,task.record_id,
                    coalesce((SELECT json_group_array(parent_task_id) FROM (
                        SELECT parent_task_id FROM task_relations
                        WHERE child_task_id=task.id ORDER BY created_at,id
                    )),'[]')
             FROM tasks
             AS task
             WHERE (?1 IS NULL OR experiment_id=?1)
               AND (?2 IS NULL OR end_time>?2)
               AND (?3 IS NULL OR start_time<?3)
             ORDER BY start_time,end_time,title,id",
        )
        .map_err(persistence)?;
    let result = statement
        .query_map(
            params![filter.experiment_id, range_start, range_end],
            task_from_row,
        )
        .map_err(persistence)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(persistence);
    result
}

pub fn get_task(connection: &Connection, task_id: &str) -> Result<Task, TaskServiceError> {
    connection
        .query_row(
            "SELECT task.id,task.experiment_id,task.title,task.start_time,task.end_time,task.status,task.record_id,
                    coalesce((SELECT json_group_array(parent_task_id) FROM (
                        SELECT parent_task_id FROM task_relations
                        WHERE child_task_id=task.id ORDER BY created_at,id
                    )),'[]')
             FROM tasks AS task WHERE task.id=?1",
            [task_id],
            task_from_row,
        )
        .optional()
        .map_err(persistence)?
        .ok_or_else(|| TaskServiceError::NotFound("Task not found".into()))
}

fn ensure_experiment(connection: &Connection, experiment_id: &str) -> Result<(), TaskServiceError> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM experiments WHERE id=?1)",
            [experiment_id],
            |row| row.get(0),
        )
        .map_err(persistence)?;
    if !exists {
        return Err(TaskServiceError::NotFound("Experiment not found".into()));
    }
    Ok(())
}

pub fn save_task(connection: &mut Connection, request: SaveTask) -> Result<Task, TaskServiceError> {
    let title = request.title.trim().to_owned();
    let start = normalize_datetime(&request.start, "starts_at")?;
    let end = normalize_datetime(&request.end, "ends_at")?;
    validate_task(&title, &start, &end)?;

    let transaction = connection.transaction().map_err(persistence)?;
    if let Some(experiment) = request.new_experiment.as_ref() {
        if experiment.title.trim().is_empty() {
            return Err(TaskServiceError::Validation(
                "Experiment name is required".into(),
            ));
        }
        transaction
            .execute(
                "INSERT INTO experiments (id,experiment_code,title,description,color) VALUES (?1,?2,?3,'','#6957e8')",
                params![experiment.id, experiment.code, experiment.title.trim()],
            )
            .map_err(persistence)?;
    } else {
        ensure_experiment(&transaction, &request.experiment_id)?;
    }

    let existing = match get_task(&transaction, &request.id) {
        Ok(task) => Some(task),
        Err(TaskServiceError::NotFound(_)) => None,
        Err(error) => return Err(error),
    };
    if existing
        .as_ref()
        .is_some_and(|task| task.experiment_id != request.experiment_id)
    {
        let has_relations: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM task_relations WHERE parent_task_id=?1 OR child_task_id=?1)",
                [&request.id],
                |row| row.get(0),
            )
            .map_err(persistence)?;
        if has_relations {
            return Err(TaskServiceError::Conflict(
                "Remove this Task's dependencies before moving it to another Experiment".into(),
            ));
        }
    }

    let status = request
        .status
        .or_else(|| existing.as_ref().map(|task| task.status))
        .unwrap_or(TaskStatus::Planned);
    transaction
        .execute(
            "INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?7)
             ON CONFLICT(id) DO UPDATE SET experiment_id=excluded.experiment_id,title=excluded.title,start_time=excluded.start_time,end_time=excluded.end_time,status=excluded.status,updated_at=excluded.updated_at",
            params![request.id, request.experiment_id, title, start, end, status.as_str(), request.changed_at],
        )
        .map_err(persistence)?;
    if request.replace_parent_relations {
        task_graph::replace_parents(
            &transaction,
            &request.id,
            &request.experiment_id,
            &request.parent_task_ids,
            &request.changed_at,
        )
        .map_err(TaskServiceError::Conflict)?;
    }
    if request.validate_temporal_relations {
        task_graph::validate_temporal_neighbors(&transaction, &request.id)
            .map_err(TaskServiceError::Conflict)?;
    }
    transaction.commit().map_err(persistence)?;
    get_task(connection, &request.id)
}

pub fn create_task(
    connection: &mut Connection,
    request: CreateTask,
) -> Result<Task, TaskServiceError> {
    let id = format!("task-{}", Uuid::new_v4());
    save_task(
        connection,
        SaveTask {
            id,
            experiment_id: request.experiment_id,
            title: request.title,
            start: request.start,
            end: request.end,
            status: Some(TaskStatus::Planned),
            parent_task_ids: request.parent_task_ids,
            replace_parent_relations: true,
            validate_temporal_relations: true,
            changed_at: request.changed_at,
            new_experiment: None,
        },
    )
}

pub fn update_task(
    connection: &mut Connection,
    request: UpdateTask,
) -> Result<Task, TaskServiceError> {
    if request.title.is_none()
        && request.start.is_none()
        && request.end.is_none()
        && request.status.is_none()
        && request.parent_task_ids.is_none()
    {
        return Err(TaskServiceError::Validation(
            "At least one Task field must be updated".into(),
        ));
    }
    let replace_parent_relations = request.parent_task_ids.is_some();
    let validate_temporal_relations = replace_parent_relations || request.start.is_some();
    let existing = get_task(connection, &request.task_id)?;
    save_task(
        connection,
        SaveTask {
            id: existing.id,
            experiment_id: existing.experiment_id,
            title: request.title.unwrap_or(existing.title),
            start: request.start.unwrap_or(existing.start),
            end: request.end.unwrap_or(existing.end),
            status: request.status.or(Some(existing.status)),
            parent_task_ids: request.parent_task_ids.unwrap_or(existing.parent_task_ids),
            replace_parent_relations,
            validate_temporal_relations,
            changed_at: request.changed_at,
            new_experiment: None,
        },
    )
}

pub fn delete_task(connection: &mut Connection, task_id: &str) -> Result<(), TaskServiceError> {
    let task = get_task(connection, task_id)?;
    if task.record_id.is_some() {
        return Err(TaskServiceError::Conflict(
            "This task already has an experimental record and cannot be deleted.".into(),
        ));
    }
    if task_graph::has_children(connection, task_id).map_err(persistence)? {
        return Err(TaskServiceError::Conflict(
            "This task is required by a downstream Task and cannot be deleted.".into(),
        ));
    }
    let transaction = connection.transaction().map_err(persistence)?;
    transaction
        .execute(
            "DELETE FROM task_relations WHERE child_task_id=?1",
            [task_id],
        )
        .map_err(persistence)?;
    if transaction
        .execute("DELETE FROM tasks WHERE id=?1", [task_id])
        .map_err(persistence)?
        != 1
    {
        return Err(TaskServiceError::NotFound("Task not found".into()));
    }
    transaction.commit().map_err(persistence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('e','EXP100','Main','','#000')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('other','EXP101','Other','','#111')",
                [],
            )
            .unwrap();
        connection
    }

    fn create(db: &mut Connection, id: &str, experiment: &str, start: &str, parents: Vec<String>) {
        save_task(
            db,
            SaveTask {
                id: id.into(),
                experiment_id: experiment.into(),
                title: id.into(),
                start: start.into(),
                end: start.replace(":00", ":30"),
                status: None,
                parent_task_ids: parents,
                replace_parent_relations: true,
                validate_temporal_relations: true,
                changed_at: "2026-08-26T00:00:00Z".into(),
                new_experiment: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn creates_lists_updates_and_deletes_tasks() {
        let mut db = database();
        create(&mut db, "parent", "e", "2026-08-26T09:00", vec![]);
        create(
            &mut db,
            "child",
            "e",
            "2026-08-26T10:00",
            vec!["parent".into()],
        );
        let tasks = list_tasks(
            &db,
            TaskFilter {
                experiment_id: Some("e".into()),
                range_start: Some("2026-08-26T09:30".into()),
                range_end: Some("2026-08-26T11:00".into()),
            },
        )
        .unwrap();
        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            ["child"]
        );
        let updated = update_task(
            &mut db,
            UpdateTask {
                task_id: "child".into(),
                title: Some("Updated".into()),
                status: Some(TaskStatus::InProgress),
                changed_at: "now".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.status, TaskStatus::InProgress);
        delete_task(&mut db, "child").unwrap();
        delete_task(&mut db, "parent").unwrap();
        assert!(list_tasks(&db, TaskFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn rejects_missing_experiment_cross_experiment_and_invalid_parent_time_atomically() {
        let mut db = database();
        create(&mut db, "parent", "e", "2026-08-26T10:00", vec![]);
        create(&mut db, "other-parent", "other", "2026-08-26T08:00", vec![]);
        let base_count: i64 = db
            .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        for (id, experiment, start, parents) in [
            ("missing-exp", "missing", "2026-08-26T11:00", vec![]),
            (
                "cross",
                "e",
                "2026-08-26T11:00",
                vec!["other-parent".into()],
            ),
            ("bad-time", "e", "2026-08-26T09:00", vec!["parent".into()]),
        ] {
            assert!(save_task(
                &mut db,
                SaveTask {
                    id: id.into(),
                    experiment_id: experiment.into(),
                    title: id.into(),
                    start: start.into(),
                    end: start.replace(":00", ":30"),
                    status: None,
                    parent_task_ids: parents,
                    replace_parent_relations: true,
                    validate_temporal_relations: true,
                    changed_at: "now".into(),
                    new_experiment: None,
                }
            )
            .is_err());
        }
        let final_count: i64 = db
            .query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(final_count, base_count);
    }

    #[test]
    fn rejects_cycles_duplicate_parents_and_time_changes_that_break_existing_edges() {
        let mut db = database();
        create(&mut db, "a", "e", "2026-08-26T08:00", vec![]);
        create(&mut db, "b", "e", "2026-08-26T09:00", vec!["a".into()]);
        create(&mut db, "c", "e", "2026-08-26T10:00", vec!["b".into()]);
        assert!(update_task(
            &mut db,
            UpdateTask {
                task_id: "a".into(),
                parent_task_ids: Some(vec!["c".into()]),
                changed_at: "now".into(),
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("cycle"));
        assert!(update_task(
            &mut db,
            UpdateTask {
                task_id: "c".into(),
                parent_task_ids: Some(vec!["b".into(), "b".into()]),
                changed_at: "now".into(),
                ..Default::default()
            }
        )
        .is_err());
        assert!(update_task(
            &mut db,
            UpdateTask {
                task_id: "a".into(),
                start: Some("2026-08-26T09:30".into()),
                end: Some("2026-08-26T09:45".into()),
                changed_at: "now".into(),
                ..Default::default()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("earlier"));
        assert_eq!(get_task(&db, "a").unwrap().start, "2026-08-26T08:00:00");
        assert_eq!(
            get_task(&db, "a").unwrap().parent_task_ids,
            Vec::<String>::new()
        );
    }

    #[test]
    fn missing_task_and_delete_guards_are_service_errors() {
        let mut db = database();
        assert!(matches!(
            get_task(&db, "missing"),
            Err(TaskServiceError::NotFound(_))
        ));
        create(&mut db, "parent", "e", "2026-08-26T08:00", vec![]);
        create(
            &mut db,
            "child",
            "e",
            "2026-08-26T09:00",
            vec!["parent".into()],
        );
        assert!(matches!(
            delete_task(&mut db, "parent"),
            Err(TaskServiceError::Conflict(_))
        ));
        assert!(get_task(&db, "parent").is_ok());
        db.execute(
            "INSERT INTO protocols (id,name,category,active_version,accent,description,origin)
             VALUES ('protocol','Protocol','test',1,'#000','','user')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at)
             VALUES ('record','child','e','protocol','{}','{}','now')",
            [],
        )
        .unwrap();
        db.execute("UPDATE tasks SET record_id='record' WHERE id='child'", [])
            .unwrap();
        assert!(matches!(
            delete_task(&mut db, "child"),
            Err(TaskServiceError::Conflict(_))
        ));
        assert!(get_task(&db, "child").is_ok());
    }
}
