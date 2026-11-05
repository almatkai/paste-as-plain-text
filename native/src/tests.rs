//! Adapter tests with the SDK's recording host. No real clipboard, window,
//! keyboard or helper process is touched: the OS helper runner is stubbed.
use crate::host::ProcessHost;
use crate::migration;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tezbar_extension_sdk::{read_store, update_store, ExtensionPaths, TestContext};
use tezbar_paste_as_plain_text::{
    handle,
    os_helpers::{CommandPlan, Platform, Runner},
    SLUG,
};

const CAPS: &[&str] = &[
    "clipboard.read",
    "clipboard.write",
    "clipboard.writeFiles",
    "clipboard.paste",
    "clipboard.pasteText",
    "window.visibility",
];

#[derive(Clone, Default)]
struct StubRunner {
    plans: Arc<Mutex<Vec<CommandPlan>>>,
    output: String,
}
#[async_trait]
impl Runner for StubRunner {
    async fn run(&self, plan: CommandPlan) -> Result<String, String> {
        self.plans.lock().unwrap().push(plan);
        Ok(self.output.clone())
    }
}

fn fixture(dir: &std::path::Path, caps: &[&str]) -> TestContext {
    TestContext::new(
        ExtensionPaths {
            package_dir: dir.join("package"),
            data_dir: dir.join("data"),
            temp_dir: dir.join("tmp"),
        },
        caps,
    )
}
fn methods(calls: &[Value]) -> Vec<String> {
    calls
        .iter()
        .map(|c| c["method"].as_str().unwrap().to_owned())
        .collect()
}
async fn run(t: &TestContext, runner: StubRunner, payload: Value) -> Result<Value, String> {
    let host = ProcessHost {
        ctx: &t.context,
        runner,
        platform: Platform::MacOS,
    };
    let mut state = json!({});
    t.run(handle(
        &host,
        SLUG,
        "extension:run-command",
        &payload,
        &mut state,
    ))
    .await
}

#[tokio::test]
async fn number_mode_pastes_through_typed_host_and_caches_choice() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    t.reply(Ok(json!(null))); // window.visibility false
    t.reply(Ok(json!("Total 123,456.7 units"))); // clipboard.read
    t.reply(Ok(json!({"ok":true}))); // pasteText
    let result = run(
        &t,
        StubRunner::default(),
        json!({"preferences":{},"argumentValues":{"advancedPasteFormat":"Number"}}),
    )
    .await
    .unwrap();
    assert_eq!(
        result,
        json!({"ok":true,"mode":"no-view","message":"🔢 Paste as Number"})
    );
    let calls = t.calls();
    assert_eq!(
        methods(&calls),
        [
            "host.window.visibility",
            "host.clipboard.read",
            "host.clipboard.pasteText"
        ]
    );
    assert_eq!(calls[0]["params"], json!({"visible":false}));
    assert_eq!(calls[1]["params"], json!({"format":"text"}));
    assert_eq!(calls[2]["params"], json!({"text":"123456.7"}));
    let store = read_store(&t.context, SLUG).await.unwrap();
    assert_eq!(store["Args"]["PasteAs"], "Number");
    assert!(t.events().is_empty(), "no custom events: host rejects them");
}

