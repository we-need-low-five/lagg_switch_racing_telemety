pub mod bundle;
pub mod database;
pub mod parquet_io;

use std::path::{Path, PathBuf};

pub use bundle::{export_session_bundle, import_session_bundle};
pub use database::Database;
pub use parquet_io::{read_lap_samples, write_lap_parquet};

pub fn default_data_dir() -> PathBuf {
    dirs_or_local_app_data().join("SimTelemetry")
}

pub fn sessions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

fn dirs_or_local_app_data() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
