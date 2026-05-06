//! Cross-command helpers: transport RPC, interface progress, game persistence,
//! BLE teardown, JSON→MessagePack. All `pub(crate)`.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::db;
use crate::lxmf::resolve_destination;
use crate::state::AppState;

pub(crate) fn transport_sender(
    state: &AppState,
) -> Option<tokio::sync::mpsc::Sender<rns_transport::messages::TransportMessage>> {
    state
        .rns
        .read()
        .ok()
        .and_then(|r| r.as_ref().map(|mgr| mgr.handle.transport_tx.clone()))
}

pub(crate) fn remove_stored_file_refs(
    files_dir: &Path,
    file_refs: impl IntoIterator<Item = String>,
) {
    for file_ref in file_refs {
        if file_ref.is_empty() {
            continue;
        }
        let sanitized: String = file_ref
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_' || *c == ' ')
            .take(240)
            .collect();
        if sanitized != file_ref {
            tracing::warn!(stored_name = %file_ref, "skipping unsafe stored attachment path");
            continue;
        }
        std::fs::remove_file(files_dir.join(sanitized)).ok();
    }
}

pub(crate) async fn transport_query(
    state: &AppState,
    query: rns_transport::messages::TransportQuery,
) -> Option<rns_transport::messages::TransportQueryResponse> {
    let tx = transport_sender(state)?;
    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
    tx.send(rns_transport::messages::TransportMessage::Rpc {
        query,
        response_tx: resp_tx,
    })
    .await
    .ok()?;
    resp_rx.await.ok()
}

pub(crate) fn blackhole_reason_display(
    reason: rns_transport::blackhole::BlackholeReason,
    reason_label: Option<&str>,
) -> String {
    reason_label.unwrap_or_else(|| reason.as_str()).to_string()
}

// Each entry: `hash`, `reason`, `created`, `expires_in` (null = permanent).
pub(crate) async fn snapshot_blackhole(state: &AppState) -> Vec<Value> {
    use rns_transport::messages::{TransportQuery, TransportQueryResponse};
    let entries = match transport_query(state, TransportQuery::GetBlackholedIdentities).await {
        Some(TransportQueryResponse::BlackholeList(v)) => v,
        _ => return Vec::new(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    entries
        .into_iter()
        .map(|e| {
            let expires_in = e.ttl.map(|t| (e.created + t - now).max(0.0));
            let reason = blackhole_reason_display(e.reason, e.reason_label.as_deref());
            json!({
                "hash": rns_crypto::hex_encode(&e.identity_hash),
                "reason": reason,
                "created": e.created,
                "expires_in": expires_in,
            })
        })
        .collect()
}

/// Broadcast `blackhole_update` after any mutation.
pub(crate) async fn broadcast_blackhole_update(state: &AppState) {
    let entries = snapshot_blackhole(state).await;
    state.emit_to_all("blackhole_update", json!({ "entries": entries }));
}

fn config_transport_enabled(state: &AppState) -> bool {
    crate::rns_config::read_config(&state.config.rns_config_dir)
        .and_then(|content| {
            content.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                if key.trim().eq_ignore_ascii_case("enable_transport") {
                    Some(matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "true" | "yes" | "1" | "on"
                    ))
                } else {
                    None
                }
            })
        })
        .unwrap_or(false)
}

pub(crate) fn hub_interfaces_payload(state: &AppState, mut ifaces: Value) -> Value {
    let mode = db::get_setting(&state.db, "transport_mode").unwrap_or_else(|| "off".to_string());
    let configured_enabled = config_transport_enabled(state);
    let suppressed = configured_enabled
        && state
            .rns
            .read()
            .ok()
            .and_then(|r| r.as_ref().map(|mgr| mgr.handle.instance_mode))
            .is_some_and(|mode| mode == rns_runtime::reticulum::InstanceMode::Client);
    let enabled = configured_enabled && !suppressed;

    if let Some(obj) = ifaces.as_object_mut() {
        obj.insert(
            "transport".to_string(),
            json!({
                "mode": mode,
                "enabled": enabled,
                "configured_enabled": configured_enabled,
                "suppressed": suppressed,
            }),
        );
    }
    ifaces
}

pub(crate) fn format_contacts_list(contacts: &[Value]) -> Vec<Value> {
    contacts
        .iter()
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
        .collect()
}

