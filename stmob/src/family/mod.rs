/// STM32G4 family implementation.
pub mod g4;

use probe_rs::Core;

use crate::error::{Error, Result};
use crate::options::{OptionField, OptionSet, RegisterMap};

/// A family of STM32 devices that share a flash option-byte layout.
///
/// Families are declarative: they provide a list of [`OptionField`]s and the
/// register read/write primitives. The default [`Family::decode`] and
/// [`Family::encode`] implementations use those fields to convert between raw
/// registers and named options, so adding a new family usually only requires a
/// new module with field descriptors and a controller layout.
pub trait Family: Send + Sync {
    /// Human-readable family name, e.g. `STM32G4`.
    fn name(&self) -> &'static str;

    /// Returns true if this family handles the given chip name.
    fn matches_chip(&self, chip: &str) -> bool;

    /// All options supported by this family.
    fn fields(&self) -> &'static [OptionField];

    /// Reads the option-byte registers from the device.
    fn read_registers(&self, core: &mut Core<'_>) -> Result<RegisterMap>;

    /// Programs the option-byte registers in `after`.
    fn write_registers(&self, core: &mut Core<'_>, after: &RegisterMap) -> Result<()>;

    /// Validates a desired configuration before any device access.
    fn validate(&self, desired: &OptionSet) -> Result<()> {
        self.validate_names(desired)
    }

    /// Checks that every option in `desired` is known to this family.
    fn validate_names(&self, desired: &OptionSet) -> Result<()> {
        for (name, _) in desired.iter() {
            if self.field(name).is_none() {
                return Err(Error::UnknownOption {
                    name: name.to_string(),
                    available: self.available_options(),
                });
            }
        }

        Ok(())
    }

    /// Checks device-specific preconditions before programming.
    ///
    /// `registers` contains the current raw register values, `current` the
    /// decoded current options, and `desired` the options the caller wants.
    fn check_preconditions(
        &self,
        _registers: &RegisterMap,
        _current: &OptionSet,
        _desired: &OptionSet,
    ) -> Result<()> {
        Ok(())
    }

    /// Looks up an option field by name.
    fn field(&self, name: &str) -> Option<&'static OptionField> {
        self.fields().iter().find(|field| field.name == name)
    }

    /// Decodes all fields from a register map.
    fn decode(&self, registers: &RegisterMap) -> Result<OptionSet> {
        let mut options = OptionSet::new();

        for field in self.fields() {
            let register = registers
                .get(field.register)
                .copied()
                .ok_or(Error::MissingRegister(field.register))?;
            options.insert(field.name, field.decode(register)?);
        }

        Ok(options)
    }

    /// Encodes the given options into a register map, preserving unrelated bits.
    fn encode(&self, registers: &mut RegisterMap, desired: &OptionSet) -> Result<()> {
        for (name, value) in desired.iter() {
            let field = self.field(name).ok_or_else(|| Error::UnknownOption {
                name: name.to_string(),
                available: self.available_options(),
            })?;
            let register = registers
                .get_mut(field.register)
                .ok_or(Error::MissingRegister(field.register))?;
            field.encode(register, value)?;
        }

        Ok(())
    }

    /// Returns a comma-separated list of the supported option names.
    fn available_options(&self) -> String {
        let mut names: Vec<_> = self.fields().iter().map(|field| field.name).collect();
        names.sort_unstable();
        names.join(", ")
    }
}

static FAMILIES: &[&dyn Family] = &[&g4::G4];

/// Returns every registered family.
pub fn all() -> &'static [&'static dyn Family] {
    FAMILIES
}

/// Finds the family that handles a chip name.
pub fn find(chip: &str) -> Option<&'static dyn Family> {
    FAMILIES
        .iter()
        .copied()
        .find(|family| family.matches_chip(chip))
}

/// Finds a family by its full name (`STM32G4`) or short suffix (`G4`).
pub fn find_by_name(name: &str) -> Option<&'static dyn Family> {
    let name = name.trim().to_ascii_lowercase();

    if name.is_empty() {
        return None;
    }

    FAMILIES.iter().copied().find(|family| {
        let family_name = family.name().to_ascii_lowercase();
        family_name == name
            || family_name
                .strip_prefix("stm32")
                .is_some_and(|suffix| suffix == name)
    })
}
