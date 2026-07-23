use crate::environment::{
    LEGACY_PACKAGE_CACHE_FILE_PREFIX, PACKAGE_CACHE_FILE_PREFIX, PACKAGE_CACHE_FOLDER,
    REPO_CACHE_FOLDER,
};
use crate::io::{DefaultEnvironmentIo, DefaultProjectIo, IoTrait, TokioFile};
use crate::repository::LocalCachedRepository;
use crate::traits::{AbortCheck, PackageInstallProgress, PackageInstallProgressKind};
use crate::utils::Sha256AsyncWrite;
use crate::{HttpClient, PackageInfo, PackageManifest, io};
use futures::io::{AsyncRead, AsyncWrite};
use futures::prelude::*;
use hex::FromHex;
use indexmap::IndexMap;
use log::debug;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const PACKAGE_EXTRACT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct PackageInstaller<'a, T: HttpClient> {
    pub(super) io: &'a DefaultEnvironmentIo,
    pub(super) http: Option<&'a T>,
    progress: Option<Arc<dyn Fn(PackageInstallProgress) + Send + Sync + 'a>>,
}

impl<'a, T: HttpClient> PackageInstaller<'a, T> {
    pub fn new(io: &'a DefaultEnvironmentIo, http: Option<&'a T>) -> Self {
        Self {
            io,
            http,
            progress: None,
        }
    }

    pub fn with_progress(
        mut self,
        progress: impl Fn(PackageInstallProgress) + Send + Sync + 'a,
    ) -> Self {
        self.progress = Some(Arc::new(progress));
        self
    }

    fn emit_progress(&self, package_name: &str, kind: PackageInstallProgressKind) {
        if let Some(callback) = &self.progress {
            callback(PackageInstallProgress {
                package_name: package_name.into(),
                kind,
            });
        }
    }

    fn emit_failed(&self, package_name: &str, error: &io::Error) {
        self.emit_progress(
            package_name,
            PackageInstallProgressKind::Failed {
                message: error.to_string().into(),
            },
        );
    }

    async fn cleanup_failed_extract(
        &self,
        io: &DefaultProjectIo,
        package_name: &str,
        dest_dir: &Path,
        error: io::Error,
    ) -> io::Error {
        self.emit_failed(package_name, &error);
        let _ = io.remove_dir_all(dest_dir).await;
        error
    }
}

impl<T: HttpClient> crate::PackageInstaller for PackageInstaller<'_, T> {
    fn report_progress(&self, progress: PackageInstallProgress) {
        if let Some(callback) = &self.progress {
            callback(progress);
        }
    }

    async fn install_package(
        &self,
        io: &DefaultProjectIo,
        package: PackageInfo<'_>,
        dest_dir: &Path,
        abort: &AbortCheck,
    ) -> io::Result<()> {
        abort.check()?;
        use crate::PackageInfoInner;
        log::debug!(
            "extracting package {} to {}",
            package.name(),
            dest_dir.display()
        );
        match package.inner {
            PackageInfoInner::Remote(package, user_repo) => {
                self.emit_progress(package.name(), PackageInstallProgressKind::DownloadStarted);
                let zip_file =
                    match get_package(self.io, self.http, user_repo, package, abort).await {
                        Ok(zip_file) => zip_file,
                        Err(e) => {
                            self.emit_failed(package.name(), &e);
                            return Err(e);
                        }
                    };
                self.emit_progress(package.name(), PackageInstallProgressKind::DownloadFinished);

                // downloading may take a long time, so check abort again
                abort.check()?;

                let zip_file = io::BufReader::new(zip_file);

                debug!(
                    "Extracting zip file for {}@{}",
                    package.name(),
                    package.version()
                );
                // remove dest folder before extract if exists
                self.emit_progress(package.name(), PackageInstallProgressKind::ExtractStarted);
                let extract_result = tokio::time::timeout(
                    PACKAGE_EXTRACT_TIMEOUT,
                    crate::utils::extract_zip(zip_file, io, dest_dir, abort),
                )
                .await
                .unwrap_or_else(|_| {
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "Timed out extracting package {}@{}",
                            package.name(),
                            package.version()
                        ),
                    ))
                });
                if let Err(e) = extract_result {
                    // if an error occurs, try to remove the dest folder
                    log::debug!(
                        "Error occurred while extracting zip file for {}@{}: {e}",
                        package.name(),
                        package.version(),
                    );
                    return Err(self
                        .cleanup_failed_extract(io, package.name(), dest_dir, e)
                        .await);
                }
                debug!(
                    "Extracted zip file for {}@{}",
                    package.name(),
                    package.version()
                );
                self.emit_progress(package.name(), PackageInstallProgressKind::ExtractFinished);

                Ok(())
            }
            PackageInfoInner::Local(_, path) => {
                self.emit_progress(package.name(), PackageInstallProgressKind::DownloadStarted);
                self.emit_progress(package.name(), PackageInstallProgressKind::DownloadFinished);
                self.emit_progress(package.name(), PackageInstallProgressKind::ExtractStarted);
                let copy_result = tokio::time::timeout(
                    PACKAGE_EXTRACT_TIMEOUT,
                    crate::utils::copy_recursive(self.io, path.into(), io, dest_dir.into()),
                )
                .await
                .unwrap_or_else(|_| {
                    Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("Timed out copying package {}", package.name()),
                    ))
                });
                if let Err(e) = copy_result {
                    return Err(self
                        .cleanup_failed_extract(io, package.name(), dest_dir, e)
                        .await);
                }
                self.emit_progress(package.name(), PackageInstallProgressKind::ExtractFinished);
                Ok(())
            }
        }
    }
}

