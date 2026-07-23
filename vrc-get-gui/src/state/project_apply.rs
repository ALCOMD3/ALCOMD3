use std::sync::Mutex;
use vrc_get_vpm::AbortCheck;

pub struct ProjectApplyState {
    current: Mutex<Option<AbortCheck>>,
}

impl ProjectApplyState {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    pub fn try_start(&self, abort: AbortCheck) -> bool {
        let mut current = self.current.lock().unwrap();
        if current.is_some() {
            return false;
        }
        *current = Some(abort);
        true
    }

    pub fn finish(&self) {
        *self.current.lock().unwrap() = None;
    }

    pub fn abort(&self) -> bool {
        let Some(abort) = self.current.lock().unwrap().take() else {
            return false;
        };
        abort.abort();
        true
    }
}
