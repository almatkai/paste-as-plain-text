use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use tezbar_paste_as_plain_text::{Preferences, ICON, MANIFEST};
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn canonical_manifest_schema_original_metadata_and_icon_provenance() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    let schema: Value =
        serde_json::from_str(include_str!("fixtures/beta/extension-v2.schema.json")).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&manifest)
        .unwrap();
    assert_eq!(manifest["id"], "com.tezbar.native.paste-as-plain-text");
    assert_eq!(manifest["replaces"], json!(["raycast.paste-as-plain-text"]));
    assert_eq!(
        manifest["icon"],
        "/extensions/icons/paste-as-plain-text.png"
    );
    assert!(manifest.get("author").is_none());
    let source: Value = serde_json::from_str(include_str!("fixtures/source/package.json")).unwrap();
    assert_eq!(manifest["preferences"], source["preferences"]);
    assert_eq!(
        manifest["commands"][0]["arguments"],
        source["commands"][0]["arguments"]
    );
    assert_eq!(manifest["platforms"], source["platforms"]);
    assert_eq!(manifest["verification"].as_array().unwrap().len(), 2);
    for entry in manifest["verification"].as_array().unwrap() {
        assert_eq!(entry["state"], "not-run");
    }
    assert_eq!(ICON.len(), 290_928);
    assert_eq!(
        digest(ICON),
        "290e0ae7fdd36df2d095e28bd5b9292c470dc3d0273e23b978500935e44b67dd"
    );
    let mut defaults = json!({});
    for pref in manifest["preferences"].as_array().unwrap() {
        defaults[pref["name"].as_str().unwrap()] = pref["default"].clone();
    }
    assert_eq!(
        Preferences::from_payload(&json!({"preferences":defaults})).unwrap(),
        Preferences::default()
    );
}
#[test]
fn byte_exact_beta_and_source_fixtures_have_self_contained_hashes() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let beta: Value = serde_json::from_str(include_str!("fixtures/beta/provenance.json")).unwrap();
    for (name, meta) in beta.as_object().unwrap() {
        assert_eq!(meta["commit"], "fd300e276356587726566026f072563eb9c34cb1");
        assert_eq!(
            digest(&std::fs::read(base.join("beta").join(name)).unwrap()),
            meta["sha256"].as_str().unwrap()
        );
    }
    let oracle: Value = serde_json::from_str(include_str!("fixtures/oracle.json")).unwrap();
    for (file, hash) in oracle["provenance"]["sourceHashes"].as_object().unwrap() {
        assert_eq!(
            digest(&std::fs::read(base.join("source").join(file)).unwrap()),
            hash.as_str().unwrap()
        );
    }
}