#[tokio::test]
async fn omitted_argument_reads_cache_and_explicit_empty_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    update_store(&t.context, SLUG, |s| {
        s["Args"] = json!({"PasteAs":"URL","other":1});
        s["ok"] = json!(false);
        Ok(())
    })
    .await
    .unwrap();
    for reply in [
        json!(null),
        json!("see https://example.com/a?b=1 now"),
        json!(null),
    ] {
        t.reply(Ok(reply));
    }
    let result = run(
        &t,
        StubRunner::default(),
        json!({"preferences":{"showTips":false}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "");
    assert_eq!(t.calls()[2]["params"]["text"], "https://example.com/a?b=1");

    for reply in [
        json!(null),
        json!("see https://example.com now"),
        json!(null),
    ] {
        t.reply(Ok(reply));
    }
    let result = run(
        &t,
        StubRunner::default(),
        json!({"argumentValues":{"advancedPasteFormat":""}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "📋 Paste as Plain Text");
    assert_eq!(
        t.calls()[5]["params"]["text"],
        "see https://example.com now"
    );
    let store = read_store(&t.context, SLUG).await.unwrap();
    assert_eq!(store["Args"], json!({"PasteAs":"","other":1}));
    assert_eq!(store["ok"], false, "unrelated root keys survive");
}

#[tokio::test]
async fn empty_clipboard_fails_reveals_window_and_never_pastes() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    t.reply(Ok(json!(null)));
    t.reply(Ok(json!("")));
    t.reply(Ok(json!(null)));
    let error = run(&t, StubRunner::default(), json!({})).await.unwrap_err();
    assert_eq!(error, "No content in clipboard");
    let calls = t.calls();
    assert_eq!(
        methods(&calls),
        [
            "host.window.visibility",
            "host.clipboard.read",
            "host.window.visibility"
        ]
    );
    assert_eq!(calls[2]["params"], json!({"visible":true}));
}

#[tokio::test]
async fn failed_paste_is_an_error_without_success_or_recopy() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    t.reply(Ok(json!(null)));
    t.reply(Ok(json!("text")));
    t.reply(Ok(json!({"ok":false,"error":"Accessibility denied"})));
    t.reply(Err("reveal failed".into()));
    let error = run(
        &t,
        StubRunner::default(),
        json!({"preferences":{"replaceClipboard":true}}),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error, "Accessibility denied",
        "reveal failure does not mask"
    );
    assert_eq!(
        methods(&t.calls()),
        [
            "host.window.visibility",
            "host.clipboard.read",
            "host.clipboard.pasteText",
            "host.window.visibility"
        ]
    );
}

#[tokio::test]
async fn replace_clipboard_recopies_text_after_successful_paste() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    for reply in [json!(null), json!("{a:1,}"), json!(null), json!(null)] {
        t.reply(Ok(reply));
    }
    let result = run(
        &t,
        StubRunner::default(),
        json!({"preferences":{"replaceClipboard":true},"argumentValues":{"advancedPasteFormat":"JSON"}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "📦 Paste as JSON");
    let calls = t.calls();
    assert_eq!(
        methods(&calls)[2..],
        ["host.clipboard.pasteText", "host.clipboard.write"]
    );
    assert_eq!(calls[2]["params"]["text"], "{\n  \"a\": 1\n}");
    assert_eq!(calls[3]["params"]["text"], "{\n  \"a\": 1\n}");
}

#[tokio::test]
async fn regular_file_uses_typed_write_files_then_paste() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note with space.txt");
    std::fs::write(&file, b"x").unwrap();
    let t = fixture(dir.path(), CAPS);
    for reply in [
        json!(null),
        json!("ignored text"),
        json!(true),
        json!(null),
        json!(true),
    ] {
        t.reply(Ok(reply));
    }
    let runner = StubRunner {
        output: format!("{}\n", file.display()),
        ..Default::default()
    };
    let result = run(
        &t,
        runner.clone(),
        json!({"preferences":{"replaceClipboard":true},"argumentValues":{"advancedPasteFormat":"File"}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "📄 Paste as File");
    let calls = t.calls();
    assert_eq!(
        methods(&calls)[2..],
        [
            "host.clipboard.writeFiles",
            "host.clipboard.paste",
            "host.clipboard.writeFiles"
        ]
    );
    assert_eq!(
        calls[2]["params"],
        json!({"paths":[file.to_str().unwrap()]})
    );
    assert_eq!(calls[3]["params"], json!({}));
    // Only the clipboard read used the local helper (no write helper).
    let plans = runner.plans.lock().unwrap();
    assert_eq!(plans.len(), 1);
    assert!(!plans[0].args.contains(&"--".to_owned()));
}

#[tokio::test]
async fn directory_uses_constant_script_helper_with_path_as_data() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("folder");
    std::fs::create_dir(&folder).unwrap();
    let t = fixture(dir.path(), CAPS);
    for reply in [json!(null), json!(folder.to_str().unwrap()), json!(null)] {
        t.reply(Ok(reply));
    }
    let runner = StubRunner::default(); // no clipboard file reference
    let result = run(
        &t,
        runner.clone(),
        json!({"argumentValues":{"advancedPasteFormat":"File"}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "📄 Paste as File");
    assert_eq!(methods(&t.calls())[2..], ["host.clipboard.paste"]);
    let plans = runner.plans.lock().unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[1].args.last().unwrap(), folder.to_str().unwrap());
    assert!(!plans[1].args[1].contains(folder.to_str().unwrap()));
}

#[tokio::test]
async fn relative_or_missing_paths_fall_back_to_text() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    for reply in [json!(null), json!("Cargo.toml"), json!(null)] {
        t.reply(Ok(reply));
    }
    let result = run(
        &t,
        StubRunner::default(),
        json!({"argumentValues":{"advancedPasteFormat":"File"}}),
    )
    .await
    .unwrap();
    assert_eq!(result["message"], "📋 Paste as Plain Text");
    let calls = t.calls();
    assert_eq!(calls[2]["method"], "host.clipboard.pasteText");
    assert_eq!(calls[2]["params"]["text"], "Cargo.toml");
}

#[tokio::test]
async fn local_file_helpers_require_grants_before_os_work() {
    use tezbar_paste_as_plain_text::{Content, Host};
    let dir = tempfile::tempdir().unwrap();
    for caps in [vec![], vec!["clipboard.read"], vec!["clipboard.paste"]] {
        let t = fixture(dir.path(), &caps);
        let runner = StubRunner::default();
        let host = ProcessHost {
            ctx: &t.context,
            runner: runner.clone(),
            platform: Platform::MacOS,
        };
        if !caps.contains(&"clipboard.read") {
            assert!(host
                .read_file()
                .await
                .unwrap_err()
                .contains("clipboard.read"));
        }
        assert!(host
            .copy(&Content::File(dir.path().display().to_string()))
            .await
            .unwrap_err()
            .contains("clipboard.writeFiles"));
        assert!(host
            .paste(&Content::File(dir.path().display().to_string()))
            .await
            .is_err());
        assert!(runner.plans.lock().unwrap().is_empty());
        assert!(t.calls().is_empty());
    }
}

#[tokio::test]
async fn ungranted_capability_fails_before_any_effect() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), &["clipboard.read"]);
    let error = run(&t, StubRunner::default(), json!({})).await.unwrap_err();
    assert!(error.contains("not granted: window.visibility"), "{error}");
    assert!(t.calls().is_empty());
}

