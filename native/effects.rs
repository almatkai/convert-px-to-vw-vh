//! Per-session ledger of irreversible effect progress.
//!
//! The SDK stages adapter state and discards it when a call fails. The Native
//! V2 adapter records completed effects (history saved, clipboard copied,
//! window hidden, copied history action) in that state so that retrying the
//! same action resumes only the incomplete stage. Discarding them on failure
//! would make a retry repeat an already-acknowledged history write or copy.
//! This ledger keeps exactly those progress fields across a failed call. It
//! never stores domain data, is keyed by the host's canonical session ID and is
//! cleared on dispose, on a fresh launch and when a launch fails.
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// Adapter state keys that describe effects which already happened.
const PROGRESS: &[&str] = &["pending", "copiedHistoryAction"];
const RUN: &str = "extension:run-command";
const DISPOSE: &str = "extension:dispose-session";

fn ledger() -> &'static Mutex<HashMap<String, serde_json::Map<String, Value>>> {
    static LEDGER: OnceLock<Mutex<HashMap<String, serde_json::Map<String, Value>>>> =
        OnceLock::new();
    LEDGER.get_or_init(Mutex::default)
}

/// Restore progress recorded by an earlier failed call of this session.
pub fn before(session: &str, operation: &str, state: &mut Value) {
    let mut ledger = ledger().lock().unwrap_or_else(|e| e.into_inner());
    if operation == RUN || operation == DISPOSE || session.is_empty() {
        ledger.remove(session);
        return;
    }
    let (Some(saved), Some(object)) = (ledger.get(session), state.as_object_mut()) else {
        return;
    };
    for key in PROGRESS {
        match saved.get(*key) {
            Some(value) => {
                object.insert((*key).into(), value.clone());
            }
            None => {
                object.remove(*key);
            }
        }
    }
}

/// Record the progress reached by this call, whether it succeeded or failed.
pub fn after(session: &str, operation: &str, state: &Value, ok: bool) {
    let mut ledger = ledger().lock().unwrap_or_else(|e| e.into_inner());
    if session.is_empty() || operation == DISPOSE || (operation == RUN && !ok) {
        // A failed launch is removed by the SDK and is never retried.
        ledger.remove(session);
        return;
    }
    let progress: serde_json::Map<String, Value> = PROGRESS
        .iter()
        .filter_map(|key| state.get(*key).map(|v| ((*key).to_string(), v.clone())))
        .collect();
    if progress.is_empty() {
        ledger.remove(session);
    } else {
        ledger.insert(session.into(), progress);
    }
}

#[cfg(test)]
pub fn len() -> usize {
    ledger().lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failed_call_progress_is_restored_and_cleared() {
        let id = "ledger-unit";
        // Failed submit after the history write: staged state is discarded by
        // the SDK, but the ledger restores the saved stage for the retry.
        after(
            id,
            "extension:invoke-action",
            &json!({"pending":{"saved":true},"x":1}),
            false,
        );
        let mut committed = json!({"submit":"px:9"});
        before(id, "extension:invoke-action", &mut committed);
        assert_eq!(committed["pending"]["saved"], true);
        assert_eq!(committed["submit"], "px:9");
        // A successful call that dropped progress clears the entry; committed
        // state is then authoritative and left untouched.
        after(id, "extension:refresh-session", &json!({"x":1}), true);
        let mut later = json!({"submit":"px:10"});
        before(id, "extension:invoke-action", &mut later);
        assert_eq!(later, json!({"submit":"px:10"}));
        // A recorded entry overrides a stale committed progress field.
        after(
            id,
            "extension:invoke-action",
            &json!({"pending":{"copied":true}}),
            false,
        );
        let mut stale = json!({"pending":{"copied":false},"copiedHistoryAction":"px:1"});
        before(id, "extension:invoke-action", &mut stale);
        assert_eq!(stale, json!({"pending":{"copied":true}}));
        after(
            id,
            "extension:invoke-action",
            &json!({"copiedHistoryAction":"px:2"}),
            false,
        );
        let mut disposed = json!({});
        before(id, DISPOSE, &mut disposed);
        assert!(disposed.get("copiedHistoryAction").is_none());
        let before_len = len();
        after(id, RUN, &json!({"pending":{}}), false);
        assert!(len() <= before_len);
    }
}
