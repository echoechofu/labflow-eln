//! Record module for the LabFlow Agent Interface.
//!
//! A Record freezes a Protocol snapshot at execution time and never depends
//! on later Protocol edits. The Agent Interface therefore only exposes read
//! access plus body edits and deletion — not Protocol-editing flows.
//!
//! All write operations delegate to [`crate::record_service`], which is
//! shared with the Desktop UI's Tauri commands. Record creation is not
//! exposed yet because it requires running the in-process Protocol engine
//! (`protocol_execution::execute_with_external`); MCP clients that need to
//! start a Record should obtain the inputs + protocol snapshot from a
//! human-driven Desktop session for now.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;

use crate::record_service::{self, RecordServiceError};

use super::AgentModuleError;

impl AgentModuleError for RecordServiceError {
    fn error_code(&self) -> &'static str {
        RecordServiceError::code(self)
    }

    fn error_message(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRecordsRequest {
    pub experiment_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetRecordRequest {
    pub record_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateRecordBodyRequest {
    pub record_id: String,
    /// New rendered body for the Record. LabFlow rejects empty payloads.
    pub rendered_content: String,
    /// Client-generated change UUID used for deduplication.
    pub change_id: String,
    /// Local datetime the change should be stamped with (RFC 3339).
    pub changed_at: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteRecordRequest {
    pub record_id: String,
}

#[tool_router(router = record_tools_router, vis = "pub(crate)")]
impl super::LabFlowMcp {
    /// List Record summaries, optionally filtered by Experiment.
    #[tool(
        name = "labflow_list_records",
        description = "List LabFlow Records (newest first). Optionally filter by experiment_id.",
        annotations(title = "List LabFlow Records", read_only_hint = true)
    )]
    pub(crate) async fn list_records(
        &self,
        Parameters(request): Parameters<ListRecordsRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let experiment_id = request.experiment_id;
        Ok(self.call(move |connection| {
            record_service::list_records(connection, experiment_id.as_deref())
        }))
    }

    /// Read a single Record summary (with inputs, outputs, results, attachments,
    /// history).
    #[tool(
        name = "labflow_get_record",
        description = "Read a LabFlow Record by ID. Returns null when the Record is absent.",
        annotations(title = "Get LabFlow Record", read_only_hint = true)
    )]
    pub(crate) async fn get_record(
        &self,
        Parameters(request): Parameters<GetRecordRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let record_id = request.record_id;
        Ok(self.call(|connection| {
            record_service::get_record(connection, &record_id).map(|option| {
                option
                    .map(|summary| serde_json::to_value(summary).unwrap_or(serde_json::Value::Null))
                    .unwrap_or(serde_json::Value::Null)
            })
        }))
    }

    /// Update a Record's rendered body, audited via `record_changes`.
    #[tool(
        name = "labflow_update_record_body",
        description = "Replace the rendered body of a LabFlow Record. Empty bodies are rejected; a change audit row is appended automatically.",
        annotations(title = "Update LabFlow Record body", destructive_hint = false)
    )]
    pub(crate) async fn update_record_body(
        &self,
        Parameters(request): Parameters<UpdateRecordBodyRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let record_id = request.record_id;
        let rendered_content = request.rendered_content;
        let change_id = request.change_id;
        let changed_at = request.changed_at;
        Ok(self.call(move |connection| {
            record_service::update_record_body(
                connection,
                &record_id,
                &rendered_content,
                &change_id,
                &changed_at,
            )
        }))
    }

    /// Delete a Record and all its dependents (assay, qPCR, lineage,
    /// attachments, audit rows).
    #[tool(
        name = "labflow_delete_record",
        description = "Delete a LabFlow Record. Refuses when the Record is included in any export manifest or when output Samples are reused by other Records or downstream events.",
        annotations(title = "Delete LabFlow Record", destructive_hint = true)
    )]
    pub(crate) async fn delete_record(
        &self,
        Parameters(request): Parameters<DeleteRecordRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let record_id = request.record_id;
        let files_dir = self.files_dir.clone();
        Ok(self.call(move |connection| {
            record_service::delete_record(connection, files_dir.as_path(), &record_id)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;

    #[test]
    fn update_request_round_trips() {
        let parsed: UpdateRecordBodyRequest = serde_json::from_value(serde_json::json!({
            "record_id": "r",
            "rendered_content": "hello",
            "change_id": "c",
            "changed_at": "2026-08-26T09:00:00Z"
        }))
        .unwrap();
        assert_eq!(parsed.record_id, "r");
        assert_eq!(parsed.rendered_content, "hello");
    }

    #[tokio::test]
    async fn mcp_delete_record_removes_the_canonical_attachment_directory() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        connection.execute_batch(
            "INSERT INTO experiments VALUES ('e','EXP','Main','','#000');
             INSERT INTO protocols (id,name,category,active_version,accent,description,origin)
               VALUES ('p','Protocol','Test',1,'#000','','user');
             INSERT INTO protocol_versions (protocol_id,version_number,schema_json,origin,created_at)
               VALUES ('p',1,'{}','user','now');
             INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,record_id,created_at,updated_at)
               VALUES ('t','e','Task','2026-08-27T09:00:00','2026-08-27T10:00:00','in_progress',NULL,'now','now');
             INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at)
               VALUES ('r','t','e','p','{}','{}','now');
             UPDATE tasks SET record_id='r' WHERE id='t';
             INSERT INTO attachments (id,record_id,file_name,relative_path,created_at)
               VALUES ('attachment-1','r','raw.csv','files/attachment-1/raw.csv','now');",
        ).unwrap();
        let files_dir = std::env::temp_dir().join(format!(
            "labflow-mcp-record-delete-{}",
            uuid::Uuid::new_v4()
        ));
        let attachment_dir = files_dir.join("attachment-1");
        std::fs::create_dir_all(&attachment_dir).unwrap();
        std::fs::write(attachment_dir.join("raw.csv"), b"well,value\nA01,1").unwrap();
        let server = super::super::LabFlowMcp::new(connection, files_dir.clone());
        let result = server
            .delete_record(Parameters(DeleteRecordRequest {
                record_id: "r".into(),
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(false));
        assert!(!attachment_dir.exists());
        std::fs::remove_dir_all(files_dir).unwrap();
    }
}
