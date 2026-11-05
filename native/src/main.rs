//! Standalone Tezbar process extension (schema 3, protocol 1). The command
//! behavior is the reused Native V2 engine in `../v2`; this binary only adapts
//! its `Host` seam to the SDK's JSONL process transport.
mod host;
mod migration;
#[cfg(test)]
mod tests;

use serde_json::{json, Value};
use tezbar_extension_sdk::{BoxFuture, CoreResult, Extension, ExtensionContext};

pub const ID: &str = "com.tezbar.native.paste-as-plain-text";

struct Adapter;
impl Extension for Adapter {
    fn id(&self) -> &'static str {
        ID
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        tezbar_paste_as_plain_text::commands()
    }
    fn initialize<'a>(&'a self, ctx: &'a ExtensionContext) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            migration::initialize(ctx).await?;
            Ok(json!({"ready":true}))
        })
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
            // This source command has no view or bound actions. Reject forged
            // action calls instead of acknowledging an action that never ran.
            if operation == "extension:invoke-action" {
                return Err("This no-view command has no bound actions".into());
            }
            let host = host::ProcessHost::system(ctx);
            tezbar_paste_as_plain_text::handle(&host, command, operation, payload, state).await
        })
    }
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("extension runtime");
    let result = runtime.block_on(tezbar_extension_sdk::serve(Adapter));
    // Session/task cleanup completes inside serve; bound runtime teardown.
    runtime.shutdown_timeout(std::time::Duration::from_millis(500));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
