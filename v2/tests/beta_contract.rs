extern crate self as tauri;
#[path = "fixtures/os-boundary.rs"]
mod os_boundary;
pub use os_boundary::{AppHandle, Emitter};
// Byte-exact beta fixtures; only Tauri's OS boundary is stubbed by a dev dependency.
#[path = "fixtures/beta/common.rs"]
pub mod beta_common;
#[path = "fixtures/beta/v2_sdk.rs"]
pub mod beta_sdk;
pub mod native_core {
    pub use crate::beta_common as common;
    pub mod extensions {
        pub use crate::beta_sdk as v2_sdk;
    }
}
#[path = "../src/tezbar_adapter.rs"]
mod adapter;

use beta_common::{CoreContext, CorePaths};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tezbar_convert_px_to_vw_vh::model::History;

type Calls = Arc<Mutex<Vec<(String, Vec<Value>)>>>;
type Failure = Arc<Mutex<Option<(String, bool)>>>;
fn fixture() -> (tempfile::TempDir, CoreContext, Calls, Failure) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let ctx = CoreContext::headless(CorePaths {
        data_dir: root.join("data"),
        config_dir: root.join("config"),
        resource_dir: root.join("resources"),
        temp_dir: root.join("tmp"),
        home_dir: root.to_path_buf(),
    });
    let calls: Calls = Arc::default();
    let fail: Failure = Arc::default();
    let (c, f) = (calls.clone(), fail.clone());
    ctx.set_router(Arc::new(move |channel, args| {
        c.lock().unwrap().push((channel.clone(), args));
        let failure = f.lock().unwrap().clone();
        Box::pin(async move {
            if let Some((target, envelope)) = failure {
                if target == channel {
                    return if envelope {
                        Ok(json!({"ok":false,"error":"denied"}))
                    } else {
                        Err("denied".into())
                    };
                }
            }
            Ok(Value::Null)
        })
    }))
    .unwrap();
    (dir, ctx, calls, fail)
}
async fn run(ctx: &CoreContext, c: &str, s: &mut Value, p: Value) -> Value {
    adapter::handle(ctx, c, "extension:run-command", &p, s)
        .await
        .unwrap()
}
async fn invoke(
    ctx: &CoreContext,
    c: &str,
    s: &mut Value,
    id: Value,
    values: Value,
) -> Result<Value, String> {
    adapter::handle(
        ctx,
        c,
        "extension:invoke-action",
        &json!({"actionId":id,"formValues":values}),
        s,
    )
    .await
}
fn fields(property: &str, device: &str, pixels: &str) -> Value {
    json!({"sizedropdown":property,"dropdown":device,"textfield":pixels})
}

#[tokio::test]
async fn all_commands_use_real_argument_keys_and_sdk_routes() {
    let (_d, ctx, calls, _) = fixture();
    let mut state = json!({});
    assert_eq!(
        adapter::commands(),
        ["index", "width", "height", "menu-bar"]
    );
    for c in ["width", "height"] {
        let mut args = json!({"pixels":"16"});
        args[c] = json!("1440.5");
        let result = run(&ctx, c, &mut state, json!({"argumentValues":args})).await;
        assert_eq!(result["mode"], "no-view");
    }
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        [
            "clipboard:write",
            "window:hide",
            "clipboard:write",
            "window:hide"
        ]
    );
    let menu = run(&ctx, "menu-bar", &mut state, json!({})).await;
    assert_eq!(menu["root"]["type"], "List");
    let store = beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap();
    assert_eq!(History::read(&store).unwrap().entries.len(), 2);
    assert!(adapter::handle(
        &ctx,
        "width",
        "extension:run-command",
        &json!({"arguments":{"width":"1440","pixels":"16"}}),
        &mut state
    )
    .await
    .is_err());
}

