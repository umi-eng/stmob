use std::fmt::Write as _;
use std::time::Duration;

use probe_rs::Session;

use crate::error::{Error, Result};
use crate::family::Family;
use crate::options::{OptionSet, OptionValue, RegisterMap};
use crate::probe::ProbeOptions;

/// Options controlling how flash options are written.
#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    /// Only report what would change; don't write.
    pub dry_run: bool,
    /// Allow RDP level 1 -> level 0 regression (full chip erase).
    pub allow_rdp_regression: bool,
    /// Read the option registers back after programming and compare them with
    /// the values that were written.
    pub verify: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            allow_rdp_regression: false,
            verify: true,
        }
    }
}

/// The result of a read or write operation.
#[derive(Clone, Debug)]
pub struct WriteReport {
    /// Chip name reported by probe-rs.
    pub chip: String,
    /// Family name.
    pub family: &'static str,
    /// Options before programming (after RDP regression, if one was needed).
    pub before: OptionSet,
    /// Options requested by the caller.
    pub desired: OptionSet,
    /// Option values after programming, or the predicted values for a dry run.
    pub after: OptionSet,
    /// Raw registers before programming.
    pub raw_before: RegisterMap,
    /// Raw registers after programming, or the values that would be written.
    pub raw_after: RegisterMap,
    /// Whether any register value changed.
    pub changed: bool,
    /// Whether the changes were actually written.
    pub written: bool,
    /// Whether the operation was a dry run.
    pub dry_run: bool,
    /// Whether an RDP regression was performed (or would be performed).
    pub regressed_rdp: bool,
    /// Human-readable notes about the change.
    pub notes: Vec<String>,
}

/// A connected target device.
pub struct Device {
    session: Session,
    family: &'static dyn Family,
    chip: String,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("chip", &self.chip)
            .field("family", &self.family.name())
            .finish_non_exhaustive()
    }
}

impl Device {
    /// Connects to a target and resolves its option-byte family.
    ///
    /// If `chip` is `None`, probe-rs attempts to detect the target
    /// automatically.
    pub fn connect(chip: Option<&str>, options: &ProbeOptions) -> Result<Self> {
        let session = options.attach(chip)?;
        let chip = session.target().name.clone();
        let family =
            crate::family::find(&chip).ok_or_else(|| Error::UnsupportedChip(chip.clone()))?;

        Ok(Self {
            session,
            family,
            chip,
        })
    }

    /// The resolved chip name.
    pub fn chip(&self) -> &str {
        &self.chip
    }

    /// The chip's option-byte family.
    pub fn family(&self) -> &dyn Family {
        self.family
    }

    /// Reads and decodes the current option bytes.
    pub fn read(&mut self) -> Result<OptionSet> {
        let raw = self.read_raw()?;
        self.family.decode(&raw)
    }

    /// Reads the raw option-byte registers.
    pub fn read_raw(&mut self) -> Result<RegisterMap> {
        let mut core = self.session.core(0)?;
        core.halt(Duration::from_millis(1000))?;
        self.family.read_registers(&mut core)
    }

