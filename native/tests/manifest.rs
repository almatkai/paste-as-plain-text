//! Frozen schema and original source metadata checks need no host checkout.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tezbar_paste_as_plain_text::{commands, Preferences, ICON};

#[test]
fn schema_three_preserves_original_command_preferences_platforms_and_icon() {
    let manifest: Value = serde_json::from_str(include_str!("../../extension.json")).unwrap();
    let source: Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
    let schema_bytes = include_bytes!("fixtures/extension-v3.schema.json");
    let schema: Value = serde_json::from_slice(schema_bytes).unwrap();
    let provenance: Value =
        serde_json::from_str(include_str!("fixtures/schema-provenance.json")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(schema_bytes)),
        provenance["sha256"]
    );
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&manifest)
        .unwrap();
    assert_eq!(manifest["schemaVersion"], 3);
    assert_eq!(manifest["id"], "com.tezbar.native.paste-as-plain-text");
    assert_eq!(manifest["slug"], source["name"]);
    assert_eq!(
        manifest["replaces"],
        json!([format!("raycast.{}", source["name"].as_str().unwrap())])
    );
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["execution"], "process");
    assert_eq!(manifest["protocolVersion"], 1);
    assert_eq!(manifest["author"], source["author"]);
    assert_eq!(manifest["platforms"], source["platforms"]);
    assert_eq!(manifest["preferences"], source["preferences"]);
    let mut defaults = json!({});
    for pref in manifest["preferences"].as_array().unwrap() {
        defaults[pref["name"].as_str().unwrap()] = pref["default"].clone();
    }
    assert_eq!(
        Preferences::from_payload(&json!({"preferences":defaults})).unwrap(),
        Preferences::default()
    );
    let entries = manifest["commands"].as_array().unwrap();
    assert_eq!(entries.len(), commands().len());
    assert_eq!(entries.len(), source["commands"].as_array().unwrap().len());
    for (entry, original) in entries.iter().zip(source["commands"].as_array().unwrap()) {
        assert!(commands().contains(&entry["name"].as_str().unwrap()));
        for (key, value) in original.as_object().unwrap() {
            assert_eq!(&entry[key], value, "original command field {key}");
        }
        assert_eq!(entry["runtime"], "rust");
        assert_eq!(entry["concurrency"], "serialize");
        assert!(entry["refreshMs"].is_null());
    }
    assert_eq!(
        manifest["capabilities"],
        json!([
            "clipboard.read",
            "clipboard.write",
            "clipboard.writeFiles",
            "clipboard.paste",
            "clipboard.pasteText",
            "window.visibility"
        ])
    );
    assert_eq!(manifest["files"], json!({}));
    assert_eq!(manifest["artifacts"], json!({}));
    for entry in manifest["verification"].as_array().unwrap() {
        assert_eq!(
            entry["state"], "not-run",
            "offline tests are not desktop QA"
        );
    }
    let icon =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest["icon"].as_str().unwrap());
    assert_eq!(std::fs::read(icon).unwrap(), ICON);
    assert_eq!(include_bytes!("../../assets/paste-icon.png"), ICON);
}
