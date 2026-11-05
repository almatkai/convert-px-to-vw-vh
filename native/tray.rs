//! Generic host tray lifecycle. Serialized with command operations by Adapter.
use serde_json::{json, Value};
use tezbar_convert_px_to_vw_vh::model::History;
use tezbar_extension_sdk::{copy, no_view, read_store, update_store, CoreResult, ExtensionContext};

const STORE: &str = "convert-px-to-vw-vh";
const RESOURCE: &str = "conversion-history";
#[derive(Default)]
pub struct Tray {
    active: bool,
    revision: Option<u64>,
    issued: Vec<String>,
}
impl Tray {
    async fn publish(&mut self, ctx: &ExtensionContext, force: bool) -> CoreResult {
        let history = History::read(&read_store(ctx, STORE).await?)?;
        if !force && self.revision == Some(history.revision) {
            return Ok(json!(true));
        }
        let mut items =
            vec![json!({"id":"configure","title":"Configure Command (Extension Settings)"})];
        for entry in &history.entries {
            items.push(
                json!({"id":format!("copy-{}-{}",history.revision,entry.id),"title":entry.label}),
            );
            items.push(json!({"id":format!("remove-{}-{}",history.revision,entry.id),"title":format!("Remove: {}",entry.label)}));
        }
        if history.entries.is_empty() {
            items.push(json!({"id":"empty","title":"No history","enabled":false}));
        } else {
            items.push(json!({"id":format!("clear-{}",history.revision),"title":"Clear History"}));
        }
        items.push(json!({"id":"refresh","title":"Refresh History"}));
        items.push(json!({"id":"stop","title":"Stop Conversion History"}));
        let issued = items
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_owned())
            .collect();
        ctx.host("host.tray.set", json!({"resourceId":RESOURCE,"icon":"resources/assets/logo.png","tooltip":"Conversion History","items":items})).await?;
        self.issued = issued;
        self.active = true;
        self.revision = Some(history.revision);
        Ok(json!(true))
    }
    pub async fn activate(&mut self, ctx: &ExtensionContext) -> CoreResult {
        let was_active = self.active;
        self.publish(ctx, true).await?;
        // The tray itself retains the current process. A durable lease restores
        // it after a host restart. Shutdown must not erase that user's choice.
        let result = ctx
            .host(
                "host.background.retain",
                json!({"key":RESOURCE,"durable":true,"data":{}}),
            )
            .await;
        if let Err(error) = result {
            // A failed first activation must not leave an orphan tray retaining
            // the process after the SDK discards the failed launcher session.
            if !was_active {
                self.shutdown(ctx)
                    .await
                    .map_err(|cleanup| format!("{error}; tray cleanup failed: {cleanup}"))?;
            }
            return Err(error);
        }
        result
    }
    pub async fn sync(&mut self, ctx: &ExtensionContext) -> CoreResult {
        if self.active {
            self.publish(ctx, false).await
        } else {
            Ok(json!(true))
        }
    }
    pub async fn event(&mut self, ctx: &ExtensionContext, params: &Value) -> CoreResult {
        if !self.active || params["resourceId"] != RESOURCE {
            return Err("Unknown or inactive tray resource".into());
        }
        let action = params["actionId"].as_str().ok_or("Missing tray action")?;
        if !self.issued.iter().any(|id| id == action) {
            return Err("Unknown or stale tray action".into());
        }
        if action == "configure" {
            return ctx
                .host("host.settings.open", json!({"surface":"extensions"}))
                .await;
        }
        if action == "refresh" {
            return self.publish(ctx, true).await;
        }
        if action == "stop" {
            ctx.host("host.background.release", json!({"key":RESOURCE}))
                .await?;
            self.shutdown(ctx).await?;
            return Ok(no_view("Conversion History stopped"));
        }
        let parts: Vec<_> = action.split('-').collect();
        let (verb, revision, entry) = match parts.as_slice() {
            [verb @ ("copy" | "remove"), revision, entry] => (*verb, *revision, Some(*entry)),
            ["clear", revision] => ("clear", *revision, None),
            _ => return Err("Unknown tray action".into()),
        };
        let expected = revision
            .parse::<u64>()
            .map_err(|_| "Invalid tray revision")?;
        let history = History::read(&read_store(ctx, STORE).await?)?;
        history.ensure_revision(expected)?;
        let entry = entry
            .map(|id| id.parse::<u64>().map_err(|_| "Invalid tray entry"))
            .transpose()?;
        if verb == "copy" {
            let entry = history
                .entries
                .iter()
                .find(|e| Some(e.id) == entry)
                .ok_or("History entry no longer exists")?;
            copy(ctx, &entry.value).await?;
            return Ok(no_view(&format!("{} copied to clipboard", entry.value)));
        }
        update_store(ctx, STORE, move |value| {
            History::change(value, expected, entry)
        })
        .await?;
        self.publish(ctx, true).await
    }
    pub async fn shutdown(&mut self, ctx: &ExtensionContext) -> CoreResult {
        if self.active {
            ctx.host("host.tray.remove", json!({"resourceId":RESOURCE}))
                .await?;
            self.active = false;
            self.revision = None;
            self.issued.clear();
        }
        Ok(json!({}))
    }
}