#[tokio::test]
async fn invalid_form_cannot_copy_and_text_survives_view_transitions() {
    let (_d, ctx, calls, _) = fixture();
    let mut state = json!({});
    let first = run(&ctx, "index", &mut state, json!({})).await;
    assert_eq!(first["root"]["children"][3]["props"]["autoFocus"], true);
    let old = state["submit"].clone();
    let text = state["textAction"].clone();
    let prop = state["propertyAction"].clone();
    for value in ["1", "12", "123"] {
        assert_eq!(
            invoke(
                &ctx,
                "index",
                &mut state,
                text.clone(),
                json!({"value":value})
            )
            .await
            .unwrap()["mode"],
            "unchanged"
        );
    }
    let view = invoke(&ctx, "index", &mut state, prop, json!({"value":"height"}))
        .await
        .unwrap();
    assert!(view["root"]["children"][3]["props"].get("value").is_none());
    assert_eq!(state["textAction"], text);
    assert!(invoke(
        &ctx,
        "index",
        &mut state,
        old,
        fields("width", "1440", "16")
    )
    .await
    .is_err());
    let id = state["submit"].clone();
    let bad = invoke(
        &ctx,
        "index",
        &mut state,
        id.clone(),
        fields("height", "1024", "bad"),
    )
    .await
    .unwrap();
    assert!(bad["root"]["children"][3]["props"]["error"].is_string());
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(
        beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap()["textfield"],
        "123"
    );
    invoke(
        &ctx,
        "index",
        &mut state,
        id,
        fields("height", "1024", "-0"),
    )
    .await
    .unwrap();
    assert_eq!(
        calls.lock().unwrap()[0].1,
        json!(["0.000vh"]).as_array().unwrap().to_vec()
    );
}

#[tokio::test]
async fn retry_copy_hide_and_history_failures_without_duplicate_completed_stages() {
    for (channel, envelope) in [
        ("clipboard:write", false),
        ("clipboard:write", true),
        ("window:hide", false),
        ("window:hide", true),
    ] {
        let (_d, ctx, calls, fail) = fixture();
        let mut state = json!({});
        run(&ctx, "index", &mut state, json!({})).await;
        let id = state["submit"].clone();
        *fail.lock().unwrap() = Some((channel.into(), envelope));
        assert!(invoke(
            &ctx,
            "index",
            &mut state,
            id.clone(),
            fields("width", "1440", "16")
        )
        .await
        .is_err());
        let saved = beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap();
        assert_eq!(History::read(&saved).unwrap().entries.len(), 1);
        *fail.lock().unwrap() = None;
        invoke(
            &ctx,
            "index",
            &mut state,
            id.clone(),
            fields("width", "1440", "999"),
        )
        .await
        .unwrap();
        assert_eq!(
            History::read(&beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap())
                .unwrap()
                .entries
                .len(),
            1
        );
        let copies = calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == "clipboard:write")
            .count();
        assert_eq!(copies, if channel == "window:hide" { 1 } else { 2 });
        assert!(
            invoke(&ctx, "index", &mut state, id, fields("width", "1440", "16"))
                .await
                .is_err()
        );
    }
    let (_d, ctx, calls, _) = fixture();
    let mut state = json!({});
    run(&ctx, "index", &mut state, json!({})).await;
    let path = ctx
        .paths
        .data_dir
        .join("native-extensions-v2")
        .join(adapter::SLUG)
        .join("storage.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    let id = state["submit"].clone();
    assert!(invoke(
        &ctx,
        "index",
        &mut state,
        id.clone(),
        fields("width", "1440", "16")
    )
    .await
    .is_err());
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    std::fs::write(&path, b"{}").unwrap();
    invoke(&ctx, "index", &mut state, id, fields("width", "1440", "16"))
        .await
        .unwrap();
}