    /// Applies `desired` to the device.
    ///
    /// Only the options present in `desired` are changed; all other option
    /// bits are preserved. If the device is at RDP level 1 and a change is
    /// requested, `options.allow_rdp_regression` must be set, because the only
    /// way to modify the option bytes is to regress to level 0, which performs
    /// a full chip erase.
    pub fn write(&mut self, desired: &OptionSet, options: WriteOptions) -> Result<WriteReport> {
        self.family.validate(desired)?;

        let mut core = self.session.core(0)?;
        core.halt(Duration::from_millis(1000))?;

        let mut raw_before = self.family.read_registers(&mut core)?;
        let mut current = self.family.decode(&raw_before)?;

        if rdp_level(&current) == Some("level2") {
            return Err(Error::RdpLevel2);
        }

        // Validate the requested values before deciding whether anything needs
        // to change. This catches invalid values even when the current value
        // happens to compare equal.
        let mut scratch = raw_before.clone();
        self.family.encode(&mut scratch, desired)?;

        // Compare decoded values rather than raw registers. Some fields have
        // multiple raw encodings for the same logical value (for example RDP
        // level 1 is any value other than 0xAA and 0xCC), and rewriting those
        // would trigger an unnecessary RDP regression.
        let changed = options_changed(&current, desired);

        if !changed {
            let notes = notes(&current, desired, false, options.dry_run, false);

            return Ok(WriteReport {
                chip: self.chip.clone(),
                family: self.family.name(),
                before: current.clone(),
                desired: desired.clone(),
                after: current,
                raw_before: raw_before.clone(),
                raw_after: raw_before,
                changed: false,
                written: false,
                dry_run: options.dry_run,
                regressed_rdp: false,
                notes,
            });
        }

        let needs_regression = rdp_level(&current) == Some("level1");

        if needs_regression && !options.allow_rdp_regression {
            return Err(Error::RdpLevel1);
        }

        if needs_regression && options.dry_run {
            self.family
                .check_preconditions(&raw_before, &current, desired)?;

            // Predict the state after regression and the requested changes.
            let mut predicted = raw_before.clone();
            let mut rdp0 = OptionSet::new();
            rdp0.insert("rdp", OptionValue::Enum("level0"));
            self.family.encode(&mut predicted, &rdp0)?;
            self.family.encode(&mut predicted, desired)?;

            let after = self.family.decode(&predicted)?;
            let notes = notes(&current, desired, false, true, true);

            return Ok(WriteReport {
                chip: self.chip.clone(),
                family: self.family.name(),
                before: current,
                desired: desired.clone(),
                after,
                raw_before,
                raw_after: predicted,
                changed: true,
                written: false,
                dry_run: true,
                regressed_rdp: true,
                notes,
            });
        }

        let mut regressed_rdp = false;

        if needs_regression {
            self.family
                .check_preconditions(&raw_before, &current, desired)?;

            // Level 1 only allows the RDP field to be reprogrammed, and doing
            // so triggers a full chip erase. Regress first, then apply the rest
            // of the requested options once Level 0 is active.
            let mut regression = raw_before.clone();
            let mut rdp0 = OptionSet::new();
            rdp0.insert("rdp", OptionValue::Enum("level0"));
            self.family.encode(&mut regression, &rdp0)?;
            self.family.write_registers(&mut core, &regression)?;

            core.reset_and_halt(Duration::from_millis(1000))?;

            raw_before = self.family.read_registers(&mut core)?;
            current = self.family.decode(&raw_before)?;
            regressed_rdp = true;

            if rdp_level(&current) != Some("level0") {
                return Err(Error::RdpRegressionFailed);
            }
        }

        self.family
            .check_preconditions(&raw_before, &current, desired)?;

        // After regression, the requested options may already match the new
        // current state (for example when only RDP level 0 was requested).
        let changed = options_changed(&current, desired);

        if !changed {
            let notes = notes(&current, desired, false, options.dry_run, regressed_rdp);

            return Ok(WriteReport {
                chip: self.chip.clone(),
                family: self.family.name(),
                before: current.clone(),
                desired: desired.clone(),
                after: current,
                raw_before: raw_before.clone(),
                raw_after: raw_before,
                changed: false,
                written: false,
                dry_run: options.dry_run,
                regressed_rdp,
                notes,
            });
        }

        let mut raw_after = raw_before.clone();
        self.family.encode(&mut raw_after, desired)?;
        let after = self.family.decode(&raw_after)?;

        if options.dry_run {
            let notes = notes(&current, desired, false, true, regressed_rdp);

            return Ok(WriteReport {
                chip: self.chip.clone(),
                family: self.family.name(),
                before: current,
                desired: desired.clone(),
                after,
                raw_before,
                raw_after,
                changed: true,
                written: false,
                dry_run: true,
                regressed_rdp,
                notes,
            });
        }

        self.family.write_registers(&mut core, &raw_after)?;

        let raw_after = if options.verify {
            let raw_verified = self.family.read_registers(&mut core)?;
            if raw_verified != raw_after {
                return Err(Error::VerificationFailed {
                    expected: format_registers(&raw_after),
                    actual: format_registers(&raw_verified),
                });
            }
            raw_verified
        } else {
            raw_after
        };

        let after = self.family.decode(&raw_after)?;
        let notes = notes(&current, desired, true, false, regressed_rdp);

        Ok(WriteReport {
            chip: self.chip.clone(),
            family: self.family.name(),
            before: current,
            desired: desired.clone(),
            after,
            raw_before,
            raw_after,
            changed: true,
            written: true,
            dry_run: false,
            regressed_rdp,
            notes,
        })
    }
}

