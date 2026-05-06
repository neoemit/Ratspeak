use std::path::PathBuf;

/// Platform data dir from `app_data_dir`, falling back to `$HOME/ratspeak-data`.
pub fn resolve_data_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;

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
