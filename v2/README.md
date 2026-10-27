# Paste as Plain Text — Native V2

A standalone Rust library for Tezbar's **compiled-in** Native V2 API. One command,
`paste-as-plain-text`, implements the original seven formats. No JavaScript runtime,
external extension loader, stdin protocol, or additional public RPC is used.
The original V1 implementation remains available in the repository root.

Original extension: **koinzhang**, with contributions by **pantajoe**, **kero13** and
**ridemountainpig**. Tezbar port: **almatkai**. Extension code/source fixtures are
MIT; the byte-exact beta host test fixtures are Apache-2.0. See `LICENSE-MIT`,
`LICENSE-APACHE`, and `tests/fixtures/beta/LICENSE` (package SPDX: `MIT AND Apache-2.0`).

## Behavior

| Mode | Native behavior | Automated coverage |
|---|---|---|
| Plain Text | Independently trim either end; optionally replace source CR/LF runs and adjacent ASCII spaces/tabs with one space | Original TS oracle and recording host |
| JSON | Native JSON5 parser, then JSON with two-space indentation; invalid/unsupported inputs fall back to transformed text | JSON5 source oracle including explicit differences below |
| URL | First case-sensitive `https?://` match through ECMAScript non-whitespace; punctuation is retained | Source regex oracle |
| Number | Original `-?[\d,]+(?:\.\d+)?\b`, ASCII digits and word boundary, commas removed; preserve regex backtracking | Source regex oracle |
| File | Read first system file reference if present, otherwise use transformed text; convert file URL, check existence, prepare a genuine file clipboard and request paste | Temporary filesystem fixtures, OS command plans and actual beta bridge with stubbed helpers |
| Markdown Link | Always trim, validate original protocol/host/TLD options, optionally fetch title, output `[title](url)` | Validator and full command source oracle; local HTTP fixture |
| Markdown Image | Same, output `![title](url)` | Validator and full command source oracle; local HTTP fixture |

Defaults are unchanged: `trimStart`, `trimEnd`, `autoFetchTitle`, `showTips` **true**;
`cleanLineBreaks`, `replaceClipboard` **false**. FEFF trims; U+0085 does not.
Line cleaning changes CR/LF only, preserving U+2028/U+2029. Original empty text
fails with `No content in clipboard` and no paste; whitespace-only text may paste
an empty string. Failed extraction falls back to transformed text. Markdown's
unconditional final trim also applies when URL validation fails. Titles and URLs
are not given new Markdown escaping rules.

The actual beta launch field is `argumentValues.advancedPasteFormat`. A supplied
string is cached, including `""` (clear the choice and select Plain Text); an omitted
key reads the last supplied string. Storage is isolated in the Native V2
`paste-as-plain-text` namespace at `Args/PasteAs`, using SDK `read_store` and
atomic `update_store`. No V1 Cache is read or migrated. Unrelated root/Args keys,
including root keys named `ok`, `error`, or `message`, survive. Corruption fails
without replacing data. SDK transactions serialize callers in the host process;
this does not promise coordination across multiple independent Tezbar processes.

### JSON5 compatibility limits

The native parser is `json5` 0.4.1 (compatible with Rust 1.85). Comments, unquoted
keys, single quotes, trailing commas, ordinary hex, decimal forms, escaped surrogate
**pairs**, and `Infinity`, `-Infinity`, `NaN`, `-NaN` are supported. Nonfinite values
serialize as JSON `null`. Numeric output uses JS number formatting (`ryu-js`) and
IEEE-754 rounding; ECMAScript integer-index keys are sorted before other keys,
whose insertion order is retained. Duplicate keys keep their original position
with the last value. These cases are checked against `JSON5.parse` followed by
`JSON.stringify(value, null, 2)` from the original JavaScript dependencies.

The following valid source inputs deliberately fall back to transformed plain
text in this native parser; the tests assert both the original output and the
native fallback. This is a compatibility limit, not full JSON5 equivalence:

