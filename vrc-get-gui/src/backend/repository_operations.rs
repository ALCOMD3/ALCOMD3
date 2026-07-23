use crate::commands::{AddedRepositoryInfo, RustError};
use crate::state::{PackagesState, SettingsState};
use indexmap::IndexMap;
use url::Url;
use vrc_get_vpm::io::DefaultEnvironmentIo;

pub(crate) async fn add_repository(
    settings: &SettingsState,
    packages: &PackagesState,
    io: &DefaultEnvironmentIo,
    http: &reqwest::Client,
    url: Url,
    headers: IndexMap<Box<str>, Box<str>>,
) -> Result<AddedRepositoryInfo, RustError> {
    crate::commands::add_repository_by_url(settings, packages, io, http, url, headers).await
}