pub(crate) fn emit_hub_interfaces(state: &AppState, ifaces: serde_json::Value) {
    crate::commands::interfaces::reconcile_auto_transport_after_interface_change(state, &ifaces);
    let ifaces = hub_interfaces_payload(state, ifaces);
    state.set_last_hub_interfaces(ifaces.clone());
    state.emit_to_all("hub_interfaces_update", ifaces);
}

// Extracts transport_tx then calls resolve_destination outside the lock
// (clippy::await_holding_lock). Failure does not block sending.
pub(crate) async fn resolve_before_send(state: &AppState, dest_hash: &str) {
    let transport_tx = {
        if let Ok(lxmf) = state.lxmf.lock() {
            lxmf.as_ref()
                .and_then(|mgr| mgr.router.transport_tx.clone())
        } else {
            None
        }
    };

    if let Some(tx) = transport_tx
        && !resolve_destination(state, dest_hash, &tx).await
    {
        tracing::warn!(dest = %dest_hash, "could not resolve destination, sending anyway");
    }
}

pub(crate) fn hex_to_array16(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        let hi = u8::from_str_radix(&s[i * 2..i * 2 + 1], 16).ok()?;
        let lo = u8::from_str_radix(&s[i * 2 + 1..i * 2 + 2], 16).ok()?;
        *byte = (hi << 4) | lo;
    }
    Some(out)
}

pub(crate) fn json_to_rmpv_map(v: &Value) -> std::collections::HashMap<String, rmpv::Value> {
    let mut map = std::collections::HashMap::new();
    if let Some(obj) = v.as_object() {
        for (key, val) in obj {
            map.insert(key.clone(), json_to_rmpv(val));
        }
    }
    map
}

fn json_to_rmpv(v: &Value) -> rmpv::Value {
    match v {
        Value::Null => rmpv::Value::Nil,
        Value::Bool(b) => rmpv::Value::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rmpv::Value::Integer(i.into())
            } else if let Some(u) = n.as_u64() {
                rmpv::Value::Integer(u.into())
            } else if let Some(f) = n.as_f64() {
                rmpv::Value::F64(f)
            } else {
                rmpv::Value::Nil
            }
        }
        Value::String(s) => rmpv::Value::String(s.as_str().into()),
        Value::Array(arr) => rmpv::Value::Array(arr.iter().map(json_to_rmpv).collect()),
        Value::Object(obj) => {
            let pairs: Vec<(rmpv::Value, rmpv::Value)> = obj
                .iter()
                .map(|(k, v)| (rmpv::Value::String(k.as_str().into()), json_to_rmpv(v)))
                .collect();
            rmpv::Value::Map(pairs)
        }
    }
}

