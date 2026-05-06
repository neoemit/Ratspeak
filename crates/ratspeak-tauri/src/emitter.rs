//! Tauri-backed `Emitter` impl. Wraps an `AppHandle` and forwards `emit` to
//! the WebView's event bus.

use ratspeak_core::Emitter;

pub struct TauriEmitter {
    handle: tauri::AppHandle,
}

impl TauriEmitter {
    pub fn new(handle: tauri::AppHandle) -> Self {
        Self { handle }
    }
}

impl Emitter for TauriEmitter {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter as _;
        if let Err(e) = self.handle.emit(event, &payload) {
            tracing::warn!(target: "events", event, error = %e, "tauri emit failed");
        }
    }
}
