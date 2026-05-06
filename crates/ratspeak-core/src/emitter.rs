//! IPC abstraction. The runtime emits events through this trait so it stays
//! free of any `tauri` dependency. The concrete `TauriEmitter` lives in
//! `ratspeak-tauri` and wraps `AppHandle::emit`.

use serde_json::Value;

pub trait Emitter: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

/// Drops every emit. Useful for headless tests where there's no IPC peer.
pub struct NoopEmitter;

impl Emitter for NoopEmitter {
    fn emit(&self, _event: &str, _payload: Value) {}
}
