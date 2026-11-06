## Paste as Plain Text

Paste text from the clipboard as any format you want.

### Installable native Rust version for Tezbar

The standalone implementation is in [`native/`](native/README.md), using the
existing [`v2/`](v2/README.md) Rust engine. The root [`extension.json`](extension.json)
is the schema 3 process manifest: `com.tezbar.native.paste-as-plain-text`, version
`1.0.0`, replacing `raycast.paste-as-plain-text`. It preserves the original
`paste-as-plain-text` command, six preferences, dropdown values and icon.

The macOS Apple Silicon package is built with Rust 1.85 and runs without Node,
Bun, Cargo or the Tezbar source tree. A Windows x64 package must be built and
verified separately. Release ZIPs use `extension.json`, `bin/`, `resources/`,
`licenses/`, `LICENSE` and `NOTICE` at the archive root. See the
[native build, ZIP smoke test and install handoff](native/README.md).

All seven paste modes have automated tests with recording hosts. Live clipboard,
focus, Finder/Explorer transfer and UI feedback remain unverified. Known gaps
include rich clipboard restoration, dynamic command subtitles and animated HUDs;
the native JSON5 parser also has documented fallback cases. These are described
in [native limitations](native/README.md#compatibility-and-qa-status).

The original Raycast TypeScript implementation, package metadata, assets and
license remain below and in `src/`; they are not used to run the native package.

### Original Raycast extension

Support Formats:
- **Plain Text**: Paste text from the clipboard without formatting


- **URL**: Extract URL from the clipboard and paste it
  - Example: `This is a URL https://www.raycast.com` will be pasted as `https://www.raycast.com`


- **File**: Extract file path from the clipboard and paste it as file
  - Example 1: File copied from Finder or other applications will be pasted as file
  - Example 2: `/Users/username/Desktop/file.txt` will be pasted as file in Finder or other applications
  - Example 3: Copy Image from Chrome or other browsers will be pasted as image in some applications


- **Json**: Paste text from the clipboard as Json format
  - Example: `{"key": "value"}` will be pasted as 
  
   ```
  {
    "key": "value"
  }
  ```


- **Number**: Extract numbers from the clipboard and paste them
  - Example: `This is a number 123,456.7` will be pasted as `123456.7`


- **Markdown Link**: Extract URL from the clipboard and paste it as Markdown Link
  - Example: `https://www.raycast.com` will be pasted as `[](https://www.raycast.com)`


- **Markdown Image**: Extract URL from the clipboard and paste it as Markdown Image
  - Example: `https://www.raycast.com/koinzhang/paste-as-plain-text/install_button@2x.png?v=1.1` will be pasted as `![](https://www.raycast.com/koinzhang/paste-as-plain-text/install_button@2x.png?v=1.1)`


Want more **Productivity** extensions? Try those. [©koinzhang](https://www.raycast.com/koinzhang)

<a id="install-extension-button" title="Install Easy New File Raycast Extension" href="https://www.raycast.com/koinzhang/easy-new-file#install">
        <img height="64" style="height: 64px" src="https://assets.raycast.com/koinzhang/easy-new-file/install_button@2x.png">
</a><a id="install-extension-button" title="Install Common Directory Raycast Extension" href="https://www.raycast.com/koinzhang/common-directory#install">
        <img height="64" src="https://assets.raycast.com/koinzhang/common-directory/install_button@2x.png">
</a><a id="install-extension-button" title="Install Quick Access Raycast Extension" href="https://www.raycast.com/koinzhang/quick-access#install">
        <img height="64" src="https://assets.raycast.com/koinzhang/quick-access/install_button@2x.png">
</a><a id="install-extension-button" title="Install Copy Path Raycast Extension" href="https://www.raycast.com/koinzhang/copy-path#install">
        <img height="64" style="height: 64px" src="https://assets.raycast.com/koinzhang/copy-path/install_button@2x.png">
</a><a id="install-extension-button" title="Install Open Path Raycast Extension" href="https://www.raycast.com/koinzhang/open-path#install">
        <img height="64" style="height: 64px" src="https://assets.raycast.com/koinzhang/open-path/install_button@2x.png">
</a><a id="install-extension-button" title="Install Hide Files Raycast Extension" href="https://www.raycast.com/koinzhang/hide-files#install">
        <img height="64" style="height: 64px" src="https://assets.raycast.com/koinzhang/hide-files/install_button@2x.png">
</a>

Previous compiled-in library: [Native V2 engine and historical integration](v2/README.md).