#[tokio::test]
async fn failed_clipboard_read_preserves_the_host_error() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    t.reply(Ok(json!(null)));
    t.reply(Ok(json!({"ok":false,"message":"Clipboard access denied"})));
    t.reply(Ok(json!(null)));
    assert_eq!(
        run(&t, StubRunner::default(), json!({})).await.unwrap_err(),
        "Clipboard access denied"
    );
    assert_eq!(
        methods(&t.calls()),
        [
            "host.window.visibility",
            "host.clipboard.read",
            "host.window.visibility"
        ]
    );
}

#[tokio::test]
async fn corrupt_cache_fails_without_replacing_data() {
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    update_store(&t.context, SLUG, |s| {
        s["Args"] = json!({"PasteAs":7});
        Ok(())
    })
    .await
    .unwrap();
    t.reply(Ok(json!(null)));
    let error = run(
        &t,
        StubRunner::default(),
        json!({"argumentValues":{"advancedPasteFormat":"URL"}}),
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Invalid native Args/PasteAs cache");
    assert_eq!(
        read_store(&t.context, SLUG).await.unwrap()["Args"]["PasteAs"],
        7
    );
}

// ---- migration ----

fn grant(t: &mut TestContext, key: &str, path: &std::path::Path) {
    t.context.migration_paths.insert(key.into(), path.into());
}

#[tokio::test]
async fn imports_native_store_choice_before_legacy_cache_and_keeps_sources() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.json");
    let legacy = dir.path().join("Args.json");
    let native_bytes = br#"{"Args":{"PasteAs":"Markdown Link"},"other":1}"#;
    let legacy_bytes = br#"{"PasteAs":"JSON"}"#;
    std::fs::write(&native, native_bytes).unwrap();
    std::fs::write(&legacy, legacy_bytes).unwrap();
    let mut t = fixture(dir.path(), CAPS);
    grant(&mut t, migration::NATIVE_STORE, &native);
    grant(&mut t, migration::LEGACY_CACHE[0], &legacy);
    let store = migration::initialize(&t.context).await.unwrap();
    assert_eq!(store["Args"]["PasteAs"], "Markdown Link");
    assert!(store.get("other").is_none(), "only the choice is imported");
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    assert_eq!(std::fs::read(&legacy).unwrap(), legacy_bytes);
    // Idempotent: a later change is not overwritten by re-import.
    update_store(&t.context, SLUG, |s| {
        s["Args"]["PasteAs"] = json!("URL");
        Ok(())
    })
    .await
    .unwrap();
    let store = migration::initialize(&t.context).await.unwrap();
    assert_eq!(store["Args"]["PasteAs"], "URL");
    assert!(t.calls().is_empty(), "migration needs no host services");
}

