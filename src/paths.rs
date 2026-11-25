//! Where Maestro keeps its own files on each system.

use std::path::PathBuf;

/// The folder for Maestro's saved state. Created on first save.
pub fn data_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    // For trying things out without touching the real files.
    if let Some(dir) = var("MAESTRO_DATA_DIR") {
        return dir;
    }
    if cfg!(windows) {
        if let Some(base) = var("LOCALAPPDATA") {
            return base.join("Maestro");
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = var("HOME") {
            return home.join("Library").join("Application Support").join("Maestro");
        }
    } else {
        if let Some(base) = var("XDG_STATE_HOME") {
            return base.join("maestro");
        }
        if let Some(home) = var("HOME") {
            return home.join(".local").join("state").join("maestro");
        }
    }
    std::env::temp_dir().join("Maestro")
}

/// Write a file in the data folder, replacing it in one step so a crash
/// part-way through never leaves half a file.
pub fn save(name: &str, text: &str) {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let temp = dir.join(format!("{name}.tmp"));
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, dir.join(name));
    }
}

pub fn load(name: &str) -> Option<String> {
    std::fs::read_to_string(data_dir().join(name)).ok()
}
