# Standalone Rust process extension

The native package uses Tezbar schema 3 / JSONL protocol 1 and the published Rust
SDK at immutable revision `aaec571673ab64a85a136cf6bd054ef660c4d93c`.
`src/main.rs` calls SDK `serve()`; `src/host.rs` adapts the existing `../v2`
engine's `Host` trait. Production links no Tezbar app/Tauri crate, test fixture,
JavaScript runtime, or local-path SDK. The only path dependency is our own v2
engine. Beta host fixtures remain development tests for that historical engine.

Identity: `com.tezbar.native.paste-as-plain-text`; replacement:
`raycast.paste-as-plain-text`, verified from root `package.json.name`.
Version `1.0.0` is the first standalone native release (no GitHub releases or
tags existed when checked on 2026-11-06). Nothing is uploaded by these scripts.

## Command and preferences

The single source command `paste-as-plain-text` (title **Paste as**, subtitle
**Plain Text**) stays `no-view`, with `advancedPasteFormat` selecting:

| Value | Behavior |
| --- | --- |
| omitted | Reuse the last explicit choice; default to Plain Text |
| `""` / Plain Text | Clear the saved choice / paste transformed text |
| URL | Extract the first case-sensitive HTTP(S) match |
| File | First OS clipboard file reference, otherwise an existing absolute path or file URL; prepare a file drop and paste |
| JSON | JSON5 parse and two-space JSON output; failed/unsupported parsing falls back to text |
| Number | Source ASCII-number extraction with commas removed |
| Markdown Link | Validate the complete URL, optionally fetch its title, paste `[title](url)` |
| Markdown Image | Same validation/title behavior, paste `![title](url)` |

