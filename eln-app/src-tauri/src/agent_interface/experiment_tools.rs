//! Experiment module for the LabFlow Agent Interface.
//!
//! Exposes CRUD over the Experiment aggregate. Both reads and writes delegate
//! to [`crate::experiment_service`], which the Desktop UI already shares with
//! Tauri commands — the adapter never owns business rules.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};

use crate::experiment_service::{self, ExperimentServiceError};
use crate::task_service;

use super::AgentModuleError;

impl AgentModuleError for ExperimentServiceError {
    fn error_code(&self) -> &'static str {
        ExperimentServiceError::code(self)
    }

    fn error_message(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetExperimentRequest {
    pub experiment_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ExperimentDraft {
    pub id: String,
    pub code: String,
    pub title: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveExperimentRequest {
    /// Full Experiment body to persist. Same shape used by the Desktop UI's
    /// `save_experiment` Tauri command.
    pub experiment: ExperimentDraft,
    /// Local datetime (RFC 3339) used to stamp the lineage audit log entry.
    pub changed_at: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteExperimentRequest {
    pub experiment_id: String,
}

#[tool_router(router = experiment_tools_router, vis = "pub(crate)")]
impl super::LabFlowMcp {
    /// Read a single Experiment by ID. Returns `null` when no row matches.
    #[tool(
        name = "labflow_get_experiment",
        description = "Read a single LabFlow Experiment by ID. Returns null when the Experiment is absent.",
        annotations(title = "Get LabFlow Experiment", read_only_hint = true)
    )]
    pub(crate) async fn get_experiment(
        &self,
        Parameters(request): Parameters<GetExperimentRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let experiment_id = request.experiment_id;
        Ok(self.call(|connection| {
            experiment_service::get_experiment(connection, &experiment_id).map(|option| {
                option
                    .map(task_service::Experiment::to_value)
                    .unwrap_or(serde_json::Value::Null)
            })
        }))
    }

    /// Persist (insert or update) an Experiment and write a lineage audit log
    /// entry.
    #[tool(
        name = "labflow_save_experiment",
        description = "Insert or update a LabFlow Experiment. Reuses the same code/title validation and lineage audit hook as the Desktop UI.",
        annotations(title = "Save LabFlow Experiment", destructive_hint = false)
    )]
    pub(crate) async fn save_experiment(
        &self,
        Parameters(request): Parameters<SaveExperimentRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let changed_at = request.changed_at;
        let experiment = request.experiment;
        Ok(self.call(|connection| {
            let experiment = serde_json::to_value(experiment)
                .map_err(|error| ExperimentServiceError::Persistence(error.to_string()))?;
            experiment_service::save_experiment(connection, experiment, &changed_at)
        }))
    }

    /// Delete an Experiment that has no tasks, samples, or lineage history.
    /// Otherwise the call rejects with `conflict` so the caller can clean up
    /// downstream rows first.
    #[tool(
        name = "labflow_delete_experiment",
        description = "Delete a LabFlow Experiment by ID. Refuses when dependent tasks, samples, or lineage events still reference it.",
        annotations(title = "Delete LabFlow Experiment", destructive_hint = true)
    )]
    pub(crate) async fn delete_experiment(
        &self,
        Parameters(request): Parameters<DeleteExperimentRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let experiment_id = request.experiment_id;
        Ok(self
            .call(|connection| experiment_service::delete_experiment(connection, &experiment_id)))
    }
}

/// Convenience trait that lets MCP clients consume the typed `Experiment`
/// struct without re-serializing the row data ourselves.
trait ExperimentExt {
    fn to_value(self) -> serde_json::Value;
}

impl ExperimentExt for task_service::Experiment {
    fn to_value(self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "code": self.code,
            "title": self.title,
            "description": self.description,
            "color": self.color,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn experiment_serializes_to_expected_fields() {
        let value = task_service::Experiment {
            id: "e1".into(),
            code: "EXP".into(),
            title: "Main".into(),
            description: "desc".into(),
            color: "#abc".into(),
        }
        .to_value();
        assert_eq!(value["id"], "e1");
        assert_eq!(value["code"], "EXP");
        assert_eq!(value["title"], "Main");
        assert_eq!(value["description"], "desc");
        assert_eq!(value["color"], "#abc");
    }

    #[test]
    fn request_deserializes_experiment_body() {
        let parsed: SaveExperimentRequest = serde_json::from_value(json!({
            "experiment": {"id": "e1", "code": "EXP", "title": "Main"},
            "changed_at": "2026-08-26T09:00:00Z"
        }))
        .unwrap();
        assert_eq!(parsed.changed_at, "2026-08-26T09:00:00Z");
        assert_eq!(parsed.experiment.code, "EXP");
    }
}
