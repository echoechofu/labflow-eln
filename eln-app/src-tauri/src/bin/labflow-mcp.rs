use labflow_lib::{agent_interface::LabFlowMcp, canonical_app_data_dir, initialize_database_at};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let platform_data =
        dirs::data_dir().ok_or("Cannot resolve the macOS application data directory")?;
    let root = canonical_app_data_dir(platform_data);
    let connection = initialize_database_at(&root)?;
    let server = LabFlowMcp::new(connection, root.join("files"))
        .serve(stdio())
        .await
        .map_err(|error| {
            eprintln!("LabFlow MCP failed to start: {error}");
            error
        })?;
    server.waiting().await?;
    Ok(())
}
