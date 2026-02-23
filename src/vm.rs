use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::image::{ensure_base_image, BASE_IMAGE};
use crate::provision::{provision_vm, ProvisionOptions};
use crate::state::{delete_state, load_state, save_state, vm_name_for, VmState};
use crate::tart::TartClient;

pub struct UpOptions {
    pub vm_number: u32,
    pub with_playwright: bool,
    pub headless: bool,
    #[allow(dead_code)]
    pub memory_gb: u32,
    #[allow(dead_code)]
    pub cpus: u32,
    pub openclaw_config_host: String,
}

fn log_path(vm_name: &str) -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".fastclaw")
        .join("logs")
        .join(format!("{vm_name}.log"))
}



pub fn up(opts: &UpOptions, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(opts.vm_number);

    ensure_base_image(tart)?;

    // ── Create VM if needed ──────────────────────────────────────────────────
    if !tart.vm_exists(&vm_name)? {
        println!("Creating VM '{vm_name}' from base image...");
        tart.clone_vm(BASE_IMAGE, &vm_name)
            .context(format!("Failed to create VM '{vm_name}'"))?;
        println!("VM '{vm_name}' created.");
    } else {
        println!("VM '{vm_name}' already exists.");
    }

    // ── Load or create state ─────────────────────────────────────────────────
    let mut state = load_state(&vm_name)?.unwrap_or_else(|| {
        VmState::new(opts.vm_number, opts.openclaw_config_host.clone())
    });

    // ── Launch VM ────────────────────────────────────────────────────────────
    if tart.vm_running(&vm_name)? {
        println!("VM '{vm_name}' is already running.");
    } else {
        println!("Launching VM '{vm_name}'...");
        let log = log_path(&vm_name);
        let mut run_args: Vec<&str> = Vec::new();
        if opts.headless {
            run_args.push("--no-graphics");
        }
        // Note: VirtioFS (--dir) is not supported for Linux guests in Tart.
        // ~/.openclaw is accessible via SSH/SFTP inside the VM.
        tart.run_background(&vm_name, &run_args, &log)?;
        println!("VM launched (log: {})", log.display());
    }

    // ── Wait for IP ───────────────────────────────────────────────────────────
    print!("Waiting for VM IP address");
    let ip = tart.wait_for_ip(&vm_name, Duration::from_secs(120))?;
    println!("\nVM IP: {ip}");

    // ── Provision if first run ────────────────────────────────────────────────
    if !state.provisioned {
        println!("\nProvisioning VM (first run — this takes ~5 minutes)...");
        let prov_opts = ProvisionOptions {
            with_playwright: opts.with_playwright,
        };
        provision_vm(&ip, &prov_opts)?;
        state.mark_provisioned(opts.with_playwright);
        save_state(&state)?;
        println!();
    } else {
        println!("VM already provisioned — skipping.");
    }

    println!();
    println!("┌──────────────────────────────────────────────────────────┐");
    println!("│  Debian 13 VM ready!                                     │");
    println!("│                                                           │");
    println!("│  XFCE desktop open in the Tart window.                   │");
    println!("│  OpenClaw Web UI: http://localhost:18789  (inside VM)    │");
    println!("│                                                           │");
    println!("│  SSH:  fastclaw shell {}                                  │", opts.vm_number);
    println!("│  Stop: fastclaw down {}                                   │", opts.vm_number);
    println!("└──────────────────────────────────────────────────────────┘");

    Ok(())
}

pub fn down_vm(number: u32, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(number);
    if !tart.vm_exists(&vm_name)? {
        bail!("VM '{vm_name}' does not exist.");
    }
    println!("Stopping VM '{vm_name}'...");
    tart.stop(&vm_name);
    println!("VM '{vm_name}' stopped.");
    Ok(())
}

pub fn delete_vm(number: u32, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(number);
    // Always clean state file, even if VM doesn't exist in Tart
    // (prevents stale provisioned=true from skipping provisioning on next up)
    let _ = delete_state(&vm_name);
    if !tart.vm_exists(&vm_name)? {
        println!("VM '{vm_name}' not found in Tart (state cleared).");
        return Ok(());
    }
    println!("Stopping and deleting VM '{vm_name}'...");
    tart.stop(&vm_name);
    std::thread::sleep(Duration::from_secs(3));
    tart.delete(&vm_name);
    println!("VM '{vm_name}' deleted.");
    Ok(())
}

pub fn ip_vm(number: u32, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(number);
    match tart.ip(&vm_name)? {
        Some(ip) => println!("{ip}"),
        None => bail!("VM '{vm_name}' has no IP address (is it running?)"),
    }
    Ok(())
}

pub fn status_vm(number: u32, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(number);
    let running = tart.vm_running(&vm_name)?;
    let state = load_state(&vm_name)?;
    let ip = if running {
        tart.ip(&vm_name)?.unwrap_or_else(|| "pending".to_string())
    } else {
        "-".to_string()
    };
    println!("VM:          {vm_name}");
    println!("State:       {}", if running { "running" } else { "stopped" });
    println!("IP:          {ip}");
    if let Some(s) = state {
        println!("Provisioned: {}", if s.provisioned { "yes" } else { "no" });
        if s.provisioned {
            println!("  at:        {}", s.provisioned_at.as_deref().unwrap_or("?"));
            println!("  playwright:{}", if s.with_playwright { "yes" } else { "no" });
        }
        println!("Config host: {}", s.openclaw_config_host);
    } else {
        println!("Provisioned: no (not created via fastclaw up)");
    }
    Ok(())
}

pub fn status_all(tart: &TartClient) -> Result<()> {
    let vms = tart.list_vms()?;
    let fastclaw_vms: Vec<_> = vms.iter().filter(|v| v.name.starts_with("fastclaw-")).collect();

    if fastclaw_vms.is_empty() {
        println!("No fastclaw VMs found. Run 'fastclaw up' to create one.");
        return Ok(());
    }

    println!("{:<20} {:<10} {}", "NAME", "STATE", "IP");
    println!("{}", "-".repeat(50));
    for vm in &fastclaw_vms {
        let state = if vm.running == Some(true) { "running" } else { "stopped" };
        let ip = if vm.running == Some(true) {
            tart.ip(&vm.name)?.unwrap_or_else(|| "pending".to_string())
        } else {
            "-".to_string()
        };
        println!("{:<20} {:<10} {}", vm.name, state, ip);
    }
    Ok(())
}

pub fn shell_vm(number: u32, tart: &TartClient) -> Result<()> {
    let vm_name = vm_name_for(number);
    let ip = tart
        .ip(&vm_name)?
        .context(format!("VM '{vm_name}' has no IP (is it running?)"))?;

    let key = crate::keys::private_key_path()?;
    let mut args = vec![
        "-o".to_string(), "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(), "UserKnownHostsFile=/dev/null".to_string(),
    ];
    if key.exists() {
        args.push("-i".to_string());
        args.push(key.to_string_lossy().to_string());
    }
    args.push(format!("{}@{ip}", crate::image::DEFAULT_USER));

    std::process::Command::new("ssh")
        .args(&args)
        .status()
        .context("Failed to run ssh")?;
    Ok(())
}
