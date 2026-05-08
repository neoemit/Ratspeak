//! Per-aspect announce handlers (`lxmf.delivery`, `lxmf.propagation`).
//! Cross-cutting announce work (history, crypto cache, contact-name refresh)
//! still runs in the poll loop.
use std::sync::Arc;
use std::time::Duration;

use rns_runtime::lifecycle::ShutdownSignal;
use rns_transport::messages::{AnnounceHandlerEvent, TransportMessage};
use serde_json::json;
use tokio::sync::mpsc;

use crate::db;
use crate::state::AppState;

const HANDLER_CHANNEL_CAP: usize = 64;
const REGISTER_ATTEMPTS: u32 = 3;
const REGISTER_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Register the lxmf.delivery handler and spawn the per-event processor.
pub async fn spawn_lxmf_delivery_handler(
    state: Arc<AppState>,
    transport_tx: mpsc::Sender<TransportMessage>,
    shutdown: ShutdownSignal,
) {
    let (htx, mut hrx) = mpsc::channel::<AnnounceHandlerEvent>(HANDLER_CHANNEL_CAP);
    if !register_with_retry(&transport_tx, Some("lxmf.delivery".to_string()), htx).await {
        return;
    }

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.wait() => break,
                ev = hrx.recv() => match ev {
                    Some(event) => {
                        process_delivery_announce(&state, event).await;
                        state.request_poll_now();
                    }
                    None => break,
                },
            }
        }
    });
}

/// Register the lxmf.propagation handler and spawn the per-event processor.
pub async fn spawn_lxmf_propagation_handler(
    state: Arc<AppState>,
    transport_tx: mpsc::Sender<TransportMessage>,
    shutdown: ShutdownSignal,
) {
    let (htx, mut hrx) = mpsc::channel::<AnnounceHandlerEvent>(HANDLER_CHANNEL_CAP);
    if !register_with_retry(&transport_tx, Some("lxmf.propagation".to_string()), htx).await {
        return;
    }

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown.wait() => break,
                ev = hrx.recv() => match ev {
                    Some(event) => {
                        process_propagation_announce(&state, event).await;
                        state.request_poll_now();
                    }
                    None => break,
                },
            }
        }
    });
}

/// Send `RegisterAnnounceHandler` to the transport actor with retries to
/// tolerate the startup race before the actor is spawned.
async fn register_with_retry(
    transport_tx: &mpsc::Sender<TransportMessage>,
    aspect_filter: Option<String>,
    callback_tx: mpsc::Sender<AnnounceHandlerEvent>,
) -> bool {
    for attempt in 0..REGISTER_ATTEMPTS {
        let cb = callback_tx.clone();
        let filter = aspect_filter.clone();
        match transport_tx
            .send(TransportMessage::RegisterAnnounceHandler {
                aspect_filter: filter,
                receive_path_responses: false,
                callback_tx: cb,
            })
            .await
        {
            Ok(()) => {
                tracing::debug!(
                    aspect = ?aspect_filter,
                    "announce-handler registered"
                );
                return true;
            }
            Err(e) => {
                tracing::warn!(
                    aspect = ?aspect_filter,
                    attempt = attempt + 1,
                    error = %e,
                    "announce-handler register failed; retrying"
                );
                tokio::time::sleep(REGISTER_RETRY_DELAY).await;
            }
        }
    }
    tracing::error!(
        aspect = ?aspect_filter,
        "announce-handler register: giving up after retries — aspect-driven updates disabled for this session"
    );
    false
}

/// `lxmf.delivery` per-event processing: activity tracking + peer batch emit.
async fn process_delivery_announce(state: &Arc<AppState>, event: AnnounceHandlerEvent) {
    // Pending-blackhole sweep: the announce already carries an identity hash
    // recovered from the validated payload, so we can escalate any queued
    // network-block requests for this dest immediately. No-op when nothing is
    // queued for this dest hash.
    if let Some(id_hash) = event.identity_hash {
        crate::blackhole::escalate_pending_if_present(state, event.destination_hash, id_hash).await;
    }

    let hash_hex = hex::encode(event.destination_hash);
    let display_name = event
        .app_data
        .as_ref()
        .map(|d| crate::extract_display_name(d))
        .filter(|s| !s.is_empty());

    let iface = lookup_path_iface(state, event.destination_hash).await;

    let activity = vec![(hash_hex.clone(), now_f64(), display_name, iface)];

    let pool = state.db.clone();
    let activity_owned = activity.clone();
    db::spawn_db(pool, move |p| {
        db::touch_identity_activity(&p, &activity_owned);
    })
    .await
    .expect("db task panicked");

    let pool = state.db.clone();
    let hashes = vec![hash_hex];
    let identity_id = crate::helpers::active_identity_id(state);
    let resolved = db::spawn_db(pool, move |p| {
        db::get_peers_by_hashes(&p, &hashes, &identity_id)
    })
    .await
    .unwrap_or_default();
    crate::emit_peers_batch(state, &resolved);
}

