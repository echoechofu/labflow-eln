//! LabFlow Agent Interface (MCP adapter).
//!
//! This module is an *adapter*, never a business layer. Every tool must parse
//! structured input, delegate to the shared Rust domain/service layer, and map
//! domain errors onto structured MCP results:
//!
//! ```text
//! MCP tool -> shared domain service -> validation -> transaction -> persistence
//! ```
//!
//! The server is intentionally **not** Task-only. Each Agent module lives in
//! its own submodule and contributes a named `ToolRouter` (see
//! [`task_tools::task_tools_router`]). [`LabFlowMcp::new`] composes the module
//! routers into one server router, so future modules (Protocol, Record, Sample
//! lineage, Terminal Assay, qPCR/ELISA/CCK8 Analysis) register here without
//! restructuring the server and without duplicating business logic.

pub mod experiment_tools;
pub mod protocol_tools;
pub mod record_tools;
pub mod task_tools;

use rmcp::{
    handler::server::{router::tool::ToolRouter, ServerHandler},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool_handler,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// Structured error contract shared by every Agent module.
///
/// Each domain service maps its own error type onto a stable `code` plus a
/// human-readable message; the adapter only forwards both to the Agent.
pub(crate) trait AgentModuleError {
    fn error_code(&self) -> &'static str;
    fn error_message(&self) -> String;
}

#[derive(Clone)]
pub struct LabFlowMcp {
    connection: Arc<Mutex<Connection>>,
    files_dir: Arc<PathBuf>,
    tool_router: ToolRouter<Self>,
}

impl LabFlowMcp {
    pub fn new(connection: Connection, files_dir: PathBuf) -> Self {
        Self {
            connection: Arc::new(Mutex::new(connection)),
            files_dir: Arc::new(files_dir),
            tool_router: Self::compose_module_routers(),
        }
    }

    /// Composition point for Agent Interface modules.
    ///
    /// Task is merely the first module; adding a module means adding a submodule
    /// with its own named router and merging it here. Modules must call shared
    /// domain services — never their own copy of business rules.
    fn compose_module_routers() -> ToolRouter<Self> {
        ToolRouter::<Self>::new()
            + Self::task_tools_router()
            + Self::experiment_tools_router()
            + Self::protocol_tools_router()
            + Self::record_tools_router()
    }

    fn call<T, E>(&self, operation: impl FnOnce(&mut Connection) -> Result<T, E>) -> CallToolResult
    where
        T: Serialize,
        E: AgentModuleError,
    {
        let mut guard = match self.connection.lock() {
            Ok(guard) => guard,
            Err(_) => return Self::domain_error(ModuleLockError),
        };
        match operation(&mut guard) {
            Ok(value) => match serde_json::to_value(value) {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => Self::domain_error(ModuleFailure(error)),
            },
            Err(error) => Self::domain_error(ModuleFailure(error)),
        }
    }

    fn domain_error<E: AgentModuleError>(error: E) -> CallToolResult {
        CallToolResult::structured_error(json!({
            "error": { "code": error.error_code(), "message": error.error_message() }
        }))
    }

    #[cfg(test)]
    fn tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }
}

/// Adapter-level failure raised when the shared connection cannot be used.
struct ModuleLockError;

impl AgentModuleError for ModuleLockError {
    fn error_code(&self) -> &'static str {
        "persistence_error"
    }

    fn error_message(&self) -> String {
        "Database lock poisoned".into()
    }
}

/// Wrapper carrying a module's domain error into the adapter's error path.
struct ModuleFailure<E>(E);

impl<E: AgentModuleError> AgentModuleError for ModuleFailure<E> {
    fn error_code(&self) -> &'static str {
        self.0.error_code()
    }

    fn error_message(&self) -> String {
        self.0.error_message()
    }
}

impl AgentModuleError for serde_json::Error {
    fn error_code(&self) -> &'static str {
        "persistence_error"
    }

    fn error_message(&self) -> String {
        self.to_string()
    }
}

