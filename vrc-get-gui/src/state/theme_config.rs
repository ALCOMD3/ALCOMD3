use crate::config::ThemeConfig;
use arc_swap::ArcSwap;
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};
use vrc_get_vpm::io::{DefaultEnvironmentIo, IoTrait};

struct ThemeConfigStateInner {
    config: ThemeConfig,
    path: PathBuf,
}

pub struct ThemeConfigState {
    inner: ArcSwap<ThemeConfigStateInner>,
    io: DefaultEnvironmentIo,
    mut_lock: Mutex<()>,
}

impl ThemeConfigState {
    pub async fn new_load(io: &DefaultEnvironmentIo) -> io::Result<Self> {
        let loaded = load_async(io).await?;
        Ok(Self {
            inner: ArcSwap::new(Arc::new(loaded)),
            io: io.clone(),
            mut_lock: Mutex::new(()),
        })
    }

    pub fn get(&self) -> ThemeConfigRef {
        ThemeConfigRef::new(self.inner.load_full())
    }

    pub async fn load_mut(&self) -> io::Result<ThemeConfigMutRef<'_>> {
        let lock = self.mut_lock.lock().await;
        let loaded = ThemeConfigRef::new(self.inner.load_full());
        Ok(ThemeConfigMutRef {
            config: loaded.state.config.clone(),
            path: loaded.state.path.clone(),
            io: &self.io,
            _mut_lock_guard: lock,
            cache: &self.inner,
        })
    }
}

pub struct ThemeConfigRef {
    state: Arc<ThemeConfigStateInner>,
}

impl ThemeConfigRef {
    fn new(state: Arc<ThemeConfigStateInner>) -> Self {
        Self { state }
    }
}

impl Deref for ThemeConfigRef {
    type Target = ThemeConfig;

    #[inline(always)]
    fn deref(&self) -> &ThemeConfig {
        &self.state.config
    }
}

pub struct ThemeConfigMutRef<'s> {
    config: ThemeConfig,
    path: PathBuf,
    io: &'s DefaultEnvironmentIo,
    _mut_lock_guard: MutexGuard<'s, ()>,
    cache: &'s ArcSwap<ThemeConfigStateInner>,
}

impl ThemeConfigMutRef<'_> {
    pub async fn save(self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(&self.config)?;
        tokio::fs::create_dir_all(self.path.parent().unwrap()).await?;
        self.io.write_atomic(&self.path, json.as_bytes()).await?;
        self.cache.swap(Arc::new(ThemeConfigStateInner {
            config: self.config,
            path: self.path,
        }));
        Ok(())
    }
}

impl Deref for ThemeConfigMutRef<'_> {
    type Target = ThemeConfig;

    #[inline(always)]
    fn deref(&self) -> &ThemeConfig {
        &self.config
    }
}

impl DerefMut for ThemeConfigMutRef<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut ThemeConfig {
        &mut self.config
    }
}

async fn load_async(io: &DefaultEnvironmentIo) -> io::Result<ThemeConfigStateInner> {
    async fn load_fs(path: &Path) -> io::Result<ThemeConfig> {
        match tokio::fs::read(path).await {
            Ok(buffer) if buffer.is_empty() => Ok(Default::default()),
            Ok(buffer) => {
                let mut loaded = serde_json::from_slice::<ThemeConfig>(&buffer)?;
                loaded.fix_defaults();
                Ok(loaded)
            }
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(ThemeConfig::default()),
            Err(e) => Err(e),
        }
    }

    async fn backup_old_config(path: &Path) -> io::Result<()> {
        let mut i = 0;
        loop {
            let backup_path = path.with_extension(format!("json.bak.{i}"));
            match tokio::fs::rename(path, &backup_path).await {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    i += 1;
                }
                Ok(()) => break Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => break Ok(()),
                Err(e) => break Err(e),
            }
        }
    }

    let path = io.resolve(crate::storage::THEME_CONFIG_PATH.as_ref());

    let config = match load_fs(&path).await {
        Ok(loaded) => loaded,
        Err(e) => {
            log::error!("Failed to load theme-config.json, using default config: {e}");

            if let Err(e) = backup_old_config(&path).await {
                log::error!("Failed to backup old theme config: {e}");
            }

            ThemeConfig::default()
        }
    };

    Ok(ThemeConfigStateInner { config, path })
}
