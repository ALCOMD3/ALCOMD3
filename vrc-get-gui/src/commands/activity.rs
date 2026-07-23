use crate::activity_log::{ActivityEntry, ActivityEntryFilter, ActivityLogState};
use crate::backend::logs;
use crate::commands::prelude::*;
use crate::os::open_that;
use tauri::{AppHandle, Manager};

#[tauri::command]
#[specta::specta]
pub async fn activity_get_entries(
    app: AppHandle,
    filter: ActivityEntryFilter,
) -> Result<Vec<ActivityEntry>, RustError> {
    let activity = app.state::<ActivityLogState>();
    Ok(logs::get_activity_entries(&activity, filter))
}

#[tauri::command]
#[specta::specta]
pub async fn activity_open_log_folder(app: AppHandle) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    open_that(activity.log_folder())?;
    Ok(())
}
