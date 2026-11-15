use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::config::Config;

pub struct JobState {
    cancel: AtomicBool,
}

impl JobState {
    pub fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub struct AppState {
    pub config: Mutex<Config>,
    pub jobs: Mutex<HashMap<String, std::sync::Arc<JobState>>>,
    pub manifest_cache: Mutex<Option<(std::time::Instant, Vec<crate::version::ManifestVersion>)>>,
    pub size_cache: Mutex<HashMap<String, crate::version::SizeBreakdown>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::load()),
            jobs: Mutex::new(HashMap::new()),
            manifest_cache: Mutex::new(None),
            size_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_job(&self, id: &str) -> std::sync::Arc<JobState> {
        let state = std::sync::Arc::new(JobState::new());
        self.jobs.lock().unwrap().insert(id.to_string(), state.clone());
        state
    }

    pub fn cancel_job(&self, id: &str) -> bool {
        if let Some(j) = self.jobs.lock().unwrap().get(id) {
            j.cancel();
            true
        } else {
            false
        }
    }

    pub fn drop_job(&self, id: &str) {
        self.jobs.lock().unwrap().remove(id);
    }
}