async fn get_package<T: HttpClient>(
    io: &DefaultEnvironmentIo,
    http: Option<&T>,
    repository: &LocalCachedRepository,
    package: &PackageManifest,
    abort: &AbortCheck,
) -> io::Result<TokioFile> {
    abort.check()?;
    let zip_file_name = package_cache_file_name(PACKAGE_CACHE_FILE_PREFIX, package);
    let legacy_zip_file_name = package_cache_file_name(LEGACY_PACKAGE_CACHE_FILE_PREFIX, package);
    let zip_path = package_cache_path(package, &zip_file_name);
    let sha_path = zip_path.with_extension("zip.sha256");
    let legacy_zip_path = package_cache_path(package, &legacy_zip_file_name);
    let legacy_sha_path = legacy_zip_path.with_extension("zip.sha256");
    let legacy_repo_zip_path = legacy_package_cache_path(package, &legacy_zip_file_name);
    let legacy_repo_sha_path = legacy_repo_zip_path.with_extension("zip.sha256");

    if let Some(cache_file) =
        try_load_package_cache(io, &zip_path, &sha_path, package.zip_sha_256(), abort).await
    {
        debug!("using cache for {}@{}", package.name(), package.version());
        Ok(cache_file)
    } else if let Some(cache_file) = try_load_package_cache(
        io,
        &legacy_zip_path,
        &legacy_sha_path,
        package.zip_sha_256(),
        abort,
    )
    .await
    {
        debug!(
            "using legacy cache for {}@{}",
            package.name(),
            package.version()
        );
        Ok(cache_file)
    } else if let Some(cache_file) = try_load_package_cache(
        io,
        &legacy_repo_zip_path,
        &legacy_repo_sha_path,
        package.zip_sha_256(),
        abort,
    )
    .await
    {
        debug!(
            "using legacy repository cache for {}@{}",
            package.name(),
            package.version()
        );
        Ok(cache_file)
    } else {
        io.create_dir_all(zip_path.parent().unwrap()).await?;

        let new_headers = IndexMap::from_iter(
            (repository
                .headers()
                .iter()
                .map(|(k, v)| (k.as_ref(), v.as_ref())))
            .chain(
                package
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.as_ref(), v.as_ref())),
            ),
        );

        let (zip_file, zip_hash) = download_package_zip(
            http,
            io,
            &new_headers,
            &zip_path,
            &sha_path,
            &zip_file_name,
            package.url().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "URL field of the package.json in the repository empty",
                )
            })?,
            abort,
        )
        .await?;

        if let Some(repo_hash) = package
            .zip_sha_256()
            .and_then(|x| <[u8; 256 / 8] as FromHex>::from_hex(x).ok())
            && repo_hash != zip_hash
        {
            drop(zip_file);
            io.remove_file(&zip_path).await.ok();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Downloaded file for {}@{} has an unexpected SHA256 hash. This may be the repository owner's fault, or the repository or package may be compromised.",
                    package.name(),
                    package.version()
                ),
            ));
        }

        Ok(zip_file)
    }
}

fn package_cache_file_name(prefix: &str, package: &PackageManifest) -> String {
    format!("{prefix}{}-{}.zip", package.name(), package.version())
}

fn package_cache_path(package: &PackageManifest, zip_file_name: &str) -> PathBuf {
    Path::new(PACKAGE_CACHE_FOLDER)
        .join(package.name())
        .join(zip_file_name)
}

fn legacy_package_cache_path(package: &PackageManifest, zip_file_name: &str) -> PathBuf {
    Path::new(REPO_CACHE_FOLDER)
        .join(package.name())
        .join(zip_file_name)
}

