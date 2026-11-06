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
default false. At the process boundary all six accept booleans or saved strings
`"true"`/`"false"`; other values remain explicit validation errors. Absent values
retain the engine defaults. Empty clipboard text fails before file lookup.
Whitespace-only text may become an empty paste. Failed format detection falls back to text;
Markdown always applies its final trim. See the detailed
[engine behavior and JSON5 limits](../v2/README.md#behavior).

SDK sessions serialize operations, stage state, cancel pending host RPCs and
dispose no-view sessions. This command has no view or bound actions; forged
`extension:invoke-action` calls fail, refresh/dispose return `unchanged`, and
unsupported operations cannot rerun a paste. The original no-view result shape
is preserved. The adapter requests only `clipboard.read`, `clipboard.write`,
`clipboard.writeFiles`, `clipboard.paste`, `clipboard.pasteText` and
`window.visibility`, plus the exact `notifications.show` grant for terminal
feedback. It does not use broad host grants or custom host channels.

## Storage migration

The process store is `dataDir/paste-as-plain-text/storage.json`, with the original
`Args/PasteAs` choice. Explicit strings, including empty strings, update it
atomically; omitted arguments read it. Unknown keys survive. Initialization
imports the choice once, only from paths granted through manifest migration keys:

1. Existing process choice wins, including an explicit empty string.
2. `nativeStore`: `native-extensions-v2/paste-as-plain-text/storage.json`.
3. `legacyArgsCache`: `extensions/paste-as-plain-text/.tezbar-support/cache/Args.json`.
4. `legacyRaycastArgsCache`: `extensions/raycast.paste-as-plain-text/.tezbar-support/cache/Args.json`.
5. `managedArgsCache`: `extensions/packages/paste-as-plain-text/.tezbar-support/cache/Args.json`.
6. `managedRaycastArgsCache`: `extensions/packages/raycast.paste-as-plain-text/.tezbar-support/cache/Args.json`.
7. `registryArgsCache`: `extension-registry/packages/paste-as-plain-text/.tezbar-support/cache/Args.json`.
8. `registryRaycastArgsCache`: `extension-registry/packages/raycast.paste-as-plain-text/.tezbar-support/cache/Args.json`.

The order above is deterministic; each grant is one file, never a package tree.
Migration sources are never changed. A corrupt current/native store is preserved;
the process still becomes ready and commands report current-store errors. A
corrupt previous native store leaves import pending rather than importing a
lower-priority cache. Malformed legacy JSON is treated as empty, as in the JS
cache shim. Existing private choices (including `""`) prevent reimport. An older
`_legacyArgsImported=true` store without `Args/PasteAs` is retried, repairing
previously overlooked managed roots. A repeated empty import does not rewrite the
store, and a malformed private value is preserved for explicit command failure.
No migration path or pinned host fixture is needed for a clean install.

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
# Optional read-only integration against a current host checkout (needs its TypeScript):
python3 scripts/smoke-package.py dist/paste-as-plain-text-aarch64-apple-darwin.zip \
  --renderer-host /path/to/Tezbar
```

The tests use recording host transports and stubbed OS runners; HTTP tests use
loopback only. They never launch the GUI or change the real clipboard. The
extracted-ZIP smoke uses a minimal environment and scripted narrow host responses
for real JSONL hello/initialize, six text modes and fallback/error paths, saved
string preferences (including six malformed-value failures), session calls,
disposal and shutdown. Process tests also prove title fetch on/off against a
loopback listener. `--renderer-host` extracts and executes the actual current
App hotkey callback and CommandBar launcher consumer with package results. It
verifies that notification requests carry terminal messages independently of
those consumers, while Show Tips=false suppresses success requests. File behavior
is tested with temporary paths and recorded helper plans, not by invoking the user's OS clipboard.

On an actual Windows x64 MSVC build host, use the same locked Rust 1.85 toolchain:

```powershell
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target'
cargo +1.85.0 test --locked -j2
cargo +1.85.0 build --release --locked -j2 --target x86_64-pc-windows-msvc
python scripts/package.py --target x86_64-pc-windows-msvc --binary target/x86_64-pc-windows-msvc/release/tezbar-paste-as-plain-text.exe
python scripts/smoke-package.py dist/paste-as-plain-text-x86_64-pc-windows-msvc.zip
```

The published Windows package passed native build and protocol checks. To reproduce, package on its target OS:
the packager executes the binary to check its identity and command inventory.
After an intentional dependency update, regenerate the per-target dependency
license inventory with `python3 scripts/package.py --write-third-party` and
`python3 scripts/vendor-licenses.py`, review them and commit the lock/inventory.
The latter covers locked normal and build dependencies for both targets (including
proc macros), excludes development-only crates and vendors exact upstream license,
notice and copyright files. For crates missing packaged documents,
`licenses/supplemental/index.json` records exact-source attribution and standard
license texts, or documents from the crate's immutable upstream revision.
All Markdown document links use POSIX separators on Windows too. Packaging refuses
a stale inventory or a changed SDK pin. Do not commit `target/`, `dist/` or compiled output.

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
the SDK Apache-2.0 license/NOTICE, a per-target linked-crate license inventory and
`licenses/THIRD_PARTY.md` / `dependency-license-index.json` with the full
both-target normal/build dependency notice closure.

## Compatibility and QA status

All original modes are implemented using the mature v2 engine. The following
source/host differences remain explicit and are not a claim of complete parity:

- Tezbar's text paste writes transformed text to the clipboard; it cannot restore
  the original rich clipboard when `replaceClipboard=false`. With true the
  extension explicitly copies again after successful paste, as in the source.
- Protocol 1 has no dynamic command-subtitle update or animated toast/HUD API.
  Terminal success and failure use the current host's typed
  `host.notifications.show` (`id`, `title`, `body`; successful reply `true`).
  This OS notification channel works independently of the hidden launcher and
  ignored no-view hotkey results. Show Tips=false suppresses success tips and the
  returned success message; errors still report their original detail, as in the
  source. No animated/progress notification is emitted. A stable `paste-result`
  ID replaces the previous terminal notice and bounds host resource ownership to
  one notification; current host semantics retain the process while that resource
  exists and clean it up on process/app shutdown. Notification permission and OS
  presentation rules apply (macOS requires a packaged Tezbar app). If submission
  fails, stderr reports it without masking the original paste outcome or retrying
  a completed paste. No visible-feedback parity is claimed under permission denial;
  actual notification delivery still needs desktop QA. Tests never send OS notices.
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
  Native Windows build/protocol checks and isolated CLI imports have passed.

## Install/release handoff

This is a real executable ZIP, with no on-install build or runtime source checkout.
Independent review, native builds, extracted-package checks and isolated CLI imports
have passed on macOS and Windows. Both ZIPs and the trusted release reference are
[published](https://github.com/almatkai/paste-as-plain-text/releases/tag/v1.0.0).
Actual desktop effects remain unverified. The source/ZIP `artifacts` objects stay
empty to avoid a circular archive hash; the separate published trusted reference
pins real per-target URLs, archive hashes and executable hashes. Store catalog
registration remains separate from this verified manual import path.

### Handled command failures

Expected engine failures return a successful protocol response with `ok: false`, `mode: "no-view"`, and the original error message. The host can then dispose the registered session normally without tearing down the process and its notification resources. No failed paste is retried automatically. Invalid protocol operations and cancellation keep their SDK error handling. Real desktop notification permission, presentation and dismissal remain unverified.
