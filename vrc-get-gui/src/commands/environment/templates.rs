use crate::activity_log::{
    ActivityDetail, ActivityImportance, ActivityInput, ActivityKind, ActivityLogState,
    ActivitySource, operations, summarize_path,
};
use crate::commands::prelude::*;
use crate::templates;
use crate::templates::{
    AlcomTemplate, new_user_template_id, parse_alcom_template, sanitize_template_file_stem,
    serialize_alcom_template,
};
use crate::utils::{find_existing_parent_dir_or_home, trash_delete};
use futures::AsyncWriteExt;
use indexmap::IndexMap;
use itertools::Itertools;
use log::{error, info};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tauri::{AppHandle, Manager, State, Window};
use tauri_plugin_dialog::DialogExt;
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};
use vrc_get_vpm::version::VersionRange;

#[tauri::command]
#[specta::specta]
pub async fn environment_export_template(
    templates: State<'_, TemplatesState>,
    io: State<'_, DefaultEnvironmentIo>,
    window: Window,
    id: String,
) -> Result<(), RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let templates = templates.get();
    let Some(template) = templates
        .as_ref()
        .and_then(|x| x.iter().find(|x| x.id == id))
        .take_if(|x| x.source_path.is_some())
    else {
        return Err(RustError::unrecoverable_str(
            "Template with such id not found (this is bug)",
        ));
    };
    let Some(path) = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(format!("{}.alcomtemplate", &template.display_name))
        .add_filter("ALCOMD3 Project Template", &["alcomtemplate"])
        .blocking_save_file()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        activity.record_info(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Open,
                ActivityImportance::Secondary,
                operations::TEMPLATE_EXPORT,
                "Template export file picker cancelled",
            )
            .target(template.display_name.clone())
            .details(vec![ActivityDetail::new("template", template.id.clone())]),
        );
        return Ok(());
    };

    info!(
        "exporting template {id} to {path}",
        id = template.id,
        path = path.display()
    );

    let source_path = io.resolve(template.source_path.as_ref().unwrap());
    let destination_path = path;
    activity
        .track_result(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::TEMPLATE_EXPORT,
                "Exporting template",
            )
            .target(template.display_name.clone())
            .details(vec![
                ActivityDetail::new("template", template.id.clone()),
                ActivityDetail::new("destinationPath", summarize_path(&destination_path)),
            ]),
            "Template exported",
            vec![
                ActivityDetail::new("template", template.id.clone()),
                ActivityDetail::new("destinationPath", summarize_path(&destination_path)),
            ],
            async {
                tokio::fs::copy(source_path, destination_path).await?;
                Ok(())
            },
        )
        .await
}

#[derive(Deserialize, Serialize, specta::Type)]
pub struct TauriAlcomTemplate {
    pub display_name: String,
    pub base: String,
    pub unity_version: Option<String>,
    pub vpm_dependencies: IndexMap<String, String>,
    pub unity_packages: Vec<String>,
}