#[tokio::test]
async fn concurrent_updates_keep_ten_duplicates_preserve_keys_and_reject_corruption() {
    let (_d, ctx, _, _) = fixture();
    beta_sdk::update_store(&ctx, adapter::SLUG, |v| {
        v["foreign"] = json!({"keep":1});
        Ok(())
    })
    .await
    .unwrap();
    let mut tasks = vec![];
    for _ in 0..32 {
        let ctx = ctx.clone();
        tasks.push(tokio::spawn(async move {
            let mut s = json!({});
            adapter::handle(
                &ctx,
                "width",
                "extension:run-command",
                &json!({"argumentValues":{"width":"100","pixels":"16"}}),
                &mut s,
            )
            .await
            .unwrap()
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let saved = beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap();
    let history = History::read(&saved).unwrap();
    assert_eq!(history.revision, 32);
    assert_eq!(history.entries.len(), 10);
    assert!(history.entries.iter().all(|e| e.value == "16.000vw"));
    assert_eq!(saved["foreign"]["keep"], 1);
    for bad in [
        json!({"history":[]}),
        json!({"historyRevision":0,"history":[{}]}),
        json!({"textfield":4}),
        json!({"historyRevision":1,"history":null}),
    ] {
        assert!(History::read(&bad).is_err());
    }
}

#[tokio::test]
async fn history_real_removal_clear_stale_actions_and_dispose_relaunch() {
    let (_d, ctx, calls, _) = fixture();
    let mut w = json!({});
    run(
        &ctx,
        "width",
        &mut w,
        json!({"argumentValues":{"width":"100","pixels":"16"}}),
    )
    .await;
    let mut a = json!({});
    let mut b = json!({});
    let av = run(&ctx, "menu-bar", &mut a, json!({})).await;
    let bv = run(&ctx, "menu-bar", &mut b, json!({})).await;
    let remove = av["actions"][1]["id"].clone();
    let stale = bv["actions"][0]["id"].clone();
    invoke(&ctx, "menu-bar", &mut a, remove.clone(), json!({}))
        .await
        .unwrap();
    let n = calls.lock().unwrap().len();
    assert!(invoke(&ctx, "menu-bar", &mut b, stale.clone(), json!({}))
        .await
        .is_err());
    assert_eq!(calls.lock().unwrap().len(), n);
    assert!(invoke(&ctx, "menu-bar", &mut a, remove, json!({}))
        .await
        .is_err());
    let refreshed = adapter::handle(
        &ctx,
        "menu-bar",
        "extension:refresh-session",
        &json!({}),
        &mut b,
    )
    .await
    .unwrap();
    assert_eq!(
        refreshed["root"]["children"][0]["props"]["title"],
        "No history"
    );
    adapter::handle(
        &ctx,
        "menu-bar",
        "extension:dispose-session",
        &json!({}),
        &mut b,
    )
    .await
    .unwrap();
    run(&ctx, "menu-bar", &mut b, json!({})).await;
    assert!(invoke(&ctx, "menu-bar", &mut b, stale, json!({}))
        .await
        .is_err());
    run(
        &ctx,
        "height",
        &mut w,
        json!({"argumentValues":{"height":"100","pixels":"20"}}),
    )
    .await;
    let new = run(&ctx, "menu-bar", &mut a, json!({})).await;
    let clear = new["actions"].as_array().unwrap().last().unwrap()["id"].clone();
    invoke(&ctx, "menu-bar", &mut a, clear, json!({}))
        .await
        .unwrap();
    assert!(
        History::read(&beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap())
            .unwrap()
            .entries
            .is_empty()
    );
}

#[tokio::test]
async fn nan_is_json_safe_saved_but_not_copied_and_nonfinite_pixels_match_source() {
    let (_d, ctx, calls, _) = fixture();
    let mut state = json!({});
    run(
        &ctx,
        "width",
        &mut state,
        json!({"argumentValues":{"width":"20px","pixels":"16"}}),
    )
    .await;
    assert!(calls.lock().unwrap().is_empty());
    let saved = beta_sdk::read_store(&ctx, adapter::SLUG).await.unwrap();
    assert_eq!(saved["history"][0]["value"], "NaNvw");
    run(
        &ctx,
        "height",
        &mut state,
        json!({"argumentValues":{"height":"1e-9999","pixels":"16"}}),
    )
    .await;
    assert_eq!(calls.lock().unwrap()[0].1[0], "Infinityvh");
}

#[tokio::test]
async fn renderer_samples_and_all_preset_options_have_supported_submit_kind() {
    let (_d, ctx, _, _) = fixture();
    let mut state = json!({});
    let initial = run(&ctx, "index", &mut state, json!({})).await;
    assert_eq!(initial["actions"][0]["kind"], "submit-form");
    let prop = state["propertyAction"].clone();
    let height = invoke(&ctx, "index", &mut state, prop, json!({"value":"height"}))
        .await
        .unwrap();
    let submit = state["submit"].clone();
    let invalid = invoke(
        &ctx,
        "index",
        &mut state,
        submit.clone(),
        fields("height", "1024", ""),
    )
    .await
    .unwrap();
    let text = state["textAction"].clone();
    let cleared = invoke(&ctx, "index", &mut state, text, json!({"value":"16"}))
        .await
        .unwrap();
    assert!(cleared["root"]["children"][3]["props"]
        .get("error")
        .is_none());
    invoke(
        &ctx,
        "index",
        &mut state,
        submit,
        fields("height", "1024", "16"),
    )
    .await
    .unwrap();
    let list = run(&ctx, "menu-bar", &mut state, json!({})).await;
    if let Ok(path) = std::env::var("TEZBAR_RENDERER_SAMPLES") {
        std::fs::write(path,serde_json::to_vec(&json!({"initial":initial,"height":height,"invalid":invalid,"cleared":cleared,"list":list})).unwrap()).unwrap();
    }
}

#[tokio::test]
async fn history_hide_failure_retries_only_hide_and_other_actions_cannot_interleave() {
    let (_d, ctx, calls, fail) = fixture();
    let mut state = json!({});
    run(
        &ctx,
        "width",
        &mut state,
        json!({"argumentValues":{"width":"100","pixels":"16"}}),
    )
    .await;
    calls.lock().unwrap().clear();
    let list = run(&ctx, "menu-bar", &mut state, json!({})).await;
    let copy = list["actions"][0]["id"].clone();
    let remove = list["actions"][1]["id"].clone();
    *fail.lock().unwrap() = Some(("window:hide".into(), true));
    assert!(
        invoke(&ctx, "menu-bar", &mut state, copy.clone(), json!({}))
            .await
            .is_err()
    );
    assert!(invoke(&ctx, "menu-bar", &mut state, remove, json!({}))
        .await
        .is_err());
    *fail.lock().unwrap() = None;
    invoke(&ctx, "menu-bar", &mut state, copy.clone(), json!({}))
        .await
        .unwrap();
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == "clipboard:write")
            .count(),
        1
    );
    assert!(invoke(&ctx, "menu-bar", &mut state, copy, json!({}))
        .await
        .is_err());
}

#[tokio::test]
async fn corrupt_owned_fields_are_not_replaced_and_legacy_data_is_not_read() {
    let (_d, ctx, calls, _) = fixture();
    let path = ctx
        .paths
        .data_dir
        .join("native-extensions-v2")
        .join(adapter::SLUG)
        .join("storage.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = ctx
        .paths
        .data_dir
        .join("extensions")
        .join(adapter::SLUG)
        .join(".tezbar-local-storage.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let original = b"{\"history\":\"unknown legacy data\"}";
    std::fs::write(&legacy, original).unwrap();
    for bad in [
        json!({"history":[]}),
        json!({"textfield":123}),
        json!({"historyRevision":1,"history":[{"id":1,"date":"2026-01-01T00:00:00.000Z","value":"poisonvw","label":"100px / 16px → poisonvw"}]}),
    ] {
        let bytes = serde_json::to_vec(&bad).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let mut state = json!({});
        assert!(adapter::handle(
            &ctx,
            "width",
            "extension:run-command",
            &json!({"argumentValues":{"width":"100","pixels":"16"}}),
            &mut state
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(calls.lock().unwrap().is_empty());
    }
    std::fs::write(&path, b"{}").unwrap();
    let mut state = json!({});
    let list = run(&ctx, "menu-bar", &mut state, json!({})).await;
    assert_eq!(list["root"]["children"][0]["props"]["title"], "No history");
    assert_eq!(std::fs::read(&legacy).unwrap(), original);
}