fn rdp_level(options: &OptionSet) -> Option<&'static str> {
    match options.get("rdp") {
        Some(OptionValue::Enum(name)) => Some(name),
        _ => None,
    }
}

fn format_registers(registers: &RegisterMap) -> String {
    let mut out = String::new();

    for (name, value) in registers {
        let _ = write!(out, "{name}={value:#010x} ");
    }

    out.trim_end().to_string()
}

fn notes(
    before: &OptionSet,
    desired: &OptionSet,
    written: bool,
    dry_run: bool,
    regressed_rdp: bool,
) -> Vec<String> {
    let mut notes = Vec::new();

    if regressed_rdp {
        if dry_run {
            notes.push(
                "RDP regression would be performed: the main flash would be erased and the device \
                 would be at Level 0."
                    .to_string(),
            );
        } else {
            notes.push(
                "RDP regression complete: the main flash has been erased and the device is now at \
                 Level 0."
                    .to_string(),
            );
            notes.push("Reflash the firmware after applying the new option bytes.".to_string());
        }
    }

    if changed(before, desired, "bank-mode") {
        notes.push(
            "Changing the bank mode changes the flash page size and memory organization. Existing \
             firmware may be unreadable; perform a mass erase and reflash before running it."
                .to_string(),
        );
    }

    if changed(before, desired, "boot") {
        notes.push("The boot source has changed.".to_string());
    }

    if changed(before, desired, "boot-lock") {
        notes.push("The boot lock setting has changed.".to_string());
    }

    if changed(before, desired, "rdp") {
        match desired.get("rdp") {
            Some(OptionValue::Enum("level1")) => notes.push(
                "Readout protection is now Level 1; debug access is disabled after the next \
                 reset. Reverting to Level 0 requires a full chip erase."
                    .to_string(),
            ),
            Some(OptionValue::Enum("level0")) => notes.push(
                "Readout protection is now Level 0; debug access is enabled after the next reset."
                    .to_string(),
            ),
            _ => {}
        }
    }

    if written {
        notes.push("Power cycle the device for the new option bytes to take effect.".to_string());
    }

    notes
}

fn changed(before: &OptionSet, desired: &OptionSet, name: &str) -> bool {
    match (before.get(name), desired.get(name)) {
        (Some(before), Some(desired)) => before != desired,
        _ => false,
    }
}

fn options_changed(current: &OptionSet, desired: &OptionSet) -> bool {
    desired
        .iter()
        .any(|(name, value)| current.get(name) != Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_compares_only_requested_options() {
        let mut current = OptionSet::new();
        current.insert("rdp", OptionValue::Enum("level1"));
        current.insert("boot", OptionValue::Enum("pin"));

        let mut desired = OptionSet::new();
        desired.insert("rdp", OptionValue::Enum("level1"));
        assert!(!options_changed(&current, &desired));

        let mut desired = OptionSet::new();
        desired.insert("rdp", OptionValue::Enum("level0"));
        assert!(options_changed(&current, &desired));

        let mut desired = OptionSet::new();
        desired.insert("boot", OptionValue::Enum("main-flash"));
        assert!(options_changed(&current, &desired));
    }
}
