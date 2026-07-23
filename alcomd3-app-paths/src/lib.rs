use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const ALCOMD3_DATA_DIR_NAME: &str = "ALCOMD3";
pub const LEGACY_VCC_DATA_DIR_NAME: &str = "VRChatCreatorCompanion";
pub const LEGACY_ALCOM_DATA_DIR_NAME: &str = "ALCOM";
pub const MCP_DATA_DIR_NAME: &str = "mcp";
pub const MCP_ENDPOINT_FILE_NAME: &str = "endpoint.json";
pub const MCP_ENDPOINT_FILE_ENV: &str = "ALCOMD3_MCP_ENDPOINT_FILE";
#[cfg(debug_assertions)]
pub const TEST_LOCAL_DATA_ROOT_ENV: &str = "ALCOMD3_TEST_LOCAL_DATA_ROOT";

pub fn local_data_root() -> PathBuf {
    #[cfg(debug_assertions)]
    if let Some(path) = test_local_data_root_from_env(std::env::var_os(TEST_LOCAL_DATA_ROOT_ENV)) {
        return path;
    }

    #[cfg(windows)]
    {
        return windows_local_data_root();
    }

    #[cfg(not(windows))]
    {
        local_data_root_from_parts(
            None,
            std::env::var_os("XDG_DATA_HOME"),
            std::env::var_os("HOME"),
        )
    }
}

#[cfg(debug_assertions)]
fn test_local_data_root_from_env(override_path: Option<OsString>) -> Option<PathBuf> {
    override_path
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

pub fn local_data_root_from_parts(
    windows_local_app_data: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    #[cfg(windows)]
    {
        let _ = (xdg_data_home, home);
        return windows_local_app_data
            .map(PathBuf::from)
            .unwrap_or_else(windows_local_data_root);
    }

    #[cfg(not(windows))]
    {
        let _ = windows_local_app_data;

        if let Some(data_home) = xdg_data_home {
            return data_home.into();
        }

        if let Some(home_folder) = home {
            return PathBuf::from(home_folder).join(".local").join("share");
        }

        std::env::temp_dir()
    }
}

#[cfg(windows)]
fn windows_local_data_root() -> PathBuf {
    dirs_sys::known_folder_local_app_data().expect("LocalAppData not found")
}

pub fn named_data_dir(name: &str) -> PathBuf {
    local_data_root().join(name)
}

pub fn alcomd3_data_dir() -> PathBuf {
    alcomd3_data_dir_from_local_data_root(&local_data_root())
}

pub fn alcomd3_data_dir_from_local_data_root(local_data_root: &Path) -> PathBuf {
    local_data_root.join(ALCOMD3_DATA_DIR_NAME)
}

pub fn legacy_vcc_data_dir() -> PathBuf {
    named_data_dir(LEGACY_VCC_DATA_DIR_NAME)
}

pub fn legacy_alcom_data_dir() -> PathBuf {
    named_data_dir(LEGACY_ALCOM_DATA_DIR_NAME)
}

pub fn mcp_endpoint_file_path() -> PathBuf {
    mcp_endpoint_file_path_from_env(std::env::var_os(MCP_ENDPOINT_FILE_ENV))
}

pub fn mcp_endpoint_file_path_from_env(override_path: Option<OsString>) -> PathBuf {
    override_path
        .map(PathBuf::from)
        .unwrap_or_else(|| mcp_endpoint_file_path_from_local_data_root(&local_data_root()))
}

pub fn mcp_endpoint_file_path_from_local_data_root(local_data_root: &Path) -> PathBuf {
    alcomd3_data_dir_from_local_data_root(local_data_root)
        .join(MCP_DATA_DIR_NAME)
        .join(MCP_ENDPOINT_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alcomd3_data_dir_lives_under_local_data_root() {
        assert_eq!(
            alcomd3_data_dir_from_local_data_root(Path::new("/data")),
            Path::new("/data").join("ALCOMD3")
        );
    }

    #[test]
    fn mcp_endpoint_file_lives_under_alcomd3_data_dir() {
        assert_eq!(
            mcp_endpoint_file_path_from_local_data_root(Path::new("/data")),
            Path::new("/data")
                .join("ALCOMD3")
                .join("mcp")
                .join("endpoint.json")
        );
    }

    #[test]
    fn mcp_endpoint_file_path_uses_explicit_override() {
        let path =
            mcp_endpoint_file_path_from_env(Some(OsString::from("C:/tmp/alcom-endpoint.json")));
        assert_eq!(path, PathBuf::from("C:/tmp/alcom-endpoint.json"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_build_accepts_non_empty_test_data_root() {
        assert_eq!(
            test_local_data_root_from_env(Some(OsString::from("C:/tmp/alcomd3-e2e"))),
            Some(PathBuf::from("C:/tmp/alcomd3-e2e"))
        );
        assert_eq!(test_local_data_root_from_env(Some(OsString::new())), None);
        assert_eq!(test_local_data_root_from_env(None), None);
    }

    #[cfg(windows)]
    #[test]
    fn local_data_root_from_parts_uses_windows_known_folder_value() {
        assert_eq!(
            local_data_root_from_parts(
                Some(OsString::from("C:/Users/Me/AppData/Local")),
                None,
                None
            ),
            PathBuf::from("C:/Users/Me/AppData/Local")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn local_data_root_from_parts_prefers_xdg_data_home() {
        assert_eq!(
            local_data_root_from_parts(
                Some(OsString::from("ignored")),
                Some(OsString::from("/xdg-data")),
                Some(OsString::from("/home/me")),
            ),
            PathBuf::from("/xdg-data")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn local_data_root_from_parts_falls_back_to_home_local_share() {
        assert_eq!(
            local_data_root_from_parts(None, None, Some(OsString::from("/home/me"))),
            Path::new("/home/me").join(".local").join("share")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn local_data_root_from_parts_falls_back_to_temp_dir_without_home() {
        assert_eq!(
            local_data_root_from_parts(None, None, None),
            std::env::temp_dir()
        );
    }
}