impl From<&AlcomTemplate> for TauriAlcomTemplate {
    fn from(value: &AlcomTemplate) -> Self {
        Self {
            display_name: value.display_name.clone(),
            base: value.base.clone().unwrap_or_default(),
            unity_version: (value.unity_version.as_ref()).map(|x| x.to_string()),
            vpm_dependencies: (value.vpm_dependencies.iter())
                .map(|(pkg, range)| (pkg.clone(), range.to_string()))
                .collect(),
            unity_packages: (value.unity_packages.iter())
                .map(|x| x.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn environment_get_alcom_template(
    templates: State<'_, TemplatesState>,
    id: String,
) -> Result<TauriAlcomTemplate, RustError> {
    match templates
        .get()
        .as_ref()
        .and_then(|x| x.iter().find(|x| x.id == id))
        .and_then(|x| x.alcom_template.as_ref())
        .filter(|x| x.is_derived())
    {
        None => Err(RustError::unrecoverable_str(
            "Template with such id not found (this is bug)",
        )),
        Some(template) => Ok(template.into()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn environment_pick_unity_packages(window: Window) -> Result<Vec<String>, RustError> {
    window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("Unity Package", &["unitypackage"])
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|x| x.into_path_buf())
        .map_ok(|x| x.to_string_lossy().into_owned())
        .collect::<Result<Vec<_>, _>>()
}

#[derive(Serialize, specta::Type)]
#[serde(tag = "type")]
pub enum TauriPickUnityPackageResult {
    NoFolderSelected,
    InvalidSelection,
    Successful { new_path: String },
}

#[tauri::command]
#[specta::specta]
pub async fn environment_pick_unity_package(
    window: Window,
    current: String,
) -> Result<TauriPickUnityPackageResult, RustError> {
    let Some(path) = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_directory(find_existing_parent_dir_or_home(current.as_ref()))
        .add_filter("Unity Package", &["unitypackage"])
        .blocking_pick_file()
        .map(|x| x.into_path_buf())
        .transpose()?
    else {
        return Ok(TauriPickUnityPackageResult::NoFolderSelected);
    };

    let Ok(path) = path.into_os_string().into_string() else {
        return Ok(TauriPickUnityPackageResult::InvalidSelection);
    };

    Ok(TauriPickUnityPackageResult::Successful { new_path: path })
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn environment_save_template(
    templates: State<'_, TemplatesState>,
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
    id: Option<String>,
    base: String,
    name: String,
    unity_range: String,
    vpm_packages: Vec<(String, String)>,
    unity_packages: Vec<String>,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let template_id = id.clone().unwrap_or_else(new_user_template_id);
    let vpm_package_count = vpm_packages.len();
    let unity_package_count = unity_packages.len();
    let template = AlcomTemplate {
        display_name: name.clone(),
        update_date: Some(chrono::Utc::now()),
        id: Some(template_id.clone()),
        base: Some(base),
        unity_version: Some(VersionRange::from_str(&unity_range).map_err(|x| {
            RustError::unrecoverable_str(format!("Bad Unity Version Range ({unity_range}): {x}"))
        })?),
        vpm_dependencies: vpm_packages
            .into_iter()
            .map(|(pkg, range)| {
                Ok::<_, RustError>((
                    pkg,
                    VersionRange::from_str(&range).map_err(|x| {
                        RustError::unrecoverable_str(format!("Bad Version Range ({range}): {x}"))
                    })?,
                ))
            })
            .collect::<Result<_, _>>()?,
        unity_packages: unity_packages.into_iter().map(PathBuf::from).collect(),
        archive: None,
    };

    let template = serialize_alcom_template(template)
        .map_err(|x| RustError::unrecoverable_str(format!("Failed to serialize template: {x}")))?;

    let editing_existing = id.is_some();
    activity
        .track_result(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::TEMPLATE_SAVE,
                if editing_existing {
                    "Updating template"
                } else {
                    "Saving template"
                },
            )
            .target(name.clone())
            .details(vec![
                ActivityDetail::new("template", template_id.clone()),
                ActivityDetail::new("vpmDependencies", vpm_package_count.to_string()),
                ActivityDetail::new("unityPackages", unity_package_count.to_string()),
            ]),
            if editing_existing {
                "Template updated"
            } else {
                "Template saved"
            },
            vec![
                ActivityDetail::new("template", template_id.clone()),
                ActivityDetail::new("vpmDependencies", vpm_package_count.to_string()),
                ActivityDetail::new("unityPackages", unity_package_count.to_string()),
            ],
            async {
                if let Some(id) = id {
                    // There is id; overwrite existing one
                    let templates = templates.get();
                    let Some(source_path) = templates
                        .as_ref()
                        .and_then(|x| x.iter().find(|x| x.id == id))
                        .and_then(|x| x.source_path.as_ref())
                    else {
                        return Err(RustError::unrecoverable_str(
                            "Template with such id not found (this is bug)",
                        ));
                    };
                    info!(
                        "updating template {name} ({id}) at {source_path}",
                        id = id,
                        source_path = source_path.display()
                    );
                    io.write_sync(source_path, &template).await?;
                } else {
                    // No id; create new one
                    info!("saving new template {name}");
                    save_template_file(&io, &name, &template).await?;
                }

                Ok(())
            },
        )
        .await
}

async fn save_template_file(
    io: &DefaultEnvironmentIo,
    name: &str,
    template: &[u8],
) -> io::Result<PathBuf> {
    let file_name = sanitize_template_file_stem(name);

    let (mut file, path) = 'create_file: {
        let template_dir = Path::new(crate::storage::TEMPLATE_DIR);
        io.create_dir_all(template_dir).await?;
        let extension = "alcomtemplate";
        // first, try original name
        let path = template_dir.join(&file_name).with_extension(extension);
        if let Ok(file) = io.create_new(&path).await {
            break 'create_file (file, path);
        }
        // Then, try _numbers up to 10
        for i in 1..=10 {
            let path = template_dir
                .join(format!("{file_name}_{i}"))
                .with_extension(extension);
            if let Ok(file) = io.create_new(&path).await {
                break 'create_file (file, path);
            }
        }
        // Finally, try random instead of file name
        let path = template_dir
            .join(uuid::Uuid::new_v4().simple().to_string())
            .with_extension(extension);
        let file = io.create_new(&path).await?;
        (file, path)
    };
    info!(
        "saving template {name} ({id}) to {path}",
        name = name,
        id = name,
        path = path.display()
    );
    file.write_all(template).await?;
    file.flush().await?;
    Ok(path)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn environment_remove_template(
    templates: State<'_, TemplatesState>,
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
    id: String,
) -> Result<(), RustError> {
    let activity = app.state::<ActivityLogState>();
    let templates = templates.get();
    let Some(template) = templates
        .as_ref()
        .and_then(|x| x.iter().find(|x| x.id == id))
        .take_if(|x| x.alcom_template.is_some())
        .take_if(|x| x.source_path.is_some())
    else {
        return Err(RustError::unrecoverable_str(
            "Template with such id not found (this is bug)",
        ));
    };

    let template_name = template.display_name.clone();
    let template_path = io.resolve(template.source_path.as_ref().unwrap());
    activity
        .track_result(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Write,
                ActivityImportance::Primary,
                operations::TEMPLATE_REMOVE,
                "Removing template",
            )
            .target(template_name)
            .details(vec![
                ActivityDetail::new("template", id.clone()),
                ActivityDetail::new("templatePath", summarize_path(&template_path)),
            ]),
            "Template removed",
            vec![
                ActivityDetail::new("template", id.clone()),
                ActivityDetail::new("templatePath", summarize_path(&template_path)),
            ],
            async {
                info!("deleting template {id}");
                if let Err(err) = trash_delete(template_path.clone()).await {
                    error!("failed to remove template: {err}");
                    return Err(RustError::unrecoverable_str(format!(
                        "failed to remove template: {err}"
                    )));
                } else {
                    info!(
                        "removed template directory: {path}",
                        path = template_path.display()
                    );
                }
                Ok(())
            },
        )
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_template(
    window: Window,
    io: State<'_, DefaultEnvironmentIo>,
) -> Result<TauriImportTemplateResult, RustError> {
    let app = window.app_handle().clone();
    let activity = app.state::<ActivityLogState>();
    let templates = window
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("ALCOMD3 Project Template", &["alcomtemplate"])
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|x| x.into_path_buf())
        .collect::<Result<Vec<_>, _>>()?;

    if templates.is_empty() {
        activity.record_info(
            Some(&app),
            ActivityInput::new(
                ActivitySource::Gui,
                ActivityKind::Open,
                ActivityImportance::Secondary,
                operations::TEMPLATE_IMPORT,
                "Template import file picker cancelled",
            ),
        );
        return Ok(TauriImportTemplateResult {
            imported: 0,
            failed: 0,
            duplicates: Vec::new(),
        });
    }

    let tracker = activity.start_activity(
        Some(&app),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::TEMPLATE_IMPORT,
            "Importing templates",
        )
        .details(vec![ActivityDetail::new(
            "selectedFiles",
            templates.len().to_string(),
        )]),
    );
    let result = import_templates(&io, &templates).await;
    let details = vec![
        ActivityDetail::new("selectedFiles", templates.len().to_string()),
        ActivityDetail::new("imported", result.imported.to_string()),
        ActivityDetail::new("failed", result.failed.to_string()),
        ActivityDetail::new("duplicates", result.duplicates.len().to_string()),
    ];
    if result.imported == 0 && result.duplicates.is_empty() && result.failed > 0 {
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Template import failed",
            details,
            format!("failed to import {} selected templates", result.failed),
        );
    } else if result.failed > 0 {
        activity.finish_info(
            Some(&app),
            &tracker,
            "Template import partially completed",
            details,
        );
    } else {
        activity.finish_success(Some(&app), &tracker, "Template import completed", details);
    }

    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub async fn environment_import_template_override(
    io: State<'_, DefaultEnvironmentIo>,
    app: AppHandle,
    import_override: Vec<TauriImportDuplicated>,
) -> Result<usize, RustError> {
    let activity = app.state::<ActivityLogState>();
    let override_count = import_override.len();
    let tracker = activity.start_activity(
        Some(&app),
        ActivityInput::new(
            ActivitySource::Gui,
            ActivityKind::Write,
            ActivityImportance::Primary,
            operations::TEMPLATE_IMPORT,
            "Overriding imported templates",
        )
        .details(vec![ActivityDetail::new(
            "duplicates",
            override_count.to_string(),
        )]),
    );

    let mut imported = 0;
    for duplicate in import_override {
        info!(
            "overriding template {id} at {existing_path}",
            id = duplicate.id,
            existing_path = duplicate.existing_path.display(),
        );
        match io
            .write_sync(&duplicate.existing_path, &duplicate.data)
            .await
        {
            Ok(()) => {
                imported += 1;
            }
            Err(e) => {
                log::error!(
                    "Failed to save imported template: {}: {e}",
                    duplicate
                        .existing_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                );
                continue;
            }
        };
    }

    let failed = override_count.saturating_sub(imported);
    let details = vec![
        ActivityDetail::new("duplicates", override_count.to_string()),
        ActivityDetail::new("imported", imported.to_string()),
        ActivityDetail::new("failed", failed.to_string()),
    ];
    if imported == override_count {
        activity.finish_success(
            Some(&app),
            &tracker,
            "Imported templates overridden",
            details,
        );
    } else if imported == 0 && override_count > 0 {
        activity.finish_failed(
            Some(&app),
            &tracker,
            "Imported template override failed",
            details,
            format!("failed to override {failed} imported templates"),
        );
    } else {
        activity.finish_info(
            Some(&app),
            &tracker,
            "Imported template override partially completed",
            details,
        );
    }

    Ok(imported)
}

#[derive(Serialize, Deserialize, Clone, specta::Type)]
pub struct TauriImportTemplateResult {
    imported: usize,
    failed: usize,
    duplicates: Vec<TauriImportDuplicated>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, specta::Type)]
pub struct TauriImportDuplicated {
    id: String,
    existing_path: PathBuf,
    existing_name: String,
    existing_update_date: Option<chrono::DateTime<chrono::offset::Utc>>,
    importing_name: String,
    importing_update_date: Option<chrono::DateTime<chrono::offset::Utc>>,
    #[serde_as(as = "serde_with::base64::Base64")]
    #[specta(type = &str)]
    data: Vec<u8>,
}

pub async fn import_templates(
    io: &DefaultEnvironmentIo,
    templates: &[PathBuf],
) -> TauriImportTemplateResult {
    let mut imported = 0;
    let mut failed = 0;

    let mut installed_ids = templates::load_alcom_templates(io)
        .await
        .into_iter()
        .filter_map(|x| x.1.id.clone().map(|id| (id, x)))
        .collect::<HashMap<_, _>>();

    let mut duplicates = Vec::new();

    for template in templates {
        let json = match tokio::fs::read(&template).await {
            Ok(json) => json,
            Err(e) => {
                failed += 1;
                log::error!(
                    "failed to load file: {}: {e}",
                    template.file_name().unwrap().to_string_lossy()
                );
                continue;
            }
        };
        let parsed = match parse_alcom_template(&json) {
            Ok(parsed) => parsed,
            Err(e) => {
                failed += 1;
                log::error!(
                    "Invalid template: {}: {e}",
                    template.file_name().unwrap().to_string_lossy()
                );
                continue;
            }
        };

        info!(
            "importing template {template}",
            template = template.display()
        );

        let parsed_id = parsed.id.clone();
        if let Some(id) = &parsed_id
            && let Some((existing_path, existing)) = installed_ids.get(id)
        {
            info!(
                "template {id} is duplicated with {existing_path}",
                existing_path = existing_path.display()
            );
            duplicates.push(TauriImportDuplicated {
                id: id.clone(),
                existing_path: existing_path.clone(),
                existing_name: existing.display_name.clone(),
                existing_update_date: existing.update_date,
                importing_name: parsed.display_name,
                importing_update_date: parsed.update_date,
                data: json,
            });
            continue;
        }

        match save_template_file(io, &parsed.display_name, &json).await {
            Ok(path) => {
                if let Some(id) = parsed_id {
                    installed_ids.insert(id, (path, parsed));
                }
            }
            Err(e) => {
                failed += 1;
                log::error!(
                    "Failed to save imported template: {}: {e}",
                    template.file_name().unwrap().to_string_lossy()
                );
                continue;
            }
        };
        imported += 1;
    }

    TauriImportTemplateResult {
        imported,
        failed,
        duplicates,
    }
}
