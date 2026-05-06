//! Identity CRUD + display-name updates.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::State;

use crate::db;
use crate::error::{AppError, AppResult};
use crate::helpers::{active_identity_id, sanitize_text, validate_hex};
use crate::state::AppState;

#[tauri::command]
pub async fn api_identity(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let active = db::spawn_db(state.db.clone(), |p| db::get_active_identity(&p))
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "db task panicked");
            Default::default()
        });
    Ok(match active {
        Some(identity) => json!({
            "exists": true,
            "hash": identity.get("hash"),
            "lxmf_destination": identity.get("lxmf_hash"),
            "display_name": identity.get("display_name").and_then(|v| v.as_str()).unwrap_or(""),
            "nickname": identity.get("nickname").and_then(|v| v.as_str()).unwrap_or(""),
        }),
        None => json!({
            "exists": false,
            "hash": null,
            "lxmf_destination": null,
            "display_name": "",
            "nickname": "",
        }),
    })
}

#[tauri::command]
pub async fn api_list_identities(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let identities = db::spawn_db(state.db.clone(), |p| db::get_all_identities(&p))
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "list_identities db task panicked");
            Default::default()
        });
    Ok(json!(identities))
}

#[derive(Deserialize)]
pub struct CreateIdentityArgs {
    #[serde(default)]
    pub nickname: Option<String>,
}

#[tauri::command]
pub async fn api_create_identity(
    state: State<'_, Arc<AppState>>,
    args: CreateIdentityArgs,
) -> AppResult<Value> {
    let nickname = sanitize_text(args.nickname.as_deref().unwrap_or(""), 64);

    let st: Arc<AppState> = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        if let Ok(lxmf) = st.lxmf.lock() {
            if let Some(mgr) = lxmf.as_ref() {
                mgr.create_identity(&nickname, &st.db).ok()
            } else {
                None
            }
        } else {
            None
        }
    })
    .await
    .map_err(|_| AppError::internal("create_identity task panicked"))?;

    match result {
        Some((hash, lxmf_hash)) => Ok(json!({ "hash": hash, "lxmf_hash": lxmf_hash })),
        None => Err(AppError::lxmf_not_initialized("LXMF not initialized")),
    }
}

#[derive(Deserialize)]
pub struct ImportIdentityArgs {
    pub key: String,
    #[serde(default)]
    pub nickname: Option<String>,
}

#[tauri::command]
pub async fn api_import_identity(
    state: State<'_, Arc<AppState>>,
    args: ImportIdentityArgs,
) -> AppResult<Value> {
    let key_bytes =
        hex::decode(args.key.trim()).map_err(|_| AppError::bad_request("Invalid hex key data"))?;
    import_identity_shared(state, key_bytes, args.nickname).await
}

#[tauri::command]
pub async fn api_import_identity_base64(
    state: State<'_, Arc<AppState>>,
    args: ImportIdentityArgs,
) -> AppResult<Value> {
    let key_bytes = B64
        .decode(args.key.trim())
        .map_err(|_| AppError::bad_request("Invalid base64 key data"))?;
    import_identity_shared(state, key_bytes, args.nickname).await
}

async fn import_identity_shared(
    state: State<'_, Arc<AppState>>,
    key_bytes: Vec<u8>,
    nickname: Option<String>,
) -> AppResult<Value> {
    let nickname = sanitize_text(nickname.as_deref().unwrap_or(""), 64);
    let st: Arc<AppState> = Arc::clone(&state);
    let result = tokio::task::spawn_blocking(move || {
        if let Ok(lxmf) = st.lxmf.lock() {
            if let Some(mgr) = lxmf.as_ref() {
                mgr.import_identity(&key_bytes, &nickname, &st.db)
                    .map_err(|e| e.to_string())
            } else {
                Err("LXMF not initialized".into())
            }
        } else {
            Err("Lock error".into())
        }
    })
    .await
    .map_err(|_| AppError::internal("import_identity task panicked"))?;

    match result {
        Ok((hash, lxmf_hash)) => Ok(json!({ "hash": hash, "lxmf_hash": lxmf_hash })),
        Err(e) => Err(AppError::bad_request(e)),
    }
}

