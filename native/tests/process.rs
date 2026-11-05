//! Drives the real compiled executable over stdin/stdout JSONL with a fake,
//! recording host. Reverse requests are answered by the test; nothing touches
//! the real clipboard, keyboard or windows (File mode is covered by unit tests).
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

const BIN: &str = env!("CARGO_BIN_EXE_tezbar-paste-as-plain-text");
const ID: &str = "com.tezbar.native.paste-as-plain-text";

fn forward_frames(stdout: ChildStdout, tx: mpsc::Sender<Value>) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { return };
        if tx.send(serde_json::from_str(&line).unwrap()).is_err() {
            return;
        }
    }
}

struct FakeHost {
    child: Child,
    stdin: ChildStdin,
    frames: mpsc::Receiver<Value>,
    next: u32,
    reverse: Vec<Value>,
}
impl Drop for FakeHost {
    fn drop(&mut self) {
        // Reap the real child even when an assertion or receive timeout panics.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl FakeHost {
    fn spawn() -> Self {
        let mut child = Command::new(BIN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, frames) = mpsc::channel();
        let host = Self {
            child,
            stdin,
            frames,
            next: 0,
            reverse: vec![],
        };
        std::thread::spawn(move || forward_frames(stdout, tx));
        host
    }
    fn recv(&self) -> Value {
        self.frames
            .recv_timeout(Duration::from_secs(10))
            .expect("frame from extension")
    }
    fn send(&mut self, value: Value) {
        writeln!(self.stdin, "{value}").unwrap();
        self.stdin.flush().unwrap();
    }
    fn start(&mut self, method: &str, params: Value) -> String {
        self.next += 1;
        let id = format!("h:00000000-0000-4000-8000-{:012}", self.next);
        self.send(json!({"type":"request","id":id,"method":method,"params":params}));
        id
    }
    /// Run one host request, answering reverse requests in order with `replies`.
    fn request(&mut self, method: &str, params: Value, replies: &[Value]) -> Result<Value, Value> {
        let id = self.start(method, params);
        let mut replies = replies.iter();
        loop {
            let frame = self.recv();
            match frame["type"].as_str().unwrap() {
                "request" => {
                    assert!(frame["id"].as_str().unwrap().starts_with("e:"));
                    self.reverse.push(frame.clone());
                    let result = replies
                        .next()
                        .unwrap_or_else(|| panic!("unexpected reverse request {frame}"));
                    let reply = if result.get("__error").is_some() {
                        json!({"type":"response","id":frame["id"],"error":{"code":"host-error","message":result["__error"]}})
                    } else {
                        json!({"type":"response","id":frame["id"],"result":result})
                    };
                    self.send(reply);
                }
                "response" if frame["id"] == id => {
                    assert!(replies.next().is_none(), "unused scripted replies");
                    return match frame.get("error") {
                        Some(error) => Err(error.clone()),
                        None => Ok(frame["result"].clone()),
                    };
                }
                other => panic!("unexpected {other} frame {frame}"),
            }
        }
    }
    fn methods_since(&self, start: usize) -> Vec<(String, Value)> {
        self.reverse[start..]
            .iter()
            .map(|f| {
                (
                    f["method"].as_str().unwrap().to_owned(),
                    f["params"].clone(),
                )
            })
            .collect()
    }
}

#[test]
fn protocol_info_reports_identity_without_host() {
    let out = Command::new(BIN).arg("--protocol-info").output().unwrap();
    assert!(out.status.success());
    let info: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(info["id"], ID);
    assert_eq!(info["protocol"], 1);
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["commands"], json!(["paste-as-plain-text"]));
}

