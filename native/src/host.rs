//! Maps the reused Native V2 `Host` seam onto narrowly granted typed SDK host
//! requests. No arbitrary core channel, Tauri type or app crate is involved.
use async_trait::async_trait;
use serde_json::{json, Value};
use tezbar_extension_sdk::{read_store, update_store, ExtensionContext};
use tezbar_paste_as_plain_text::{
    http,
    os_helpers::{self, Platform, Runner, SystemRunner},
    Content, Host, Notice, Result, SLUG,
};

/// Reject RPC envelopes that report failure. Only host replies are checked,
/// never storage objects (which may legitimately own ok/error keys).
pub fn rpc_reply(value: Value) -> Result<Value> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(value["error"]
            .as_str()
            .or_else(|| value["message"].as_str())
            .unwrap_or("Native host request failed")
            .into());
    }
    Ok(value)
}

/// Validated `Args/PasteAs` read. Missing values mean the default (Plain Text).
pub fn cached(store: &Value) -> Result<String> {
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

pub struct ProcessHost<'a, R = SystemRunner> {
    pub ctx: &'a ExtensionContext,
    pub runner: R,
    pub platform: Platform,
}

impl<'a> ProcessHost<'a> {
    pub fn system(ctx: &'a ExtensionContext) -> Self {
        Self {
            ctx,
            runner: SystemRunner,
            platform: Platform::current(),
        }
    }
}

impl<R: Runner> ProcessHost<'_, R> {
    // The typed host enforces grants for RPCs. Local file-reference helpers
    // must enforce the same grants before touching the OS clipboard.
    fn require(&self, capability: &str) -> Result<()> {
        let grants = &self.ctx.granted_capabilities;
        if grants.contains(capability)
            || grants.contains(&format!("host.{capability}"))
            || grants.contains(capability.split('.').next().unwrap())
        {
            Ok(())
        } else {
            Err(format!("Host capability not granted: {capability}"))
        }
    }
    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        rpc_reply(self.ctx.host(method, params).await?)
    }
    /// The host's typed file clipboard accepts existing absolute regular files.
    /// Directories (allowed by the source's `pathExistsSync`) use the
    /// extension's constant-script OS helper with the path passed as data.
    async fn write_file_clipboard(&self, path: &str) -> Result<()> {
        self.require("clipboard.writeFiles")?;
        let owned = std::path::PathBuf::from(path);
        let regular = tokio::task::spawn_blocking(move || owned.is_file())
            .await
            .map_err(|e| e.to_string())?;
        if regular {
            self.call("host.clipboard.writeFiles", json!({"paths":[path]}))
                .await
                .map(|_| ())
        } else {
            os_helpers::write_file(&self.runner, self.platform, path).await
        }
    }
}

#[async_trait]
impl<R: Runner> Host for ProcessHost<'_, R> {
    async fn resolve_argument(&self, supplied: Option<&str>) -> Result<String> {
        let store = if let Some(supplied) = supplied {
            let supplied = supplied.to_owned();
            update_store(self.ctx, SLUG, move |store| {
                // Validate before modifying; preserve unrelated root/namespace keys.
                cached(store)?;
                if store.get("Args").is_none() {
                    store["Args"] = json!({});
                }
                store["Args"]["PasteAs"] = json!(supplied);
                Ok(())
            })
            .await?
        } else {
            read_store(self.ctx, SLUG).await?
        };
        cached(&store)
    }
    async fn hide(&self) -> Result<()> {
        self.call("host.window.visibility", json!({"visible":false}))
            .await
            .map(|_| ())
    }
    async fn read_text(&self) -> Result<String> {
        self.call("host.clipboard.read", json!({"format":"text"}))
            .await?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "host.clipboard.read must return a string".into())
    }
    async fn read_file(&self) -> Result<Option<String>> {
        self.require("clipboard.read")?;
        // Protocol 1 has no typed clipboard file-reference read; use the
        // extension's bounded, kill-on-drop constant-script helper.
        os_helpers::read_file(&self.runner, self.platform).await
    }
    async fn path_exists(&self, path: &str) -> Result<bool> {
        // A relative path would resolve against the process working directory,
        // which is unrelated to the user's context. Only absolute paths qualify.
        let path = std::path::PathBuf::from(path);
        tokio::task::spawn_blocking(move || Ok(path.is_absolute() && path.exists()))
            .await
            .map_err(|e| e.to_string())?
    }
    async fn title(&self, url: &str) -> Result<String> {
        http::fetch_title(url).await
    }
    async fn paste(&self, content: &Content) -> Result<()> {
        match content {
            Content::Text(text) => self
                .call("host.clipboard.pasteText", json!({"text":text}))
                .await
                .map(|_| ()),
            Content::File(path) => {
                self.require("clipboard.paste")?;
                self.write_file_clipboard(path).await?;
                self.call("host.clipboard.paste", json!({}))
                    .await
                    .map(|_| ())
            }
        }
    }
    async fn copy(&self, content: &Content) -> Result<()> {
        match content {
            Content::Text(text) => self
                .call("host.clipboard.write", json!({"text":text}))
                .await
                .map(|_| ()),
            Content::File(path) => self.write_file_clipboard(path).await,
        }
    }
    async fn notice(&self, style: Notice, _title: &str) {
        // Protocol 1 has no toast/HUD host API and process hosts reject custom
        // events. Success text is returned in the no-view result; failures are
        // returned as the session error. Reveal the launcher so the renderer can
        // show it after the initial hide; a reveal failure must not mask it.
        if style == Notice::Failure {
            let _ = self
                .ctx
                .host("host.window.visibility", json!({"visible":true}))
                .await;
        }
    }
}