| Source input | Original JSON output | Native output |
|---|---|---|
| `+Infinity`, `+NaN` | `null` | Unchanged text |
| `-0xFF` | `-255` | Unchanged text |
| `9223372036854775808` (integer token outside i64) | `9223372036854776000` | Unchanged text |
| `0xffffffffffffffff` (hex token outside u32) | `18446744073709552000` | Unchanged text |
| `1e400` | `null` | Unchanged text |
| `"\uD800"` (unpaired surrogate) | `"\ud800"` | Unchanged text |

Other integer tokens outside i64 and hex tokens outside u32 have the same parser
limit. JSON5 input is bounded to 1 MiB and nesting to 64; larger/deeper inputs
also fall back to transformed plain text. This prevents clipboard input from
causing unbounded parser recursion.

### Host and OS limitations

* The beta API reference is Tezbar commit
  `fd300e276356587726566026f072563eb9c34cb1`, not a moving checkout.
  `tezbar_adapter.rs` compiles unchanged against its actual `common.rs` and
  `v2_sdk.rs` in `tests/bridge.rs`; only Tauri, the router and OS runner are stubbed.
* Text paste calls real `core:platform:paste-text[text]`, which writes the clipboard,
  hides the application, waits 80 ms, and sends Cmd/Ctrl+V. That API **cannot restore
  rich clipboard formats**. Even with `replaceClipboard=false`, the clipboard
  remains the transformed output. With `true`, the extension explicitly copies
  the pasted text/file again **after** successful paste. Raycast's clipboard
  preservation semantics cannot be reproduced for the false preference; the beta
  V1 paste shim also writes clipboard text. No rich-format snapshot is available.
* Generic `clipboard:write` accepts text, and the platform file-write route is an
  image loader; neither prepares arbitrary file drops. This adapter instead uses
  macOS `/usr/bin/osascript` to read/set a clipboard alias, or Windows PowerShell
  (`-STA`, `System.Windows.Forms`) to read/set a FileDropList. Write paths travel as
  argv on macOS and an environment value on Windows, never as script source.
  After preparing the file clipboard it calls existing `core:platform:paste[]`.
  Helpers have a 5-second timeout, bounded stdout, and kill-on-drop. Missing or
  failed helpers produce an error, never a successful text-path substitute.
* File reads use those OS capabilities, not the clipboard history (which may be
  stale) or invented metadata. Only the first file is used, matching the singular
  source `file` property. The original empty-text guard still runs before this
  lookup: a file-only clipboard whose host text read is empty fails as in the
  source. Image-only clipboard bytes are not converted to files.
* Title fetching is real async HTTP(S), bounded to five seconds, five redirects,
  and 1 MiB of body, parsed with an HTML parser. Failed fetch/unsupported FTP
  fetching/timeout/oversize gives an empty title. FTP URLs can still form Markdown.
  These resource limits are an intentional improvement over unbounded Axios.
* Source dynamic command subtitle updates have no working beta equivalent and are
  omitted. `showTips` gates animated/success effects; failures always emit an
  effect and return an error for the renderer. On failure the adapter also
  requests existing `window:show[]` so the launcher can display that result
  after the initial hide, even with tips disabled. A secondary reveal failure
  does not mask the original error. `extension:effect` follows the
  existing beta event shape, but beta has no connected live notice listener for
  this no-view path. Animated feedback/native HUD visibility after hiding is not
  guaranteed. The final no-view message (empty when tips are off) and failure
  result use the renderer's actual result handling.
* Success is reported only after paste and optional re-copy complete, correcting
  the source's success-before-paste ordering. Host completion means the keypress
  was accepted by the host, not proof that the target application consumed it.

macOS and Windows implementations **compile**, but neither has completed real
OS/UI QA. Both manifest verification entries intentionally remain `not-run`.
Linux is not declared supported; its file helper returns a capability error.
No real clipboard helper runs during automated tests.

## Build and verification

