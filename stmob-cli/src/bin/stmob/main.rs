mod args;
mod commands;
mod output;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use args::{ProbeArgs, WriteArgs};

/// Read and modify STM32 flash option bytes over probe-rs.
#[derive(Parser, Debug)]
#[command(name = "stmob", version, about, long_about = None)]
struct Cli {
    #[command(flatten)]
    probe: ProbeArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List the flash options supported by a family.
    List {
        /// Family to list (e.g. STM32G4 or G4). Defaults to the --chip family,
        /// or all registered families when neither is given.
        #[arg(long)]
        family: Option<String>,
    },

    /// Read the current flash options from the device.
    Read,

    /// Read a single flash option.
    Get {
        /// Option name (see `list`).
        option: String,
    },

    /// Set a single flash option.
    Set {
        /// Option name (see `list`).
        option: String,

        /// New value (see `list` for accepted values).
        value: String,

        #[command(flatten)]
        write: WriteArgs,
    },

    /// Apply all flash options from a TOML file.
    Apply {
        /// Path to the TOML configuration file.
        file: PathBuf,

        #[command(flatten)]
        write: WriteArgs,
    },

    /// List the connected debug probes.
    Probes,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let probe_options = cli.probe.to_options()?;

    match cli.command {
        Command::List { family } => commands::list(&cli.probe, family.as_deref()),
        Command::Read => commands::read(&cli.probe, &probe_options),
        Command::Get { option } => commands::get(&cli.probe, &probe_options, &option),
        Command::Set {
            option,
            value,
            write,
        } => commands::set(&cli.probe, &probe_options, &option, &value, write),
        Command::Apply { file, write } => commands::apply(&cli.probe, &probe_options, &file, write),
        Command::Probes => commands::probes(),
    }
}
