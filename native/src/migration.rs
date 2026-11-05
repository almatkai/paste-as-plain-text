//! One-time import of the last Paste-as choice from explicit host grants only.
//! Precedence: current process store > compiled-in Native V2 store > V1 JS
//! `Cache({namespace:"Args"})` shim file. Sources are never modified.
use crate::host::cached;
use serde_json::{json, Value};
use tezbar_extension_sdk::{read_store, update_store, CoreResult, ExtensionContext};
use tezbar_paste_as_plain_text::SLUG;

pub const IMPORTED: &str = "_legacyArgsImported";
/// Keys of `migrationPaths` in extension.json, highest priority first.
pub const NATIVE_STORE: &str = "nativeStore";
pub const LEGACY_CACHE: [&str; 2] = ["legacyArgsCache", "legacyRaycastArgsCache"];

async fn read(path: &std::path::Path) -> Result<Option<Vec<u8>>, String> {
    match tokio::fs::read(path).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// The previous native store had the same `Args/PasteAs` contract; corruption
/// fails (as it did there) instead of silently discarding the choice.
async fn native_choice(ctx: &ExtensionContext) -> Result<Option<String>, String> {
    let Some(path) = ctx.migration_paths.get(NATIVE_STORE) else {
        return Ok(None);
    };
    let Some(bytes) = read(path).await? else {
        return Ok(None);
    };
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid native migration store: {e}"))?;
    if !value.is_object() {
        return Err("Native migration store must be an object".into());
    }
    let has = value.get("Args").and_then(|a| a.get("PasteAs")).is_some();
    let choice = cached(&value)?;
    Ok(has.then_some(choice))
}

/// The V1 JS cache shim treated unreadable/malformed files as empty; keep that.
async fn legacy_choice(ctx: &ExtensionContext) -> Result<Option<String>, String> {
    for key in LEGACY_CACHE {
        let Some(path) = ctx.migration_paths.get(key) else {
            continue;
        };
        let bytes = match read(path).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => continue,
            Err(error) => {
                // CacheShim::readAll catches read/parse failures and returns {}.
                eprintln!("paste-as-plain-text: legacy cache skipped: {error}");
                continue;
            }
        };
        let parsed: Option<Value> = serde_json::from_slice(&bytes).ok();
        if let Some(Value::String(choice)) = parsed.as_ref().and_then(|v| v.get("PasteAs")) {
            return Ok(Some(choice.clone()));
        }
    }
    Ok(None)
}

/// Never blocks process readiness on bad data: a corrupt current store is left
/// untouched (the command itself reports it, as the V2 engine did), and a
/// corrupt previous native store is skipped without marking the import done,
/// so nothing is discarded and the import is retried on a later start.
pub async fn initialize(ctx: &ExtensionContext) -> CoreResult {
    let current = match read_store(ctx, SLUG).await {
        Ok(current) => current,
        Err(error) => {
            eprintln!("paste-as-plain-text: store unreadable, migration skipped: {error}");
            return Ok(Value::Null);
        }
    };
    if let Err(error) = cached(&current) {
        eprintln!("paste-as-plain-text: store invalid, migration skipped: {error}");
        return Ok(current);
    }
    if current[IMPORTED] == true {
        return Ok(current);
    }
    let choice = match native_choice(ctx).await {
        Ok(Some(choice)) => Some(choice),
        Ok(None) => legacy_choice(ctx).await?,
        Err(error) => {
            eprintln!("paste-as-plain-text: previous native store skipped: {error}");
            return Ok(current);
        }
    };
    update_store(ctx, SLUG, move |store| {
        cached(store)?;
        let has_current = store.get("Args").and_then(|a| a.get("PasteAs")).is_some();
        if let (Some(choice), false) = (choice, has_current) {
            if store.get("Args").is_none() {
                store["Args"] = json!({});
            }
            store["Args"]["PasteAs"] = json!(choice);
        }
        store[IMPORTED] = json!(true);
        Ok(())
    })
    .await
}