#[tool_handler]
impl ServerHandler for LabFlowMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "LabFlow's local Agent Interface. Use labflow_list_experiments before creating Tasks or Records. Times are local calendar values in YYYY-MM-DDTHH:mm[:ss]. LabFlow—not the Agent—validates Experiments, Tasks, Protocols, Records, parent timing, DAG cycles, deletion guards, and transactions. Protocols describe how to record results; Records freeze a Protocol snapshot at execution time and never rewrite later Protocol edits. This MCP server exposes Task, Experiment, Protocol, and Record tools; Sample lineage, Terminal Assay, and qPCR/ELISA/CCK8 Analysis tools are added incrementally without copying business rules."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_interface::task_tools::{CreateTaskRequest, GetTaskRequest};
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;

    fn server() -> LabFlowMcp {
        let connection = Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO experiments VALUES ('e','EXP100','Main','','#000')",
                [],
            )
            .unwrap();
        LabFlowMcp::new(
            connection,
            std::env::temp_dir().join("labflow-mcp-test-files"),
        )
    }

    #[test]
    fn composed_router_exposes_task_experiment_protocol_record_tools() {
        let server = server();
        let tools = server.tools();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        // Task module (Calendar)
        for expected in [
            "labflow_list_experiments",
            "labflow_list_tasks",
            "labflow_get_task",
            "labflow_create_task",
            "labflow_update_task",
            "labflow_delete_task",
        ] {
            assert!(names.contains(&expected), "missing task tool {expected}");
        }
        // Experiment module
        for expected in [
            "labflow_get_experiment",
            "labflow_save_experiment",
            "labflow_delete_experiment",
        ] {
            assert!(
                names.contains(&expected),
                "missing experiment tool {expected}"
            );
        }
        // Protocol module
        for expected in [
            "labflow_list_protocols",
            "labflow_get_protocol",
            "labflow_create_protocol",
            "labflow_save_protocol_version",
            "labflow_delete_protocol",
        ] {
            assert!(
                names.contains(&expected),
                "missing protocol tool {expected}"
            );
        }
        // Record module
        for expected in [
            "labflow_list_records",
            "labflow_get_record",
            "labflow_update_record_body",
            "labflow_delete_record",
        ] {
            assert!(names.contains(&expected), "missing record tool {expected}");
        }
        assert!(tools
            .iter()
            .all(|tool| tool.input_schema.get("type") == Some(&Value::String("object".into()))));
        let update = tools
            .iter()
            .find(|tool| tool.name == "labflow_update_task")
            .unwrap();
        let properties = update
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap();
        assert!(properties.contains_key("parent_task_ids"));
        assert!(properties.contains_key("status"));
        let create_protocol = tools
            .iter()
            .find(|tool| tool.name == "labflow_create_protocol")
            .unwrap();
        let protocol_fields = create_protocol.input_schema["$defs"]["ProtocolDraftRequest"]
            ["properties"]
            .as_object()
            .expect("typed Protocol request schema");
        for expected in [
            "id",
            "name",
            "description",
            "inputType",
            "outputBehavior",
            "consumptionPolicy",
            "template",
            "createdAt",
        ] {
            assert!(protocol_fields.contains_key(expected), "missing {expected}");
        }
        let save_experiment = tools
            .iter()
            .find(|tool| tool.name == "labflow_save_experiment")
            .unwrap();
        let experiment_fields = save_experiment.input_schema["$defs"]["ExperimentDraft"]
            ["properties"]
            .as_object()
            .expect("typed Experiment request schema");
        for expected in ["id", "code", "title", "description", "color"] {
            assert!(
                experiment_fields.contains_key(expected),
                "missing {expected}"
            );
        }
    }

    #[tokio::test]
    async fn adapter_delegates_to_shared_service_and_maps_domain_errors() {
        let server = server();
        let created = server
            .create_task(Parameters(CreateTaskRequest {
                experiment_id: "e".into(),
                title: "MCP".into(),
                starts_at: "2026-08-26T09:00".into(),
                ends_at: "2026-08-26T10:00".into(),
                parent_task_ids: vec![],
            }))
            .await
            .unwrap();
        assert_eq!(created.is_error, Some(false));
        let missing = server
            .get_task(Parameters(GetTaskRequest {
                task_id: "missing".into(),
            }))
            .await
            .unwrap();
        assert_eq!(missing.is_error, Some(true));
        assert_eq!(
            missing.structured_content.as_ref().unwrap()["error"]["code"],
            "not_found"
        );
    }

    fn empty_server() -> LabFlowMcp {
        let connection = Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        LabFlowMcp::new(
            connection,
            std::env::temp_dir().join("labflow-mcp-empty-test-files"),
        )
    }

    #[tokio::test]
    async fn bootstrap_workspace_list_save_create_round_trip() {
        // Simulates a brand-new workspace where the agent must first
        // `list_experiments`, then `save_experiment`, then `create_task`
        // with `parent_task_ids: []` (no parents yet).
        use crate::agent_interface::experiment_tools::{ExperimentDraft, SaveExperimentRequest};
        use crate::agent_interface::task_tools::CreateTaskRequest;

        let server = empty_server();

        // 1) list_experiments returns empty
        let listed = server.list_experiments().await.unwrap();
        assert_eq!(listed.is_error, Some(false));
        // Empty result is exposed in `content[0].text` as a JSON-encoded array.
        let text = listed
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        assert_eq!(text, "[]");

        // 2) save_experiment creates a brand-new experiment
        let saved = server
            .save_experiment(Parameters(SaveExperimentRequest {
                experiment: ExperimentDraft {
                    id: "fresh-exp".into(),
                    code: "EXP200".into(),
                    title: "Fresh workspace".into(),
                    description: Some("Bootstrapped via agent".into()),
                    color: Some("#6957e8".into()),
                },
                changed_at: "2026-08-27T09:00:00+08:00".into(),
            }))
            .await
            .unwrap();
        assert_eq!(saved.is_error, Some(false));

        // 3) list_experiments now sees the row
        let listed = server.list_experiments().await.unwrap();
        let text = listed
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap_or("");
        let experiments: serde_json::Value = serde_json::from_str(text).unwrap();
        let exp_ids: Vec<&str> = experiments
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.get("id").and_then(serde_json::Value::as_str).unwrap())
            .collect();
        assert_eq!(exp_ids, vec!["fresh-exp"]);

        // 4) create_task with empty parent_task_ids succeeds
        let created = server
            .create_task(Parameters(CreateTaskRequest {
                experiment_id: "fresh-exp".into(),
                title: "Bootstrap task".into(),
                starts_at: "2026-08-27T09:00".into(),
                ends_at: "2026-08-27T10:00".into(),
                parent_task_ids: vec![],
            }))
            .await
            .unwrap();
        assert_eq!(created.is_error, Some(false));
        assert_eq!(
            created
                .structured_content
                .as_ref()
                .unwrap()
                .get("experimentId")
                .and_then(serde_json::Value::as_str),
            Some("fresh-exp")
        );
    }
}
