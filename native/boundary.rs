//! Current renderer adaptations, outside the preserved v2 implementation.
use serde_json::Value;
use tezbar_extension_sdk::CoreResult;

fn clear_in_rows(node: &mut Value, clear: &Value) {
    if node["type"] == "List.Item" {
        if let Some(ids) = node["props"]["actionIds"].as_array_mut() {
            if !ids.contains(clear) {
                ids.push(clear.clone());
            }
        }
    }
    if let Some(children) = node["children"].as_array_mut() {
        for child in children {
            clear_in_rows(child, clear);
        }
    }
}
fn omit_pixel_value(node: &mut Value) {
    if node["type"] == "Form.TextField" && node["props"]["id"] == "textfield" {
        if let Some(props) = node["props"].as_object_mut() {
            props.remove("value");
        }
    }
    if let Some(children) = node["children"].as_array_mut() {
        for child in children {
            omit_pixel_value(child);
        }
    }
}

pub fn adapt(mut result: Value, operation: &str, state: &mut Value) -> CoreResult {
    // invoke-action currently consumes every non-no-view success as a full view.
    // Keep v2's omitted-value reconciliation so queued keystrokes stay local.
    if result["mode"] == "unchanged" && operation == "extension:invoke-action" {
        result = state
            .get("rendererView")
            .cloned()
            .ok_or("Missing renderer view")?;
        omit_pixel_value(&mut result["root"]);
    }
    if result["mode"] == "view" {
        if let Some(clear) = result["actions"].as_array().and_then(|actions| {
            actions
                .iter()
                .find(|a| a["title"] == "Clear History")
                .map(|a| a["id"].clone())
        }) {
            clear_in_rows(&mut result["root"], &clear);
        }
        state["rendererView"] = result.clone();
    }
    Ok(result)
}
