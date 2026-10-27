//! Actual pinned beta sources plus unchanged adapter. Only Tauri, router and OS runner are stubbed.
extern crate self as tauri;
#[derive(Clone)]
pub struct AppHandle;
pub trait Emitter {
    fn emit(&self, channel: &str, value: serde_json::Value) -> std::result::Result<(), String>;
}
impl Emitter for AppHandle {
    fn emit(&self, _: &str, _: serde_json::Value) -> std::result::Result<(), String> {
        Ok(())
    }
}
#[rustfmt::skip]
#[path="fixtures/beta/common.rs"]
pub mod pinned_common;
#[rustfmt::skip]
#[path="fixtures/beta/v2_sdk.rs"]
pub mod v2_sdk;
pub mod native_core {
    pub use crate::pinned_common as common;
}
#[path = "../tezbar_adapter.rs"]
mod adapter;

use async_trait::async_trait;
use pinned_common::{CoreContext, CorePaths};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tezbar_paste_as_plain_text::{
    os_helpers::{CommandPlan, Platform, Runner},
    Content, Host, SLUG,
};
#[derive(Clone, Default)]
struct StubRunner {
    plans: Arc<Mutex<Vec<CommandPlan>>>,
    output: String,
    fail: bool,
}
#[async_trait]
impl Runner for StubRunner {
    async fn run(&self, plan: CommandPlan) -> Result<String, String> {
        self.plans.lock().unwrap().push(plan);
        if self.fail {
            Err("helper failed".into())
        } else {
            Ok(self.output.clone())
        }
    }
}
fn context(dir: &std::path::Path) -> CoreContext {
    CoreContext::headless(CorePaths {
        data_dir: dir.join("data"),
        config_dir: dir.join("config"),
        resource_dir: dir.join("resources"),
        temp_dir: dir.join("tmp"),
        home_dir: dir.into(),
    })
}
#[tokio::test]
async fn unchanged_adapter_compiles_with_actual_beta_and_routes_real_calls() {
    assert_eq!(adapter::commands(), &["paste-as-plain-text"]);
    assert!(!adapter::ICON.is_empty());
    assert!(adapter::MANIFEST.contains("schemaVersion"));
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let calls = Arc::new(Mutex::new(vec![]));
    let log = calls.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        let log = log.clone();
        Box::pin(async move {
            log.lock().unwrap().push((channel.clone(), args));
            match channel.as_str() {
                "clipboard:read" => Ok(json!(" 42 ")),
                "window:hide" => Ok(Value::Null),
                "clipboard:write" | "core:platform:paste-text" | "core:platform:paste" => {
                    Ok(json!({"ok":true}))
                }
                _ => panic!("Invented route: {channel}"),
            }
        })
    }))
    .unwrap();
    let result=adapter::handle(&ctx,"paste-as-plain-text","extension:run-command",&json!({"argumentValues":{"advancedPasteFormat":"Number"},"preferences":{"replaceClipboard":true}}),&mut Value::Null).await.unwrap();
    assert_eq!(result["mode"], "no-view");
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            ("window:hide".into(), vec![]),
            ("clipboard:read".into(), vec![]),
            ("core:platform:paste-text".into(), vec![json!("42")]),
            ("clipboard:write".into(), vec![json!("42")])
        ]
    );
    let host = adapter::CoreHost {
        ctx: &ctx,
        runner: StubRunner::default(),
        platform: Platform::MacOS,
    };
    host.paste(&Content::File("/fixture/Қазақ ' \" $(x) `x`.pdf".into()))
        .await
        .unwrap();
    assert_eq!(
        calls.lock().unwrap().last().unwrap(),
        &("core:platform:paste".into(), vec![])
    );
    assert_eq!(host.runner.plans.lock().unwrap().len(), 1);
    assert!(!host
        .path_exists(temp.path().join("missing").to_str().unwrap())
        .await
        .unwrap());
    let path = temp.path().join("fixture.txt");
    std::fs::write(&path, "fixture").unwrap();
    assert!(host.path_exists(path.to_str().unwrap()).await.unwrap());
}
#[tokio::test]
async fn rpc_errors_and_helper_errors_are_not_successful_pastes() {
    for error in [
        Err("router failure".into()),
        Ok(json!({"ok":false,"error":"envelope failure"})),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        ctx.set_router(Arc::new(move |_, _| {
            let error = error.clone();
            Box::pin(async move { error })
        }))
        .unwrap();
        let host = adapter::CoreHost {
            ctx: &ctx,
            runner: StubRunner::default(),
            platform: Platform::Windows,
        };
        assert!(host.hide().await.is_err());
        assert!(host.paste(&Content::Text("x".into())).await.is_err());
        assert!(host.copy(&Content::Text("x".into())).await.is_err());
        assert!(host.read_text().await.is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path()); // Uninitialized router must never be reached after helper failure.
    let host = adapter::CoreHost {
        ctx: &ctx,
        runner: StubRunner {
            fail: true,
            ..Default::default()
        },
        platform: Platform::Windows,
    };
    assert_eq!(
        host.paste(&Content::File("C:\\x.txt".into()))
            .await
            .unwrap_err(),
        "helper failed"
    );
    assert!(host.read_file().await.is_err());
}
#[tokio::test]
async fn sdk_cache_restart_concurrency_isolation_and_unknown_error_keys() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    v2_sdk::update_store(&ctx,SLUG,|v|{*v=json!({"ok":false,"error":"keep","message":"keep","future":{"x":1},"Args":{"future":2}});Ok(())}).await.unwrap();
    let host = adapter::CoreHost {
        ctx: &ctx,
        runner: StubRunner::default(),
        platform: Platform::MacOS,
    };
    assert_eq!(host.resolve_argument(None).await.unwrap(), "");
    assert_eq!(host.resolve_argument(Some("JSON")).await.unwrap(), "JSON");
    let restarted = context(temp.path());
    let new_host = adapter::CoreHost {
        ctx: &restarted,
        runner: StubRunner::default(),
        platform: Platform::MacOS,
    };
    assert_eq!(new_host.resolve_argument(None).await.unwrap(), "JSON");
    assert_eq!(new_host.resolve_argument(Some("")).await.unwrap(), "");
    assert_eq!(host.resolve_argument(None).await.unwrap(), "");
    let mut tasks = vec![];
    for i in 0..30 {
        let ctx = context(temp.path());
        tasks.push(tokio::spawn(async move {
            let host = adapter::CoreHost {
                ctx: &ctx,
                runner: StubRunner::default(),
                platform: Platform::MacOS,
            };
            let expected = format!("case{i}");
            assert_eq!(
                host.resolve_argument(Some(&expected)).await.unwrap(),
                expected
            );
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    let store = v2_sdk::read_store(&ctx, SLUG).await.unwrap();
    assert_eq!(store["ok"], false);
    assert_eq!(store["error"], "keep");
    assert_eq!(store["message"], "keep");
    assert_eq!(store["future"], json!({"x":1}));
    assert_eq!(store["Args"]["future"], 2);
    assert!(!ctx.paths.data_dir.join("extensions").exists());
    assert!(!ctx.paths.data_dir.join("Cache").exists());
    let other = tempfile::tempdir().unwrap();
    let isolated = context(other.path());
    assert_eq!(
        v2_sdk::read_store(&isolated, SLUG).await.unwrap(),
        json!({})
    );
}
#[tokio::test]
async fn malformed_cache_is_never_silently_replaced() {
    for bytes in [
        b"{broken".as_slice(),
        b"[]",
        br#"{"Args":12}"#,
        br#"{"Args":{"PasteAs":false}}"#,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let ctx = context(temp.path());
        let path = ctx
            .paths
            .data_dir
            .join("native-extensions-v2")
            .join(SLUG)
            .join("storage.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let host = adapter::CoreHost {
            ctx: &ctx,
            runner: StubRunner::default(),
            platform: Platform::MacOS,
        };
        assert!(host.resolve_argument(None).await.is_err());
        assert!(host.resolve_argument(Some("")).await.is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn failure_reveals_launcher_for_actual_renderer_error_even_without_tips() {
    let temp = tempfile::tempdir().unwrap();
    let calls = Arc::new(Mutex::new(vec![]));
    let events = Arc::new(Mutex::new(vec![]));
    let sink = events.clone();
    let ctx = context(temp.path()).with_event_sink(Arc::new(move |channel, value| {
        sink.lock().unwrap().push((channel.to_owned(), value));
    }));
    let log = calls.clone();
    ctx.set_router(Arc::new(move |channel, args| {
        let log = log.clone();
        Box::pin(async move {
            assert!(args.is_empty());
            log.lock().unwrap().push(channel.clone());
            match channel.as_str() {
                "window:hide" | "window:show" => Ok(Value::Null),
                "clipboard:read" => Ok(json!("")),
                _ => panic!("Unexpected route {channel}"),
            }
        })
    }))
    .unwrap();
    let result = adapter::handle(
        &ctx,
        "paste-as-plain-text",
        "extension:run-command",
        &json!({"preferences":{"showTips":false}}),
        &mut Value::Null,
    )
    .await;
    assert_eq!(result.unwrap_err(), "No content in clipboard");
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["window:hide", "clipboard:read", "window:show"]
    );
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "extension:effect");
    assert_eq!(events[0].1["style"], "failure");
}
