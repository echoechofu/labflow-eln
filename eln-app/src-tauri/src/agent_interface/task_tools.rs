//! Task module for the LabFlow Agent Interface.
//!
//! These tools are the first Agent Interface module; they expose Experiment
//! queries and Task CRUD over the existing Rust [`task_service`] — the same
//! service consumed by the Desktop UI through Tauri commands. No business
//! rules live here: every mutation flows through the shared service, which
//! owns validation and the SQLite transaction.

use chrono::Utc;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;

use crate::task_service::{self, TaskServiceError};

use super::AgentModuleError;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksRequest {
    /// Limit results to this Experiment ID.
    pub experiment_id: Option<String>,
    /// Inclusive local datetime boundary (YYYY-MM-DDTHH:mm[:ss]).
    pub range_start: Option<String>,
    /// Exclusive local datetime boundary (YYYY-MM-DDTHH:mm[:ss]).
    pub range_end: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTaskRequest {
    pub task_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskRequest {
    pub experiment_id: String,
    pub title: String,
    /// Local Task start time (YYYY-MM-DDTHH:mm[:ss]).
    pub starts_at: String,
    /// Local Task end time (YYYY-MM-DDTHH:mm[:ss]).
    pub ends_at: String,
    #[serde(default)]
    pub parent_task_ids: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskRequest {
    pub task_id: String,
    pub title: Option<String>,
    /// New local Task start time (YYYY-MM-DDTHH:mm[:ss]).
    pub starts_at: Option<String>,
    /// New local Task end time (YYYY-MM-DDTHH:mm[:ss]).
    pub ends_at: Option<String>,
    pub status: Option<task_service::TaskStatus>,
    /// When omitted, existing parents remain unchanged; [] clears every parent.
    pub parent_task_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteTaskRequest {
    pub task_id: String,
}

#[derive(Debug, serde::Serialize)]
struct DeleteTaskResult {
    deleted_task_id: String,
}

impl AgentModuleError for TaskServiceError {
    fn error_code(&self) -> &'static str {
        TaskServiceError::code(self)
    }

    fn error_message(&self) -> String {
        self.to_string()
    }
}

#[tool_router(router = task_tools_router, vis = "pub(crate)")]
impl super::LabFlowMcp {
    #[tool(
        name = "labflow_list_experiments",
        description = "List LabFlow Experiments so an Agent can choose a valid experiment_id for Task operations.",
        annotations(title = "List LabFlow Experiments", read_only_hint = true)
    )]
    pub(crate) async fn list_experiments(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| task_service::list_experiments(connection)))
    }

    #[tool(
        name = "labflow_list_tasks",
        description = "List LabFlow calendar Tasks, optionally filtered by Experiment and an overlapping local datetime range.",
        annotations(title = "List LabFlow Tasks", read_only_hint = true)
    )]
    pub(crate) async fn list_tasks(
        &self,
        Parameters(request): Parameters<ListTasksRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| {
            task_service::list_tasks(
                connection,
                task_service::TaskFilter {
                    experiment_id: request.experiment_id,
                    range_start: request.range_start,
                    range_end: request.range_end,
                },
            )
        }))
    }

    #[tool(
        name = "labflow_get_task",
        description = "Get one LabFlow Task including status, Record link, and parent Task IDs.",
        annotations(title = "Get LabFlow Task", read_only_hint = true)
    )]
    pub(crate) async fn get_task(
        &self,
        Parameters(request): Parameters<GetTaskRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| task_service::get_task(connection, &request.task_id)))
    }

    #[tool(
        name = "labflow_create_task",
        description = "Create a planned LabFlow calendar Task in an existing Experiment. LabFlow validates schedule and parent relations transactionally.",
        annotations(
            title = "Create LabFlow Task",
            read_only_hint = false,
            destructive_hint = false
        )
    )]
    pub(crate) async fn create_task(
        &self,
        Parameters(request): Parameters<CreateTaskRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| {
            task_service::create_task(
                connection,
                task_service::CreateTask {
                    experiment_id: request.experiment_id,
                    title: request.title,
                    start: request.starts_at,
                    end: request.ends_at,
                    parent_task_ids: request.parent_task_ids,
                    changed_at: Utc::now().to_rfc3339(),
                },
            )
        }))
    }

    #[tool(
        name = "labflow_update_task",
        description = "Atomically update supplied fields of a LabFlow Task. Omitted parent_task_ids are preserved; [] clears them.",
        annotations(
            title = "Update LabFlow Task",
            read_only_hint = false,
            destructive_hint = false
        )
    )]
    pub(crate) async fn update_task(
        &self,
        Parameters(request): Parameters<UpdateTaskRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| {
            task_service::update_task(
                connection,
                task_service::UpdateTask {
                    task_id: request.task_id,
                    title: request.title,
                    start: request.starts_at,
                    end: request.ends_at,
                    status: request.status,
                    parent_task_ids: request.parent_task_ids,
                    changed_at: Utc::now().to_rfc3339(),
                },
            )
        }))
    }

    #[tool(
        name = "labflow_delete_task",
        description = "Delete a LabFlow Task only when existing domain rules allow it. Tasks with Records or downstream Tasks are protected.",
        annotations(
            title = "Delete LabFlow Task",
            read_only_hint = false,
            destructive_hint = true
        )
    )]
    pub(crate) async fn delete_task(
        &self,
        Parameters(request): Parameters<DeleteTaskRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let task_id = request.task_id;
        Ok(
            self.call(|connection| -> Result<DeleteTaskResult, TaskServiceError> {
                task_service::delete_task(connection, &task_id)?;
                Ok(DeleteTaskResult {
                    deleted_task_id: task_id,
                })
            }),
        )
    }
}