#[tokio::test]
async fn imports_v1_cache_and_ignores_malformed_v1_like_the_shim() {
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.json");
    let good = dir.path().join("good.json");
    std::fs::write(&broken, b"{nope").unwrap();
    std::fs::write(&good, br#"{"PasteAs":"Number","Other":"x"}"#).unwrap();
    let mut t = fixture(dir.path(), CAPS);
    grant(&mut t, migration::LEGACY_CACHE[0], &broken);
    grant(&mut t, migration::LEGACY_CACHE[1], &good);
    let store = migration::initialize(&t.context).await.unwrap();
    assert_eq!(store["Args"], json!({"PasteAs":"Number"}));

    // A read failure is also empty in the JS shim; it must not block readiness
    // or prevent a valid alternative legacy cache from being imported.
    let dir = tempfile::tempdir().unwrap();
    let mut t = fixture(dir.path(), CAPS);
    grant(&mut t, migration::LEGACY_CACHE[0], dir.path());
    grant(&mut t, migration::LEGACY_CACHE[1], &good);
    assert_eq!(
        migration::initialize(&t.context).await.unwrap()["Args"]["PasteAs"],
        "Number"
    );

    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    let store = migration::initialize(&t.context).await.unwrap();
    assert_eq!(store, json!({migration::IMPORTED: true}));
}

#[tokio::test]
async fn existing_choice_wins_and_corrupt_data_never_blocks_readiness() {
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.json");
    std::fs::write(&native, br#"{"Args":{"PasteAs":"JSON"}}"#).unwrap();
    let mut t = fixture(dir.path(), CAPS);
    grant(&mut t, migration::NATIVE_STORE, &native);
    update_store(&t.context, SLUG, |s| {
        s["Args"] = json!({"PasteAs":""});
        Ok(())
    })
    .await
    .unwrap();
    let store = migration::initialize(&t.context).await.unwrap();
    assert_eq!(store["Args"]["PasteAs"], "");

    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("native.json");
    std::fs::write(&native, br#"{"Args":{"PasteAs":false}}"#).unwrap();
    let mut t = fixture(dir.path(), CAPS);
    grant(&mut t, migration::NATIVE_STORE, &native);
    let legacy = dir.path().join("Args.json");
    std::fs::write(&legacy, br#"{"PasteAs":"URL"}"#).unwrap();
    grant(&mut t, migration::LEGACY_CACHE[0], &legacy);
    // Skipped (not imported from a lower-priority source, not marked done).
    assert!(migration::initialize(&t.context).await.is_ok());
    assert_eq!(read_store(&t.context, SLUG).await.unwrap(), json!({}));
    assert_eq!(
        std::fs::read(&native).unwrap(),
        br#"{"Args":{"PasteAs":false}}"#
    );

    // Corrupt current store: readiness succeeds, bytes stay, commands report it.
    let dir = tempfile::tempdir().unwrap();
    let t = fixture(dir.path(), CAPS);
    let path = dir.path().join("data").join(SLUG).join("storage.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    assert!(migration::initialize(&t.context).await.is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    t.reply(Ok(json!(null)));
    assert!(run(&t, StubRunner::default(), json!({})).await.is_err());
}