/// Try to load from the zip file
///
/// # Arguments
///
/// * `zip_path`: the path to zip file
/// * `sha_path`: the path to sha256 file
/// * `sha256`: sha256 hash if specified
///
/// returns: Option<File> readable zip file or None
async fn try_load_package_cache(
    io: &DefaultEnvironmentIo,
    zip_path: &Path,
    sha_path: &Path,
    sha256: Option<&str>,
    abort: &AbortCheck,
) -> Option<TokioFile> {
    abort.check().ok()?;
    let mut cache_file = io.open(zip_path).await.ok()?;

    let mut buf = [0u8; 256 / 4];
    io.open(sha_path)
        .await
        .ok()?
        .read_exact(&mut buf)
        .await
        .ok()?;

    let hex: [u8; 256 / 8] = FromHex::from_hex(buf).ok()?;

    // if stored sha doesn't match sha in repo: current cache is invalid
    if let Some(repo_hash) = sha256.and_then(|x| <[u8; 256 / 8] as FromHex>::from_hex(x).ok())
        && repo_hash != hex
    {
        return None;
    }

    let mut hasher = Sha256AsyncWrite::new(io::sink());

    copy_with_abort(&mut cache_file, &mut hasher, abort)
        .await
        .ok()?;

    let hash = &hasher.finalize().1[..];
    if hash != &hex[..] {
        return None;
    }

    cache_file.seek(SeekFrom::Start(0)).await.ok()?;

    Some(cache_file)
}

/// downloads the zip file from the url to the specified path
///
/// # Arguments
///
/// * `http`: http client. returns error if none
/// * `zip_path`: the path to zip file
/// * `sha_path`: the path to sha256 file
/// * `zip_file_name`: the name of zip file. will be used in the sha file
/// * `url`: url to zip file
///
/// returns: Result<File, Error> the readable zip file.
async fn download_package_zip(
    http: Option<&impl HttpClient>,
    io: &DefaultEnvironmentIo,
    headers: &IndexMap<&str, &str>,
    zip_path: &Path,
    sha_path: &Path,
    zip_file_name: &str,
    url: &Url,
    abort: &AbortCheck,
) -> io::Result<(TokioFile, [u8; 256 / 8])> {
    abort.check()?;
    let Some(http) = http else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Offline mode"));
    };

    // file not found: err
    let cache_file = io.create(zip_path).await?;

    debug!("Download started for {url}");
    let mut response = pin!(http.get(url, headers).await?);

    let mut writer = Sha256AsyncWrite::new(cache_file);
    copy_with_abort(&mut response, &mut writer, abort).await?;
    debug!("finished downloading {url}");

    let (mut cache_file, hash) = writer.finalize();
    let hash: [u8; 256 / 8] = hash.into();

    cache_file.flush().await?;
    cache_file.seek(SeekFrom::Start(0)).await?;

    // write sha file
    io.write(
        sha_path,
        format!("{} {zip_file_name}\n", hex::encode(&hash[..])).as_bytes(),
    )
    .await?;

    Ok((cache_file, hash))
}

