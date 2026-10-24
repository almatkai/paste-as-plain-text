//! Native V2 core. The concrete host bridge is vendored from tezbar_adapter.rs.
use async_trait::async_trait;
use serde_json::{json, Value};

pub mod formats;
pub mod http;
pub mod os_helpers;

pub const ICON: &[u8] = include_bytes!("assets/paste-icon.png");
pub const MANIFEST: &str = include_str!("manifest.json");
pub const SLUG: &str = "paste-as-plain-text";
pub fn commands() -> &'static [&'static str] {
    &["paste-as-plain-text"]
}
pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub trim_start: bool,
    pub trim_end: bool,
    pub clean_line_breaks: bool,
    pub auto_fetch_title: bool,
    pub replace_clipboard: bool,
    pub show_tips: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            trim_start: true,
            trim_end: true,
            clean_line_breaks: false,
            auto_fetch_title: true,
            replace_clipboard: false,
            show_tips: true,
        }
    }
}
impl Preferences {
    pub fn from_payload(payload: &Value) -> Result<Self> {
        let mut p = Self::default();
        for (key, value) in [
            ("trimStart", &mut p.trim_start),
            ("trimEnd", &mut p.trim_end),
            ("cleanLineBreaks", &mut p.clean_line_breaks),
            ("autoFetchTitle", &mut p.auto_fetch_title),
            ("replaceClipboard", &mut p.replace_clipboard),
            ("showTips", &mut p.show_tips),
        ] {
            if let Some(v) = payload["preferences"].get(key) {
                *value = v
                    .as_bool()
                    .ok_or_else(|| format!("Preference {key} must be boolean"))?;
            }
        }
        Ok(p)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    File(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    Animated,
    Success,
    Failure,
}

/// OS, persistent storage and network effects are explicit, replaceable seams.
/// resolve_argument must use one atomic update when supplied, a read when omitted.
#[async_trait]
pub trait Host: Sync {
    async fn resolve_argument(&self, supplied: Option<&str>) -> Result<String>;
    async fn hide(&self) -> Result<()>;
    async fn read_text(&self) -> Result<String>;
    async fn read_file(&self) -> Result<Option<String>>;
    async fn path_exists(&self, path: &str) -> Result<bool>;
    async fn title(&self, url: &str) -> Result<String>;
    async fn paste(&self, content: &Content) -> Result<()>;
    async fn copy(&self, content: &Content) -> Result<()>;
    async fn notice(&self, style: Notice, title: &str);
}

/// Actual beta launch field. Presence is checked before conversion: empty is supplied.
fn argument(payload: &Value) -> Result<Option<&str>> {
    payload
        .get("argumentValues")
        .and_then(|v| v.get("advancedPasteFormat"))
        .map(|v| {
            v.as_str()
                .ok_or_else(|| "advancedPasteFormat must be a string".to_owned())
        })
        .transpose()
}

pub async fn handle(
    host: &impl Host,
    command: &str,
    operation: &str,
    payload: &Value,
    _state: &mut Value,
) -> Result<Value> {
    if !commands().contains(&command) {
        return Err(format!("Unknown command: {command}"));
    }
    match operation {
        "extension:dispose-session" | "extension:refresh-session" | "extension:invoke-action" => {
            return Ok(json!({"ok":true,"mode":"unchanged"}));
        }
        "extension:run-command" => {}
        _ => return Err(format!("Unknown operation: {operation}")),
    }
    let outcome = run(host, payload).await;
    if let Err(error) = &outcome {
        host.notice(Notice::Failure, error).await;
    }
    outcome
}
async fn run(host: &impl Host, payload: &Value) -> Result<Value> {
    let prefs = Preferences::from_payload(payload)?;
    let mode = host.resolve_argument(argument(payload)?).await?;
    host.hide().await?;
    if prefs.show_tips {
        host.notice(Notice::Animated, "Pasting...").await;
    }
    let original = host.read_text().await?;
    if original.is_empty() {
        return Err("No content in clipboard".into());
    }
    let text = formats::transform(&original, prefs);
    let mut content = Content::Text(text.clone());
    let mut actual = "Plain Text";
    let mut icon = "📋";
    match mode.as_str() {
        "JSON" => {
            if let Some(value) = formats::pretty_json5(&text) {
                content = Content::Text(value);
                actual = "JSON";
                icon = "📦";
            }
        }
        "URL" => {
            if let Some(value) = formats::extract_url(&text) {
                content = Content::Text(value.into());
                actual = "URL";
                icon = "🔗";
            }
        }
        "Number" => {
            if let Some(value) = formats::extract_number(&text).filter(|s| !s.is_empty()) {
                content = Content::Text(value);
                actual = "Number";
                icon = "🔢";
            }
        }
        "File" => {
            let candidate = host
                .read_file()
                .await?
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| text.clone());
            let path = formats::file_url_path(&candidate).unwrap_or(candidate);
            if host.path_exists(&path).await? {
                content = Content::File(path);
                actual = "File";
                icon = "📄";
            }
        }
        "Markdown Link" | "Markdown Image" => {
            let url = text.trim_matches(formats::js_whitespace);
            content = Content::Text(url.into());
            if formats::valid_markdown_url(url) {
                let title = if prefs.auto_fetch_title {
                    host.title(url).await.unwrap_or_default()
                } else {
                    String::new()
                };
                let prefix = if mode == "Markdown Image" { "!" } else { "" };
                content = Content::Text(format!("{prefix}[{title}]({url})"));
                actual = &mode;
                icon = if prefix.is_empty() { "📄" } else { "🏞️" };
            }
        }
        _ => {}
    }
    host.paste(&content).await?;
    if prefs.replace_clipboard {
        host.copy(&content).await?;
    }
    let message = if prefs.show_tips {
        format!("{icon} Paste as {actual}")
    } else {
        String::new()
    };
    if prefs.show_tips {
        host.notice(Notice::Success, &message).await;
    }
    Ok(json!({"ok":true,"mode":"no-view","message":message}))
}
