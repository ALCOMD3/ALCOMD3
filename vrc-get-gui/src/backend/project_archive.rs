use crate::commands::{RustError, create_dir_all_with_err};
use crate::compressor::{TauriCreateBackupProgress, parallel_compress_zip_with_progress};
use crate::utils::collect_notable_project_files_tree;
use async_zip::{Compression, DeflateOption};
use log::{info, warn};
use std::path::{Component, Path, PathBuf};

const BACKUP_ARCHIVE_EXTENSION: &str = "zip";
const MAX_BACKUP_NAME_BYTES: usize = 251;

#[cfg(windows)]
const WINDOWS_RESERVED_FILE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub(crate) fn default_project_backup_name(project_path: &str) -> Result<String, RustError> {
    let project_name = project_name_from_path(project_path)?;
    Ok(format!(
        "{project_name}-{timestamp}",
        timestamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S"),
    ))
}

pub(crate) fn normalize_project_backup_name(backup_name: &str) -> Result<String, RustError> {
    let backup_name = backup_name.trim();
    let mut components = Path::new(backup_name).components();
    let valid_single_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    let invalid = backup_name.is_empty()
        || backup_name.len() > MAX_BACKUP_NAME_BYTES
        || backup_name.to_ascii_lowercase().ends_with(".zip")
        || !valid_single_component
        || backup_name.chars().any(char::is_control);

    #[cfg(windows)]
    let invalid = {
        let reserved_base = backup_name
            .trim_end_matches([' ', '.'])
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        invalid
            || backup_name.ends_with([' ', '.'])
            || backup_name.contains(['<', '>', ':', '"', '|', '?', '*', '\\', '/'])
            || WINDOWS_RESERVED_FILE_NAMES.contains(&reserved_base.as_str())
    };

    if invalid {
        return Err(RustError::unrecoverable_str(
            "backup_name is not a valid backup file name",
        ));
    }

    Ok(backup_name.to_string())
}

pub(crate) fn project_backup_archive_path(backup_dir: &str, backup_name: &str) -> PathBuf {
    Path::new(backup_dir)
        .join(backup_name)
        .with_added_extension(BACKUP_ARCHIVE_EXTENSION)
}

fn project_name_from_path(project_path: &str) -> Result<&str, RustError> {
    Path::new(project_path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| RustError::unrecoverable_str("project path has no valid folder name"))
}

pub(crate) async fn create_project_backup_archive<F>(
    project_path: String,
    backup_dir: String,
    backup_name: Option<String>,
    backup_format: String,
    exclude_vpm: bool,
    progress: F,
) -> Result<PathBuf, RustError>
where
    F: Fn(TauriCreateBackupProgress) + Clone + Send + Sync + 'static,
{
    let project_name = project_name_from_path(&project_path)?;
    let backup_name = match backup_name {
        Some(backup_name) => normalize_project_backup_name(&backup_name)?,
        None => default_project_backup_name(&project_path)?,
    };

    create_dir_all_with_err(&backup_dir).await?;

    info!("backup project: {project_name} with {backup_format}");
    let timer = std::time::Instant::now();

    let backup_path = project_backup_archive_path(&backup_dir, &backup_name);

    match backup_format.as_str() {
        "zip-store" => {
            create_backup_zip_with_progress(
                &backup_path,
                Path::new(&project_path),
                Compression::Stored,
                DeflateOption::Fast,
                exclude_vpm,
                progress,
            )
            .await?;
        }
        "default" | "zip-fast" => {
            create_backup_zip_with_progress(
                &backup_path,
                Path::new(&project_path),
                Compression::Deflate,
                DeflateOption::Fast,
                exclude_vpm,
                progress,
            )
            .await?;
        }
        "zip-best" => {
            create_backup_zip_with_progress(
                &backup_path,
                Path::new(&project_path),
                Compression::Deflate,
                DeflateOption::Maximum,
                exclude_vpm,
                progress,
            )
            .await?;
        }
        _ => {
            warn!("unknown backup format: {backup_format}, using zip-fast");

            create_backup_zip_with_progress(
                &backup_path,
                Path::new(&project_path),
                Compression::Deflate,
                DeflateOption::Fast,
                exclude_vpm,
                progress,
            )
            .await?;
        }
    };

    info!("backup finished in {:?}", timer.elapsed());
    Ok(backup_path)
}