#[tauri::command]
pub async fn api_activate_identity(
    state: State<'_, Arc<AppState>>,
    hash_hex: String,
) -> AppResult<Value> {
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let hash_for_db = hash_hex.clone();
    let result = db::spawn_db(state.db.clone(), move |p| {
        db::set_active_identity(&p, &hash_for_db)
    })
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "activate db task panicked");
        AppError::internal("db task panicked")
    })?;
    result.map_err(|e| AppError::internal(format!("Failed to activate: {e}")))?;
    Ok(json!({ "hash": hash_hex }))
}

/// Existence check; the actual bytes ship via `api_export_identity_base64`.
#[tauri::command]
pub async fn api_export_identity(
    state: State<'_, Arc<AppState>>,
    hash_hex: String,
) -> AppResult<Value> {
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let exists = state
        .lxmf
        .lock()
        .ok()
        .and_then(|l| l.as_ref().and_then(|mgr| mgr.export_identity(&hash_hex)))
        .is_some();
    if exists {
        Ok(json!({ "message": "Use export-base64 endpoint for key data" }))
    } else {
        Err(AppError::not_found("Identity file not found"))
    }
}

#[tauri::command]
pub async fn api_export_identity_base64(
    state: State<'_, Arc<AppState>>,
    hash_hex: String,
) -> AppResult<Value> {
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let key_bytes = state
        .lxmf
        .lock()
        .ok()
        .and_then(|l| l.as_ref().and_then(|mgr| mgr.export_identity(&hash_hex)));
    match key_bytes {
        Some(bytes) => Ok(json!({ "key": B64.encode(&bytes) })),
        None => Err(AppError::not_found("Identity file not found")),
    }
}

#[derive(Deserialize)]
pub struct UpdateIdentityArgs {
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[tauri::command]
pub async fn api_update_identity(
    state: State<'_, Arc<AppState>>,
    hash_hex: String,
    args: UpdateIdentityArgs,
) -> AppResult<Value> {
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let nickname = args.nickname.as_deref().map(|s| sanitize_text(s, 64));
    let display_name = args.display_name.as_deref().map(|s| sanitize_text(s, 64));
    let hash_for_db = hash_hex.clone();
    let nick_for_db = nickname.clone();
    let dn_for_db = display_name.clone();
    let result = db::spawn_db(state.db.clone(), move |p| {
        db::update_identity(
            &p,
            &hash_for_db,
            nick_for_db.as_deref(),
            dn_for_db.as_deref(),
        )
    })
    .await
    .map_err(|_| AppError::internal("update_identity db task panicked"))?;
    result.map_err(|e| {
        tracing::error!(error = %e, "update_identity failed");
        AppError::internal("failed to update identity")
    })?;
    Ok(json!(null))
}

#[tauri::command]
pub async fn api_delete_identity(
    state: State<'_, Arc<AppState>>,
    hash_hex: String,
    #[allow(non_snake_case)] cascade: Option<bool>,
) -> AppResult<Value> {
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid hash"));
    }
    let active = active_identity_id(&state);
    if active == hash_hex {
        return Err(AppError::bad_request("Cannot delete active identity"));
    }
    let hash_for_db = hash_hex.clone();
    let cascade = cascade.unwrap_or(false);
    let result = db::spawn_db(state.db.clone(), move |p| {
        db::delete_identity(&p, &hash_for_db, cascade)
    })
    .await
    .map_err(|_| AppError::internal("delete_identity db task panicked"))?;
    result.map_err(|e| AppError::internal(format!("Failed to delete: {e}")))?;
    Ok(json!(null))
}

