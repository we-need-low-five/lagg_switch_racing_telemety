use anyhow::{bail, Result};
use std::path::{Component, Path, PathBuf};

/// Resolve a relative path under `data_dir`, rejecting absolute paths and `..`.
pub fn resolve_data_relative(data_dir: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() {
        bail!("path must be relative to the data directory");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                bail!("path must not contain '..' or drive prefixes");
            }
        }
    }
    if relative.is_empty() {
        bail!("path must not be empty");
    }
    Ok(data_dir.join(path))
}

/// Ensure an export/import bundle path uses the `.stb` extension.
pub fn validate_bundle_path(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("stb"))
        .unwrap_or(false);
    if !ext {
        bail!("bundle path must end with .stb");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rejects_absolute_and_traversal() {
        let data = PathBuf::from("C:/data");
        assert!(resolve_data_relative(&data, "C:/elsewhere/x.parquet").is_err());
        assert!(resolve_data_relative(&data, "../x.parquet").is_err());
        assert!(resolve_data_relative(&data, "sessions/../x.parquet").is_err());
    }

    #[test]
    fn accepts_relative_session_path() {
        let data = PathBuf::from("C:/data");
        let p = resolve_data_relative(&data, "sessions/abc/laps/1.parquet").unwrap();
        assert_eq!(p, data.join("sessions/abc/laps/1.parquet"));
    }

    #[test]
    fn validates_stb_extension() {
        assert!(validate_bundle_path(Path::new("out.stb")).is_ok());
        assert!(validate_bundle_path(Path::new("out.STB")).is_ok());
        assert!(validate_bundle_path(Path::new("out.zip")).is_err());
    }
}