The six original preferences and defaults are preserved: `trimStart`, `trimEnd`,
`autoFetchTitle`, `showTips` default true; `cleanLineBreaks`, `replaceClipboard`
default false. Empty clipboard text fails before file lookup. Whitespace-only
text may become an empty paste. Failed format detection falls back to text;
Markdown always applies its final trim. See the detailed
[engine behavior and JSON5 limits](../v2/README.md#behavior).

SDK sessions serialize operations, stage state, cancel pending host RPCs and
dispose no-view sessions. This command has no view or bound actions; forged
`extension:invoke-action` calls fail, refresh/dispose return `unchanged`, and
unsupported operations cannot rerun a paste. The original no-view result shape
is preserved. The adapter requests only `clipboard.read`, `clipboard.write`,
`clipboard.writeFiles`, `clipboard.paste`, `clipboard.pasteText` and
`window.visibility`. It does not use broad host grants or custom host channels.

## Storage migration

The process store is `dataDir/paste-as-plain-text/storage.json`, with the original
`Args/PasteAs` choice. Explicit strings, including empty strings, update it
atomically; omitted arguments read it. Unknown keys survive. Initialization
imports the choice once, only from paths granted through manifest migration keys:

1. Existing process choice wins, including an explicit empty string.
2. `nativeStore`: `native-extensions-v2/paste-as-plain-text/storage.json`.
3. `legacyArgsCache`: `extensions/paste-as-plain-text/.tezbar-support/cache/Args.json`.
4. `legacyRaycastArgsCache`: `extensions/raycast.paste-as-plain-text/.tezbar-support/cache/Args.json`.

Migration sources are never changed. A corrupt current/native store is preserved;
the process still becomes ready and commands report current-store errors. A
corrupt previous native store leaves import pending rather than importing a
lower-priority cache. Malformed legacy JSON is treated as empty, as in the JS
cache shim. The marker `_legacyArgsImported` prevents later reimport. No migration
path or pinned host fixture is needed for a clean install.

## Build, test and package

**MSRV: Rust 1.85.0.** Both the source and committed locks are tested with that
toolchain on macOS arm64. Keep `--locked`; the local Cargo resolver configuration
selects Rust 1.85-compatible dependencies when the lock is deliberately updated.
No global Cargo configuration is needed. Run from this `native/` directory:

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target"
cargo +1.85.0 fmt --all -- --check
cargo +1.85.0 test --locked -j2
cargo +1.85.0 clippy --locked --all-targets -j2 -- -D warnings
# Preserved engine, source oracles, beta bridge, file plans and loopback HTTP:
cargo +1.85.0 test --manifest-path ../v2/Cargo.toml --locked -j2
cargo +1.85.0 build --release --locked -j2 --target aarch64-apple-darwin
python3 scripts/package.py --target aarch64-apple-darwin \
  --binary target/aarch64-apple-darwin/release/tezbar-paste-as-plain-text
python3 scripts/smoke-package.py dist/paste-as-plain-text-aarch64-apple-darwin.zip
```

The tests use recording host transports and stubbed OS runners; HTTP tests use
loopback only. They never launch the GUI or change the real clipboard. The
extracted-ZIP smoke uses a minimal environment and scripted narrow host responses
for real JSONL hello/initialize, six text modes and fallback/error paths, session
calls, disposal and shutdown. File behavior is tested with temporary paths and
recorded helper plans, not by invoking the user's OS clipboard.

On an actual Windows x64 MSVC build host, use the same locked Rust 1.85 toolchain:

```powershell
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target'
cargo +1.85.0 test --locked -j2
cargo +1.85.0 build --release --locked -j2 --target x86_64-pc-windows-msvc
python scripts/package.py --target x86_64-pc-windows-msvc --binary target/x86_64-pc-windows-msvc/release/tezbar-paste-as-plain-text.exe
python scripts/smoke-package.py dist/paste-as-plain-text-x86_64-pc-windows-msvc.zip
```

Windows commands are instructions, not claimed results. Package on its target OS:
the packager executes the binary to check its identity and command inventory.
After an intentional dependency update, regenerate the per-target dependency
license inventory with `python3 scripts/package.py --write-third-party`, review
it and commit the lock/inventory. Packaging refuses a stale inventory or a changed
SDK pin. Do not commit `target/`, `dist/` or compiled output.

Each target package produces these files under `dist/`:

- `paste-as-plain-text-<target>.zip` and `.zip.sha256` (archive SHA256).
- `paste-as-plain-text-<target>.tezbar-paste-as-plain-text[.exe].sha256` (executable SHA256).
- `paste-as-plain-text-<target>.artifact.json` (target, archive/executable hashes, executable path).
- `paste-as-plain-text-<target>.files.json` (exact packaged file SHA256 inventory).

The ZIP contains root `extension.json`, `bin/tezbar-paste-as-plain-text[.exe]`,
`resources/assets/paste-icon.png`, `LICENSE`, `NOTICE`, and all files in
`licenses/`. Its `files` object inventories every member except the self-referential
manifest. Scripts verify the inventory, icon, executable mode, identity and
command coverage. ZIP timestamps are fixed. Licenses retain original MIT credits,
the SDK Apache-2.0 license/NOTICE, and a per-target linked-crate license inventory.

## Compatibility and QA status

All original modes are implemented using the mature v2 engine. The following
source/host differences remain explicit and are not a claim of complete parity:

- Tezbar's text paste writes transformed text to the clipboard; it cannot restore
  the original rich clipboard when `replaceClipboard=false`. With true the
  extension explicitly copies again after successful paste, as in the source.
- Protocol 1 has no dynamic command-subtitle update or toast/HUD API. Animated
  feedback is omitted; success uses the original final no-view message, suppressed
  with `showTips=false`. Failures return an error and attempt to reveal the launcher.
- The reused JSON5 parser deliberately falls back to transformed plain text for
  `+Infinity`, `+NaN`, signed hex, integers outside i64, hex outside u32, overflowing
  exponents, unpaired surrogates, input over 1 MiB or nesting over 64. See the
  [oracle-tested exact cases](../v2/README.md#json5-compatibility-limits).
- File references use bounded constant-script helpers: macOS `/usr/bin/osascript`
  or Windows PowerShell `-STA` with Windows Forms. File paths are arguments/env
  data, never script source. Regular files use typed `host.clipboard.writeFiles`;
  directories use the existing OS helper because that host API accepts regular
  files only. Helper reads/writes honor their clipboard grants. Missing helpers,
  timeout or denial fail explicitly. Relative paths are rejected because a
  standalone child's working directory has no user-context meaning. Image-only
  clipboard bytes are not converted to files. File-only clipboard text that is
  empty fails before reference lookup, matching the source guard.
- Title fetch uses native HTTP(S), capped at five seconds, five redirects and
  1 MiB. Failures and FTP title fetches produce an empty title. Valid FTP URLs
  still form Markdown links/images.
- Both platform verification entries stay `not-run`: offline protocol tests do
  not establish real clipboard restoration, focus, actual target-app paste,
  Finder/Explorer file transfer, helper availability, permissions or launcher UI QA.
  Windows build/runtime verification is assigned to the parent rollout.

## Install/release handoff

This is a real executable ZIP, with no on-install build or runtime source checkout.
Independent review, Windows artifacts, actual host install/desktop QA and
publication are still required. The source and packaged `artifacts` objects are
empty intentionally: after publishing, the parent must add real HTTPS artifact
URLs, archive hashes, executable paths and hashes to the trusted host catalog
before a catalog install can succeed. Do not put a ZIP hash inside its own ZIP.
The macOS `.artifact.json` and sidecars provide those values. Local development
ZIP import must use the host's verified package-import path. No host install,
release, tag, push or publication is performed by this implementation task.
