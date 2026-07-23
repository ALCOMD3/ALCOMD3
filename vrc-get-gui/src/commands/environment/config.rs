use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::prelude::*;
use crate::config::{
    SidebarExtension, UnityHubAccessMethod, UpdateReminderConfig, normalize_sidebar_extensions,
};
use crate::logging::LogLevel;

#[tauri::command]
#[specta::specta]
pub async fn environment_language(config: State<'_, GuiConfigState>) -> Result<String, RustError> {
    Ok(config.get().language.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_language(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    language: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("language", language.clone());
    activity
        .track_result(
            Some(&app),
            input,
            "Language setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.language = language;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_theme(config: State<'_, ThemeConfigState>) -> Result<String, RustError> {
    Ok(config.get().theme.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_theme(
    app: AppHandle,
    config: State<'_, ThemeConfigState>,
    theme: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("theme", theme.clone());
    activity
        .track_result(
            Some(&app),
            input,
            "Theme setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.theme = theme;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_get_project_sorting(
    config: State<'_, GuiConfigState>,
) -> Result<String, RustError> {
    Ok(config.get().project_sorting.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_project_sorting(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    sorting: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("projectSorting", sorting.clone());
    activity
        .track_result(
            Some(&app),
            input,
            "Project sorting setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.project_sorting = sorting;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[derive(Serialize, Deserialize, specta::Type, Copy, Clone)]
pub enum SetupPages {
    Appearance,
    LegacyImport,
    UnityHub,
    ProjectPath,
    Backups,
    SystemSetting,
}

impl SetupPages {
    pub fn as_flag(&self) -> u32 {
        match self {
            SetupPages::Appearance => 0x00000001,
            SetupPages::UnityHub => 0x00000002,
            SetupPages::ProjectPath => 0x00000004,
            SetupPages::Backups => 0x00000008,
            SetupPages::SystemSetting => 0x00000010,
            SetupPages::LegacyImport => 0x00000020,
        }
    }

    pub fn is_finished(&self, flags: u32) -> bool {
        flags & self.as_flag() == self.as_flag()
    }

    pub fn pages(app: &AppHandle) -> &'static [SetupPages] {
        // currently, SystemSetting page only has deep link support
        if !crate::deep_link_support::should_install_deep_link(app) {
            &[
                SetupPages::Appearance,
                SetupPages::LegacyImport,
                SetupPages::UnityHub,
                SetupPages::ProjectPath,
                SetupPages::Backups,
            ]
        } else {
            &[
                SetupPages::Appearance,
                SetupPages::LegacyImport,
                SetupPages::UnityHub,
                SetupPages::ProjectPath,
                SetupPages::Backups,
                SetupPages::SystemSetting,
            ]
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            SetupPages::Appearance => "/setup/appearance/",
            SetupPages::LegacyImport => "/setup/legacy-import/",
            SetupPages::UnityHub => "/setup/unity-hub/",
            SetupPages::ProjectPath => "/setup/project-path/",
            SetupPages::Backups => "/setup/backups/",
            SetupPages::SystemSetting => "/setup/system-setting/",
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn environment_get_finished_setup_pages(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
) -> Result<Vec<SetupPages>, RustError> {
    let setup_process_progress = config.get().setup_process_progress;

    Ok(SetupPages::pages(&app)
        .iter()
        .copied()
        .filter(|page| page.is_finished(setup_process_progress))
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_finished_setup_page(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    page: SetupPages,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("setupPageFinished", page.path());
    activity
        .track_result(
            Some(&app),
            input,
            "Setup page marked as finished",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.setup_process_progress |= page.as_flag();
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_clear_setup_process(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("setupProcess", "cleared");
    activity
        .track_result(
            Some(&app),
            input,
            "Setup progress cleared",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.setup_process_progress = 0;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_logs_level(
    config: State<'_, GuiConfigState>,
) -> Result<Vec<LogLevel>, RustError> {
    Ok(config.get().logs_level.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_logs_level(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    logs_level: Vec<LogLevel>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("logsLevel", format!("{logs_level:?}"));
    activity
        .track_result(
            Some(&app),
            input,
            "Log level filter updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.logs_level = logs_level;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_gui_animation(
    config: State<'_, GuiConfigState>,
) -> Result<bool, RustError> {
    Ok(config.get().gui_animation)
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_gui_animation(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    gui_animation: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("guiAnimation", gui_animation.to_string());
    activity
        .track_result(
            Some(&app),
            input,
            "Animation setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.gui_animation = gui_animation;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_gui_compact(config: State<'_, GuiConfigState>) -> Result<bool, RustError> {
    Ok(config.get().gui_compact)
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_gui_compact(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    gui_compact: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("guiCompact", gui_compact.to_string());
    activity
        .track_result(
            Some(&app),
            input,
            "Compact mode setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.gui_compact = gui_compact;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_project_view_mode(
    config: State<'_, GuiConfigState>,
) -> Result<String, RustError> {
    Ok(config.get().project_view_mode.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_project_view_mode(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    project_view_mode: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity("projectViewMode", project_view_mode.clone());
    activity
        .track_result(
            Some(&app),
            input,
            "Project view setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.project_view_mode = project_view_mode;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_unity_hub_access_method(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    unity_hub_access_method: UnityHubAccessMethod,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = setting_activity(
        "unityHubAccessMethod",
        format!("{unity_hub_access_method:?}"),
    );
    activity
        .track_result(
            Some(&app),
            input,
            "Unity Hub access method updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.unity_hub_access_method = unity_hub_access_method;
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_template_favorite(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    template_id: String,
    favorite: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input =
        setting_activity("templateFavorite", favorite.to_string()).target(template_id.clone());
    activity
        .track_result(
            Some(&app),
            input,
            "Template favorite setting updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                if favorite {
                    if !config.favorite_templates.contains(&template_id) {
                        config.favorite_templates.push(template_id);
                    }
                } else {
                    config
                        .favorite_templates
                        .retain(|name| name != &template_id);
                }
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_get_sidebar_extensions(
    config: State<'_, GuiConfigState>,
) -> Result<Vec<SidebarExtension>, RustError> {
    Ok(config.get().sidebar_extensions.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_sidebar_extension_order(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    sidebar_extensions: Vec<SidebarExtension>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let order = sidebar_extensions
        .iter()
        .map(|extension| extension.id.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::SIDEBAR_EXTENSION_REORDER,
        "Reordering sidebar extensions",
    )
    .details(vec![ActivityDetail::new("order", order)]);

    activity
        .track_result(
            Some(&app),
            input,
            "Sidebar extension order updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                config.sidebar_extensions = normalize_sidebar_extensions(sidebar_extensions);
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_sidebar_extension_installed(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    id: String,
    installed: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::SIDEBAR_EXTENSION_INSTALLED,
        "Changing sidebar extension install status",
    )
    .target(id.clone())
    .details(vec![ActivityDetail::new(
        "installed",
        installed.to_string(),
    )]);

    activity
        .track_result(
            Some(&app),
            input,
            "Sidebar extension install status updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                let sidebar_extensions = &mut config.sidebar_extensions;
                if let Some(extension) = sidebar_extensions.iter_mut().find(|x| x.id == id) {
                    extension.installed = installed;
                    if !installed {
                        extension.visible = false;
                    }
                } else {
                    sidebar_extensions.push(SidebarExtension {
                        id,
                        installed,
                        visible: false,
                    });
                }
                config.sidebar_extensions =
                    normalize_sidebar_extensions(std::mem::take(sidebar_extensions));
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_sidebar_extension_visible(
    app: AppHandle,
    config: State<'_, GuiConfigState>,
    id: String,
    visible: bool,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let input = ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::SIDEBAR_EXTENSION_VISIBLE,
        "Changing sidebar extension visibility",
    )
    .target(id.clone())
    .details(vec![ActivityDetail::new("visible", visible.to_string())]);

    activity
        .track_result(
            Some(&app),
            input,
            "Sidebar extension visibility updated",
            Vec::new(),
            async move {
                let mut config = config.load_mut().await?;
                let sidebar_extensions = &mut config.sidebar_extensions;
                if let Some(extension) = sidebar_extensions.iter_mut().find(|x| x.id == id) {
                    extension.visible = visible;
                } else {
                    sidebar_extensions.push(SidebarExtension {
                        id,
                        installed: false,
                        visible,
                    });
                }
                config.sidebar_extensions =
                    normalize_sidebar_extensions(std::mem::take(sidebar_extensions));
                config.save().await?;
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_update_reminder(
    config: State<'_, GuiConfigState>,
) -> Result<Option<UpdateReminderConfig>, RustError> {
    Ok(config.get().update_reminder.clone())
}

#[tauri::command]
#[specta::specta]
pub async fn environment_set_update_reminder(
    config: State<'_, GuiConfigState>,
    update_reminder: Option<UpdateReminderConfig>,
) -> Result<(), RustError> {
    let mut config = config.load_mut().await?;
    config.update_reminder = update_reminder;
    config.save().await?;
    Ok(())
}

fn setting_activity(setting: &str, value: impl Into<String>) -> ActivityInput {
    ActivityInput::new(
        ActivitySource::Gui,
        ActivityKind::Write,
        ActivityImportance::Primary,
        operations::SETTINGS_SET,
        format!("Updating setting {setting}"),
    )
    .target(setting)
    .details(vec![ActivityDetail::new("value", value)])
}
