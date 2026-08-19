use sim_daemon::RecordingService;
use sim_storage::Database;
use std::path::PathBuf;

pub struct AppState {
    recorder: RecordingService,
    data_dir: PathBuf,
    last_notification: Option<String>,
}

impl AppState {
    pub fn new(recorder: RecordingService, data_dir: PathBuf) -> Self {
        Self {
            recorder,
            data_dir,
            last_notification: None,
        }
    }

    pub fn recorder(&self) -> &RecordingService {
        &self.recorder
    }

    pub fn recorder_mut(&mut self) -> &mut RecordingService {
        &mut self.recorder
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn db(&self) -> &Database {
        self.recorder.db()
    }

    pub fn set_last_notification(&mut self, msg: String) {
        self.last_notification = Some(msg);
    }
}