From `v2/`:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo +1.85.0 test --locked
cargo +1.85.0 clippy --all-targets --locked -- -D warnings
cargo check --all-targets --target x86_64-pc-windows-gnu --locked
cargo clippy --all-targets --target x86_64-pc-windows-gnu --locked -- -D warnings
cargo package --locked --allow-dirty
```

Use an installed Windows target/toolchain for cross-checks; cross-compiling is not
Windows runtime verification. `Cargo.lock` resolves Rust 1.85-compatible versions, including `pest` 2.8.3
(without a stacker C build); HTTP uses the OS TLS backend on macOS/Windows.
Normal Cargo tests need no Node/Bun and no external network. The HTTP tests bind
only loopback; clipboard calls use recording stubs, and file existence tests use
temporary fixtures. Tests also verify the canonical manifest against the pinned
schema, exact original icon bytes/hash, cache restart/concurrency/corruption,
error propagation, no duplicate paste, and no misleading success.

The optional `scripts/generate-oracle.cjs` transpiles and executes the **original
TS bodies** with mocks. To regenerate, create a disposable V1 copy under
`v2/target/v1-check` with the root package/lock/tsconfig/types/src/test. If only
`package-lock.json` exists, run `pnpm import` there; then:

```sh
# In target/v1-check:
pnpm install --frozen-lockfile --ignore-scripts
pnpm test
pnpm typecheck
# In v2:
node scripts/generate-oracle.cjs target/v1-check/package.json
```

All normal test fixtures are included in the Cargo package. Their SHA-256 hashes
and dependency versions are recorded in the fixture provenance files. The icon is
byte-exact from `raycast/extensions` commit
`6734d85992290066fd31a7f5bee3cca3d3545a1a`, path
`extensions/paste-as-plain-text/assets/paste-icon.png`, 290,928 bytes, SHA-256
`290e0ae7fdd36df2d095e28bd5b9292c470dc3d0273e23b978500935e44b67dd`.

## Host integration (separate host change required)

The stable vendoring location is **`src-tauri/native-extensions/paste-as-plain-text/`**
in the Tezbar repository. Copy the contents of this repository's `v2/` there,
excluding `target/`. Do not reference a temporary clone or an installed profile.

1. Add to the host `src-tauri/Cargo.toml` dependencies:
   ```toml
   tezbar-paste-as-plain-text = { path = "native-extensions/paste-as-plain-text" }
   async-trait = "0.1.89"
   ```
   The host already uses `serde_json` and Tokio; keep Tokio's `rt`, `sync`, and
   `time` features. The vendored library declares its other dependencies.
2. Copy `tezbar_adapter.rs` unchanged to
   `src-tauri/src/native_core/extensions/v2_paste_as_plain_text.rs` and declare
   `mod v2_paste_as_plain_text;` in the adjacent `mod.rs`.
3. Register `paste-as-plain-text` in `v2_runtime.rs::slug`, dispatch `commands` to
   `super::v2_paste_as_plain_text::commands()`, and dispatch `handle` to
   `super::v2_paste_as_plain_text::handle(ctx, command, operation, payload, state).await`.
   Do not add custom public RPC routes.
4. Append `manifest.json` as one object in
   `src-tauri/src/native_core/extensions/v2_catalog.json`; preserve the canonical
   ID, replacement ID, original argument/preferences, and `not-run` verification.
5. Copy the unchanged `assets/paste-icon.png` to
   **`src/renderer/public/extensions/icons/paste-as-plain-text.png`**. Keep the
   manifest's `/extensions/icons/paste-as-plain-text.png` reference.
6. Rebuild the host and renderer, list compiled Native V2 coverage, then install
   the canonical bundled ID through the host's existing catalog/install flow.
   Rust crates cannot be installed directly into the extension store: copying
   a manifest/marker to a profile does not register executable dispatch.
7. Perform real macOS and Windows QA: all seven modes, hotkey vs explicit empty
   dropdown, focus return, helper availability/permissions, actual file transfer
   in Finder/Explorer and a target editor, clipboard effects with both preference
   values, network title failure and UI notices. Only then update verification
   with evidence. Target application acceptance and HUD timing remain unverified.
