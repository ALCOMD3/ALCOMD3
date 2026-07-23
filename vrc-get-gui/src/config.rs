use crate::logging::LogLevel;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiConfig {
    #[serde(default)]
    pub gui_hidden_repositories: IndexSet<String>,
    #[serde(default)]
    pub hide_local_user_packages: bool,
    #[serde(default)]
    pub window_size: WindowSize,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default = "language_default")]
    pub language: String,
    #[serde(default = "backup_default")]
    pub backup_format: String,
    #[serde(default = "project_sorting_default")]
    pub project_sorting: String,
    #[serde(default = "release_channel_default")]
    // "stable" or "beta"
    pub release_channel: String,
    #[serde(default = "automatic_update_default")]
    pub automatic_update: bool,
    #[serde(default = "use_alcom_for_vcc_protocol_default")]
    pub use_alcom_for_vcc_protocol: bool,
    #[serde(default)]
    pub setup_process_progress: u32,
    #[serde(default)]
    pub default_unity_arguments: Option<Vec<String>>,
    #[serde(default = "log_level_default")]
    pub logs_level: Vec<LogLevel>,
    #[serde(default = "gui_animation_default")]
    pub gui_animation: bool,
    #[serde(default = "gui_compact_default")]
    pub gui_compact: bool,
    #[serde(default)]
    pub mcp_enabled: bool,
    #[serde(default = "project_view_mode_default")]
    pub project_view_mode: String,
    #[serde(default)]
    pub unity_hub_access_method: UnityHubAccessMethod,
    // last element is the most recent one
    // 8 paths are saved
    #[serde(default)]
    pub recent_project_locations: Vec<String>,
    /// the list of favorite templates by id
    /// those templates will be shown at the top of template selection on project creation
    /// or derived templates
    #[serde(default)]
    pub favorite_templates: Vec<String>,
    /// The lastly used template, this will be the initially selected template
    #[serde(default)]
    pub last_used_template: Option<String>,
    #[serde(default)]
    pub update_reminder: Option<UpdateReminderConfig>,
    #[serde(default)]
    pub sidebar_extensions: Vec<SidebarExtension>,
}

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SidebarExtension {
    pub id: String,
    #[serde(default = "default_true")]
    pub installed: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReminderConfig {
    pub latest_version: String,
    pub remind_after: f64,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Default, specta::Type)]
pub enum UnityHubAccessMethod {
    /// Reads config files of Unity Hub
    #[default]
    ReadConfig,
    /// Launches headless Unity Hub in background
    CallHub,
}

impl Default for GuiConfig {
    fn default() -> Self {
        GuiConfig {
            gui_hidden_repositories: IndexSet::new(),
            hide_local_user_packages: false,
            window_size: WindowSize::default(),
            fullscreen: false,
            language: language_default(),
            backup_format: backup_default(),
            project_sorting: project_sorting_default(),
            release_channel: release_channel_default(),
            automatic_update: automatic_update_default(),
            use_alcom_for_vcc_protocol: use_alcom_for_vcc_protocol_default(),
            setup_process_progress: 0,
            default_unity_arguments: None,
            logs_level: log_level_default(),
            gui_animation: true,
            gui_compact: gui_compact_default(),
            mcp_enabled: false,
            project_view_mode: project_view_mode_default(),
            unity_hub_access_method: UnityHubAccessMethod::ReadConfig,
            recent_project_locations: Vec::new(),
            favorite_templates: vec![],
            last_used_template: None,
            update_reminder: None,
            sidebar_extensions: default_sidebar_extensions(),
        }
    }
}

impl GuiConfig {
    pub(crate) fn fix_defaults(&mut self) {
        if self.language.is_empty() {
            self.language = language_default();
        }
        if self.language == "zh_cn" {
            self.language = "zh_hans".to_string();
        }
        if self.backup_format.is_empty() {
            self.backup_format = backup_default();
        }
        if self.project_sorting.is_empty() {
            self.project_sorting = project_sorting_default();
        }
        if self.sidebar_extensions.is_empty() {
            self.sidebar_extensions = default_sidebar_extensions();
            return;
        }
        self.sidebar_extensions = normalize_sidebar_extensions(self.sidebar_extensions.clone());
    }
}

fn language_default() -> String {
    for locale in sys_locale::get_locales() {
        if locale.starts_with("en") {
            return "en".to_string();
        }
        if locale.starts_with("de") {
            return "de".to_string();
        }
        if locale.starts_with("ja") {
            return "ja".to_string();
        }
        if locale.starts_with("zh") {
            return "zh_hans".to_string();
        }
    }

    "en".to_string()
}

fn theme_default() -> String {
    "system".to_string()
}

fn backup_default() -> String {
    "default".to_string()
}

fn project_sorting_default() -> String {
    "lastModified".to_string()
}

fn release_channel_default() -> String {
    "stable".to_string()
}

fn automatic_update_default() -> bool {
    true
}

fn use_alcom_for_vcc_protocol_default() -> bool {
    true
}

fn log_level_default() -> Vec<LogLevel> {
    vec![
        LogLevel::Debug,
        LogLevel::Error,
        LogLevel::Warn,
        LogLevel::Info,
    ]
}

fn gui_animation_default() -> bool {
    true
}

fn gui_compact_default() -> bool {
    false
}

fn project_view_mode_default() -> String {
    "List".to_string()
}

fn default_true() -> bool {
    true
}

const LOCKED_SIDEBAR_ITEM_IDS: &[&str] = &["extensions"];

fn is_configurable_sidebar_extension(id: &str) -> bool {
    !LOCKED_SIDEBAR_ITEM_IDS.contains(&id)
}

fn default_sidebar_extensions() -> Vec<SidebarExtension> {
    vec![
        SidebarExtension {
            id: "projects".to_string(),
            installed: true,
            visible: true,
        },
        SidebarExtension {
            id: "packages".to_string(),
            installed: true,
            visible: true,
        },
        SidebarExtension {
            id: "settings".to_string(),
            installed: true,
            visible: true,
        },
        SidebarExtension {
            id: "mcp".to_string(),
            installed: true,
            visible: true,
        },
        SidebarExtension {
            id: "log".to_string(),
            installed: true,
            visible: true,
        },
    ]
}

pub(crate) fn normalize_sidebar_extensions(
    existing: Vec<SidebarExtension>,
) -> Vec<SidebarExtension> {
    let mut seen = HashSet::<String>::new();
    let mut updated = Vec::new();
    for extension in existing {
        if extension.id.is_empty() || !is_configurable_sidebar_extension(&extension.id) {
            continue;
        }
        if seen.insert(extension.id.clone()) {
            updated.push(extension);
        }
    }

    for default_extension in default_sidebar_extensions() {
        if seen.insert(default_extension.id.clone()) {
            updated.push(default_extension);
        }
    }

    updated
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    #[serde(default = "theme_default")]
    pub theme: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            theme: theme_default(),
        }
    }
}

impl ThemeConfig {
    pub(crate) fn fix_defaults(&mut self) {
        if self.theme.is_empty() {
            self.theme = theme_default();
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowSize {
    fn default() -> Self {
        WindowSize {
            width: 1400,
            height: 800,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GuiConfig;

    #[test]
    fn automatic_updates_default_to_enabled_for_existing_configs() {
        let config: GuiConfig = serde_json::from_str("{}").unwrap();
        assert!(config.automatic_update);
    }

    #[test]
    fn automatic_updates_can_be_disabled() {
        let config: GuiConfig = serde_json::from_str(r#"{"automaticUpdate":false}"#).unwrap();
        assert!(!config.automatic_update);
    }
}
