//! Background pruning of stale entries from `LxmfManager::known_identities`.
//! Two passes per sweep: time-based (configurable, off when `prune_days = 0`)
//! and cap-based (always on, evicts oldest beyond `CAP_HARD_FLOOR_DAYS` once
//! over `SOFT_CAP_IDENTITIES`). Protection set: contacts, blocked contacts,
//! message peers, propagation_node identities, discovered PN cache.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::db;
use crate::state::AppState;

/// Run one sweep (time pass + cap pass). Returns `(pruned, kept)`.
pub async fn sweep_once(state: Arc<AppState>) -> (usize, usize) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let protected_extra: std::collections::HashSet<String> = state
        .discovered_propagation_nodes
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    let mut total_pruned = 0usize;
    let mut kept_count = 0usize;

    // Pass 1: time-based.
    let prune_days_opt = db::spawn_db(state.db.clone(), |p| db::get_prune_days(&p))
        .await
        .ok()
        .flatten();
    if let Some(prune_days) = prune_days_opt {
        let cutoff = now - (prune_days as f64) * 86_400.0;
        let victims = {
            let protected_extra = protected_extra.clone();
            db::spawn_db(state.db.clone(), move |p| {
                db::find_prune_candidates(&p, cutoff, &protected_extra)
            })
            .await
            .unwrap_or_default()
        };
        if victims.is_empty() {
            tracing::debug!(
                prune_days,
                "identity prune sweep: no stale non-protected identities"
            );
        } else {
            let (pruned, kept) = apply_eviction(&state, victims, "time-based identity prune").await;
            total_pruned += pruned;
            kept_count = kept;
        }
    } else {
        tracing::debug!("time-based identity pruning disabled — cap pass only");
    }

    // Pass 2: cap-based.
    let current_len = {
        state
            .lxmf
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|m| m.known_identities.len()))
    };
    if let Some(current_len) = current_len {
        kept_count = current_len;
        if current_len > db::SOFT_CAP_IDENTITIES {
            let overflow = current_len - db::SOFT_CAP_IDENTITIES;
            let cap_cutoff = now - (db::CAP_HARD_FLOOR_DAYS as f64) * 86_400.0;
            let cap_victims = {
                let protected_extra = protected_extra.clone();
                db::spawn_db(state.db.clone(), move |p| {
                    db::find_cap_eviction_candidates(&p, cap_cutoff, overflow, &protected_extra)
                })
                .await
                .unwrap_or_default()
            };
            if cap_victims.is_empty() {
                tracing::warn!(
                    current = current_len,
                    soft_cap = db::SOFT_CAP_IDENTITIES,
                    floor_days = db::CAP_HARD_FLOOR_DAYS,
                    "known_identities above soft cap but all overflow is within the recency floor — no cap eviction this pass"
                );
            } else {
                let (pruned, kept) =
                    apply_eviction(&state, cap_victims, "cap-based identity eviction").await;
                total_pruned += pruned;
                kept_count = kept;
            }
        }
    }

    if total_pruned > 0 {
        state.emit_to_all(
            "identity_prune_completed",
            json!({
                "pruned":       total_pruned,
                "kept":         kept_count,
                "cutoff_days":  prune_days_opt,
                "timestamp":    now,
            }),
        );
    } else {
        tracing::debug!(
            kept = kept_count,
            cutoff_days = ?prune_days_opt,
            soft_cap = db::SOFT_CAP_IDENTITIES,
            "identity prune sweep: nothing to prune"
        );
    }

    (total_pruned, kept_count)
}

/// Retain-filter the in-memory map, rewrite the disk file, then delete the
/// matching `identity_activity` rows. Disk rewrite MUST commit before DB
/// delete: reverse order could strand a peer with no activity row and make
/// them appear freshly-seen forever.
async fn apply_eviction(
    state: &Arc<AppState>,
    victims: Vec<String>,
    label: &'static str,
) -> (usize, usize) {
    let victims_set: std::collections::HashSet<String> = victims.iter().cloned().collect();

    let (removed_from_map, kept_count) = {
        let mut lxmf = match state.lxmf.lock() {
            Ok(g) => g,
            Err(_) => return (0, 0),
        };
        let mgr = match lxmf.as_mut() {
            Some(m) => m,
            None => return (0, 0),
        };
        let before = mgr.known_identities.len();
        mgr.known_identities
            .retain(|hash_hex, _| !victims_set.contains(hash_hex));
        let after = mgr.known_identities.len();
        mgr.save_crypto_state();
        (before - after, after)
    };

    let removed_rows = db::spawn_db(state.db.clone(), move |p| {
        db::delete_identity_activity(&p, &victims)
    })
    .await
    .unwrap_or(0);

    for hash in &victims_set {
        state.emit_to_all("peer_removed", json!({ "hash": hash }));
    }

    tracing::info!(
        pass = label,
        pruned = removed_from_map,
        kept = kept_count,
        removed_rows,
        "identity prune pass complete"
    );

    (removed_from_map, kept_count)
}

/// Spawn the background sweeper: one post-ready cleanup, then a 24h tick.
/// Spawn after `set_startup_stage("ready")`.
pub fn spawn_scheduler(state: Arc<AppState>, shutdown: rns_runtime::lifecycle::ShutdownSignal) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;

        if shutdown.is_triggered() {
            return;
        }
        sweep_once(state.clone()).await;

        let mut ticker = tokio::time::interval(Duration::from_secs(24 * 3600));
        ticker.tick().await; // discard immediate first tick

        loop {
            tokio::select! {
                _ = shutdown.wait() => break,
                _ = ticker.tick() => {
                    sweep_once(state.clone()).await;
                }
            }
        }
    });
}
