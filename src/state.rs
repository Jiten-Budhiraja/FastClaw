use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmState {
    pub vm_name: String,
    pub number: u32,
    pub provisioned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provisioned_at: Option<String>,
    pub with_playwright: bool,
    pub openclaw_config_host: String,
}

impl VmState {
    pub fn new(number: u32, openclaw_config_host: String) -> Self {
        VmState {
            vm_name: vm_name_for(number),
            number,
            provisioned: false,
            provisioned_at: None,
            with_playwright: false,
            openclaw_config_host,
        }
    }

    pub fn mark_provisioned(&mut self, with_playwright: bool) {
        self.provisioned = true;
        self.provisioned_at = Some(Utc::now().to_rfc3339());
        self.with_playwright = with_playwright;
    }
}

pub fn vm_name_for(number: u32) -> String {
    format!("fastclaw-{number}")
}

fn state_dir() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("Cannot determine home directory")?
        .join(".fastclaw")
        .join("state");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn state_path(vm_name: &str) -> Result<PathBuf> {
    Ok(state_dir()?.join(format!("{vm_name}.json")))
}

pub fn load_state(vm_name: &str) -> Result<Option<VmState>> {
    let path = state_path(vm_name)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .context(format!("Cannot read state for '{vm_name}'"))?;
    let state: VmState =
        serde_json::from_str(&text).context(format!("Cannot parse state for '{vm_name}'"))?;
    Ok(Some(state))
}

pub fn save_state(state: &VmState) -> Result<()> {
    let path = state_path(&state.vm_name)?;
    let text = serde_json::to_string_pretty(state)?;
    // Atomic write via temp file
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &text)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn delete_state(vm_name: &str) -> Result<()> {
    let path = state_path(vm_name)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn list_all_states() -> Result<Vec<VmState>> {
    let dir = state_dir()?;
    let mut states = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(state) = serde_json::from_str::<VmState>(&text) {
                    states.push(state);
                }
            }
        }
    }
    states.sort_by_key(|s| s.number);
    Ok(states)
}