/// `delivery_state = Some` stamps metadata; `None` preserves existing.
pub(crate) async fn save_session_from_state(
    state: &AppState,
    session_id: &str,
    identity_id: &str,
    app_id: &str,
    contact_hash: &str,
    session_state: &std::collections::HashMap<String, serde_json::Value>,
    delivery_state: Option<&str>,
) {
    // Empty session_id is unaddressable; bail loudly.
    if session_id.is_empty() {
        tracing::warn!(
            target: "ttt_trace",
            step = "save_session.empty_sid_rejected",
            app_id = %app_id,
            identity_id = %identity_id,
            contact_hash = %contact_hash,
            delivery_state = ?delivery_state,
            "refusing to persist app_session with empty session_id"
        );
        return;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let status = session_state
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");
    let initiator = session_state
        .get("initiator")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Unwrap nested "metadata" so DB column has flat fields.
    let mut metadata_map: std::collections::HashMap<String, serde_json::Value> = session_state
        .get("metadata")
        .and_then(|v| v.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    if let Some(ds) = delivery_state {
        metadata_map.insert("delivery_state".to_string(), json!(ds));
    }

    let session = lrgp::session::Session {
        session_id: session_id.to_string(),
        identity_id: identity_id.to_string(),
        app_id: app_id.to_string(),
        app_version: 1,
        contact_hash: contact_hash.to_string(),
        initiator: initiator.to_string(),
        status: status.to_string(),
        metadata: metadata_map,
        unread: 0,
        created_at: session_state
            .get("created_at")
            .and_then(|v| v.as_f64())
            .unwrap_or(now),
        updated_at: now,
        last_action_at: now,
    };
    let _ = db::spawn_db(state.db.clone(), move |p| {
        db::save_game_session(&p, &session);
    })
    .await;
}

pub(crate) async fn emit_game_sessions(
    state: &AppState,
    identity_id: &str,
    contact_hash: Option<&str>,
) {
    let id_c = identity_id.to_string();
    let ch_c = contact_hash.map(|s| s.to_string());
    let (per_contact, all) = db::spawn_db(state.db.clone(), move |p| {
        let per = ch_c
            .as_deref()
            .map(|ch| db::list_game_sessions(&p, &id_c, Some(ch), None));
        let all = db::list_game_sessions(&p, &id_c, None, None);
        (per, all)
    })
    .await
    .expect("db task panicked");

    if let (Some(sessions), Some(ch)) = (per_contact, contact_hash) {
        state.emit_to_all("active_games", json!({ "hash": ch, "games": sessions }));
    }
    state.emit_to_all("all_game_sessions", json!(all));
}

pub(crate) fn emit_op_status_broadcast(
    state: &AppState,
    operation: &str,
    node: &str,
    step: &str,
    done: bool,
    error: Option<&str>,
) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    state.emit_to_all(
        "node_operation_status",
        json!({
            "operation": operation,
            "node": node,
            "step": step,
            "done": done,
            "error": error,
            "timestamp": ts,
        }),
    );
}

pub(crate) async fn disable_ble_peer_inner(state: &Arc<AppState>) {
    tracing::info!("disable_ble_peer_inner: start");
    let _ = db::spawn_db(state.db.clone(), |p| {
        db::set_setting(&p, "ble_peer_enabled", "0");
    })
    .await;
    state.emit_to_all("ble_peer_status_update", json!({ "enabled": false }));
    state
        .ble_peer_count
        .store(0, std::sync::atomic::Ordering::Relaxed);

    let rns_handle = {
        state
            .rns
            .read()
            .ok()
            .and_then(|r| r.as_ref().map(|mgr| mgr.handle.clone()))
    };
    if let Some(handle) = rns_handle {
        let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
        if handle
            .transport_tx
            .send(rns_transport::messages::TransportMessage::Rpc {
                query: rns_transport::messages::TransportQuery::GetInterfaceStats,
                response_tx: resp_tx,
            })
            .await
            .is_ok()
            && let Ok(rns_transport::messages::TransportQueryResponse::InterfaceStats(stats)) =
                resp_rx.await
        {
            #[cfg(feature = "ble")]
            let mut torn_down = false;
            let iface_count = stats.len();
            tracing::info!(
                iface_count,
                "disable_ble_peer_inner: searching for Bluetooth Peer interface"
            );
            for iface in stats {
                if iface.name == "Bluetooth Peer" || iface.name == "BLE Mesh" {
                    tracing::info!(
                        id = iface.id,
                        "disable_ble_peer_inner: tearing down Bluetooth Peer interface"
                    );
                    #[cfg(feature = "ble")]
                    {
                        rns_runtime::reticulum::teardown_ble_peer_interface(&handle, iface.id)
                            .await;
                        torn_down = true;
                    }
                    #[cfg(not(feature = "ble"))]
                    {
                        rns_runtime::reticulum::teardown_interface(&handle, iface.id).await;
                    }
                    break;
                }
            }

            #[cfg(feature = "ble")]
            if !torn_down {
                tracing::info!(
                    "disable_ble_peer_inner: no live interface, forcing stop_ble_peer_interface"
                );
                rns_interface::ble_peer::stop_ble_peer_interface().await;
            }
        } else {
            tracing::warn!(
                "disable_ble_peer_inner: failed to query interface stats, forcing stop_ble_peer_interface"
            );
            #[cfg(feature = "ble")]
            rns_interface::ble_peer::stop_ble_peer_interface().await;
        }
    } else {
        tracing::info!("disable_ble_peer_inner: no RNS runtime, clearing BLE state");
        #[cfg(feature = "ble")]
        rns_interface::ble_peer::stop_ble_peer_interface().await;
    }
    tracing::info!("disable_ble_peer_inner: done");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rns_transport::blackhole::BlackholeReason;

    #[test]
    fn blackhole_reason_display_prefers_custom_label() {
        assert_eq!(
            blackhole_reason_display(BlackholeReason::Manual, Some("operator note")),
            "operator note"
        );
        assert_eq!(
            blackhole_reason_display(BlackholeReason::RateLimit, None),
            "rate_limit"
        );
    }
}
