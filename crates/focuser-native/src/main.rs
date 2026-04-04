//! Focuser Native Messaging Host
//!
//! Bridges the browser extension to the Focuser service via the Native Messaging
//! protocol (stdin/stdout length-prefixed JSON).
//!
//! ## Architecture
//!
//! ```text
//! Browser Extension <──stdio──> focuser-native <──IPC──> focuser-service
//! ```
//!
//! The extension sends `NativeMessage` envelopes with `ExtensionEvent` payloads.
//! This binary translates them to `IpcRequest::ExtensionEvent` and forwards to
//! the service, then returns the response as a `NativeMessage` on stdout.

mod ipc_client;

use std::io::{self, Read, Write};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use focuser_common::extension::{BrowserType, ExtensionEvent, NativeMessage, NativeMessageType};
use focuser_common::ipc::{IpcRequest, IpcResponse};
use tracing::{debug, error, info};

#[derive(Parser)]
#[command(
    name = "focuser-native",
    about = "Focuser Native Messaging host — bridges browser extension to service"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Register this binary as a Native Messaging host for browsers.
    Install {
        /// Browser extension ID (for Chrome/Chromium allowed_origins).
        #[arg(long, default_value = "")]
        extension_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Log to file instead of stdout (stdout is reserved for Native Messaging)
    let log_dir = directories::ProjectDirs::from("com", "focuser", "Focuser")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("focuser-native.log");

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();

    if let Some(file) = log_file {
        tracing_subscriber::fmt()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false)
            .init();
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Install { extension_id }) => {
            install_native_host(&extension_id)?;
            return Ok(());
        }
        None => {
            // Default: run as Native Messaging host
            run_native_host().await?;
        }
    }

    Ok(())
}

/// Run the Native Messaging host loop.
async fn run_native_host() -> Result<()> {
    info!("Focuser native messaging host started");

    // Send Connected event to service
    let browser = detect_parent_browser();
    let connect_event = ExtensionEvent::Connected {
        browser: browser.clone(),
        extension_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let _ = ipc_client::send(IpcRequest::ExtensionEvent(connect_event)).await;

    // Main loop: read messages from stdin, forward to service
    loop {
        let msg = match read_native_message() {
            Ok(msg) => msg,
            Err(e) => {
                // EOF or read error — extension disconnected
                debug!(error = %e, "Failed to read native message, shutting down");
                break;
            }
        };

        debug!(msg_type = ?msg.msg_type, "Received native message");

        let response = match msg.msg_type {
            NativeMessageType::Ping => NativeMessage {
                msg_type: NativeMessageType::Pong,
                payload: serde_json::Value::Null,
            },
            NativeMessageType::Event => {
                // Deserialize the event payload
                match serde_json::from_value::<ExtensionEvent>(msg.payload) {
                    Ok(event) => {
                        let ipc_resp = ipc_client::send(IpcRequest::ExtensionEvent(event)).await;
                        match ipc_resp {
                            Ok(resp) => NativeMessage {
                                msg_type: NativeMessageType::Event,
                                payload: serde_json::to_value(&resp)
                                    .unwrap_or(serde_json::Value::Null),
                            },
                            Err(e) => NativeMessage {
                                msg_type: NativeMessageType::Error,
                                payload: serde_json::Value::String(e.to_string()),
                            },
                        }
                    }
                    Err(e) => NativeMessage {
                        msg_type: NativeMessageType::Error,
                        payload: serde_json::Value::String(format!("Invalid event payload: {e}")),
                    },
                }
            }
            NativeMessageType::RuleUpdate => {
                // Extension requesting rules
                match ipc_client::send(IpcRequest::GetExtensionRules).await {
                    Ok(IpcResponse::ExtensionRules(rules)) => NativeMessage {
                        msg_type: NativeMessageType::RuleUpdate,
                        payload: serde_json::to_value(&rules).unwrap_or(serde_json::Value::Null),
                    },
                    Ok(resp) => NativeMessage {
                        msg_type: NativeMessageType::RuleUpdate,
                        payload: serde_json::to_value(&resp).unwrap_or(serde_json::Value::Null),
                    },
                    Err(e) => NativeMessage {
                        msg_type: NativeMessageType::Error,
                        payload: serde_json::Value::String(e.to_string()),
                    },
                }
            }
            _ => NativeMessage {
                msg_type: NativeMessageType::Pong,
                payload: serde_json::Value::Null,
            },
        };

        if let Err(e) = write_native_message(&response) {
            error!(error = %e, "Failed to write native message");
            break;
        }
    }

    // Send Disconnected event
    let disconnect_event = ExtensionEvent::Disconnected {
        browser: browser.clone(),
    };
    let _ = ipc_client::send(IpcRequest::ExtensionEvent(disconnect_event)).await;

    info!("Focuser native messaging host stopped");
    Ok(())
}

// ─── Native Messaging Protocol ──────────────────────────────────────

/// Read a length-prefixed JSON message from stdin.
fn read_native_message() -> Result<NativeMessage> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    // Read 4-byte little-endian length
    let mut len_buf = [0u8; 4];
    handle
        .read_exact(&mut len_buf)
        .context("Failed to read message length")?;
    let len = u32::from_le_bytes(len_buf) as usize;

    if len == 0 || len > 1024 * 1024 {
        anyhow::bail!("Invalid message length: {len}");
    }

    // Read JSON payload
    let mut buf = vec![0u8; len];
    handle
        .read_exact(&mut buf)
        .context("Failed to read message body")?;

    let msg: NativeMessage =
        serde_json::from_slice(&buf).context("Failed to parse native message JSON")?;
    Ok(msg)
}

