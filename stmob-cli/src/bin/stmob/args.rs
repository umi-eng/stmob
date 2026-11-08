use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use stmob::{ProbeOptions, ProbeSelector, Protocol};

/// Options shared by every command that talks to a probe.
#[derive(Args, Debug)]
pub struct ProbeArgs {
    /// Target chip, e.g. STM32G474CE. If omitted, probe-rs auto-detects it.
    #[arg(short = 'c', long, env = "STMOB_CHIP")]
    pub chip: Option<String>,

    /// Probe to use: zero-based index or `VID:PID[:SERIAL]`.
    #[arg(short = 'p', long, env = "STMOB_PROBE")]
    pub probe: Option<String>,

    /// Debug probe speed in kHz.
    #[arg(long, env = "STMOB_SPEED")]
    pub speed: Option<u32>,

    /// Wire protocol to use.
    #[arg(long, value_enum, env = "STMOB_PROTOCOL")]
    pub protocol: Option<ProtocolArg>,

    /// Attach to the target under reset.
    #[arg(long)]
    pub connect_under_reset: bool,

    /// Allow probe-rs to erase all memory.
    #[arg(long)]
    pub allow_erase_all: bool,
}

impl ProbeArgs {
    /// Converts the CLI arguments into library probe options.
    pub fn to_options(&self) -> Result<ProbeOptions> {
        let selector = match &self.probe {
            Some(selector) => Some(
                ProbeSelector::from_str(selector)
                    .with_context(|| format!("invalid probe selector {selector:?}"))?,
            ),
            None => None,
        };

        Ok(ProbeOptions {
            selector,
            speed_khz: self.speed,
            protocol: self.protocol.map(Into::into),
            connect_under_reset: self.connect_under_reset,
            allow_erase_all: self.allow_erase_all,
        })
    }
}

/// Wire protocol accepted on the command line.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ProtocolArg {
    /// Serial Wire Debug.
    Swd,
    /// JTAG.
    Jtag,
}

impl From<ProtocolArg> for Protocol {
    fn from(protocol: ProtocolArg) -> Self {
        match protocol {
            ProtocolArg::Swd => Protocol::Swd,
            ProtocolArg::Jtag => Protocol::Jtag,
        }
    }
}

/// Options shared by commands that modify option bytes.
#[derive(Args, Debug)]
pub struct WriteArgs {
    /// Only report what would change; don't write.
    #[arg(long)]
    pub dry_run: bool,

    /// Exit with an error if the device does not match the requested configuration.
    #[arg(long)]
    pub check: bool,

    /// Allow RDP level 1 -> level 0 regression (full chip erase).
    #[arg(long)]
    pub allow_rdp_regression: bool,

    /// Don't read the option registers back after programming.
    #[arg(long)]
    pub no_verify: bool,
}

impl WriteArgs {
    /// Converts the CLI arguments into library write options.
    pub fn to_options(&self) -> stmob::WriteOptions {
        stmob::WriteOptions {
            dry_run: self.dry_run || self.check,
            allow_rdp_regression: self.allow_rdp_regression,
            verify: !self.no_verify,
        }
    }
}
