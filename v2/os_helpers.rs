//! Typed OS clipboard helpers. Scripts are constant; paths never become script source.
use crate::Result;
use async_trait::async_trait;
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Windows,
    Unsupported,
}
impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOS
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unsupported
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}
const MAC_WRITE: &str =
    "on run argv\nset the clipboard to (POSIX file (item 1 of argv) as alias)\nend run";
const MAC_READ: &str =
    "try\nreturn POSIX path of (the clipboard as alias)\non error\nreturn \"\"\nend try";
const WIN_WRITE: &str = "$ErrorActionPreference = 'Stop'; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Add-Type -AssemblyName System.Windows.Forms; $files = New-Object System.Collections.Specialized.StringCollection; [void]$files.Add($env:TEZBAR_PASTE_FILE); [System.Windows.Forms.Clipboard]::SetFileDropList($files)";
const WIN_READ: &str = "$ErrorActionPreference = 'Stop'; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); Add-Type -AssemblyName System.Windows.Forms; $files = [System.Windows.Forms.Clipboard]::GetFileDropList(); if ($files.Count -gt 0) { [Console]::Write($files[0]) }";
pub fn plan(platform: Platform, path: Option<&str>) -> Result<CommandPlan> {
    if path.is_some_and(|p| p.contains('\0')) {
        return Err("File path contains NUL".into());
    }
    let mut env = BTreeMap::new();
    let (program, args) = match platform {
        Platform::MacOS => {
            let mut args = vec![
                "-e".into(),
                if path.is_some() { MAC_WRITE } else { MAC_READ }.into(),
            ];
            if let Some(p) = path {
                args.push("--".into());
                args.push(p.into());
            }
            ("/usr/bin/osascript", args)
        }
        Platform::Windows => {
            if let Some(p) = path {
                env.insert("TEZBAR_PASTE_FILE".into(), p.into());
            }
            (
                "powershell.exe",
                vec![
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-STA".into(),
                    "-Command".into(),
                    if path.is_some() { WIN_WRITE } else { WIN_READ }.into(),
                ],
            )
        }
        Platform::Unsupported => return Err("File clipboard requires macOS or Windows".into()),
    };
    Ok(CommandPlan {
        program: program.into(),
        args,
        env,
    })
}
#[async_trait]
pub trait Runner: Sync {
    async fn run(&self, plan: CommandPlan) -> Result<String>;
}
pub struct SystemRunner;
#[async_trait]
impl Runner for SystemRunner {
    async fn run(&self, plan: CommandPlan) -> Result<String> {
        use std::process::Stdio;
        use tokio::{io::AsyncReadExt, process::Command};
        let mut child = Command::new(&plan.program)
            .args(&plan.args)
            .envs(&plan.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("File clipboard helper unavailable: {e}"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or("Helper stdout missing")?
            .take(64 * 1024 + 1);
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            stdout
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| e.to_string())?;
            if bytes.len() > 64 * 1024 {
                return Err("File clipboard helper output exceeds limit".into());
            }
            let status = child.wait().await.map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(format!("File clipboard helper failed ({status})"));
            }
            String::from_utf8(bytes).map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| "File clipboard helper timed out".to_owned())?
    }
}
pub async fn read_file(runner: &impl Runner, platform: Platform) -> Result<Option<String>> {
    let raw = runner.run(plan(platform, None)?).await?;
    // osascript adds exactly one record terminator; don't trim path whitespace.
    let path = if platform == Platform::MacOS {
        raw.strip_suffix('\n').unwrap_or(&raw)
    } else {
        &raw
    };
    Ok((!path.is_empty()).then(|| path.to_owned()))
}
pub async fn write_file(runner: &impl Runner, platform: Platform, path: &str) -> Result<()> {
    runner.run(plan(platform, Some(path))?).await.map(|_| ())
}