#[derive(Deserialize)]
pub struct DisplayNameArgs {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[tauri::command]
pub async fn switch_identity(state: State<'_, Arc<AppState>>, hash: String) -> AppResult<Value> {
    let hash_hex = sanitize_text(&hash, 128);
    if !validate_hex(&hash_hex, 16, 128) {
        return Err(AppError::bad_request("Invalid identity hash"));
    }

    // load_identity does disk I/O; keep off the async runtime.
    let state_io: Arc<AppState> = Arc::clone(&state);
    let hash_io = hash_hex.clone();
    let switch_result = tokio::task::spawn_blocking(move || {
        if let Ok(mut lxmf) = state_io.lxmf.lock() {
            if let Some(mgr) = lxmf.as_mut() {
                mgr.load_identity(&hash_io).map_err(|e| e.to_string())
            } else {
                Err("LXMF not initialized".into())
            }
        } else {
            Err("Lock error".into())
        }
    })
    .await
    .unwrap_or_else(|e| Err(format!("Identity switch task failed: {e}")));

    match switch_result {
        Ok(()) => {
            let pool_set = state.db.clone();
            let hash_set = hash_hex.clone();
            let _ = db::spawn_db(pool_set, move |p| {
                if let Err(e) = db::set_active_identity(&p, &hash_set) {
                    tracing::error!("Failed to set active identity: {e}");
                }
            })
            .await;

            if let Ok(mut times) = state.message_send_times.lock() {
                times.clear();
            }
            if let Ok(mut map) = state.msg_id_map.lock() {
                map.clear();
            }

            // One blocking task so the MutexGuard never crosses an .await.
            let st: Arc<AppState> = Arc::clone(&state);
            let (switched_lxmf_hash, switched_display_name) =
                tokio::task::spawn_blocking(move || {
                    if let Ok(mut lxmf) = st.lxmf.lock()
                        && let Some(mgr) = lxmf.as_mut()
                    {
                        mgr.display_name = db::get_active_identity(&st.db)
                            .and_then(|id| {
                                id.get("display_name")
                                    .and_then(|v| v.as_str())
                                    .map(String::from)
                            })
                            .unwrap_or_default();
                    }
                    if let Ok(lxmf) = st.lxmf.lock() {
                        if let Some(mgr) = lxmf.as_ref() {
                            (mgr.lxmf_hash.clone(), mgr.display_name.clone())
                        } else {
                            (String::new(), String::new())
                        }
                    } else {
                        (String::new(), String::new())
                    }
                })
                .await
                .unwrap_or((String::new(), String::new()));

            let payload = json!({
                "hash": hash_hex,
                "lxmf_hash": switched_lxmf_hash,
                "display_name": switched_display_name,
            });
            state.emit_to_all("identity_switched", payload.clone());
            Ok(payload)
        }
        Err(e) => Err(AppError::internal(e)),
    }
}

#[tauri::command]
pub async fn api_set_display_name(
    state: State<'_, Arc<AppState>>,
    args: DisplayNameArgs,
) -> AppResult<Value> {
    let display_name = sanitize_text(args.display_name.as_deref().unwrap_or(""), 64);
    if display_name.is_empty() {
        return Err(AppError::bad_request("display_name required"));
    }
    let identity_id = active_identity_id(&state);
    if identity_id.is_empty() {
        return Err(AppError::conflict("no active identity"));
    }

    // Prefer in-memory LXMF mgr; fall back to DB-only on startup race.
    let updated_in_memory = {
        let mut guard = state
            .lxmf
            .lock()
            .map_err(|_| AppError::internal("lxmf state lock poisoned"))?;
        match guard.as_mut() {
            Some(mgr) => {
                mgr.update_display_name(&display_name, &state.db, &identity_id)
                    .map_err(|e| {
                        tracing::error!(error = %e, "display_name: update_identity failed");
                        AppError::internal("failed to save display name")
                    })?;
                true
            }
            None => false,
        }
    };

    if !updated_in_memory {
        let id = identity_id.clone();
        let dn = display_name.clone();
        db::spawn_db(state.db.clone(), move |p| {
            db::update_identity(&p, &id, None, Some(&dn))
        })
        .await
        .map_err(|_| AppError::internal("failed to save display name"))?
        .map_err(|e| {
            tracing::error!(error = %e, "display_name: update_identity failed (no lxmf)");
            AppError::internal("failed to save display name")
        })?;
    }

    if updated_in_memory {
        crate::send_announce_from_state(&state).await;
    }

    Ok(json!({ "display_name": display_name }))
}
