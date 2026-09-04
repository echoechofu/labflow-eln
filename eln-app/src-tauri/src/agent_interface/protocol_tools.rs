//! Protocol module for the LabFlow Agent Interface.
//!
//! Reads existing Protocol templates and persists new user-defined templates
//! plus new template versions. Always call [`super::record_tools::delete_record`]
//! style operations via the shared [`crate::protocol_service`] module so the
//! Desktop UI and the Agent Interface stay in lock-step.
//!
//! **Important**: Protocol edits do **not** rewrite existing Records. Records
//! freeze a Protocol snapshot at execution time (see the agent interaction
//! contract skill). Mutating a Protocol only changes the template's active
//! version; already-frozen Record snapshots remain immutable.

use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::protocol_service::{self, ProtocolServiceError};

use super::AgentModuleError;

impl AgentModuleError for ProtocolServiceError {
    fn error_code(&self) -> &'static str {
        ProtocolServiceError::code(self)
    }

    fn error_message(&self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetProtocolRequest {
    pub protocol_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteProtocolRequest {
    pub protocol_id: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolOutputBehavior {
    SameSample,
    DerivedOne,
    DerivedMultiple,
    MeasurementOnly,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolConsumptionPolicy {
    Retain,
    Consume,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolMultipleSampleMode {
    Identical,
    ConditionGroups,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolDraftRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub accent: Option<String>,
    pub input_type: String,
    pub input_type_display_name: Option<String>,
    pub output_behavior: ProtocolOutputBehavior,
    pub multiple_sample_mode: Option<ProtocolMultipleSampleMode>,
    pub plate_mapping: Option<bool>,
    pub output_type: Option<String>,
    pub output_type_display_name: Option<String>,
    pub consumption_policy: ProtocolConsumptionPolicy,
    pub template: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveProtocolRequest {
    /// Full user-defined Protocol body. Same shape as the Desktop UI's
    /// `save_user_protocol` Tauri command. Built-in Protocols cannot be
    /// redefined through MCP.
    pub request: ProtocolDraftRequest,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolVersionDraftRequest {
    pub protocol_id: String,
    pub template: Option<String>,
    pub template_variants: Option<BTreeMap<String, String>>,
    pub created_at: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaveProtocolVersionRequest {
    /// Version update body. Provide either `template` (legacy Protocols) or
    /// `templateVariants` (split-template Protocols).
    pub request: ProtocolVersionDraftRequest,
}

fn view_to_value(view: protocol_service::ProtocolView) -> serde_json::Value {
    serde_json::json!({
        "id": view.id,
        "name": view.name,
        "category": view.category,
        "version": view.version,
        "accent": view.accent,
        "description": view.description,
        "origin": view.origin,
        "activeVersionOrigin": view.active_version_origin,
        "blocks": view.spec.get("blocks").cloned().unwrap_or(serde_json::json!([])),
        "fields": view.spec.get("fields").cloned().unwrap_or(serde_json::json!([])),
        "template": view.spec.get("template").cloned().unwrap_or(serde_json::Value::Null),
        "templateSelector": view.spec.get("templateSelector").cloned().unwrap_or(serde_json::Value::Null),
        "templateVariants": view.spec.get("templateVariants").cloned().unwrap_or(serde_json::Value::Null),
        "execution": view.spec.get("execution").cloned().unwrap_or(serde_json::Value::Null),
        "terminalAssay": view.spec.get("terminalAssay").cloned().unwrap_or(serde_json::Value::Null),
    })
}

#[tool_router(router = protocol_tools_router, vis = "pub(crate)")]
impl super::LabFlowMcp {
    /// List every Protocol row joined with the active version's schema.
    #[tool(
        name = "labflow_list_protocols",
        description = "List LabFlow Protocol templates (built-in and user-defined) with their active version's schema summary.",
        annotations(title = "List LabFlow Protocols", read_only_hint = true)
    )]
    pub(crate) async fn list_protocols(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.call(|connection| {
            protocol_service::list_protocols(connection)
                .map(|views| views.into_iter().map(view_to_value).collect::<Vec<_>>())
        }))
    }

    /// Read a single Protocol row.
    #[tool(
        name = "labflow_get_protocol",
        description = "Read a LabFlow Protocol template by ID. Returns null when the Protocol is absent.",
        annotations(title = "Get LabFlow Protocol", read_only_hint = true)
    )]
    pub(crate) async fn get_protocol(
        &self,
        Parameters(request): Parameters<GetProtocolRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let protocol_id = request.protocol_id;
        Ok(self.call(|connection| {
            protocol_service::get_protocol(connection, &protocol_id)
                .map(|option| option.map(view_to_value).unwrap_or(serde_json::Value::Null))
        }))
    }

    /// Persist a user-defined Protocol at version 1.
    #[tool(
        name = "labflow_create_protocol",
        description = "Create a LabFlow Protocol template. Validates the template body and registers new input/output Sample types. Refuses when the ID is taken.",
        annotations(title = "Create LabFlow Protocol", destructive_hint = false)
    )]
    pub(crate) async fn create_protocol(
        &self,
        Parameters(request): Parameters<SaveProtocolRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = request.request;
        Ok(self.call(|connection| {
            let body = serde_json::to_value(body)
                .map_err(|error| ProtocolServiceError::Persistence(error.to_string()))?;
            protocol_service::save_user_protocol(connection, body)
        }))
    }

    /// Add a new schema version to an existing Protocol and promote it to
    /// the active version.
    #[tool(
        name = "labflow_save_protocol_version",
        description = "Append a new version to a LabFlow Protocol's history and make it the active version. Use template or templateVariants depending on the Protocol.",
        annotations(title = "Save LabFlow Protocol Version", destructive_hint = false)
    )]
    pub(crate) async fn save_protocol_version(
        &self,
        Parameters(request): Parameters<SaveProtocolVersionRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let body = request.request;
        Ok(self.call(|connection| {
            let body = serde_json::to_value(body)
                .map_err(|error| ProtocolServiceError::Persistence(error.to_string()))?;
            protocol_service::save_protocol_template_version(connection, body)
        }))
    }

    /// Permanently delete a user-defined Protocol and its version history.
    /// Historical Records remain self-contained through their frozen snapshot.
    #[tool(
        name = "labflow_delete_protocol",
        description = "Delete a user-defined LabFlow Protocol and all of its template versions. Built-in Protocols are protected. Existing Records remain unchanged because they use frozen Protocol snapshots; incomplete legacy snapshots block deletion.",
        annotations(title = "Delete LabFlow Protocol", destructive_hint = true)
    )]
    pub(crate) async fn delete_protocol(
        &self,
        Parameters(request): Parameters<DeleteProtocolRequest>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let protocol_id = request.protocol_id;
        Ok(self.call(|connection| protocol_service::delete_protocol(connection, &protocol_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_request_deserializes_typed_inner_fields() {
        let parsed: SaveProtocolRequest = serde_json::from_value(serde_json::json!({
            "request": {
                "id": "p1", "name": "Protocol", "description": "Description",
                "inputType": "RNA", "outputBehavior": "derived_one",
                "outputType": "CDNA", "consumptionPolicy": "consume",
                "template": "{{date}}", "createdAt": "2026-08-27T09:00:00"
            }
        }))
        .unwrap();
        assert_eq!(parsed.request.input_type, "RNA");
        assert!(matches!(
            parsed.request.output_behavior,
            ProtocolOutputBehavior::DerivedOne
        ));
    }

    #[tokio::test]
    async fn mcp_delete_protocol_uses_shared_service_rules() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        crate::apply_schema(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO protocols (id,name,category,active_version,accent,description,origin)
               VALUES ('custom','Custom','Test',1,'#000','','user');
             INSERT INTO protocol_versions VALUES ('custom',1,'{}','user','now');",
            )
            .unwrap();
        let server = super::super::LabFlowMcp::new(
            connection,
            std::env::temp_dir().join("labflow-mcp-protocol-delete"),
        );

        let result = server
            .delete_protocol(Parameters(DeleteProtocolRequest {
                protocol_id: "custom".into(),
            }))
            .await
            .unwrap();

        assert_eq!(result.is_error, Some(false));
        let missing = server
            .get_protocol(Parameters(GetProtocolRequest {
                protocol_id: "custom".into(),
            }))
            .await
            .unwrap();
        assert_eq!(missing.is_error, Some(false));
        assert_eq!(missing.structured_content, Some(serde_json::Value::Null));
    }
}
