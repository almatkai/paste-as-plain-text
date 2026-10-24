//! Compile this file unchanged as native_core::extensions::v2_paste_as_plain_text.
//! Uses the existing beta SDK and CoreContext; no dynamic loader or extra RPC routes.
use super::v2_sdk;
use crate::native_core::common::{CoreContext, CoreResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use tezbar_paste_as_plain_text::{
    os_helpers::{self, Platform, Runner, SystemRunner},
    Content, Host, Notice, Result, SLUG,
};

pub use tezbar_paste_as_plain_text::{commands, ICON, MANIFEST};

/// Check RPC envelopes only, never native storage objects (which may own ok/error keys).
fn rpc_reply(value: Value) -> CoreResult {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(value["error"]
            .as_str()
            .or_else(|| value["message"].as_str())
            .unwrap_or("Native host request failed")
            .into());
    }
    Ok(value)
}
fn cached(store: &Value) -> Result<String> {
    match store.get("Args") {
        None => Ok(String::new()),
        Some(args) if args.is_object() => match args.get("PasteAs") {
            None => Ok(String::new()),
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err("Invalid native Args/PasteAs cache".into()),
        },
        _ => Err("Invalid native Args cache".into()),
    }
}

pub struct CoreHost<'a, R = SystemRunner> {
    pub ctx: &'a CoreContext,
    pub runner: R,
    pub platform: Platform,
}
#[async_trait]
impl<R: Runner> Host for CoreHost<'_, R> {
    async fn resolve_argument(&self, supplied: Option<&str>) -> Result<String> {
        let store = if let Some(supplied) = supplied {
            let supplied = supplied.to_owned();
            v2_sdk::update_store(self.ctx, SLUG, move |store| {
                // Validate before modifying; preserve unrelated root and namespace fields.
                cached(store)?;
                if store.get("Args").is_none() {
                    store["Args"] = json!({});
                }
                store["Args"]["PasteAs"] = json!(supplied);
                Ok(())
            })
            .await?
        } else {
            v2_sdk::read_store(self.ctx, SLUG).await?
        };
        cached(&store)
    }
    async fn hide(&self) -> Result<()> {
        rpc_reply(self.ctx.request("window:hide", vec![]).await?).map(|_| ())
    }
    async fn read_text(&self) -> Result<String> {
        self.ctx
            .request("clipboard:read", vec![])
            .await?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "clipboard:read must return a string".into())
    }
    async fn read_file(&self) -> Result<Option<String>> {
        os_helpers::read_file(&self.runner, self.platform).await
    }
    async fn path_exists(&self, path: &str) -> Result<bool> {
        let path = std::path::PathBuf::from(path);
        tokio::task::spawn_blocking(move || Ok(path.exists()))
            .await
            .map_err(|e| e.to_string())?
    }
    async fn title(&self, url: &str) -> Result<String> {
        tezbar_paste_as_plain_text::http::fetch_title(url).await
    }
    async fn paste(&self, content: &Content) -> Result<()> {
        let reply = match content {
            Content::Text(text) => {
                self.ctx
                    .request("core:platform:paste-text", vec![json!(text)])
                    .await?
            }
            Content::File(path) => {
                os_helpers::write_file(&self.runner, self.platform, path).await?;
                self.ctx.request("core:platform:paste", vec![]).await?
            }
        };
        rpc_reply(reply).map(|_| ())
    }
    async fn copy(&self, content: &Content) -> Result<()> {
        match content {
            Content::Text(text) => rpc_reply(
                self.ctx
                    .request("clipboard:write", vec![json!(text)])
                    .await?,
            )
            .map(|_| ()),
            Content::File(path) => os_helpers::write_file(&self.runner, self.platform, path).await,
        }
    }
    async fn notice(&self, style: Notice, title: &str) {
        if style == Notice::Failure {
            // No-view effects have no live beta listener. Reveal the launcher so
            // its returned error message is visible, even with showTips disabled.
            // Preserve the primary error if revealing the window also fails.
            let _ = self.ctx.request("window:show", vec![]).await;
        }
        self.ctx.emit("extension:effect",json!({"kind":if style == Notice::Success {"hud"} else {"toast"},"style":match style { Notice::Animated => "animated", Notice::Success => "success", Notice::Failure => "failure" },"title":title}));
    }
}

pub async fn handle(
    ctx: &CoreContext,
    command: &str,
    operation: &str,
    payload: &Value,
    state: &mut Value,
) -> CoreResult {
    tezbar_paste_as_plain_text::handle(
        &CoreHost {
            ctx,
            runner: SystemRunner,
            platform: Platform::current(),
        },
        command,
        operation,
        payload,
        state,
    )
    .await
}
