//! Standalone schema-3 process executable (protocol 1) for Pixels to Viewport
//! Width or Height. The reviewed Native V2 adapter (`v2/src/tezbar_adapter.rs`)
//! is compiled here byte-for-byte unchanged. Its `crate::native_core` imports
//! resolve to the independent SDK through the narrow shim below, so the only
//! host services it can reach are typed `host.clipboard.write` and
//! `host.window.visibility {visible:false}` reverse RPCs, both gated by the
//! manifest grants. Storage is the SDK's package-private atomic store.
mod effects;

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

struct Adapter;
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
            let session = payload["sessionId"].as_str().unwrap_or_default();
            effects::before(session, operation, state);
            let result = adapter::handle(ctx, command, operation, payload, state).await;
            effects::after(session, operation, state, result.is_ok());
            result
        })
    }
}

fn main() {
    debug_assert_eq!(adapter::SLUG, "convert-px-to-vw-vh");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("extension runtime");
    let result = runtime.block_on(tezbar_extension_sdk::serve(Adapter));
    runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
