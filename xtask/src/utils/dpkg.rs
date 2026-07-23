use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

pub fn dpkg_apt_available() -> bool {
    fn has_command(cmd: &str) -> bool {
        Command::new(cmd)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    has_command("apt-cache") && has_command("dpkg") && has_command("dpkg-query")
}

pub fn dpkg_architecture() -> io::Result<&'static str> {
    static STORAGE: OnceLock<String> = OnceLock::new();
    if let Some(got) = STORAGE.get() {
        return Ok(got);
    }

    let output = Command::new("dpkg")
        .arg("--print-architecture")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "dpkg --print-architecture exited with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut data = String::from_utf8(output.stdout)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    data.truncate(data.trim_end().len());
    STORAGE.set(data).ok();
    Ok(STORAGE.get().unwrap())
}

#[derive(Debug)]
pub struct PackageInfo {
    pub package_name: String,
    pub architecture: Option<String>,
}

pub fn dpkg_query_search(
    files: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> io::Result<HashMap<String, Vec<PackageInfo>>> {
    let mut child = Command::new("dpkg-query")
        .arg("--search")
        .arg("--")
        .args(files)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .spawn()?;

    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line_buf = String::new();
    let mut result = HashMap::new();

    while reader.read_line(&mut line_buf)? != 0 {
        let line = line_buf.trim_end_matches(['\r', '\n']);
        let Some((packages, path)) = line.split_once(": ") else {
            return Err(io::Error::other("dpkg-query output does not include ': '"));
        };

        result.insert(
            path.to_string(),
            packages
                .split(", ")
                .map(|package| {
                    if let Some((pkg, arch)) = package.rsplit_once(':') {
                        PackageInfo {
                            package_name: pkg.to_owned(),
                            architecture: Some(arch.to_owned()),
                        }
                    } else {
                        PackageInfo {
                            package_name: package.to_owned(),
                            architecture: None,
                        }
                    }
                })
                .collect::<Vec<_>>(),
        );

        line_buf.clear();
    }

    let output = child.wait_with_output()?;

    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(io::Error::other(format!(
            "dpkg-query --search returned non-zero status code: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(result)
}

pub fn dpkg_query_list_files(
    packages: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> io::Result<Vec<String>> {
    let mut child = Command::new("dpkg-query")
        .arg("--listfiles")
        .arg("--")
        .args(packages)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .spawn()?;

    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line_buf = String::new();
    let mut result = Vec::new();
    let mut last_path_index = None;

    while reader.read_line(&mut line_buf)? != 0 {
        let line = line_buf.trim_end_matches(['\r', '\n']);
        collect_dpkg_query_list_files_line(&mut result, &mut last_path_index, line)?;
        line_buf.clear();
    }

    let output = child.wait_with_output()?;

    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(io::Error::other(format!(
            "dpkg-query --listfiles returned non-zero status code: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(result)
}

fn collect_dpkg_query_list_files_line(
    result: &mut Vec<String>,
    last_path_index: &mut Option<usize>,
    line: &str,
) -> io::Result<()> {
    if line.is_empty() {
        *last_path_index = None;
        return Ok(());
    }

    if line.starts_with('/') {
        result.push(line.to_owned());
        *last_path_index = Some(result.len() - 1);
        return Ok(());
    }

    let current_package_file = line.strip_prefix("locally diverted to: ").or_else(|| {
        line.strip_prefix("diverted by ")
            .and_then(|diversion| diversion.split_once(" to: ").map(|(_, path)| path))
    });

    if let Some(path) = current_package_file {
        if !path.starts_with('/') {
            return Err(unexpected_dpkg_query_list_files_output(line));
        }

        let Some(index) = last_path_index.take() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("dpkg-query diversion has no preceding file path: {line:?}"),
            ));
        };
        result[index] = path.to_owned();
        return Ok(());
    }

    if let Some(path) = line.strip_prefix("package diverts others to: ") {
        if !path.starts_with('/') {
            return Err(unexpected_dpkg_query_list_files_output(line));
        }
        if last_path_index.take().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("dpkg-query diversion has no preceding file path: {line:?}"),
            ));
        }
        return Ok(());
    }

    Err(unexpected_dpkg_query_list_files_output(line))
}

fn unexpected_dpkg_query_list_files_output(line: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unexpected dpkg-query --listfiles output: {line:?}"),
    )
}

#[derive(Debug)]
pub struct PackageDepends {
    // depends[and][or]
    pub depends: Vec<Vec<String>>,
}

#[derive(Default)]
pub struct AptCacheDepends {
    pub recurse: bool,
}

impl AptCacheDepends {
    pub fn recurse(mut self) -> AptCacheDepends {
        self.recurse = true;
        self
    }

    pub fn run(
        &self,
        packages: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> io::Result<HashMap<String, PackageDepends>> {
        apt_cache_depends(packages, self)
    }
}

pub fn apt_cache_depends(
    packages: impl IntoIterator<Item = impl AsRef<OsStr>>,
    options: &AptCacheDepends,
) -> io::Result<HashMap<String, PackageDepends>> {
    let mut child = Command::new("apt-cache")
        .arg("depends")
        .args([
            "--no-generate",
            "--no-recommends",
            "--no-suggests",
            "--no-conflicts",
            "--no-breaks",
            "--no-replaces",
            "--no-enhances",
        ])
        .args(options.recurse.then_some("--recurse"))
        .arg("--")
        .args(packages)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .spawn()?;

    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line_buf = String::new();

    struct CollectContext {
        current_package: String,
        depends: Vec<Vec<String>>,
        options: Vec<String>,
        result: HashMap<String, PackageDepends>,
    }

    impl CollectContext {
        fn start_new_package(&mut self) -> io::Result<()> {
            if !self.current_package.is_empty() {
                if !self.options.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "unfinished dependency alternatives for {}",
                            self.current_package
                        ),
                    ));
                }
                self.result.insert(
                    self.current_package.clone(),
                    PackageDepends {
                        depends: std::mem::take(&mut self.depends),
                    },
                );
            }
            Ok(())
        }
    }

    let mut context = CollectContext {
        current_package: String::new(),
        depends: Vec::new(),
        options: Vec::new(),
        result: HashMap::new(),
    };

    while reader.read_line(&mut line_buf)? != 0 {
        let line = line_buf.trim_end_matches(['\r', '\n']);
        if let Some(pkg) = line
            .strip_prefix("  Depends: ")
            .or(line.strip_prefix("  PreDepends: "))
        {
            context.options.push(pkg.to_owned());
            context.depends.push(std::mem::take(&mut context.options));
        } else if let Some(pkg) = line
            .strip_prefix(" |Depends: ")
            .or(line.strip_prefix(" |PreDepends: "))
        {
            context.options.push(pkg.to_owned());
        } else if line.strip_prefix("    ").is_some() {
            // This is a list of concrete providers for a virtual package.
        } else if line.starts_with(' ') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown apt-cache dependency line '{line}'"),
            ));
        } else {
            context.start_new_package()?;
            context.current_package = line.to_string();
        }

        line_buf.clear();
    }

    context.start_new_package()?;

    let output = child.wait_with_output()?;

    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(io::Error::other(format!(
            "apt-cache depends returned non-zero status code: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(context.result)
}

#[cfg(test)]
mod tests {
    use super::collect_dpkg_query_list_files_line;

    fn collect(lines: &[&str]) -> std::io::Result<Vec<String>> {
        let mut result = Vec::new();
        let mut last_path_index = None;

        for line in lines {
            collect_dpkg_query_list_files_line(&mut result, &mut last_path_index, line)?;
        }

        Ok(result)
    }

    #[test]
    fn parses_dpkg_query_list_files_paths_and_separators() {
        assert_eq!(
            collect(&["/usr/lib/libexample.so", "", "/usr/share/example"]).unwrap(),
            vec![
                "/usr/lib/libexample.so".to_owned(),
                "/usr/share/example".to_owned()
            ]
        );
    }

    #[test]
    fn replaces_current_package_files_with_diverted_targets() {
        assert_eq!(
            collect(&[
                "/usr/bin/example",
                "locally diverted to: /usr/bin/example.local",
                "/bin/sh",
                "diverted by dash to: /bin/sh to: distrib",
            ])
            .unwrap(),
            vec![
                "/usr/bin/example.local".to_owned(),
                "/bin/sh to: distrib".to_owned()
            ]
        );
    }

    #[test]
    fn ignores_targets_for_files_diverted_by_the_current_package() {
        assert_eq!(
            collect(&["/lib64", "package diverts others to: /lib64.usr-is-merged",]).unwrap(),
            vec!["/lib64".to_owned()]
        );
    }

    #[test]
    fn rejects_unknown_dpkg_query_list_files_output() {
        let error = collect(&["unexpected output"]).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("unexpected output"));
    }

    #[test]
    fn rejects_diversions_without_a_preceding_file() {
        let error = collect(&["diverted by dash to: /bin/sh.distrib"]).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("no preceding file path"));
    }
}
