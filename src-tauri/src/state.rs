use parking_lot::Mutex;
use sim_daemon::RecordingService;
use sim_storage::Database;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppState {
    pub recorder: Mutex<RecordingService>,
    pub db: Arc<Mutex<Database>>,
    pub data_dir: PathBuf,
    pub last_notification: Mutex<Option<String>>,
}

impl AppState {
    pub fn new(db: Arc<Mutex<Database>>, recorder: RecordingService, data_dir: PathBuf) -> Self {
        Self {
            recorder: Mutex::new(recorder),
            db,
            data_dir,
            last_notification: Mutex::new(None),
        }
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn set_last_notification(&self, msg: String) {
        *self.last_notification.lock() = Some(msg);
    }
}