/// Write a length-prefixed JSON message to stdout.
fn write_native_message(msg: &NativeMessage) -> Result<()> {
    let json = serde_json::to_vec(msg).context("Failed to serialize native message")?;
    let len = json.len() as u32;

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    handle
        .write_all(&len.to_le_bytes())
        .context("Failed to write message length")?;
    handle
        .write_all(&json)
        .context("Failed to write message body")?;
    handle.flush().context("Failed to flush stdout")?;

    Ok(())
}

// ─── Browser Detection ──────────────────────────────────────────────

/// Try to detect which browser launched this native host.
fn detect_parent_browser() -> BrowserType {
    // The parent process is typically the browser.
    // We can check environment variables or parent process name.
    // For now, check common env vars set by browsers.
    if std::env::var("CHROME_WRAPPER").is_ok() {
        return BrowserType::Chrome;
    }
    // Default to Other — the extension will send a Connected event with the correct type
    BrowserType::Other("unknown".to_string())
}

// ─── Native Messaging Host Installation ─────────────────────────────

/// Install the native messaging host manifest for supported browsers.
fn install_native_host(extension_id: &str) -> Result<()> {
    let exe_path = std::env::current_exe().context("Cannot determine executable path")?;
    let exe_str = exe_path.to_string_lossy().to_string();

    println!("Installing Focuser native messaging host...");
    println!("Binary: {exe_str}");

    #[cfg(windows)]
    install_windows(&exe_str, extension_id)?;

    #[cfg(target_os = "linux")]
    install_linux(&exe_str, extension_id)?;

    #[cfg(target_os = "macos")]
    install_macos(&exe_str, extension_id)?;

    println!("Installation complete.");
    Ok(())
}

/// Generate the native messaging host manifest JSON.
fn host_manifest(exe_path: &str, extension_id: &str) -> String {
    let allowed_origins = if extension_id.is_empty() {
        // During development, allow all chrome extensions
        "\"chrome-extension://*/\"".to_string()
    } else {
        format!("\"chrome-extension://{extension_id}/\"")
    };

    format!(
        r#"{{
  "name": "com.focuser.native",
  "description": "Focuser Native Messaging Host",
  "path": "{}",
  "type": "stdio",
  "allowed_origins": [{}]
}}"#,
        exe_path.replace('\\', "\\\\"),
        allowed_origins,
    )
}

/// Firefox-specific manifest (uses allowed_extensions instead of allowed_origins).
fn host_manifest_firefox(exe_path: &str) -> String {
    format!(
        r#"{{
  "name": "com.focuser.native",
  "description": "Focuser Native Messaging Host",
  "path": "{}",
  "type": "stdio",
  "allowed_extensions": ["focuser@focuser-app"]
}}"#,
        exe_path.replace('\\', "\\\\"),
    )
}

