use std::fs::{File, OpenOptions};
use std::io::{Read, Write, BufWriter};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use ssh2::Session;

use crate::image::{DEFAULT_PASSWORD, DEFAULT_USER};
use crate::keys::{ensure_key_pair, read_public_key};

pub struct ProvisionOptions {
    pub with_playwright: bool,
}

const VM_LOG: &str = "/var/log/fastclaw-provision.log";

fn connect_ssh(ip: &str, user: &str, password: &str) -> Result<Session> {
    let addr = format!("{ip}:22");
    let tcp =
        TcpStream::connect_timeout(&addr.parse().context("Invalid SSH address")?, Duration::from_secs(10))
            .context(format!("Cannot connect SSH to {addr}"))?;
    let mut sess = Session::new().context("Failed to create SSH session")?;
    sess.set_tcp_stream(tcp);
    sess.handshake().context("SSH handshake failed")?;
    sess.userauth_password(user, password)
        .context("SSH password auth failed")?;
    if !sess.authenticated() {
        bail!("SSH authentication failed for '{user}' at {ip}");
    }
    Ok(sess)
}

fn retry_connect(ip: &str, user: &str, password: &str, timeout: Duration) -> Result<Session> {
    let start = Instant::now();
    loop {
        match connect_ssh(ip, user, password) {
            Ok(sess) => return Ok(sess),
            Err(_) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_secs(3));
            }
            Err(e) => return Err(e.context("SSH connection retries exhausted")),
        }
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Run a command via SSH. Output is logged to both the VM and host log files.
fn run(sess: &Session, cmd: &str, host_log: &mut BufWriter<File>) -> Result<String> {
    let _ = writeln!(host_log, ">>> {cmd}");
    let _ = host_log.flush();

    let logged_cmd = format!("{{ {cmd} ; }} 2>&1 | sudo tee -a {VM_LOG}");
    let wrapped = format!("bash -lc {}", shell_quote(&logged_cmd));

    let mut channel = sess.channel_session().context("Failed to open SSH channel")?;
    channel.exec(&wrapped).context(format!("Failed to exec: {cmd}"))?;

    let mut output = String::new();
    channel.read_to_string(&mut output).context("Failed to read command output")?;
    channel.wait_close()?;
    let exit = channel.exit_status()?;

    if !output.is_empty() {
        let _ = write!(host_log, "{output}");
    }
    let _ = writeln!(host_log, "<<< exit {exit}");
    let _ = host_log.flush();

    if exit != 0 {
        eprintln!("  ✗ FAILED (exit {exit}): {cmd}");
        if !output.is_empty() {
            let last_lines: Vec<&str> = output.lines().rev().take(10).collect();
            for line in last_lines.iter().rev() {
                eprintln!("    {line}");
            }
        }
        bail!("Remote command failed (exit {exit}):\n  cmd: {cmd}\n  output: {output}");
    }
    Ok(output)
}

/// Run a verbose command (apt-get, npm, curl). Output goes only to log files
/// to avoid SSH channel buffer overflow.
fn run_logged(sess: &Session, label: &str, cmd: &str, host_log: &mut BufWriter<File>) -> Result<()> {
    println!("    → {label}");
    let _ = writeln!(host_log, "\n=== [{label}] ===\n>>> {cmd}");
    let _ = host_log.flush();

    let full_cmd = format!(
        "echo '=== [{label}] ===' | sudo tee -a {VM_LOG} > /dev/null && {cmd} 2>&1 | sudo tee -a {VM_LOG} > /dev/null"
    );
    let wrapped = format!("bash -lc {}", shell_quote(&full_cmd));

    let mut channel = sess.channel_session().context("Failed to open SSH channel")?;
    channel.exec(&wrapped).context(format!("Failed to exec: {cmd}"))?;

    let mut output = String::new();
    channel.read_to_string(&mut output).ok();
    channel.wait_close()?;
    let exit = channel.exit_status()?;

    let _ = writeln!(host_log, "<<< exit {exit} (output on VM at {VM_LOG})");
    let _ = host_log.flush();

    if exit != 0 {
        let diag = run(sess, &format!("tail -15 {VM_LOG} 2>/dev/null"), host_log).unwrap_or_default();
        eprintln!("  ✗ FAILED: {label}");
        eprintln!("  Last log lines:\n{diag}");
        let _ = writeln!(host_log, "FAILED: {label}\n{diag}");
        bail!("Remote command failed (exit {exit}):\n  step: {label}\n  cmd: {cmd}\n  log tail:\n{diag}");
    }

    println!("    ✓ {label}");
    Ok(())
}

