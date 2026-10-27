//! Production beta adapter: compile this unchanged inside native_core::extensions.
//! The standalone package contract test supplies byte-exact beta common/SDK modules.
use crate::native_core::common::{CoreContext, CoreResult};
use crate::native_core::extensions::v2_sdk;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tezbar_convert_px_to_vw_vh::{
    model::{self, Conversion, History},
    presets,
};

pub const SLUG: &str = "convert-px-to-vw-vh";
static NEXT: AtomicU64 = AtomicU64::new(1);
fn token() -> Result<String, String> {
    NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        .map(|v| format!("px:{v}"))
        .map_err(|_| "Action IDs exhausted".into())
}
pub fn commands() -> &'static [&'static str] {
    &["index", "width", "height", "menu-bar"]
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Missing string {key}"))
}
fn checked(value: Value) -> CoreResult {
    if value.get("error").is_some_and(|e| !e.is_null())
        || value["ok"] == false
        || value["success"] == false
    {
        return Err(format!("Host operation failed: {value}"));
    }
    Ok(value)
}
async fn store(ctx: &CoreContext) -> CoreResult {
    let value = v2_sdk::read_store(ctx, SLUG).await?;
    History::read(&value)?;
    Ok(value)
}
fn rotate(state: &mut Value) -> Result<(), String> {
    state["submit"] = json!(token()?);
    Ok(())
}
fn form(state: &Value, initial: bool) -> Value {
    let property = state["property"].as_str().unwrap_or("width");
    let presets = presets::for_property(property).unwrap_or(presets::WIDTH);
    let groups: Vec<Value> = ["Desktop", "Tablet", "Mobile"].into_iter().map(|group| {
        let children: Vec<Value> = presets.iter().filter(|p| p.group == group).map(|p| json!({"type":"Form.Dropdown.Item","props":{"value":p.value,"title":p.title}})).collect();
        json!({"type":"Form.Dropdown.Section","props":{"title":group},"children":children})
    }).collect();
    let mut field = json!({"type":"Form.TextField","props":{"id":"textfield","title":"Size in pixels","placeholder":"16","storeValue":true,"autoFocus":true,"actionId":state["textAction"]}});
    // After mount omit value: beta reconciliation then preserves queued local typing.
    if initial {
        field["props"]["value"] = state["draft"].clone();
    }
    if let Some(error) = state.get("error").and_then(Value::as_str) {
        field["props"]["error"] = json!(error);
    }
    v2_sdk::view(
        "Convert PX to VW or VH",
        json!({"type":"Form","children":[
            {"type":"Form.Description","props":{"text":"Select your device and enter the size in pixels to be converted."}},
            {"type":"Form.Dropdown","props":{"id":"sizedropdown","title":"Property","value":property,"actionId":state["propertyAction"]},"children":[
                {"type":"Form.Dropdown.Item","props":{"value":"width","title":"Width"}},
                {"type":"Form.Dropdown.Item","props":{"value":"height","title":"Height"}}]},
            {"type":"Form.Dropdown","props":{"id":"dropdown","title":"Device","value":state["device"],"actionId":state["deviceAction"]},"children":groups},
            field, {"type":"Form.Description","props":{"text":"Based on Figma frames."}}
        ]}),
        vec![json!({"id":state["submit"],"title":"Convert and Copy","kind":"submit-form"})],
    )
}
fn history_view(state: &mut Value, saved: &Value) -> CoreResult {
    let history = History::read(saved)?;
    state
        .as_object_mut()
        .ok_or("Invalid history session")?
        .remove("copiedHistoryAction");
    state["snapshot"] = json!(history.revision);
    let mut actions = vec![];
    let mut targets = serde_json::Map::new();
    let mut rows = vec![];
    for entry in &history.entries {
        let mut ids = vec![];
        for (verb, title) in [("copy", "Copy Value"), ("remove", "Remove from History")] {
            let id = token()?;
            actions.push(json!({"id":id,"title":title,"kind":"action"}));
            targets.insert(id.clone(), json!({"verb":verb,"entry":entry}));
            ids.push(id);
        }
        rows.push(json!({"type":"List.Item","props":{"id":entry.id.to_string(),"title":entry.label,"subtitle":entry.date,"actionIds":ids}}));
    }
    if !history.entries.is_empty() {
        let id = token()?;
        actions.push(json!({"id":id,"title":"Clear History","kind":"action"}));
        targets.insert(id, json!({"verb":"clear"}));
    }
    state["targets"] = json!(targets);
    state["history"] = json!(history.entries);
    Ok(v2_sdk::view(
        "Conversion History",
        json!({"type":"List","props":{"searchBarPlaceholder":"Search conversions"},"children":[
            {"type":"List.Section","props":{"title":if history.entries.is_empty() {"No history"} else {"Last conversions"}},"children":rows}
        ]}),
        actions,
    ))
}
async fn finish(ctx: &CoreContext, state: &mut Value) -> CoreResult {
    let conversion: Conversion = serde_json::from_value(state["pending"]["conversion"].clone())
        .map_err(|e| e.to_string())?;
    // Save before clipboard, as upstream does. Retry only the incomplete stage.
    if state["pending"]["saved"] != true {
        let item = conversion.clone();
        let timestamp = model::timestamp()?;
        v2_sdk::update_store(ctx, SLUG, move |v| History::add(v, &item, timestamp)).await?;
        state["pending"]["saved"] = json!(true);
    }
    if !conversion.copy {
        state["done"] = json!(true);
        return Ok(v2_sdk::no_view(
            "Conversion was NaN; saved in history, nothing copied",
        ));
    }
    if state["pending"]["copied"] != true {
        checked(v2_sdk::copy(ctx, &conversion.value).await?)?;
        state["pending"]["copied"] = json!(true);
    }
    if state["pending"]["hidden"] != true {
        checked(ctx.request("window:hide", vec![]).await?)?;
        state["pending"]["hidden"] = json!(true);
    }
    state["done"] = json!(true);
    Ok(v2_sdk::no_view(&format!(
        "{} copied to clipboard",
        conversion.value
    )))
}
async fn invoke_form(ctx: &CoreContext, payload: &Value, state: &mut Value) -> CoreResult {
    let id = text(payload, "actionId")?;
    if state["done"] == true {
        return Err("Conversion already completed; reopen the command".into());
    }
    if state.get("pending").is_some() {
        if state["submit"] != id {
            return Err("Retry conversion before changing the form, or reopen it".into());
        }
        return finish(ctx, state).await;
    }
    if state["textAction"] == id {
        let draft = text(&payload["formValues"], "value")?.to_string();
        let saved = draft.clone();
        v2_sdk::update_store(ctx, SLUG, move |v| {
            History::read(v)?;
            v["textfield"] = json!(saved);
            Ok(())
        })
        .await?;
        state["draft"] = json!(draft);
        let had_error = state
            .as_object_mut()
            .ok_or("Invalid session")?
            .remove("error")
            .is_some();
        return Ok(if had_error {
            form(state, false)
        } else {
            v2_sdk::unchanged()
        });
    }
    if state["propertyAction"] == id {
        let property = text(&payload["formValues"], "value")?;
        let options = presets::for_property(property).ok_or("Invalid property")?;
        state["property"] = json!(property);
        state["device"] = json!(options[0].value);
        state["deviceAction"] = json!(token()?);
        rotate(state)?;
        return Ok(form(state, false));
    }
    if state["deviceAction"] == id {
        let device = text(&payload["formValues"], "value")?;
        let options = presets::for_property(text(state, "property")?).ok_or("Invalid property")?;
        if !options.iter().any(|p| p.value == device) {
            return Err("Select a device for the current property".into());
        }
        state["device"] = json!(device);
        // Stable field action ID accepts queued device edits; submit IDs expire.
        rotate(state)?;
        return Ok(form(state, false));
    }
    if state["submit"] != id {
        return Err("Unknown or stale form action".into());
    }
    let values = &payload["formValues"];
    if text(values, "sizedropdown")? != text(state, "property")?
        || text(values, "dropdown")? != text(state, "device")?
    {
        return Err("Form changed; wait for the current device selection".into());
    }
    let pixels = text(values, "textfield")?;
    let mut item = match model::convert("index", text(state, "device")?, pixels) {
        Ok(item) => item,
        Err(e) => {
            state["error"] = json!(e);
            return Ok(form(state, false));
        }
    };
    if state["property"] == "height" {
        item.value = item.value.strip_suffix("vw").unwrap().to_string() + "vh";
        item.label = item.label.strip_suffix("vw").unwrap().to_string() + "vh";
    }
    let draft = pixels.to_string();
    v2_sdk::update_store(ctx, SLUG, move |v| {
        History::read(v)?;
        v["textfield"] = json!(draft);
        Ok(())
    })
    .await?;
    state["pending"] = json!({"conversion":item,"saved":false,"copied":false,"hidden":false});
    finish(ctx, state).await
}
async fn invoke_history(ctx: &CoreContext, payload: &Value, state: &mut Value) -> CoreResult {
    let id = text(payload, "actionId")?;
    if state
        .get("copiedHistoryAction")
        .is_some_and(|copied| copied != id)
    {
        return Err("Retry the copied history action to finish hiding, or reopen history".into());
    }
    let target = state["targets"]
        .get(id)
        .cloned()
        .ok_or("Unknown or stale history action")?;
    let expected = state["snapshot"]
        .as_u64()
        .ok_or("Missing history snapshot")?;
    let latest = store(ctx).await?;
    let history = History::read(&latest)?;
    history.ensure_revision(expected)?;
    match text(&target, "verb")? {
        "copy" => {
            let entry: model::Entry =
                serde_json::from_value(target["entry"].clone()).map_err(|e| e.to_string())?;
            if !history.entries.contains(&entry) {
                return Err("History entry changed".into());
            }
            if state["copiedHistoryAction"] != id {
                checked(v2_sdk::copy(ctx, &entry.value).await?)?;
                state["copiedHistoryAction"] = json!(id);
            }
            checked(ctx.request("window:hide", vec![]).await?)?;
            // Expire the IDs even if a caller keeps the completed session.
            state["targets"] = json!({});
            state["done"] = json!(true);
            Ok(v2_sdk::no_view(&format!(
                "{} copied to clipboard",
                entry.value
            )))
        }
        "remove" | "clear" => {
            let remove = if target["verb"] == "remove" {
                Some(target["entry"]["id"].as_u64().ok_or("Missing entry ID")?)
            } else {
                None
            };
            let saved =
                v2_sdk::update_store(ctx, SLUG, move |v| History::change(v, expected, remove))
                    .await?;
            history_view(state, &saved)
        }
        _ => Err("Unknown history operation".into()),
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    if !commands().contains(&command) {
        return Err("Unknown conversion command".into());
    }
    if operation == "extension:dispose-session" {
        *state = Value::Null;
        return Ok(v2_sdk::unchanged());
    }
    if operation == "extension:run-command" {
        // A launch is a new invocation, never a retry of an earlier launch.
        *state = json!({"command":command,"session":token()?});
        let saved = store(ctx).await?;
        return match command {
            "index" => {
                state["property"] = json!("width");
                state["device"] = json!("1440");
                state["draft"] = saved.get("textfield").cloned().unwrap_or(json!(""));
                state["propertyAction"] = json!(token()?);
                state["textAction"] = json!(token()?);
                state["deviceAction"] = json!(token()?);
                rotate(state)?;
                Ok(form(state, true))
            }
            "menu-bar" => history_view(state, &saved),
            "width" | "height" => {
                let args = &payload["argumentValues"];
                let item = model::convert(command, text(args, command)?, text(args, "pixels")?)?;
                state["pending"] =
                    json!({"conversion":item,"saved":false,"copied":false,"hidden":false});
                finish(ctx, state).await
            }
            _ => unreachable!(),
        };
    }
    if state["command"] != command || state.get("session").is_none() {
        return Err("Session is not active".into());
    }
    if state["done"] == true {
        return Err("Conversion session completed; reopen the command".into());
    }
    match operation {
        "extension:invoke-action" if command == "index" => invoke_form(ctx, payload, state).await,
        "extension:invoke-action" if command == "menu-bar" => {
            invoke_history(ctx, payload, state).await
        }
        "extension:refresh-session" if command == "menu-bar" => {
            let saved = store(ctx).await?;
            if state["snapshot"] == History::read(&saved)?.revision {
                Ok(v2_sdk::unchanged())
            } else {
                history_view(state, &saved)
            }
        }
        "extension:refresh-session" if command == "index" => Ok(v2_sdk::unchanged()),
        _ => Err("Unsupported conversion operation".into()),
    }
}
