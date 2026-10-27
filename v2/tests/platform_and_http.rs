use async_trait::async_trait;
use std::{sync::Mutex, time::Duration};
use tezbar_paste_as_plain_text::{
    formats, http,
    os_helpers::{self, CommandPlan, Platform, Runner},
};

struct Stub {
    calls: Mutex<Vec<CommandPlan>>,
    output: String,
}
#[async_trait]
impl Runner for Stub {
    async fn run(&self, plan: CommandPlan) -> Result<String, String> {
        self.calls.lock().unwrap().push(plan);
        Ok(self.output.clone())
    }
}
#[tokio::test]
async fn safe_file_helpers_preserve_paths_and_read_real_metadata_shape_with_stubs() {
    let paths = [
        "/tmp/Қазақ 😀 ' \" `touch nope` $(x);.txt",
        "C:\\Users\\Қазақ\\a'b\"$(x) & ;.pdf",
        "/tmp/-leading\nnewline\t end ",
    ];
    for platform in [Platform::MacOS, Platform::Windows] {
        for path in paths {
            let runner = Stub {
                calls: Mutex::new(vec![]),
                output: String::new(),
            };
            os_helpers::write_file(&runner, platform, path)
                .await
                .unwrap();
            let plans = runner.calls.lock().unwrap();
            let plan = &plans[0];
            if platform == Platform::MacOS {
                assert_eq!(plan.program, "/usr/bin/osascript");
                assert_eq!(plan.args.last().unwrap(), path);
                assert!(!plan.args[1].contains(path));
                assert_eq!(plan.args[2], "--");
            } else {
                assert_eq!(plan.program, "powershell.exe");
                assert!(plan.args.contains(&"-STA".into()));
                assert_eq!(plan.env["TEZBAR_PASTE_FILE"], path);
                assert!(!plan.args.last().unwrap().contains(path));
                assert!(plan.args.last().unwrap().contains("SetFileDropList"));
            }
        }
    }
    for (platform, output, expected) in [
        (Platform::MacOS, "/tmp/ spaced \n", Some("/tmp/ spaced ")),
        (Platform::Windows, "C:\\😀\\a.txt", Some("C:\\😀\\a.txt")),
        (Platform::MacOS, "\n", None),
        (Platform::Windows, "", None),
    ] {
        let runner = Stub {
            calls: Mutex::new(vec![]),
            output: output.into(),
        };
        assert_eq!(
            os_helpers::read_file(&runner, platform)
                .await
                .unwrap()
                .as_deref(),
            expected
        );
    }
    for path in [None, Some("C:\\a.txt")] {
        let plan = os_helpers::plan(Platform::Windows, path).unwrap();
        assert!(plan
            .args
            .last()
            .unwrap()
            .starts_with("$ErrorActionPreference = 'Stop';"));
    }
    assert!(os_helpers::plan(Platform::Unsupported, Some("x")).is_err());
    assert!(os_helpers::plan(Platform::MacOS, Some("a\0b")).is_err());
}
#[test]
fn node_file_url_conversion_rules() {
    #[cfg(not(windows))]
    {
        assert_eq!(
            formats::file_url_path("file:///tmp/a%20b%F0%9F%98%80.txt").as_deref(),
            Some("/tmp/a b😀.txt")
        );
        assert_eq!(
            formats::file_url_path("file://localhost/tmp/a").as_deref(),
            Some("/tmp/a")
        );
        assert_eq!(formats::file_url_path("file://server/tmp/a"), None);
    }
    #[cfg(windows)]
    {
        assert_eq!(
            formats::file_url_path("file:///C:/a%20b.txt").as_deref(),
            Some("C:\\a b.txt")
        );
    }
    for input in [
        "/tmp/a",
        "https://example.com",
        "file:///tmp/a%2fb",
        "file:///tmp/a%zz",
        "file:///tmp/a%2",
    ] {
        assert_eq!(formats::file_url_path(input), None, "{input}");
    }
}
// One bounded local request, with no DNS or external server.
fn fixture(response: Vec<u8>, delay: Duration) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 2048];
        let _ = socket.read(&mut request);
        std::thread::sleep(delay);
        let _ = socket.write_all(&response);
    });
    (url, thread)
}
#[tokio::test]
async fn bounded_title_fetch_success_error_timeout_and_size_limits() {
    let cases = [
        (
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n<title>A &amp; B</title><p>x</p>"
                .to_vec(),
            Duration::ZERO,
            1024,
            Some("A & B"),
        ),
        (
            b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n<p>no title</p>".to_vec(),
            Duration::ZERO,
            1024,
            Some(""),
        ),
        (
            b"HTTP/1.1 500 Error\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_vec(),
            Duration::ZERO,
            1024,
            None,
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 99999\r\n\r\n".to_vec(),
            Duration::ZERO,
            128,
            None,
        ),
        (
            format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
                "x".repeat(512)
            )
            .into_bytes(),
            Duration::ZERO,
            128,
            None,
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
            Duration::from_millis(150),
            1024,
            None,
        ),
    ];
    for (response, delay, limit, expected) in cases {
        let (url, thread) = fixture(response, delay);
        let reply = http::fetch_title_with_limits(&url, Duration::from_millis(80), limit).await;
        match expected {
            Some(s) => assert_eq!(reply.unwrap(), s),
            None => assert!(reply.is_err(), "{reply:?}"),
        }
        thread.join().unwrap();
    }
    assert!(http::fetch_title("ftp://example.invalid").await.is_err());
}
