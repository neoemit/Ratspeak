//! Domain validation errors. Per-layer error types compose at boundaries —
//! `ratspeak-db::DbError`, `ratspeak-runtime::RuntimeError`, and
//! `ratspeak-tauri::AppError` all wrap `CoreError` via `From` impls.

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    Invalid(String),
}
