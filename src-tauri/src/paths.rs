use std::path::PathBuf;

/// Platform data dir from `app_data_dir`, falling back to `$HOME/ratspeak-data`.
/// `RATSPEAK_DATA_DIR` overrides it (dev/test — e.g. a throwaway profile that
/// leaves the real config untouched).
pub fn resolve_data_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;

    if let Ok(dir) = std::env::var("RATSPEAK_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }

    app.path().app_data_dir().unwrap_or_else(|_| {
        let home = dirs_fallback();
        home.join("ratspeak-data")
    })
}

fn dirs_fallback() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile)
    } else {
        PathBuf::from(".")
    }
}
