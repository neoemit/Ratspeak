//! Contact list + block list reads/writes + transport blackhole controls.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::State;

use crate::commands::shared::{
    broadcast_blackhole_update, format_contacts_list, hex_to_array16, snapshot_blackhole,
    transport_query,
};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::helpers::{active_identity_id, sanitize_text, validate_hex};
use crate::state::AppState;

#[tauri::command]
pub async fn api_contacts(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let identity_id = active_identity_id(&state);
    let id_for_db = identity_id.clone();
    let contacts = db::spawn_db(state.db.clone(), move |p| {
        db::get_all_contacts(&p, &id_for_db)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!(error = %e, "contacts db task panicked");
        Default::default()
    });
    let result: Vec<Value> = contacts
        .into_iter()
        .map(|c| {
            json!({
                "hash": c.get("dest_hash"),
                "display_name": c.get("display_name"),
                "trust": c.get("trust"),
                "notes": c.get("notes"),
                "first_seen": c.get("first_seen"),
                "last_seen": c.get("last_seen"),
            })
        })
        .collect();
    Ok(json!(result))
}

#[tauri::command]
pub async fn api_blocked_contacts(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let identity_id = active_identity_id(&state);
    let id_for_db = identity_id.clone();
    let blocked = db::spawn_db(state.db.clone(), move |p| {
        db::get_blocked_contacts(&p, &id_for_db)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!(error = %e, "blocked-contacts db task panicked");
        Default::default()
    });

    let blackholed_set = current_blackholed_set(&state).await;
    let decorated: Vec<Value> = blocked
        .into_iter()
        .map(|mut row| {
            let hash = row
                .get("hash")
                .and_then(|h| h.as_str())
                .unwrap_or("")
                .to_string();
            let is_network_blocked = blackholed_set.contains(&hash);
            if let Some(obj) = row.as_object_mut() {
                obj.insert("is_network_blocked".to_string(), json!(is_network_blocked));
            }
            row
        })
        .collect();
    Ok(json!(decorated))
}

