use std::fmt;
use std::str::FromStr;

use probe_rs::config::TargetSelector;
use probe_rs::probe::list::Lister;
use probe_rs::probe::{DebugProbeSelector, Probe, WireProtocol};
use probe_rs::{Permissions, Session};

use crate::error::{Error, Result};

/// Wire protocol used to talk to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// Serial Wire Debug.
    Swd,
    /// JTAG.
    Jtag,
}

impl From<Protocol> for WireProtocol {
    fn from(protocol: Protocol) -> Self {
        match protocol {
            Protocol::Swd => WireProtocol::Swd,
            Protocol::Jtag => WireProtocol::Jtag,
        }
    }
}

/// How to select a debug probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeSelector {
    /// Zero-based index into the list of connected probes.
    Index(usize),
    /// USB `VID:PID[:SERIAL]` selector.
    Usb(DebugProbeSelector),
}

impl FromStr for ProbeSelector {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        let input = input.trim();

        if let Ok(index) = input.parse::<usize>() {
            return Ok(Self::Index(index));
        }

        input
            .parse::<DebugProbeSelector>()
            .map(Self::Usb)
            .map_err(|_| Error::InvalidProbeSelector {
                selector: input.to_string(),
            })
    }
}

/// Options for opening a probe and attaching to a target.
#[derive(Clone, Debug, Default)]
pub struct ProbeOptions {
    /// Probe to open. If `None`, the first connected probe is used.
    pub selector: Option<ProbeSelector>,
    /// Debug probe speed in kHz.
    pub speed_khz: Option<u32>,
    /// Wire protocol to use.
    pub protocol: Option<Protocol>,
    /// Attach to the target under reset.
    pub connect_under_reset: bool,
    /// Allow probe-rs to erase all memory.
    pub allow_erase_all: bool,
}

impl ProbeOptions {
    /// Opens the selected probe.
    pub fn open(&self) -> Result<Probe> {
        let lister = Lister::new();

        match &self.selector {
            Some(ProbeSelector::Index(index)) => {
                let probes = lister.list_all();
                let info = probes.get(*index).ok_or(Error::ProbeIndexOutOfRange {
                    index: *index,
                    count: probes.len(),
                })?;
                Ok(info.open()?)
            }
            Some(ProbeSelector::Usb(selector)) => Ok(lister.open(selector.clone())?),
            None => {
                let probes = lister.list_all();
                let info = probes.first().ok_or(Error::NoProbesFound)?;
                Ok(info.open()?)
            }
        }
    }

    /// Opens the selected probe and attaches to the target.
    ///
    /// If `chip` is `None`, probe-rs attempts to auto-detect the target. The
    /// attach is retried a few times because some probes need a moment after
    /// being opened before they can talk to the target.
    pub fn attach(&self, chip: Option<&str>) -> Result<Session> {
        let mut last_err = None;

        for _ in 0..10 {
            match self.try_attach(chip) {
                Ok(session) => return Ok(session),
                Err(err) => {
                    last_err = Some(err);
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }

        Err(last_err.expect("at least one attach attempt"))
    }

    fn try_attach(&self, chip: Option<&str>) -> Result<Session> {
        let mut probe = self.open()?;

        if let Some(speed) = self.speed_khz {
            probe.set_speed(speed)?;
        }

        if let Some(protocol) = self.protocol {
            probe.select_protocol(protocol.into())?;
        }

        let permissions = if self.allow_erase_all {
            Permissions::new().allow_erase_all()
        } else {
            Permissions::new()
        };

        let target = match chip {
            Some(chip) => TargetSelector::Unspecified(chip.to_string()),
            None => TargetSelector::Auto,
        };

        let session = if self.connect_under_reset {
            probe.attach_under_reset(target, permissions)?
        } else {
            probe.attach(target, permissions)?
        };

        Ok(session)
    }
}

/// Information about a connected debug probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeInfo {
    /// Zero-based index of the probe.
    pub index: usize,
    /// Probe identifier, e.g. `ST-LINK V3`.
    pub identifier: String,
    /// USB vendor ID.
    pub vendor_id: u16,
    /// USB product ID.
    pub product_id: u16,
    /// USB serial number, if available.
    pub serial_number: Option<String>,
    /// USB interface index, if applicable.
    pub interface: Option<u8>,
}

impl fmt::Display for ProbeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} -- {:04x}:{:04x}",
            self.index, self.identifier, self.vendor_id, self.product_id
        )?;

        if let Some(interface) = self.interface {
            write!(f, "-{interface}")?;
        }

        if let Some(serial) = &self.serial_number {
            write!(f, ":{serial}")?;
        }

        Ok(())
    }
}

/// Lists the connected debug probes.
pub fn list_probes() -> Vec<ProbeInfo> {
    Lister::new()
        .list_all()
        .into_iter()
        .enumerate()
        .map(|(index, info)| ProbeInfo {
            index,
            identifier: info.identifier,
            vendor_id: info.vendor_id,
            product_id: info.product_id,
            serial_number: info.serial_number,
            interface: info.interface,
        })
        .collect()
}
