mod cli;
mod errors;
mod image;
mod keys;
mod port_forward;
mod provision;
mod state;
mod tart;
mod vm;

use clap::Parser;
use cli::{Cli, Commands, ImageCommands};
use tart::TartClient;

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> errors::Result<()> {
    let tart = TartClient::new()?;

    match cli.command {
        Commands::Up {
            number,
            memory_gb,
            cpus,
            with_playwright,
            headless,
            openclaw_config,
        } => {
            // Ensure ~/.openclaw exists on the host before mounting
            std::fs::create_dir_all(&openclaw_config)?;

            let opts = vm::UpOptions {
                vm_number: number,
                with_playwright,
                headless,
                memory_gb,
                cpus,
                openclaw_config_host: openclaw_config,
            };
            vm::up(&opts, &tart)?;
        }

        Commands::Down { number } => {
            vm::down_vm(number, &tart)?;
        }

        Commands::Delete { number } => {
            vm::delete_vm(number, &tart)?;
        }

        Commands::Shell { number } => {
            vm::shell_vm(number, &tart)?;
        }

        Commands::Ip { number } => {
            vm::ip_vm(number, &tart)?;
        }

        Commands::Status { number } => match number {
            Some(n) => vm::status_vm(n, &tart)?,
            None => vm::status_all(&tart)?,
        },

        Commands::Image { cmd } => match cmd {
            ImageCommands::Pull => {
                image::ensure_base_image(&tart)?;
                println!("Base image ready: {}", image::BASE_IMAGE);
            }
        },
    }

    Ok(())
}
