use std::sync::{Arc, Mutex};
use tokio::task::AbortHandle;

#[derive(Clone)]
pub struct ProjectRestoreState {
    current: Arc<Mutex<Option<ActiveProjectRestoreTask>>>,
}

enum ActiveProjectRestoreTask {
    Cancellable { _abort: AbortHandle },
    Uncancellable,
}

impl ProjectRestoreState {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self, abort: AbortHandle) {
        *self.current.lock().unwrap() =
            Some(ActiveProjectRestoreTask::Cancellable { _abort: abort });
    }

    pub fn try_start_uncancellable(&self) -> bool {
        let mut current = self.current.lock().unwrap();
        if current.is_some() {
            return false;
        }

        *current = Some(ActiveProjectRestoreTask::Uncancellable);
        true
    }

    pub fn finish(&self) {
        *self.current.lock().unwrap() = None;
    }
}