#[test]
fn handshake_migration_sessions_failure_cancellation_and_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let legacy = dir.path().join("Args.json");
    std::fs::write(&legacy, br#"{"PasteAs":"URL"}"#).unwrap();
    let manifest: Value = serde_json::from_str(include_str!("../../extension.json")).unwrap();

    let mut host = FakeHost::spawn();
    let hello = host.recv();
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["id"], ID);
    assert_eq!(hello["protocol"], 1);
    assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));

    let ready = host
        .request(
            "initialize",
            json!({"protocol":1,"extensionId":ID,"platform":"macOS",
                "packageDir":dir.path().join("package"),"dataDir":data,
                "tempDir":dir.path().join("tmp"),
                "grantedCapabilities":manifest["capabilities"],"reason":"command",
                "migrationPaths":{"legacyArgsCache":legacy}}),
            &[],
        )
        .unwrap();
    assert_eq!(ready["ready"], true);

    // Hotkey launch (argument omitted) uses the migrated V1 choice: URL.
    let mark = host.reverse.len();
    let result = host
        .request(
            "session.open",
            json!({"sessionId":"s1","commandName":"paste-as-plain-text","preferences":{},"argumentValues":{}}),
            &[json!(null), json!("  go https://example.com/x?y=1 now "), json!({"ok":true,"message":"Pasted text"}), json!(true)],
        )
        .unwrap();
    assert_eq!(
        result,
        json!({"ok":true,"mode":"no-view","message":"🔗 Paste as URL"})
    );
    assert_eq!(
        host.methods_since(mark),
        [
            ("host.window.visibility".into(), json!({"visible":false})),
            ("host.clipboard.read".into(), json!({"format":"text"})),
            (
                "host.clipboard.pasteText".into(),
                json!({"text":"https://example.com/x?y=1"})
            ),
            (
                "host.notifications.show".into(),
                json!({"id":"paste-result","title":"Paste as Plain Text","body":"🔗 Paste as URL"})
            ),
        ]
    );
    // Calls never replay a no-view command. There are no bound actions;
    // refresh is unchanged and forged actions are explicitly rejected.
    let before_calls = host.reverse.len();
    let refreshed = host
        .request(
            "session.call",
            json!({"sessionId":"s1",
        "operation":"extension:refresh-session", "payload":{}}),
            &[],
        )
        .unwrap();
    assert_eq!(refreshed, json!({"ok":true,"mode":"unchanged"}));
    for payload in [
        json!({}),
        json!({"actionId":"paste","preferences":{"replaceClipboard":true}}),
    ] {
        let error = host
            .request(
                "session.call",
                json!({"sessionId":"s1",
            "operation":"extension:invoke-action", "payload":payload}),
                &[],
            )
            .unwrap_err();
        assert_eq!(
            error["message"],
            "This no-view command has no bound actions"
        );
    }
    let error = host
        .request(
            "session.call",
            json!({"sessionId":"s1",
        "operation":"extension:run-command", "payload":{}}),
            &[],
        )
        .unwrap_err();
    assert_eq!(error["message"], "Unsupported session operation");
    assert_eq!(host.reverse.len(), before_calls);
    host.request("session.dispose", json!({"sessionId":"s1"}), &[])
        .unwrap();

    // Explicit argument replaces the cache; replaceClipboard re-copies after paste.
    let mark = host.reverse.len();
    let result = host
        .request(
            "session.open",
            json!({"sessionId":"s2","commandName":"paste-as-plain-text",
                "preferences":{"replaceClipboard":true,"cleanLineBreaks":true},
                "argumentValues":{"advancedPasteFormat":"Number"}}),
            &[
                json!(null),
                json!("a\r\n-1,234.50 b"),
                json!({"ok":true,"message":"Pasted text"}),
                json!({"ok":true}),
                json!(true),
            ],
        )
        .unwrap();
    assert_eq!(result["message"], "🔢 Paste as Number");
    let calls = host.methods_since(mark);
    assert_eq!(
        calls[2],
        (
            "host.clipboard.pasteText".into(),
            json!({"text":"-1234.50"})
        )
    );
    assert_eq!(
        calls[3],
        ("host.clipboard.write".into(), json!({"text":"-1234.50"}))
    );
    host.request("session.dispose", json!({"sessionId":"s2"}), &[])
        .unwrap();
    let store: Value = serde_json::from_slice(
        &std::fs::read(data.join("paste-as-plain-text/storage.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(store["Args"]["PasteAs"], "Number");

    // Empty clipboard: visible terminal failure even on hotkey, never pasted.
    let mark = host.reverse.len();
    let error = host
        .request(
            "session.open",
            json!({"sessionId":"s3","commandName":"paste-as-plain-text","preferences":{"showTips":false}}),
            &[json!(null), json!(""), json!(true)],
        )
        .unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["mode"], "no-view");
    assert_eq!(error["message"], "No content in clipboard");
    assert_eq!(
        host.methods_since(mark)[2],
        (
            "host.notifications.show".into(),
            json!({"id":"paste-result","title":"Paste as Plain Text","body":"No content in clipboard"})
        )
    );
    // The actual host disposes handled no-view failures too; an error here
    // would kill the process and remove its process-owned failure notice.
    let disposed = host
        .request("session.dispose", json!({"sessionId":"s3"}), &[])
        .expect("current host treats a disposal failure as fatal");
    assert_eq!(disposed, json!({"ok":true,"mode":"unchanged"}));
    assert!(host.child.try_wait().unwrap().is_none());

    // A deliberate new launch succeeds in the same process after the failure;
    // no automatic retry or clipboard effect was issued by disposal itself.
    let mark = host.reverse.len();
    let retried = host
        .request(
            "session.open",
            json!({"sessionId":"s3-retry","commandName":"paste-as-plain-text",
            "preferences":{"showTips":false},
            "argumentValues":{"advancedPasteFormat":"Plain Text"}}),
            &[
                json!(null),
                json!("retry"),
                json!({"ok":true,"message":"Pasted text"}),
            ],
        )
        .unwrap();
    assert_eq!(retried["ok"], true);
    assert_eq!(
        host.methods_since(mark)
            .iter()
            .filter(|(method, _)| method == "host.clipboard.pasteText")
            .count(),
        1
    );
    host.request("session.dispose", json!({"sessionId":"s3-retry"}), &[])
        .unwrap();

    // Host errors preserve their original detail as a truthful domain failure.
    let error = host
        .request(
            "session.open",
            json!({"sessionId":"s4","commandName":"paste-as-plain-text"}),
            &[
                json!(null),
                json!("x"),
                json!({"__error":"Accessibility permission denied"}),
                json!(true),
            ],
        )
        .unwrap();
    assert_eq!(error["ok"], false);
    assert_eq!(error["message"], "Accessibility permission denied");
    let disposed = host
        .request("session.dispose", json!({"sessionId":"s4"}), &[])
        .expect("current host treats a disposal failure as fatal");
    assert_eq!(disposed, json!({"ok":true,"mode":"unchanged"}));

    // Cancellation while a reverse request is pending: the extension cancels
    // the pending reverse request and fails the open; nothing is pasted.
    let open = host.start(
        "session.open",
        json!({"sessionId":"s5","commandName":"paste-as-plain-text"}),
    );
    let hide = host.recv();
    assert_eq!(hide["method"], "host.window.visibility");
    host.send(json!({"type":"response","id":hide["id"],"result":null}));
    let read = host.recv();
    assert_eq!(read["method"], "host.clipboard.read");
    host.send(json!({"type":"cancel","requestId":open,"reason":"user"}));
    let (mut cancelled, mut failed) = (false, false);
    while !(cancelled && failed) {
        let frame = host.recv();
        match frame["type"].as_str().unwrap() {
            "cancel" => {
                assert_eq!(frame["requestId"], read["id"]);
                cancelled = true;
            }
            "response" => {
                assert_eq!(frame["id"], open.as_str());
                assert!(frame["error"].is_object(), "{frame}");
                failed = true;
            }
            _ => panic!("unexpected frame after cancel: {frame}"),
        }
    }
    // Late reply to the cancelled reverse request is tolerated.
    host.send(json!({"type":"response","id":read["id"],"result":"late"}));
    // A failed open releases its SDK slot; there is nothing left to dispose.
    let gone = host
        .request("session.dispose", json!({"sessionId":"s5"}), &[])
        .unwrap_err();
    assert_eq!(gone["message"], "Unknown session");

    // Unknown command is rejected by the SDK before any effect.
    let before = host.reverse.len();
    assert!(host
        .request(
            "session.open",
            json!({"sessionId":"s6","commandName":"nope"}),
            &[]
        )
        .is_err());
    assert_eq!(host.reverse.len(), before);

    host.request("shutdown", json!({}), &[]).unwrap();
    let status = host.child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn saved_string_preferences_control_transform_copy_fetch_and_terminal_feedback() {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let manifest: Value = serde_json::from_str(include_str!("../../extension.json")).unwrap();
    let mut host = FakeHost::spawn();
    host.recv();
    host.request("initialize", json!({"protocol":1,"extensionId":ID,"platform":"macOS","packageDir":dir.path().join("package"),"dataDir":dir.path().join("data"),"tempDir":dir.path().join("tmp"),"grantedCapabilities":manifest["capabilities"],"reason":"command"}), &[]).unwrap();
    let mark = host.reverse.len();
    let result = host.request("session.open", json!({"sessionId":"false","commandName":"paste-as-plain-text","preferences":{"trimStart":"false","trimEnd":"false","cleanLineBreaks":"false","autoFetchTitle":"false","replaceClipboard":"false","showTips":"false"},"argumentValues":{"advancedPasteFormat":""}}), &[json!(null),json!("  a\r\nb  "),json!({"ok":true,"message":"Pasted text"})]).unwrap();
    assert_eq!(result["message"], "");
    assert_eq!(
        host.methods_since(mark),
        [
            ("host.window.visibility".into(), json!({"visible":false})),
            ("host.clipboard.read".into(), json!({"format":"text"})),
            (
                "host.clipboard.pasteText".into(),
                json!({"text":"  a\r\nb  "})
            )
        ]
    );
    host.request("session.dispose", json!({"sessionId":"false"}), &[])
        .unwrap();
    let mark = host.reverse.len();
    let result = host.request("session.open", json!({"sessionId":"true","commandName":"paste-as-plain-text","preferences":{"trimStart":"true","trimEnd":"true","cleanLineBreaks":"true","autoFetchTitle":"true","replaceClipboard":"true","showTips":"true"},"argumentValues":{"advancedPasteFormat":""}}), &[json!(null),json!("  a\r\nb  "),json!({"ok":true,"message":"Pasted text"}),json!({"ok":true}),json!(true)]).unwrap();
    assert_eq!(result["message"], "📋 Paste as Plain Text");
    let calls = host.methods_since(mark);
    assert_eq!(
        calls[2],
        ("host.clipboard.pasteText".into(), json!({"text":"a b"}))
    );
    assert_eq!(
        calls[3],
        ("host.clipboard.write".into(), json!({"text":"a b"}))
    );
    assert_eq!(
        calls[4],
        (
            "host.notifications.show".into(),
            json!({"id":"paste-result","title":"Paste as Plain Text","body":"📋 Paste as Plain Text"})
        )
    );
    host.request("session.dispose", json!({"sessionId":"true"}), &[])
        .unwrap();

    // A local listener proves false does not fetch, and true really does. No
    // external DNS/network or OS clipboard effects are permitted in this test.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/title", listener.local_addr().unwrap());
    let mark = host.reverse.len();
    host.request("session.open", json!({"sessionId":"no-fetch","commandName":"paste-as-plain-text","preferences":{"autoFetchTitle":"false","showTips":"false"},"argumentValues":{"advancedPasteFormat":"Markdown Link"}}), &[json!(null),json!(url),json!({"ok":true,"message":"Pasted text"})]).unwrap();
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        host.methods_since(mark)[2].1,
        json!({"text":format!("[]({url})")})
    );
    host.request("session.dispose", json!({"sessionId":"no-fetch"}), &[])
        .unwrap();
    let server = std::thread::spawn(move || {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("loopback title request: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buffer = [0; 2048];
        assert!(stream.read(&mut buffer).unwrap() > 0);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n<title>Saved &amp; Exact</title>",
            )
            .unwrap();
    });
    let mark = host.reverse.len();
    host.request("session.open", json!({"sessionId":"fetch","commandName":"paste-as-plain-text","preferences":{"autoFetchTitle":"true","showTips":"false"},"argumentValues":{"advancedPasteFormat":"Markdown Link"}}), &[json!(null),json!(url),json!({"ok":true,"message":"Pasted text"})]).unwrap();
    assert_eq!(
        host.methods_since(mark)[2].1,
        json!({"text":format!("[Saved & Exact]({url})")})
    );
    server.join().unwrap();
    host.request("session.dispose", json!({"sessionId":"fetch"}), &[])
        .unwrap();
    host.request("shutdown", json!({}), &[]).unwrap();
    assert!(host.child.wait().unwrap().success());
}

#[test]
fn managed_cache_files_repair_previous_empty_import_over_real_pipes() {
    let manifest: Value = serde_json::from_str(include_str!("../../extension.json")).unwrap();
    for (key, relative) in manifest["migrationPaths"].as_object().unwrap() {
        if key == "nativeStore" {
            continue;
        }
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(relative.as_str().unwrap());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        let bytes = b"{\n \"PasteAs\":\"Number\", \"private-source-key\":1\n}\n";
        std::fs::write(&legacy, bytes).unwrap();
        let data = dir.path().join("data");
        let private = data.join("paste-as-plain-text/storage.json");
        std::fs::create_dir_all(private.parent().unwrap()).unwrap();
        std::fs::write(
            &private,
            br#"{"_legacyArgsImported":true,"private-key":"kept"}"#,
        )
        .unwrap();
        let mut host = FakeHost::spawn();
        host.recv();
        host.request("initialize",json!({"protocol":1,"extensionId":ID,"platform":"macOS","packageDir":dir.path().join("package"),"dataDir":data,"tempDir":dir.path().join("tmp"),"grantedCapabilities":manifest["capabilities"],"reason":"command","migrationPaths":{key:legacy}}),&[]).unwrap();
        let mark = host.reverse.len();
        let result = host.request("session.open",json!({"sessionId":"migrated","commandName":"paste-as-plain-text","preferences":{"showTips":"false"}}),&[json!(null),json!("a 1,234 b"),json!({"ok":true,"message":"Pasted text"})]).unwrap();
        assert_eq!(result, json!({"ok":true,"mode":"no-view","message":""}));
        assert_eq!(
            host.methods_since(mark)[2].1,
            json!({"text":"1234"}),
            "manifest path {key}"
        );
        let migrated: Value = serde_json::from_slice(&std::fs::read(private).unwrap()).unwrap();
        assert_eq!(
            migrated,
            json!({"_legacyArgsImported":true,"private-key":"kept","Args":{"PasteAs":"Number"}})
        );
        assert_eq!(std::fs::read(legacy).unwrap(), bytes);
        host.request("session.dispose", json!({"sessionId":"migrated"}), &[])
            .unwrap();
        host.request("shutdown", json!({}), &[]).unwrap();
        assert!(host.child.wait().unwrap().success());
    }
}