#[derive(Deserialize)]
pub struct AddContactArgs {
    pub hash: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Emit single-row `peers_updated` if visible, else `peer_removed`.
async fn emit_peer_delta_for(state: &Arc<AppState>, dest_hash: &str) {
    let pool = state.db.clone();
    let key = dest_hash.to_string();
    let identity_id = crate::helpers::active_identity_id(state);
    let resolved = db::spawn_db(pool, move |p| {
        db::get_peers_by_hashes(&p, &[key], &identity_id)
    })
    .await
    .unwrap_or_default();
    if let Some(row) = resolved.into_iter().next() {
        state.emit_to_all(
            "peers_updated",
            json!({
                "peers": [{
                    "hash": row.hash,
                    "last_seen": row.last_seen,
                    "first_seen": row.first_seen,
                    "display_name": row.display_name,
                    "is_contact": row.is_contact,
                    "last_interface": row.last_interface,
                }]
            }),
        );
    } else {
        state.emit_to_all("peer_removed", json!({ "hash": dest_hash }));
    }
}

#[tauri::command]
pub async fn add_contact(
    state: State<'_, Arc<AppState>>,
    args: AddContactArgs,
) -> AppResult<Value> {
    let dest_hash = sanitize_text(&args.hash, 128);
    let display_name = args.display_name.as_deref().map(|s| sanitize_text(s, 64));

    if !validate_hex(&dest_hash, 16, 64) {
        return Err(AppError::bad_request(
            "Invalid identity hash. Must be 16-64 hex characters (0-9, a-f).",
        ));
    }

    let identity_id = active_identity_id(&state);
    let dh = dest_hash.clone();
    let dn = display_name.clone();
    let id_c = identity_id.clone();
    let contacts_list = db::spawn_db(state.db.clone(), move |p| {
        let conn = match p.get() {
            Ok(c) => c,
            Err(_) => return Vec::<Value>::new(),
        };
        db::save_contact(&p, &dh, dn.as_deref(), "trusted", &id_c);
        let contacts = db::get_all_contacts_conn(&conn, &id_c);
        format_contacts_list(&contacts)
    })
    .await
    .map_err(|_| AppError::internal("add_contact db task panicked"))?;

    state.emit_to_all("contacts_update", json!(contacts_list));
    state.emit_to_all(
        "contact_added",
        json!({
            "hash": dest_hash,
            "display_name": display_name.clone().unwrap_or_else(|| dest_hash[..12.min(dest_hash.len())].to_string()),
        }),
    );
    emit_peer_delta_for(&state, &dest_hash).await;
    Ok(json!({ "hash": dest_hash, "display_name": display_name }))
}

#[tauri::command]
pub async fn remove_contact(state: State<'_, Arc<AppState>>, hash: String) -> AppResult<Value> {
    let dest_hash = sanitize_text(&hash, 128);
    if !validate_hex(&dest_hash, 16, 64) {
        return Err(AppError::bad_request("Invalid hash for removal."));
    }

    let identity_id = active_identity_id(&state);
    let dh = dest_hash.clone();
    let id_c = identity_id.clone();
    let contacts_list = db::spawn_db(state.db.clone(), move |p| {
        let conn = match p.get() {
            Ok(c) => c,
            Err(_) => return Vec::<Value>::new(),
        };
        conn.execute(
            "DELETE FROM contacts WHERE dest_hash = ?1 AND identity_id = ?2",
            rusqlite::params![dh, id_c],
        )
        .ok();
        let contacts = db::get_all_contacts_conn(&conn, &id_c);
        format_contacts_list(&contacts)
    })
    .await
    .map_err(|_| AppError::internal("remove_contact db task panicked"))?;

    state.emit_to_all("contacts_update", json!(contacts_list));
    emit_peer_delta_for(&state, &dest_hash).await;
    Ok(json!(null))
}

#[derive(Deserialize)]
pub struct BlockContactArgs {
    pub hash: String,
    /// Also blackhole at transport layer (node-global).
    #[serde(default)]
    pub escalate_to_blackhole: bool,
}

#[tauri::command]
pub async fn block_contact(
    state: State<'_, Arc<AppState>>,
    args: BlockContactArgs,
) -> AppResult<Value> {
    let dest_hash = sanitize_text(&args.hash, 128);
    if !validate_hex(&dest_hash, 16, 64) {
        return Err(AppError::bad_request("Invalid hash for blocking."));
    }

    let identity_id = active_identity_id(&state);
    let dh = dest_hash.clone();
    let id_c = identity_id.clone();
    let result = db::spawn_db(state.db.clone(), move |p| {
        let conn = p.get().ok()?;
        let display_name: String = conn
            .query_row(
                "SELECT display_name FROM contacts WHERE dest_hash = ?1 AND identity_id = ?2",
                rusqlite::params![dh, id_c],
                |row| row.get(0),
            )
            .unwrap_or_default();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        conn.execute(
            "INSERT OR REPLACE INTO blocked_contacts (dest_hash, identity_id, display_name, blocked_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![dh, id_c, display_name, now],
        ).ok();
        conn.execute(
            "DELETE FROM contacts WHERE dest_hash = ?1 AND identity_id = ?2",
            rusqlite::params![dh, id_c],
        )
        .ok();

        let contacts = db::get_all_contacts_conn(&conn, &id_c);
        let contacts_list = format_contacts_list(&contacts);
        Some((display_name, contacts_list))
    })
    .await
    .map_err(|_| AppError::internal("block_contact db task panicked"))?;

    let (display_name, contacts_list) =
        result.ok_or_else(|| AppError::database_unavailable("Contact DB unavailable"))?;

    // Manual reason + permanent TTL.
    let mut blackholed = false;
    if args.escalate_to_blackhole
        && let Some(hash_bytes) = hex_to_array16(&dest_hash)
    {
        use rns_transport::messages::{TransportQuery, TransportQueryResponse};
        let resp = transport_query(
            &state,
            TransportQuery::BlackholeIdentity {
                hash: hash_bytes,
                ttl: None,
                reason: rns_transport::blackhole::BlackholeReason::Manual,
                reason_label: None,
            },
        )
        .await;
        blackholed = matches!(resp, Some(TransportQueryResponse::Ok));
        if blackholed {
            broadcast_blackhole_update(&state).await;
        }
    }

    state.emit_to_all("contacts_update", json!(contacts_list));
    state.emit_to_all(
        "contact_blocked",
        json!({
            "ok": true,
            "hash": dest_hash,
            "display_name": display_name,
            "blackholed": blackholed,
        }),
    );
    state.emit_to_all("peer_removed", json!({ "hash": dest_hash }));
    crate::commands::messaging::broadcast_conversations(Arc::clone(&state));
    Ok(json!({
        "hash": dest_hash,
        "display_name": display_name,
        "blackholed": blackholed,
    }))
}

#[derive(Deserialize)]
pub struct UnblockContactArgs {
    pub hash: String,
    /// Also lift transport-layer blackhole.
    #[serde(default)]
    pub also_remove_blackhole: bool,
}

#[tauri::command]
pub async fn unblock_contact(
    state: State<'_, Arc<AppState>>,
    args: UnblockContactArgs,
) -> AppResult<Value> {
    let dest_hash = sanitize_text(&args.hash, 128);
    if !validate_hex(&dest_hash, 16, 64) {
        return Err(AppError::bad_request("Invalid hash for unblocking."));
    }

    let identity_id = active_identity_id(&state);
    let dh = dest_hash.clone();
    let id_c = identity_id.clone();
    let contacts_list = db::spawn_db(state.db.clone(), move |p| {
        let conn = match p.get() {
            Ok(c) => c,
            Err(_) => return Vec::<Value>::new(),
        };
        conn.execute(
            "DELETE FROM blocked_contacts WHERE dest_hash = ?1 AND identity_id = ?2",
            rusqlite::params![dh, id_c],
        )
        .ok();
        let contacts = db::get_all_contacts_conn(&conn, &id_c);
        format_contacts_list(&contacts)
    })
    .await
    .map_err(|_| AppError::internal("unblock_contact db task panicked"))?;

    let mut unblackholed = false;
    if args.also_remove_blackhole
        && let Some(hash_bytes) = hex_to_array16(&dest_hash)
    {
        use rns_transport::messages::{TransportQuery, TransportQueryResponse};
        let resp = transport_query(
            &state,
            TransportQuery::UnblackholeIdentity { hash: hash_bytes },
        )
        .await;
        unblackholed = matches!(resp, Some(TransportQueryResponse::BoolResult(true)));
        if unblackholed {
            broadcast_blackhole_update(&state).await;
        }
    }

    state.emit_to_all("contacts_update", json!(contacts_list));
    state.emit_to_all(
        "contact_unblocked",
        json!({
            "ok": true,
            "hash": dest_hash,
            "unblackholed": unblackholed,
        }),
    );
    emit_peer_delta_for(&state, &dest_hash).await;
    crate::commands::messaging::broadcast_conversations(Arc::clone(&state));
    Ok(json!({ "hash": dest_hash, "unblackholed": unblackholed }))
}

/// Same shape as `blackhole_update` broadcast.
#[tauri::command]
pub async fn get_blackhole(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let entries = snapshot_blackhole(&state).await;
    Ok(json!({ "entries": entries }))
}

/// Flushes every entry whose reason is not `Manual`.
#[tauri::command]
pub async fn clear_system_blackholes(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    use rns_transport::messages::{TransportQuery, TransportQueryResponse};
    let resp = transport_query(&state, TransportQuery::ClearSystemBlackholes).await;
    let cleared = match resp {
        Some(TransportQueryResponse::IntResult(n)) => n,
        _ => 0,
    };
    if cleared > 0 {
        broadcast_blackhole_update(&state).await;
    }
    Ok(json!({ "cleared": cleared }))
}

#[tauri::command]
pub async fn check_contact_status(state: State<'_, Arc<AppState>>) -> AppResult<Value> {
    let identity_id = active_identity_id(&state);
    let known_hashes = state
        .known_path_hashes
        .lock()
        .map(|h| h.clone())
        .unwrap_or_default();
    let st: Arc<AppState> = Arc::clone(&state);
    let id_c = identity_id.clone();
    let status = tokio::task::spawn_blocking(move || {
        if let Ok(lxmf) = st.lxmf.lock() {
            lxmf.as_ref()
                .map(|mgr| mgr.check_contacts_identity_status(&st.db, &id_c, &known_hashes))
        } else {
            None
        }
    })
    .await
    .unwrap_or(None);
    Ok(status.unwrap_or(json!({})))
}

/// Returns empty if transport unreachable.
async fn current_blackholed_set(state: &AppState) -> std::collections::HashSet<String> {
    use rns_transport::messages::{TransportMessage, TransportQuery, TransportQueryResponse};
    let tx = match state
        .rns
        .read()
        .ok()
        .and_then(|r| r.as_ref().map(|mgr| mgr.handle.transport_tx.clone()))
    {
        Some(t) => t,
        None => return std::collections::HashSet::new(),
    };
    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
    if tx
        .send(TransportMessage::Rpc {
            query: TransportQuery::GetBlackholedIdentities,
            response_tx: resp_tx,
        })
        .await
        .is_err()
    {
        return std::collections::HashSet::new();
    }
    match resp_rx.await {
        Ok(TransportQueryResponse::BlackholeList(entries)) => entries
            .into_iter()
            .map(|e| rns_crypto::hex_encode(&e.identity_hash))
            .collect(),
        _ => std::collections::HashSet::new(),
    }
}
