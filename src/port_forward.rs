use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};

use crate::keys::private_key_path;

pub struct PortForwardHandles {
    gateway: Child,
    bridge: Child,
}

impl PortForwardHandles {
    /// Kill both SSH tunnel processes.
    pub fn stop(mut self) {
        let _ = self.gateway.kill();
        let _ = self.bridge.kill();
    }
}

fn start_tunnel(ip: &str, local_port: u16, remote_port: u16) -> Result<Child> {
    let key = private_key_path()?;
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-N",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "ExitOnForwardFailure=yes",
        "-o", "ServerAliveInterval=10",
        "-L", &format!("{local_port}:127.0.0.1:{remote_port}"),
    ]);
    if key.exists() {
        cmd.arg("-i").arg(&key);
    }
    cmd.arg(format!("{}@{}", crate::image::DEFAULT_USER, ip));
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().context(format!(
        "Failed to start SSH tunnel {local_port}→{remote_port}"
    ))
}

/// Start port forwarding for the OpenClaw gateway and bridge ports.
/// Returns handles; dropping or calling `.stop()` kills the tunnels.
pub fn start_port_forwarding(ip: &str) -> Result<PortForwardHandles> {
    println!("Setting up port forwarding:");
    println!("  localhost:18789 → VM:18789  (OpenClaw Web UI)");
    println!("  localhost:18790 → VM:18790  (OpenClaw Bridge)");

    // Small delay to let SSH service settle if we just started the VM
    std::thread::sleep(std::time::Duration::from_secs(2));

    let gateway = start_tunnel(ip, 18789, 18789)?;
    let bridge = start_tunnel(ip, 18790, 18790)?;

    Ok(PortForwardHandles { gateway, bridge })
}
