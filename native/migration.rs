//! Read only explicitly supplied Tezbar JS Cache files, never discover user paths.
use serde_json::{json, Value};
use tezbar_convert_px_to_vw_vh::model::History;
use tezbar_extension_sdk::{read_store, update_store, CoreResult, ExtensionContext};

const STORE: &str = "convert-px-to-vw-vh";
const MARKER: &str = "tezbarJsCacheMigration";
pub const KEYS: &[&str] = &["jsCacheFlat", "jsCacheManaged", "jsCacheRegistry"];

pub async fn initialize(ctx: &ExtensionContext) -> CoreResult {
    let saved = read_store(ctx, STORE).await?;
    let native = History::read(&saved)?;
    // A recorded revision, even an intentionally cleared history, owns its data.
    if saved.get(MARKER).is_some() || native.revision != 0 || !saved.as_object().unwrap().is_empty()
    {
        return Ok(json!({"ready":true}));
    }
    let mut imported = None;
    for key in KEYS {
        let manifest_key = key.replace("jsCache", "jsPackage");
        let Some(manifest_path) = ctx.migration_paths.get(&manifest_key).cloned() else {
            continue;
        };
        // Match registry::collect: parseable JSON object, first identity wins.
        let manifest = tokio::task::spawn_blocking(move || std::fs::read(manifest_path))
            .await
            .map_err(|e| e.to_string())?;
        if !manifest
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|v| v.is_object())
        {
            continue;
        }
        let Some(path) = ctx.migration_paths.get(*key).cloned() else {
            break; // Selected owner has no cache; never import an inactive copy.
        };
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("Legacy Cache read failed: {e}"))?;
        let cache: Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid legacy Cache: {e}"))?;
        let cache = cache.as_object().ok_or("Legacy Cache must be an object")?;
        let Some(history) = cache.get("history") else {
            break;
        };
        let items: Value = serde_json::from_str(
            history
                .as_str()
                .ok_or("Legacy history must be a JSON string")?,
        )
        .map_err(|e| format!("Invalid legacy history: {e}"))?;
        let items = items.as_array().ok_or("Legacy history must be an array")?;
        let count = items.len().min(10);
        let mut entries = Vec::new();
        // Validate the entire source, preserving order and duplicate values.
        for (index, item) in items.iter().enumerate() {
            let entry = json!({"id":items.len()-index,"date":item["date"],"value":item["data"]["value"],"label":item["data"]["label"]});
            History::read(&json!({"historyRevision":items.len(),"history":[entry.clone()]}))?;
            if index < count {
                entries.push(entry);
            }
        }
        let candidate = json!({"historyRevision":items.len(),"history":entries});
        History::read(&candidate)?;
        imported = Some(candidate);
        break; // host flat, managed, registry priority; never merge caches
    }
    // Marker and imported data commit together. A failed write can safely retry.
    update_store(ctx, STORE, move |value| {
        let current = History::read(value)?;
        if value.get(MARKER).is_none()
            && current.revision == 0
            && current.entries.is_empty()
            && value.as_object().unwrap().is_empty()
        {
            if let Some(imported) = imported {
                value["historyRevision"] = imported["historyRevision"].clone();
                value["history"] = imported["history"].clone();
            }
            value[MARKER] = json!(1);
        }
        Ok(())
    })
    .await?;
    Ok(json!({"ready":true}))
}
