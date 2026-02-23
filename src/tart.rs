use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VmInfo {
    pub name: String,
    pub running: Option<bool>,
}

pub struct TartClient;

impl TartClient {
    pub fn new() -> Result<Self> {
        which::which("tart")
            .context("tart not found. Install with:\n  brew install cirruslabs/cli/tart")?;
        Ok(TartClient)
    }

    pub fn list_vms(&self) -> Result<Vec<VmInfo>> {
        let out = Command::new("tart")
            .args(["list", "--format", "json"])
            .output()
            .context("Failed to run tart list")?;
        if !out.status.success() {
            bail!("tart list failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        let vms: Vec<VmInfo> =
            serde_json::from_slice(&out.stdout).context("Failed to parse tart list output")?;
        Ok(vms)
    }

    pub fn vm_exists(&self, name: &str) -> Result<bool> {
        Ok(self.list_vms()?.iter().any(|v| v.name == name))
    }

    pub fn vm_running(&self, name: &str) -> Result<bool> {
        Ok(self
            .list_vms()?
            .iter()
            .any(|v| v.name == name && v.running == Some(true)))
    }

    pub fn clone_vm(&self, base: &str, name: &str) -> Result<()> {
        let status = Command::new("tart")
            .args(["clone", base, name])
            .status()
            .context("Failed to run tart clone")?;
        if !status.success() {
            bail!("tart clone failed for '{name}'");
        }
        Ok(())
    }

    pub fn pull(&self, image: &str) -> Result<()> {
        let status = Command::new("tart")
            .args(["pull", image])
            .status()
            .context("Failed to run tart pull")?;
        if !status.success() {
            bail!("tart pull failed for '{image}'");
        }
        Ok(())
    }

    pub fn stop(&self, name: &str) {
        let _ = Command::new("tart").args(["stop", name]).output();
    }

    pub fn delete(&self, name: &str) {
        let _ = Command::new("tart").args(["delete", name]).output();
    }

    pub fn ip(&self, name: &str) -> Result<Option<String>> {
        for extra in [Some("--resolver=agent"), None] {
            let mut args = vec!["ip"];
            if let Some(e) = extra {
                args.push(e);
            }
            args.push(name);
            if let Ok(out) = Command::new("tart").args(&args).output() {
                if out.status.success() {
                    let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !ip.is_empty() {
                        return Ok(Some(ip));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Launch VM in the background, streaming Tart log to `log_path`.
    /// Returns the spawned Child so the caller can track it.
    pub fn run_background(
        &self,
        name: &str,
        extra_args: &[&str],
        log_path: &Path,
    ) -> Result<std::process::Child> {
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log_file = std::fs::File::create(log_path)
            .context(format!("Cannot create log file: {}", log_path.display()))?;
        let log_copy = log_file.try_clone()?;
        let child = Command::new("tart")
            .arg("run")
            .arg(name)
            .args(extra_args)
            .stdin(Stdio::null())
            .stdout(log_file)
            .stderr(log_copy)
            .spawn()
            .context(format!("Failed to launch VM '{name}'"))?;
        Ok(child)
    }

    pub fn wait_for_ip(&self, name: &str, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        loop {
            if let Ok(Some(ip)) = self.ip(name) {
                return Ok(ip);
            }
            if start.elapsed() >= timeout {
                bail!("Timeout waiting for IP address of VM '{name}'");
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}
