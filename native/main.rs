//! Standalone process boundary around the byte-unchanged v2 engine and adapter.
mod boundary;
mod effects;
mod migration;
mod tray;

/// Narrow mapping of the beta host surface used by the adapter onto the SDK.
pub mod native_core {
    pub mod common {
        pub use tezbar_extension_sdk::{CoreResult, ExtensionContext as CoreContext};
    }
    pub mod extensions {
        pub mod v2_sdk {
            // Same signatures as the beta helpers: store name, FnOnce mutation,
            // clipboard write and view/no-view/unchanged envelopes.
            pub use tezbar_extension_sdk::{
                copy, no_view, read_store, unchanged, update_store, view,
            };
        }
    }
}

#[path = "../v2/src/tezbar_adapter.rs"]
mod adapter;

use serde_json::Value;
use tezbar_extension_sdk::{BoxFuture, CoreResult, Extension, ExtensionContext};

pub const ID: &str = "com.tezbar.native.convert-px-to-vw-vh";

#[derive(Default)]
struct Adapter {
    tray: tokio::sync::Mutex<tray::Tray>,
}
impl Extension for Adapter {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        adapter::commands()
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        payload: &'a Value,
        state: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            let mut tray = self.tray.lock().await;
            let session = payload["sessionId"].as_str().unwrap_or_default();
            effects::before(session, operation, state);
            let result = adapter::handle(ctx, command, operation, payload, state).await;
            effects::after(session, operation, state, result.is_ok());
            let mut result = boundary::adapt(result?, operation, state)?;
            if command == "menu-bar" && operation == "extension:run-command" {
                tray.activate(ctx).await?;
            } else if operation != "extension:dispose-session" {
                if let Err(error) = tray.sync(ctx).await {
                    tray.mark_dirty();
                    result["trayDiagnostic"] = serde_json::json!(error);
                    if let Some(message) = result["message"].as_str() {
                        result["message"] =
                            serde_json::json!(format!("{message} (History tray refresh pending)"));
                    }
                }
            }
            Ok(result)
        })
    }
    fn initialize<'a>(&'a self, ctx: &'a ExtensionContext) -> BoxFuture<'a, CoreResult> {
        Box::pin(migration::initialize(ctx))
    }
    fn background_activate<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        params: &'a Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            if params["key"] != "conversion-history" {
                return Err("Unknown background activation".into());
            }
            self.tray.lock().await.activate(ctx).await
        })
    }
    fn background_event<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        params: &'a Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            let mut tray = self.tray.lock().await;
            match tray.event(ctx, params).await {
                Ok(value) => Ok(value),
                Err(error) => {
                    tray.mark_dirty();
                    Ok(serde_json::json!({"ok":true,"diagnostic":error}))
                }
            }
        })
    }
    fn shutdown<'a>(&'a self, ctx: &'a ExtensionContext) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move { self.tray.lock().await.shutdown(ctx).await })
    }
}

fn main() {
    debug_assert_eq!(adapter::SLUG, "convert-px-to-vw-vh");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("extension runtime");
    let result = runtime.block_on(tezbar_extension_sdk::serve(Adapter::default()));
    runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