/// `lxmf.propagation` per-event processing. Drop on parse failure
/// (matches Python `LXMF.py:214`); preserve static badge + region when
/// upgrading an existing static-bundle entry.
async fn process_propagation_announce(state: &Arc<AppState>, event: AnnounceHandlerEvent) {
    use std::sync::atomic::Ordering;

    let hash_hex = hex::encode(event.destination_hash);
    let timestamp = now_f64();

    let pn = match event.app_data.as_ref() {
        None => {
            state.pn_parse_failures.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                dest = %hash_hex,
                reason = "no_app_data",
                "lxmf.propagation announce dropped: no app_data"
            );
            return;
        }
        Some(bytes) => match lxmf_core::handlers::parse_pn_announce_data(bytes) {
            Some(p) => p,
            None => {
                state.pn_parse_failures.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    dest = %hash_hex,
                    reason = "parse_failed",
                    app_data_len = bytes.len(),
                    "lxmf.propagation announce dropped: app_data did not parse as PN format"
                );
                return;
            }
        },
    };

    let display_name_from_announce = event
        .app_data
        .as_ref()
        .and_then(|d| lxmf_core::handlers::pn_name_from_app_data(d))
        .filter(|s| !s.is_empty());

    if let Ok(mut lxmf) = state.lxmf.lock()
        && let Some(mgr) = lxmf.as_mut()
    {
        mgr.router
            .set_stamp_cost(event.destination_hash, pn.stamp_cost);
    }

    let mut entry = json!({
        "hash": hash_hex,
        "hops": event.hops,
        "stamp_cost": pn.stamp_cost,
        "transfer_limit_kb": pn.transfer_limit,
        "last_seen": timestamp,
        "node_state": if pn.node_state { "enabled" } else { "disabled" },
    });

    let inserted = if let Ok(mut registry) = state.discovered_propagation_nodes.lock() {
        let key = hash_hex.clone();
        let existing = registry.get(&key).cloned();

        if let Some(obj) = entry.as_object_mut() {
            let preserved_static = existing
                .as_ref()
                .and_then(|v| v.get("static").and_then(|s| s.as_bool()))
                .unwrap_or(false);
            let preserved_region = existing
                .as_ref()
                .and_then(|v| v.get("region").cloned())
                .unwrap_or(serde_json::Value::Null);
            let preserved_name = existing
                .as_ref()
                .and_then(|v| v.get("display_name").and_then(|s| s.as_str()))
                .map(String::from);
            obj.insert("static".to_string(), json!(preserved_static));
            obj.insert("region".to_string(), preserved_region);
            let final_name = display_name_from_announce
                .clone()
                .or(preserved_name)
                .unwrap_or_else(|| format!("Relay {}", &hash_hex[..8.min(hash_hex.len())]));
            obj.insert("display_name".to_string(), json!(final_name));
        }

        registry.insert(key, entry);
        true
    } else {
        false
    };

    if inserted {
        crate::propagation::mark_relay_path_success(state, event.destination_hash);
        state.trim_propagation_nodes();
        crate::propagation::maybe_reselect_on_announce(state).await;
    }
}

async fn lookup_path_iface(state: &Arc<AppState>, dest: [u8; 16]) -> Option<String> {
    use rns_transport::messages::{TransportQuery, TransportQueryResponse};

    let tx = {
        let rns = state.rns.read().ok()?;
        rns.as_ref().map(|mgr| mgr.handle.transport_tx.clone())?
    };
    let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
    if tx
        .send(TransportMessage::Rpc {
            query: TransportQuery::GetPathTable,
            response_tx: resp_tx,
        })
        .await
        .is_err()
    {
        return None;
    }
    let entries = match resp_rx.await {
        Ok(TransportQueryResponse::PathTable(e)) => e,
        _ => return None,
    };
    entries.into_iter().find(|e| e.hash == dest).and_then(|e| {
        if e.interface.is_empty() {
            None
        } else {
            Some(e.interface)
        }
    })
}

fn now_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