async fn copy_with_abort(
    reader: &mut (impl AsyncRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    abort: &AbortCheck,
) -> io::Result<u64> {
    let mut copied = 0;
    let mut buffer = [0; 64 * 1024];

    loop {
        abort.check()?;
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(copied);
        }
        writer.write_all(&buffer[..read]).await?;
        copied += read as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::RemoteRepository;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::convert::Infallible;

    struct EmptyHttp;

    impl HttpClient for EmptyHttp {
        async fn get(
            &self,
            _url: &Url,
            _headers: &IndexMap<&str, &str>,
        ) -> io::Result<impl AsyncRead + Send> {
            Ok(io::empty())
        }

        async fn get_with_etag(
            &self,
            _url: &Url,
            _headers: &IndexMap<Box<str>, Box<str>>,
            _current_etag: Option<&str>,
        ) -> io::Result<Option<(impl AsyncRead + Send, Option<Box<str>>)>> {
            Ok(Some((io::empty(), None)))
        }
    }

    fn temp_environment() -> (DefaultEnvironmentIo, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "alcomd3-package-cache-test-{}",
            uuid::Uuid::new_v4().as_simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        (
            DefaultEnvironmentIo::new(root.clone().into_boxed_path()),
            root,
        )
    }

    async fn remove_temp_environment(root: PathBuf) {
        tokio::fs::remove_dir_all(root).await.ok();
    }

    fn test_repository() -> LocalCachedRepository {
        let repo: RemoteRepository = serde_json::from_value(json!({
            "id": "com.example.repo",
            "name": "Example Repository",
            "url": "https://example.com/index.json",
            "packages": {}
        }))
        .unwrap();
        LocalCachedRepository::new(repo, IndexMap::new())
    }

    fn remote_package() -> PackageManifest {
        serde_json::from_value(json!({
            "name": "com.example.package",
            "version": "1.0.0",
            "url": "https://example.com/com.example.package-1.0.0.zip"
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn downloads_package_cache_to_package_cache_directory() {
        let (io, root) = temp_environment();
        let repository = test_repository();
        let package = remote_package();
        let abort = AbortCheck::new();
        let http = EmptyHttp;

        get_package(&io, Some(&http), &repository, &package, &abort)
            .await
            .unwrap();

        let zip_file_name = format!("alcomd3-{}-{}.zip", package.name(), package.version());
        let cache_path = Path::new(PACKAGE_CACHE_FOLDER)
            .join(package.name())
            .join(&zip_file_name);
        assert!(io.resolve(&cache_path).is_file());
        assert!(
            io.resolve(&cache_path.with_extension("zip.sha256"))
                .is_file()
        );

        let legacy_path = Path::new(REPO_CACHE_FOLDER)
            .join(package.name())
            .join(&zip_file_name);
        assert!(!io.resolve(&legacy_path).exists());

        remove_temp_environment(root).await;
    }

    #[tokio::test]
    async fn reads_legacy_repos_package_cache_as_fallback() {
        let (io, root) = temp_environment();
        let repository = test_repository();
        let package = remote_package();
        let abort = AbortCheck::new();
        let zip_file_name = format!("vrc-get-{}-{}.zip", package.name(), package.version());
        let legacy_path = Path::new(REPO_CACHE_FOLDER)
            .join(package.name())
            .join(&zip_file_name);
        let legacy_sha_path = legacy_path.with_extension("zip.sha256");
        let legacy_content = b"legacy cache";
        let legacy_hash = Sha256::digest(legacy_content);

        io.create_dir_all(legacy_path.parent().unwrap())
            .await
            .unwrap();
        io.write(&legacy_path, legacy_content).await.unwrap();
        io.write(
            &legacy_sha_path,
            format!("{} {zip_file_name}\n", hex::encode(legacy_hash)).as_bytes(),
        )
        .await
        .unwrap();

        let mut cache_file = get_package::<Infallible>(&io, None, &repository, &package, &abort)
            .await
            .unwrap();
        let mut content = Vec::new();
        cache_file.read_to_end(&mut content).await.unwrap();

        assert_eq!(content, legacy_content);

        remove_temp_environment(root).await;
    }

    #[tokio::test]
    async fn clear_package_cache_removes_new_and_legacy_cache_files() {
        let (io, root) = temp_environment();
        let new_zip = Path::new(PACKAGE_CACHE_FOLDER)
            .join("com.example.package")
            .join("alcomd3-com.example.package-1.0.0.zip");
        let new_sha = new_zip.with_extension("zip.sha256");
        let legacy_package_zip = Path::new(PACKAGE_CACHE_FOLDER)
            .join("com.example.package")
            .join("vrc-get-com.example.package-1.0.0.zip");
        let legacy_package_sha = legacy_package_zip.with_extension("zip.sha256");
        let legacy_zip = Path::new(REPO_CACHE_FOLDER)
            .join("com.example.package")
            .join("vrc-get-com.example.package-1.0.0.zip");
        let legacy_sha = legacy_zip.with_extension("zip.sha256");
        let repo_json = Path::new(REPO_CACHE_FOLDER).join("community.json");

        io.create_dir_all(new_zip.parent().unwrap()).await.unwrap();
        io.create_dir_all(legacy_zip.parent().unwrap())
            .await
            .unwrap();
        io.write(&new_zip, b"new").await.unwrap();
        io.write(&new_sha, b"new-sha").await.unwrap();
        io.write(&legacy_package_zip, b"legacy-package")
            .await
            .unwrap();
        io.write(&legacy_package_sha, b"legacy-package-sha")
            .await
            .unwrap();
        io.write(&legacy_zip, b"legacy").await.unwrap();
        io.write(&legacy_sha, b"legacy-sha").await.unwrap();
        io.write(&repo_json, b"{}").await.unwrap();

        crate::environment::clear_package_cache(&io).await.unwrap();

        assert!(!io.resolve(&new_zip).exists());
        assert!(!io.resolve(&new_sha).exists());
        assert!(!io.resolve(&legacy_package_zip).exists());
        assert!(!io.resolve(&legacy_package_sha).exists());
        assert!(!io.resolve(&legacy_zip).exists());
        assert!(!io.resolve(&legacy_sha).exists());
        assert!(io.resolve(&repo_json).is_file());

        remove_temp_environment(root).await;
    }
}
