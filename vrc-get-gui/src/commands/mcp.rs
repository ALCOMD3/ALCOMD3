use crate::activity_log::{
    ActivityImportance, ActivityInput, ActivityKind, ActivityLogState, ActivitySource, operations,
};
use crate::commands::prelude::*;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
#[specta::specta]
pub async fn mcp_status(
    config: State<'_, GuiConfigState>,
    mcp: State<'_, crate::mcp::McpState>,
) -> Result<crate::mcp::McpStatus, RustError> {
    Ok(mcp.status(config.get().mcp_enabled).await)
}

#[tauri::command]
#[specta::specta]
pub async fn mcp_set_enabled(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    mcp: State<'_, crate::mcp::McpState>,
    enabled: bool,
) -> Result<crate::mcp::McpStatus, RustError> {
    let app_for_activity = app.clone();
    let activity = app_for_activity.state::<ActivityLogState>();
    activity
        .track_result(
            Some(&app_for_activity),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::MCP_SET_ENABLED,
                if enabled {
                    "Enabling MCP"
                } else {
                    "Disabling MCP"
                },
            ),
            if enabled {
                "MCP enabled"
            } else {
                "MCP disabled"
            },
            Vec::new(),
            async move {
                {
                    let mut config = config.load_mut().await?;
                    config.mcp_enabled = enabled;
                    config.save().await?;
                }

                mcp.set_enabled(app.clone(), enabled).await?;
                Ok(mcp.status(enabled).await)
            },
        )
        .await
}
