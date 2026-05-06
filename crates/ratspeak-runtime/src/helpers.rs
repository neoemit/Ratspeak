use crate::db;
use crate::state::AppState;

pub fn validate_hex(value: &str, min_len: usize, max_len: usize) -> bool {
    if value.len() < min_len || value.len() > max_len {
        return false;
    }
    value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn sanitize_text(value: &str, max_len: usize) -> String {
    value
        .chars()
        .take(max_len)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn active_identity_id(state: &AppState) -> String {
    db::get_active_identity(&state.db)
        .and_then(|id| id.get("hash").and_then(|h| h.as_str()).map(String::from))
        .unwrap_or_default()
}

pub fn active_lxmf_hash(state: &AppState) -> String {
    db::get_active_identity(&state.db)
        .and_then(|id| {
            id.get("lxmf_hash")
                .and_then(|h| h.as_str())
                .map(String::from)
        })
        .unwrap_or_default()
}