/// Write a text file on the VM line-by-line via SSH (shell-safe).
fn write_text_file(sess: &Session, lines: &[&str], dest: &str, host_log: &mut BufWriter<File>) -> Result<()> {
    let _ = writeln!(host_log, "--- write_text_file: {dest} ({} lines) ---", lines.len());
    run(sess, &format!(": > {dest}"), host_log)?;
    for line in lines {
        let safe_line = line.replace('\'', "'\\''");
        run(sess, &format!("printf '%s\\n' '{safe_line}' >> {dest}"), host_log)?;
    }
    let check = run(sess, &format!("wc -l < {dest}"), host_log)?;
    let _ = writeln!(host_log, "--- {dest}: {} lines written (expected {}) ---", check.trim(), lines.len());
    Ok(())
}

pub fn wait_for_ssh(ip: &str, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    let addr = format!("{ip}:22");
    loop {
        let parsed: std::net::SocketAddr = addr.parse().context("Invalid address")?;
        if TcpStream::connect_timeout(&parsed, Duration::from_secs(3)).is_ok() {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            bail!("Timeout waiting for SSH at {addr}");
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn host_log_path() -> Result<std::path::PathBuf> {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("fastclaw");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("provision.log"))
}

pub fn provision_vm(ip: &str, opts: &ProvisionOptions) -> Result<()> {
    let log_path = host_log_path()?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context(format!("Cannot open host log: {}", log_path.display()))?;
    let mut log = BufWriter::new(log_file);
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let _ = writeln!(log, "\n\n============================================================\nProvisioning started at {timestamp} — VM IP: {ip}\n");

    println!("📋 Host log: {}", log_path.display());
    println!("Waiting for SSH...");
    wait_for_ssh(ip, Duration::from_secs(120))?;

    println!("Connecting...");
    let sess = retry_connect(ip, DEFAULT_USER, DEFAULT_PASSWORD, Duration::from_secs(60))?;
    let _ = writeln!(log, "SSH connected to {ip} as {DEFAULT_USER}");

    run(&sess, &format!("echo '=== fastclaw provisioning started ===' | sudo tee {VM_LOG} > /dev/null"), &mut log)?;

    // [1] System packages
    println!("[1/7] Updating system packages...");
    run(&sess, "for i in $(seq 1 60); do \
         sudo fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1 || break; \
         echo \"Waiting for dpkg lock ($i)...\"; sleep 2; done", &mut log)?;
    run_logged(&sess, "apt-get update", "sudo apt-get update -qq", &mut log)?;
    run_logged(
        &sess,
        "install base packages",
        "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
         curl ca-certificates build-essential python3 pkg-config git \
         libvips-dev unzip apt-transport-https gnupg",
        &mut log,
    )?;

    // [2] XFCE desktop
    println!("[2/7] Installing XFCE desktop...");
    run_logged(
        &sess,
        "install XFCE + LightDM",
        "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
         xfce4 xfce4-terminal xfce4-goodies \
         lightdm lightdm-gtk-greeter \
         dbus-x11 at-spi2-core \
         fonts-dejavu fonts-noto \
         xdg-utils x11-xserver-utils",
        &mut log,
    )?;
    run(&sess, "sudo systemctl enable lightdm", &mut log)?;
    run(&sess, "sudo systemctl set-default graphical.target", &mut log)?;

    // [3] Firefox ESR
    println!("[3/7] Installing Firefox ESR...");
    run_logged(&sess, "install Firefox ESR",
        "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y firefox-esr", &mut log)?;

    // [4] Node.js 22
    println!("[4/7] Installing Node.js 22...");
    run_logged(&sess, "add nodesource repo",
        "curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -", &mut log)?;
    run_logged(&sess, "install nodejs",
        "sudo apt-get install -y nodejs", &mut log)?;

    let node_ver = run(&sess, "node --version", &mut log)?;
    println!("    Node.js version: {}", node_ver.trim());

    // [5] OpenClaw
    println!("[5/7] Installing OpenClaw...");
    run_logged(&sess, "npm install -g openclaw@latest",
        "sudo npm install -g openclaw@latest", &mut log)?;

    println!("    Verifying OpenClaw...");
    let oc_check = run(&sess,
        "echo '--- which ---' && which openclaw 2>&1 && \
         echo '--- readlink ---' && readlink -f $(which openclaw) 2>&1 && \
         echo '--- file size ---' && wc -c $(readlink -f $(which openclaw)) 2>&1 && \
         echo '--- version ---' && openclaw --version 2>&1 || echo 'OPENCLAW_CHECK_FAILED'",
        &mut log)?;

    for line in oc_check.lines() {
        println!("      {line}");
    }

    if oc_check.contains("OPENCLAW_CHECK_FAILED") || oc_check.contains(" 0 ") {
        eprintln!("  ⚠ OpenClaw may not be correctly installed — check logs");
    }

    if opts.with_playwright {
        println!("  Installing Playwright + Chromium...");
        run_logged(&sess, "playwright install-deps", "sudo npx playwright install-deps chromium", &mut log)?;
        run_logged(&sess, "playwright install", "npx playwright install chromium", &mut log)?;
    }

    // [6] Desktop files + gateway autostart
    println!("[6/7] Setting up desktop and autostart...");
    run(&sess, &format!("mkdir -p /home/{DEFAULT_USER}/.openclaw"), &mut log)?;
    run(&sess, &format!("mkdir -p /home/{DEFAULT_USER}/.config/autostart"), &mut log)?;
    run(&sess, &format!("mkdir -p /home/{DEFAULT_USER}/Desktop"), &mut log)?;

    let script_path = format!("/home/{DEFAULT_USER}/.config/autostart/openclaw-start.sh");
    write_text_file(&sess, &[
        "#!/bin/bash",
        r#"LOG="$HOME/.openclaw/gateway.log""#,
        r#"mkdir -p "$HOME/.openclaw""#,
        r#"echo "[$(date)] Starting openclaw gateway..." >> "$LOG""#,
        r#"exec openclaw gateway --port 18789 --allow-unconfigured >> "$LOG" 2>&1"#,
    ], &script_path, &mut log)?;
    run(&sess, &format!("chmod +x {script_path}"), &mut log)?;

    let autostart_path = format!("/home/{DEFAULT_USER}/.config/autostart/openclaw-gateway.desktop");
    write_text_file(&sess, &[
        "[Desktop Entry]",
        "Type=Application",
        "Name=OpenClaw Gateway",
        &format!("Exec={script_path}"),
        "Hidden=false",
        "NoDisplay=false",
        "X-GNOME-Autostart-enabled=true",
        "Comment=OpenClaw AI gateway",
    ], &autostart_path, &mut log)?;

    let shortcut_path = format!("/home/{DEFAULT_USER}/Desktop/OpenClaw.desktop");
    write_text_file(&sess, &[
        "[Desktop Entry]",
        "Version=1.0",
        "Type=Application",
        "Name=OpenClaw",
        "Comment=OpenClaw Web UI",
        "Exec=firefox-esr http://localhost:18789",
        "Icon=firefox-esr",
        "Terminal=false",
        "Categories=Network;",
    ], &shortcut_path, &mut log)?;
    run(&sess, &format!("chmod +x {shortcut_path}"), &mut log)?;
    run(&sess, &format!("gio set {shortcut_path} metadata::trusted true 2>/dev/null || true"), &mut log)?;

    run(&sess, "sudo rm -f /etc/systemd/system/openclaw-gateway.service 2>/dev/null || true", &mut log)?;

    // [7] SSH key + auto-login
    println!("[7/7] Setting up SSH key and auto-login...");
    ensure_key_pair()?;
    let pub_key = read_public_key()?;
    run(
        &sess,
        &format!(
            "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
             touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && \
             grep -qxF {quoted} ~/.ssh/authorized_keys || \
             printf '%s\\n' {quoted} >> ~/.ssh/authorized_keys",
            quoted = shell_quote(&pub_key)
        ),
        &mut log,
    )?;

    run(
        &sess,
        &format!(
            "sudo mkdir -p /etc/lightdm/lightdm.conf.d && \
             printf '[Seat:*]\\nautologin-user={DEFAULT_USER}\\nautologin-user-timeout=0\\n' \
             | sudo tee /etc/lightdm/lightdm.conf.d/50-autologin.conf > /dev/null"
        ),
        &mut log,
    )?;

    let _ = writeln!(log, "\nProvisioning completed at {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    let _ = log.flush();

    println!("\nSyncing filesystem...");
    run(&sess, "sync", &mut log)?;

    println!("Rebooting VM into XFCE desktop...");
    let _ = run(&sess, "sudo reboot", &mut log);

    println!("\n✓ Provisioning complete!");
    println!("  Host log: {}", log_path.display());
    println!("  VM log:   ssh admin@{ip} cat {VM_LOG}");
    println!("  Wait ~30s for the VM to reboot into XFCE.");
    println!("  OpenClaw gateway starts automatically at login.");
    Ok(())
}
