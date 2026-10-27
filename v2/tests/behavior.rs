use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Mutex;
use tezbar_paste_as_plain_text::{formats, handle, Content, Host, Notice, Preferences, Result};

struct Recording {
    scenario: Value,
    cache: Mutex<String>,
    events: Mutex<Vec<Value>>,
    fail: Option<&'static str>,
}
impl Recording {
    fn new(scenario: Value) -> Self {
        Self {
            scenario,
            cache: Mutex::new(String::new()),
            events: Mutex::new(vec![]),
            fail: None,
        }
    }
    fn record(&self, name: &str, value: Value) -> Result<()> {
        self.events.lock().unwrap().push(json!([name, value]));
        if self.fail == Some(name) {
            Err(format!("{name} failed"))
        } else {
            Ok(())
        }
    }
    fn contents(&self, name: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v[0] == name)
            .map(|v| v[1].clone())
            .collect()
    }
}
fn value(content: &Content) -> Value {
    match content {
        Content::Text(s) => json!(s),
        Content::File(s) => json!({"file":s}),
    }
}
#[async_trait]
impl Host for Recording {
    async fn resolve_argument(&self, supplied: Option<&str>) -> Result<String> {
        self.record("cache", json!(supplied))?;
        let mut cache = self.cache.lock().unwrap();
        if let Some(s) = supplied {
            *cache = s.into();
        }
        Ok(cache.clone())
    }
    async fn hide(&self) -> Result<()> {
        self.record("hide", Value::Null)
    }
    async fn read_text(&self) -> Result<String> {
        self.record("read", Value::Null)?;
        Ok(self.scenario["text"].as_str().unwrap_or("").into())
    }
    async fn read_file(&self) -> Result<Option<String>> {
        self.record("readFile", Value::Null)?;
        Ok(self.scenario["file"].as_str().map(str::to_owned))
    }
    async fn path_exists(&self, path: &str) -> Result<bool> {
        self.record("exists", json!(path))?;
        Ok(self.scenario["existing"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(path))))
    }
    async fn title(&self, url: &str) -> Result<String> {
        self.record("title", json!(url))?;
        if self.scenario["titleFailure"] == true {
            return Err("title failed".into());
        }
        let html = scraper::Html::parse_document(
            self.scenario["html"]
                .as_str()
                .unwrap_or("<title>Fixture &amp; Title</title>"),
        );
        Ok(html
            .select(&scraper::Selector::parse("title").unwrap())
            .flat_map(|n| n.text())
            .collect())
    }
    async fn paste(&self, content: &Content) -> Result<()> {
        self.record("paste", value(content))
    }
    async fn copy(&self, content: &Content) -> Result<()> {
        self.record("copy", value(content))
    }
    async fn notice(&self, style: Notice, title: &str) {
        self.record(&format!("notice:{style:?}"), json!(title))
            .unwrap();
    }
}
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/oracle.json")).unwrap()
}
#[test]
fn source_transform_extraction_and_validator_oracles() {
    let oracle = oracle();
    for c in oracle["transforms"].as_array().unwrap() {
        let prefs = Preferences::from_payload(&json!({"preferences":c["preferences"]})).unwrap();
        assert_eq!(
            formats::transform(c["text"].as_str().unwrap(), prefs),
            c["expected"].as_str().unwrap(),
            "{c}"
        );
    }
    for c in oracle["extraction"].as_array().unwrap() {
        let s = c["text"].as_str().unwrap();
        assert_eq!(formats::extract_url(s), c["url"].as_str(), "url {c}");
        assert_eq!(
            formats::extract_number(s).unwrap_or_default(),
            c["number"].as_str().unwrap(),
            "number {c}"
        );
    }
    for c in oracle["validation"].as_array().unwrap() {
        assert_eq!(
            formats::valid_markdown_url(c["text"].as_str().unwrap()),
            c["expected"].as_bool().unwrap(),
            "{c}"
        );
    }
}
#[test]
fn json5_source_oracle_and_explicit_native_parser_gaps() {
    for c in oracle()["jsonCases"].as_array().unwrap() {
        let text = c["text"].as_str().unwrap();
        let expected = c["expected"].as_str().map(str::to_owned);
        let actual = formats::pretty_json5(text);
        match text {
            // Native json5 0.4.1 cannot represent unpaired surrogates or out-of-i64 integer tokens.
            "\"\\uD800\""
            | "9223372036854775808"
            | "0xffffffffffffffff"
            | "+Infinity"
            | "+NaN"
            | "-0xFF"
            | "1e400" => {
                assert!(expected.is_some());
                assert_eq!(actual, None, "documented native fallback for {text}");
            }
            _ => assert_eq!(actual, expected, "JSON5 source oracle {text}"),
        }
    }
}
#[tokio::test]
async fn all_seven_modes_against_executed_original_command() {
    for c in oracle()["cases"].as_array().unwrap() {
        let host = Recording::new(c.clone());
        let reply=handle(&host,"paste-as-plain-text","extension:run-command",&json!({"argumentValues":{"advancedPasteFormat":c["mode"]},"preferences":c.get("preferences").cloned().unwrap_or(json!({}))}),&mut Value::Null).await;
        assert_eq!(reply.is_err(), c["failure"].as_bool().unwrap(), "{c}");
        assert_eq!(
            host.contents("paste"),
            c["pastes"].as_array().unwrap().clone(),
            "{c}"
        );
        assert_eq!(
            host.contents("copy"),
            c["copies"].as_array().unwrap().clone(),
            "{c}"
        );
    }
}
#[tokio::test]
async fn defaults_empty_whitespace_preferences_and_argument_presence() {
    assert_eq!(
        Preferences::from_payload(&json!({})).unwrap(),
        Preferences::default()
    );
    let host = Recording::new(json!({"text":" value 12 "}));
    for (args, expected) in [
        (json!({"advancedPasteFormat":"Number"}), "12"),
        (json!({}), "12"),
        (json!({"advancedPasteFormat":""}), "value 12"),
        (json!({}), "value 12"),
    ] {
        handle(
            &host,
            "paste-as-plain-text",
            "extension:run-command",
            &json!({"argumentValues":args}),
            &mut Value::Null,
        )
        .await
        .unwrap();
        assert_eq!(host.contents("paste").last().unwrap(), expected);
    }
    let host = Recording::new(json!({"text":"   "}));
    handle(
        &host,
        "paste-as-plain-text",
        "extension:run-command",
        &json!({}),
        &mut Value::Null,
    )
    .await
    .unwrap();
    assert_eq!(host.contents("paste"), vec![json!("")]);
    let host = Recording::new(json!({"text":""}));
    assert!(handle(
        &host,
        "paste-as-plain-text",
        "extension:run-command",
        &json!({"preferences":{"showTips":false}}),
        &mut Value::Null
    )
    .await
    .is_err());
    assert_eq!(
        host.contents("notice:Failure"),
        vec![json!("No content in clipboard")]
    );
    assert!(host.contents("paste").is_empty());
}
#[tokio::test]
async fn errors_propagate_without_duplicate_paste_or_false_success() {
    for point in ["cache", "hide", "read", "paste", "copy"] {
        let mut host = Recording::new(json!({"text":"hello"}));
        host.fail = Some(point);
        let result = handle(
            &host,
            "paste-as-plain-text",
            "extension:run-command",
            &json!({"preferences":{"replaceClipboard":true,"showTips":false}}),
            &mut Value::Null,
        )
        .await;
        assert!(result.unwrap_err().contains(point));
        assert!(host.contents("notice:Success").is_empty());
        assert!(host.contents("notice:Animated").is_empty());
        assert_eq!(host.contents("notice:Failure").len(), 1);
        assert!(host.contents("paste").len() <= 1);
    }
    let host = Recording::new(json!({"text":"hello"}));
    handle(
        &host,
        "paste-as-plain-text",
        "extension:run-command",
        &json!({"preferences":{"replaceClipboard":true}}),
        &mut Value::Null,
    )
    .await
    .unwrap();
    let events = host.events.lock().unwrap();
    let pos = |name: &str| events.iter().position(|v| v[0] == name).unwrap();
    assert!(pos("paste") < pos("copy") && pos("copy") < pos("notice:Success"));
}
#[tokio::test]
async fn file_clipboard_metadata_precedes_text_but_missing_file_falls_back() {
    for (file, existing, expected) in [
        ("/real", vec!["/real"], json!({"file":"/real"})),
        ("/gone", vec!["/text"], json!("/text")),
    ] {
        let host = Recording::new(json!({"text":"/text","file":file,"existing":existing}));
        handle(&host,"paste-as-plain-text","extension:run-command",&json!({"argumentValues":{"advancedPasteFormat":"File"},"preferences":{"replaceClipboard":true}}),&mut Value::Null).await.unwrap();
        assert_eq!(host.contents("paste"), vec![expected.clone()]);
        assert_eq!(host.contents("copy"), vec![expected]);
    }
}
#[tokio::test]
async fn invalid_input_and_lifecycle_never_perform_paste() {
    let host = Recording::new(json!({"text":"hello"}));
    assert!(handle(
        &host,
        "unknown",
        "extension:run-command",
        &json!({}),
        &mut Value::Null
    )
    .await
    .is_err());
    for operation in [
        "extension:dispose-session",
        "extension:refresh-session",
        "extension:invoke-action",
    ] {
        assert_eq!(
            handle(
                &host,
                "paste-as-plain-text",
                operation,
                &json!({}),
                &mut Value::Null
            )
            .await
            .unwrap()["mode"],
            "unchanged"
        );
    }
    assert_eq!(
        handle(
            &host,
            "paste-as-plain-text",
            "extension:invented-operation",
            &json!({}),
            &mut Value::Null,
        )
        .await
        .unwrap_err(),
        "Unknown operation: extension:invented-operation"
    );
    for payload in [
        json!({"argumentValues":{"advancedPasteFormat":null}}),
        json!({"preferences":{"trimStart":"false"}}),
    ] {
        assert!(handle(
            &host,
            "paste-as-plain-text",
            "extension:run-command",
            &payload,
            &mut Value::Null
        )
        .await
        .is_err());
    }
    assert!(host.contents("paste").is_empty());
}

#[test]
fn json5_resource_limits_preserve_plain_fallback_without_parsing_excessive_nesting() {
    let too_deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    assert_eq!(formats::pretty_json5(&too_deep), None);
    assert_eq!(formats::pretty_json5(&" ".repeat(1024 * 1024 + 1)), None);
    assert!(formats::pretty_json5(&format!("{}0{}", "[".repeat(64), "]".repeat(64))).is_some());
    let brackets_in_string = format!("'{}'", "[".repeat(100));
    assert!(formats::pretty_json5(&brackets_in_string).is_some());
}

#[tokio::test]
async fn source_argument_cache_oracle() {
    let host = Recording::new(json!({"text":"value 12"}));
    for case in oracle()["argumentsOracle"].as_array().unwrap() {
        assert_eq!(
            host.resolve_argument(case["supplied"].as_str())
                .await
                .unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
}
