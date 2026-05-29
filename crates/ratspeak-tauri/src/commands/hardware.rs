//! Hardware (YubiKey/Nitrokey PIV) identity commands — thin wrappers over
//! `ratspeak_runtime::hardware`. Card I/O is blocking, so each runs on a
//! blocking task. Only compiled with the `hardware` feature.

use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::helpers::{sanitize_announced_display_name, validate_hex};
use crate::state::AppState;

fn to_value<T: serde::Serialize>(v: T) -> AppResult<Value> {
    serde_json::to_value(v).map_err(|e| AppError::internal(e.to_string()))
}

fn check_pin(pin: &str) -> AppResult<()> {
    if pin.len() < 6 || pin.len() > 8 {
        return Err(AppError::bad_request("PIN must be 6-8 characters"));
    }
    Ok(())
}

fn clean_nickname(nickname: &str) -> AppResult<String> {
    sanitize_announced_display_name(nickname).map_err(AppError::bad_request)
}

#[tauri::command]
pub async fn hw_detect() -> AppResult<Value> {
    let d = tokio::task::spawn_blocking(ratspeak_runtime::hardware::detect)
        .await
        .map_err(|_| AppError::internal("hw detect task panicked"))?;
    to_value(d)
}

#[tauri::command]
pub async fn hw_provision_recoverable(
    state: State<'_, Arc<AppState>>,
    pin: String,
    nickname: String,
) -> AppResult<Value> {
    check_pin(&pin)?;
    let nickname = clean_nickname(&nickname)?;
    let data_dir = state.config.data_dir.clone();
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        ratspeak_runtime::hardware::provision_recoverable(&data_dir, &db, &pin, &nickname)
    })
    .await
    .map_err(|_| AppError::internal("provision task panicked"))?
    .map_err(AppError::bad_request)?;
    to_value(res)
}

#[tauri::command]
pub async fn hw_provision_hardware_only(
    state: State<'_, Arc<AppState>>,
    pin: String,
    nickname: String,
) -> AppResult<Value> {
    check_pin(&pin)?;
    let nickname = clean_nickname(&nickname)?;
    let data_dir = state.config.data_dir.clone();
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        ratspeak_runtime::hardware::provision_hardware_only(&data_dir, &db, &pin, &nickname)
    })
    .await
    .map_err(|_| AppError::internal("provision task panicked"))?
    .map_err(AppError::bad_request)?;
    to_value(res)
}

#[tauri::command]
pub async fn hw_import_existing(
    state: State<'_, Arc<AppState>>,
    nickname: String,
) -> AppResult<Value> {
    let nickname = clean_nickname(&nickname)?;
    let data_dir = state.config.data_dir.clone();
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        ratspeak_runtime::hardware::import_existing(&data_dir, &db, &nickname)
    })
    .await
    .map_err(|_| AppError::internal("import task panicked"))?
    .map_err(AppError::bad_request)?;
    to_value(res)
}

#[tauri::command]
pub async fn hw_restore(
    state: State<'_, Arc<AppState>>,
    phrase: String,
    pin: String,
    nickname: String,
) -> AppResult<Value> {
    check_pin(&pin)?;
    let nickname = clean_nickname(&nickname)?;
    let data_dir = state.config.data_dir.clone();
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        ratspeak_runtime::hardware::restore(&data_dir, &db, &phrase, &pin, &nickname)
    })
    .await
    .map_err(|_| AppError::internal("restore task panicked"))?
    .map_err(AppError::bad_request)?;
    to_value(res)
}

#[tauri::command]
pub async fn hw_remove(state: State<'_, Arc<AppState>>, hash: String) -> AppResult<Value> {
    if !validate_hex(&hash, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let data_dir = state.config.data_dir.clone();
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || ratspeak_runtime::hardware::remove(&data_dir, &db, &hash))
        .await
        .map_err(|_| AppError::internal("remove task panicked"))?
        .map_err(AppError::bad_request)?;
    to_value(serde_json::json!({ "removed": true }))
}