async fn create_backup_zip_with_progress<F>(
    backup_path: &Path,
    project_path: &Path,
    compression: Compression,
    deflate_option: DeflateOption,
    exclude_vpm: bool,
    progress: F,
) -> Result<(), RustError>
where
    F: Fn(TauriCreateBackupProgress) + Clone + Send + Sync + 'static,
{
    info!("Collecting files to backup {}...", project_path.display());

    let start = std::time::Instant::now();
    let file_tree =
        collect_notable_project_files_tree(PathBuf::from(project_path), exclude_vpm, true).await?;

    let total_files = file_tree.count_all();

    info!(
        "Collecting files took {}, starting creating archive with {total_files} files...",
        start.elapsed().as_secs_f64()
    );

    let backup_file = tokio::fs::File::create_new(backup_path).await?;
    let remove_on_drop = RemoveOnDrop::new(backup_path);

    parallel_compress_zip_with_progress(
        file_tree,
        backup_file,
        compression,
        deflate_option,
        progress,
    )
    .await?;

    remove_on_drop.forget();

    info!(
        "Creating backup archive for {} finished!",
        project_path.display()
    );

    Ok(())
}

struct RemoveOnDrop<'a>(&'a Path);

impl<'a> RemoveOnDrop<'a> {
    fn new(path: &'a Path) -> Self {
        RemoveOnDrop(path)
    }

    fn forget(self) {
        std::mem::forget(self);
    }
}

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        create_project_backup_archive, normalize_project_backup_name, project_backup_archive_path,
    };
    use std::path::PathBuf;

    #[test]
    fn backup_name_is_trimmed_and_restricted_to_one_file_name() {
        assert_eq!(
            normalize_project_backup_name("  Custom Backup  ").unwrap(),
            "Custom Backup"
        );
        assert!(normalize_project_backup_name("").is_err());
        assert!(normalize_project_backup_name("..").is_err());
        assert!(normalize_project_backup_name("nested/backup").is_err());
        assert!(normalize_project_backup_name("line\nbreak").is_err());
        assert!(normalize_project_backup_name("backup.ZIP").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn backup_name_rejects_windows_reserved_file_names() {
        assert!(normalize_project_backup_name("CON").is_err());
        assert!(normalize_project_backup_name("con.archive").is_err());
        assert!(normalize_project_backup_name("backup.").is_err());
        assert!(normalize_project_backup_name("backup?").is_err());
    }

    #[test]
    fn backup_archive_path_adds_zip_extension() {
        assert_eq!(
            project_backup_archive_path("C:/Backups", "Custom Backup"),
            PathBuf::from("C:/Backups/Custom Backup.zip")
        );
    }

    #[test]
    fn existing_backup_is_preserved_when_name_conflicts() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(existing_backup_is_preserved_when_name_conflicts_inner());
    }

    async fn existing_backup_is_preserved_when_name_conflicts_inner() {
        let temp = tempfile::tempdir().unwrap();
        let project_path = temp.path().join("Project");
        let backup_directory = temp.path().join("Backups");
        tokio::fs::create_dir_all(&project_path).await.unwrap();
        tokio::fs::create_dir_all(&backup_directory).await.unwrap();
        tokio::fs::write(project_path.join("project.txt"), b"project")
            .await
            .unwrap();

        let backup_path = backup_directory.join("Existing.zip");
        tokio::fs::write(&backup_path, b"existing backup")
            .await
            .unwrap();

        let result = create_project_backup_archive(
            project_path.to_string_lossy().into_owned(),
            backup_directory.to_string_lossy().into_owned(),
            Some("Existing".to_string()),
            "zip-fast".to_string(),
            false,
            |_| {},
        )
        .await;

        assert!(result.is_err());
        assert_eq!(
            tokio::fs::read(backup_path).await.unwrap(),
            b"existing backup"
        );
    }
}
