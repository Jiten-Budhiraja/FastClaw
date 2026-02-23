use anyhow::Result;

use crate::tart::TartClient;

/// OCI image reference used as base for all fastclaw VMs.
/// Cirrus Labs provides Debian 13 Trixie arm64 images optimised for Tart.
pub const BASE_IMAGE: &str = "ghcr.io/cirruslabs/debian:trixie";

/// Default SSH credentials for the Cirrus Labs Debian image.
pub const DEFAULT_USER: &str = "admin";
pub const DEFAULT_PASSWORD: &str = "admin";

/// Ensure the base Debian 13 image is available locally in Tart's image store.
/// If not present, pulls it from the OCI registry (one-time, ~400 MB).
pub fn ensure_base_image(tart: &TartClient) -> Result<()> {
    if tart.vm_exists(BASE_IMAGE)? {
        return Ok(());
    }
    println!("Base image not found. Pulling '{BASE_IMAGE}'...");
    println!("This is a one-time download (~400 MB) and may take a few minutes.");
    tart.pull(BASE_IMAGE)?;
    println!("Base image ready.");
    Ok(())
}
