use crate::commands::{RustError, load_project};
use std::path::Path;
use vrc_get_vpm::environment::UserProject;
use vrc_get_vpm::version::UnityVersion;
use vrc_get_vpm::{PackageManifest, ProjectType};

#[derive(Debug, Clone)]
pub(crate) struct ProjectSummarySnapshot {
    pub(crate) name: Option<String>,
    pub(crate) path: String,
    pub(crate) project_type: ProjectType,
    pub(crate) unity: Option<String>,
    pub(crate) unity_revision: Option<String>,
    pub(crate) last_modified: Option<i64>,
    pub(crate) created_at: Option<i64>,
    pub(crate) favorite: bool,
    pub(crate) exists: bool,
    pub(crate) is_valid: Option<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectDetailsSnapshot {
    pub(crate) unity: (u16, u8),
    pub(crate) unity_version: UnityVersion,
    pub(crate) unity_str: String,
    pub(crate) unity_revision: Option<String>,
    pub(crate) installed_packages: Vec<ProjectInstalledPackageSnapshot>,
    pub(crate) should_resolve: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectInstalledPackageSnapshot {
    pub(crate) id: String,
    pub(crate) package: PackageManifest,
}

pub(crate) fn project_summary_snapshot(project: &UserProject) -> Option<ProjectSummarySnapshot> {
    let path = project.path()?.to_string();
    Some(ProjectSummarySnapshot {
        name: project.name().map(ToOwned::to_owned),
        path: path.clone(),
        project_type: project.project_type(),
        unity: project.unity_version().map(|version| version.to_string()),
        unity_revision: project.unity_revision().map(ToOwned::to_owned),
        last_modified: project
            .last_modified()
            .map(|value| value.as_unix_milliseconds()),
        created_at: project
            .crated_at()
            .map(|value| value.as_unix_milliseconds()),
        favorite: project.favorite(),
        exists: Path::new(&path).is_dir(),
        is_valid: project.is_valid_project(),
    })
}

pub(crate) async fn load_project_details_snapshot(
    project_path: String,
) -> Result<ProjectDetailsSnapshot, RustError> {
    let unity_project = load_project(project_path).await?;
    Ok(ProjectDetailsSnapshot {
        unity: (
            unity_project.unity_version().major(),
            unity_project.unity_version().minor(),
        ),
        unity_version: unity_project.unity_version(),
        unity_str: unity_project.unity_version().to_string(),
        unity_revision: unity_project.unity_revision().map(ToOwned::to_owned),
        installed_packages: unity_project
            .installed_packages()
            .map(|(id, package)| ProjectInstalledPackageSnapshot {
                id: id.to_string(),
                package: package.clone(),
            })
            .collect(),
        should_resolve: unity_project.should_resolve(),
    })
}