#[cfg(windows)]
fn install_windows(exe_path: &str, extension_id: &str) -> Result<()> {
    use std::fs;

    // Write manifest files
    let data_dir = directories::ProjectDirs::from("com", "focuser", "Focuser")
        .context("Cannot determine data directory")?;
    let manifest_dir = data_dir.data_dir().join("native-messaging");
    fs::create_dir_all(&manifest_dir)?;

    let chrome_manifest = manifest_dir.join("com.focuser.native.json");
    let firefox_manifest = manifest_dir.join("com.focuser.native.firefox.json");

    fs::write(&chrome_manifest, host_manifest(exe_path, extension_id))?;
    fs::write(&firefox_manifest, host_manifest_firefox(exe_path))?;

    println!("Wrote Chrome manifest:  {}", chrome_manifest.display());
    println!("Wrote Firefox manifest: {}", firefox_manifest.display());

    // Register in Windows Registry
    // Chrome: HKCU\Software\Google\Chrome\NativeMessagingHosts\com.focuser.native
    // Edge:   HKCU\Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native
    // Firefox: HKCU\Software\Mozilla\NativeMessagingHosts\com.focuser.native
    let registry_entries = [
        (
            r"Software\Google\Chrome\NativeMessagingHosts\com.focuser.native",
            chrome_manifest.to_string_lossy().to_string(),
        ),
        (
            r"Software\Microsoft\Edge\NativeMessagingHosts\com.focuser.native",
            chrome_manifest.to_string_lossy().to_string(),
        ),
        (
            r"Software\Mozilla\NativeMessagingHosts\com.focuser.native",
            firefox_manifest.to_string_lossy().to_string(),
        ),
    ];

    for (key_path, manifest_path) in &registry_entries {
        match write_registry_key(key_path, manifest_path) {
            Ok(()) => println!("Registered: {key_path}"),
            Err(e) => eprintln!("Warning: Failed to register {key_path}: {e}"),
        }
    }

    Ok(())
}

#[cfg(windows)]
fn write_registry_key(key_path: &str, value: &str) -> Result<()> {
    use std::process::Command;

    // Use reg.exe to set the registry key (avoids winreg dependency)
    let output = Command::new("reg")
        .args([
            "add",
            &format!(r"HKCU\{key_path}"),
            "/ve",
            "/d",
            value,
            "/f",
        ])
        .output()
        .context("Failed to run reg.exe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("reg.exe failed: {stderr}");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_linux(exe_path: &str, extension_id: &str) -> Result<()> {
    use std::fs;

    let home = std::env::var("HOME").context("HOME not set")?;

    // Chrome
    let chrome_dir = format!("{home}/.config/google-chrome/NativeMessagingHosts");
    fs::create_dir_all(&chrome_dir)?;
    fs::write(
        format!("{chrome_dir}/com.focuser.native.json"),
        host_manifest(exe_path, extension_id),
    )?;
    println!("Installed for Chrome: {chrome_dir}/com.focuser.native.json");

    // Chromium
    let chromium_dir = format!("{home}/.config/chromium/NativeMessagingHosts");
    fs::create_dir_all(&chromium_dir)?;
    fs::write(
        format!("{chromium_dir}/com.focuser.native.json"),
        host_manifest(exe_path, extension_id),
    )?;
    println!("Installed for Chromium: {chromium_dir}/com.focuser.native.json");

    // Firefox
    let firefox_dir = format!("{home}/.mozilla/native-messaging-hosts");
    fs::create_dir_all(&firefox_dir)?;
    fs::write(
        format!("{firefox_dir}/com.focuser.native.json"),
        host_manifest_firefox(exe_path),
    )?;
    println!("Installed for Firefox: {firefox_dir}/com.focuser.native.json");

    Ok(())
}

#[cfg(target_os = "macos")]
fn install_macos(exe_path: &str, extension_id: &str) -> Result<()> {
    use std::fs;

    let home = std::env::var("HOME").context("HOME not set")?;

    // Chrome
    let chrome_dir =
        format!("{home}/Library/Application Support/Google/Chrome/NativeMessagingHosts");
    fs::create_dir_all(&chrome_dir)?;
    fs::write(
        format!("{chrome_dir}/com.focuser.native.json"),
        host_manifest(exe_path, extension_id),
    )?;
    println!("Installed for Chrome: {chrome_dir}/com.focuser.native.json");

    // Firefox
    let firefox_dir = format!("{home}/Library/Application Support/Mozilla/NativeMessagingHosts");
    fs::create_dir_all(&firefox_dir)?;
    fs::write(
        format!("{firefox_dir}/com.focuser.native.json"),
        host_manifest_firefox(exe_path),
    )?;
    println!("Installed for Firefox: {firefox_dir}/com.focuser.native.json");

    Ok(())
}
