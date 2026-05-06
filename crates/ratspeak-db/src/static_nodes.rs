//! Bundled static propagation nodes from `nodes.json` (compile-time
//! `include_str!`, no runtime fetch).

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct StaticPropNode {
    pub hash: [u8; 16],
    pub display_name: String,
    pub region: Option<String>,
}

/// On-disk shape: `hash` is a 32-char hex string for the 16-byte truncated
/// destination hash.
#[derive(Debug, Deserialize)]
struct RawStaticPropNode {
    hash: String,
    display_name: String,
    #[serde(default)]
    region: Option<String>,
}

const NODES_JSON: &str = include_str!("../nodes.json");
// TODO(release): populate `nodes.json` with Ratspeak-operated propagation
// nodes before shipping relay Auto mode as an out-of-box feature.

static NODES: OnceLock<Vec<StaticPropNode>> = OnceLock::new();
static HASHES: OnceLock<HashSet<[u8; 16]>> = OnceLock::new();

/// Parsed once, lazily. Malformed JSON → warn + empty list.
pub fn load() -> &'static Vec<StaticPropNode> {
    NODES.get_or_init(parse_nodes_json)
}

pub fn hash_set() -> &'static HashSet<[u8; 16]> {
    HASHES.get_or_init(|| load().iter().map(|n| n.hash).collect())
}

fn parse_nodes_json() -> Vec<StaticPropNode> {
    let raw: Vec<RawStaticPropNode> = match serde_json::from_str(NODES_JSON) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "static nodes.json failed to parse; bundled list is empty for this session"
            );
            return Vec::new();
        }
    };

    raw.into_iter()
        .filter_map(|r| {
            let bytes = match hex::decode(&r.hash) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(hash = %r.hash, error = %e, "static node hash is not valid hex; skipping");
                    return None;
                }
            };
            if bytes.len() != 16 {
                tracing::warn!(
                    hash = %r.hash,
                    bytes = bytes.len(),
                    "static node hash is not 16 bytes; skipping"
                );
                return None;
            }
            let mut hash = [0u8; 16];
            hash.copy_from_slice(&bytes);
            Some(StaticPropNode {
                hash,
                display_name: r.display_name,
                region: r.region,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_json_parses_at_compile_time() {
        let _ = parse_nodes_json();
    }

    #[test]
    fn empty_bundle_returns_empty_list() {
        assert!(load().is_empty(), "bundle should be empty pre-launch");
        assert!(hash_set().is_empty());
    }

    #[test]
    fn malformed_hash_skipped() {
        let raw = r#"[{"hash":"not-hex","display_name":"Bad"}]"#;
        let parsed: Vec<RawStaticPropNode> = serde_json::from_str(raw).unwrap();
        let nodes: Vec<StaticPropNode> = parsed
            .into_iter()
            .filter_map(|r| {
                let bytes = hex::decode(&r.hash).ok()?;
                if bytes.len() != 16 {
                    return None;
                }
                let mut h = [0u8; 16];
                h.copy_from_slice(&bytes);
                Some(StaticPropNode {
                    hash: h,
                    display_name: r.display_name,
                    region: r.region,
                })
            })
            .collect();
        assert!(nodes.is_empty());
    }
}
