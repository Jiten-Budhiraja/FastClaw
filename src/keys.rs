use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Returns the path to the fastclaw SSH private key.
pub fn private_key_path() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("Cannot determine home directory")?
        .join(".fastclaw")
        .join("keys");
    Ok(dir.join("fastclaw_ed25519"))
}

pub fn public_key_path() -> Result<PathBuf> {
    Ok(private_key_path()?.with_extension("pub"))
}

/// Generate an ED25519 key pair if not already present.
pub fn ensure_key_pair() -> Result<()> {
    let priv_key = private_key_path()?;
    if priv_key.exists() && priv_key.with_extension("pub").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(priv_key.parent().unwrap())?;
    println!("Generating SSH key pair for fastclaw...");
    let status = Command::new("ssh-keygen")
        .args([
            "-q", "-t", "ed25519", "-N", "",
            "-f", priv_key.to_str().unwrap(),
        ])
        .status()
        .context("Failed to run ssh-keygen")?;
    if !status.success() {
        bail!("ssh-keygen failed");
    }
    Ok(())
}

/// Read the public key content (trimmed).
pub fn read_public_key() -> Result<String> {
    let pub_path = public_key_path()?;
    Ok(std::fs::read_to_string(&pub_path)
        .context(format!("Cannot read public key: {}", pub_path.display()))?
        .trim()
        .to_string())
}
