use futures::io::Cursor;
use indexmap::IndexMap;
use std::path::Path;
use std::sync::Mutex;
use url::Url;
use vrc_get_vpm::HttpClient;
use vrc_get_vpm::environment::{AddRepositoryErr, REPO_CACHE_FOLDER, Settings, add_remote_repo};
use vrc_get_vpm::io::DefaultEnvironmentIo;

mod common;

struct StaticHttpClient {
    body: Vec<u8>,
    requested_headers: Mutex<Vec<IndexMap<Box<str>, Box<str>>>>,
}

impl StaticHttpClient {
    fn new(body: impl Into<Vec<u8>>) -> Self {
        Self {
            body: body.into(),
            requested_headers: Mutex::new(Vec::new()),
        }
    }

    fn requested_headers(&self) -> Vec<IndexMap<Box<str>, Box<str>>> {
        self.requested_headers.lock().unwrap().clone()
    }
}

impl HttpClient for StaticHttpClient {
    async fn get(
        &self,
        _: &Url,
        _: &IndexMap<&str, &str>,
    ) -> std::io::Result<impl futures::io::AsyncRead + Send> {
        Ok(Cursor::new(self.body.clone()))
    }

    async fn get_with_etag(
        &self,
        _: &Url,
        headers: &IndexMap<Box<str>, Box<str>>,
        _: Option<&str>,
    ) -> std::io::Result<Option<(impl futures::io::AsyncRead + Send, Option<Box<str>>)>> {
        self.requested_headers.lock().unwrap().push(headers.clone());
        Ok(Some((
            Cursor::new(self.body.clone()),
            Some("test-etag".into()),
        )))
    }
}

fn clean_dir(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Ok(()) => {}
        Err(e) => panic!("error cleaning dir {}: {}", path.display(), e),
    }
    std::fs::create_dir_all(path).unwrap();
}

fn repository_json(id: &str, url: &str) -> String {
    format!(
        r#"{{
  "name": "Test Repository",
  "id": "{id}",
  "url": "{url}",
  "packages": {{
    "com.example.package": {{
      "versions": {{
        "1.0.0": {{
          "name": "com.example.package",
          "version": "1.0.0"
        }}
      }}
    }}
  }}
}}"#
    )
}

#[tokio::test]
async fn add_remote_repo_adds_repo_and_reports_duplicate_without_panic() {
    let root = common::get_temp_path("add_repository");
    clean_dir(&root);

    let io = DefaultEnvironmentIo::new(root.into_boxed_path());
    let mut settings = Settings::load(&io).await.unwrap();
    let url = Url::parse("https://example.com/vpm.json").unwrap();
    let http = StaticHttpClient::new(repository_json("com.example.repo", url.as_str()));
    let headers = IndexMap::from([
        (
            Box::<str>::from("Authorization"),
            Box::<str>::from("Bearer private-token"),
        ),
        (Box::<str>::from("X-Channel"), Box::<str>::from("beta")),
    ]);

    add_remote_repo(
        &mut settings,
        url.clone(),
        None,
        headers.clone(),
        &io,
        &http,
    )
    .await
    .unwrap();

    let repos = settings.get_user_repos();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].id(), Some("com.example.repo"));
    assert_eq!(repos[0].url(), Some(&url));
    assert_eq!(repos[0].headers(), &headers);
    assert_eq!(http.requested_headers(), vec![headers.clone()]);
    assert!(
        io.resolve(&Path::new(REPO_CACHE_FOLDER).join("com.example.repo.json"))
            .is_file()
    );

    settings.save(&io).await.unwrap();
    let reloaded = Settings::load(&io).await.unwrap();
    assert_eq!(reloaded.get_user_repos().len(), 1);
    assert_eq!(reloaded.get_user_repos()[0].headers(), &headers);

    let duplicate = add_remote_repo(&mut settings, url, None, headers, &io, &http).await;

    assert!(matches!(duplicate, Err(AddRepositoryErr::AlreadyAdded)));
    assert_eq!(settings.get_user_repos().len(), 1);
}
