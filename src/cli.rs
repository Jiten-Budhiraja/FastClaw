use clap::{Parser, Subcommand};
use dirs::home_dir;

fn default_openclaw_config() -> String {
    home_dir()
        .unwrap_or_default()
        .join(".openclaw")
        .to_string_lossy()
        .to_string()
}

/// Fastclaw — Fast Linux VM manager for OpenClaw on macOS Apple Silicon.
#[derive(Parser, Debug)]
#[command(name = "fastclaw", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create, launch, and provision a VM (all-in-one)
    Up {
        /// VM number (default: auto-select lowest available)
        #[arg(short, long, default_value = "1")]
        number: u32,

        /// RAM in GB
        #[arg(long, default_value = "4")]
        memory_gb: u32,

        /// Number of vCPUs
        #[arg(long, default_value = "2")]
        cpus: u32,

        /// Install Playwright + Chromium (adds ~300 MB, enables browser automation)
        #[arg(long)]
        with_playwright: bool,

        /// Run the VM without a graphical window
        #[arg(long)]
        headless: bool,

        /// Host path to mount as ~/.openclaw inside the VM
        #[arg(long, default_value_t = default_openclaw_config())]
        openclaw_config: String,
    },

    /// Stop a running VM
    Down {
        /// VM number
        number: u32,
    },

    /// Delete a VM and its local state
    Delete {
        /// VM number
        number: u32,
    },

    /// Open an SSH shell into a running VM
    Shell {
        /// VM number
        number: u32,
    },

    /// Print the IP address of a VM
    Ip {
        /// VM number
        number: u32,
    },

    /// Show status of one VM or all fastclaw VMs
    Status {
        /// VM number (omit to show all)
        number: Option<u32>,
    },

    /// Manage the base Linux image
    Image {
        #[command(subcommand)]
        cmd: ImageCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum ImageCommands {
    /// Pull the base Linux image (one-time, ~500 MB)
    Pull,
}
