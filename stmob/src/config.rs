use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::family::Family;
use crate::options::{OptionField, OptionSet, OptionValue};

/// A flash-option configuration file.
///
/// The `[options]` table is parsed by the selected family, so each family
/// defines its own option names and value types.
///
/// ```toml
/// chip = "STM32G474CE"
///
/// [options]
/// rdp = 1
/// brownout = 4
/// bank-mode = "dual"
/// boot = "main-flash"
/// boot-lock = true
/// ```
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Target chip, e.g. `STM32G474CE`. Can be overridden with `--chip`.
    pub chip: Option<String>,
    /// Family-specific flash options.
    #[serde(default)]
    pub options: toml::Table,
}

impl Config {
    /// Reads and parses a configuration file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| Error::ConfigRead {
            path: path.display().to_string(),
            source,
        })?;

        toml::from_str(&text).map_err(|source| Error::ConfigParse {
            path: path.display().to_string(),
            source,
        })
    }

    /// Converts the configured options into a typed option set for `family`.
    pub fn to_option_set(&self, family: &dyn Family) -> Result<OptionSet> {
        family.parse_config(&self.options)
    }
}

/// Inserts a typed option if it is present, validating the name against
/// `family`.
pub(crate) fn insert_option<T: Into<OptionValue>>(
    set: &mut OptionSet,
    family: &dyn Family,
    name: &'static str,
    value: Option<T>,
) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };

    let field = field(family, name)?;
    set.insert(field.name, value.into());

    Ok(())
}

/// Inserts a numeric level, canonicalising it to the family's named variant so
/// that it compares equal to a value decoded from the device.
///
/// Level options are ordered from level 0 upwards, so the level number is the
/// index into the field's variants. This bounds-checks the value.
pub(crate) fn insert_level(
    set: &mut OptionSet,
    family: &dyn Family,
    name: &'static str,
    value: Option<u8>,
) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };

    let field = field(family, name)?;
    let variants = field.variants().ok_or_else(|| Error::InvalidValue {
        option: name.to_string(),
        value: value.to_string(),
        expected: "a level option".to_string(),
    })?;
    let variant = variants
        .get(value as usize)
        .ok_or_else(|| Error::InvalidValue {
            option: name.to_string(),
            value: value.to_string(),
            expected: format!("a level between 0 and {}", variants.len().saturating_sub(1)),
        })?;
    set.insert(field.name, OptionValue::Enum(variant.name));

    Ok(())
}

fn field(family: &dyn Family, name: &'static str) -> Result<&'static OptionField> {
    family.field(name).ok_or_else(|| Error::UnknownOption {
        name: name.to_string(),
        available: family.available_options(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::g4::{BankMode, Boot, G4};
    use crate::options::OptionValue;

    #[test]
    fn parses_config() {
        let config: Config = toml::from_str(
            r#"
            chip = "STM32G474CE"

            [options]
            rdp = 1
            bank-mode = "dual"
            boot-lock = true
            brownout = 4
            "#,
        )
        .unwrap();

        assert_eq!(config.chip.as_deref(), Some("STM32G474CE"));

        let options = config.to_option_set(&G4).unwrap();
        assert_eq!(options.get("rdp"), Some(&OptionValue::Enum("level1")));
        assert_eq!(options.get("bank-mode"), Some(&OptionValue::Enum("dual")));
        assert_eq!(options.get("boot-lock"), Some(&OptionValue::Bool(true)));
        assert_eq!(options.get("brownout"), Some(&OptionValue::Enum("level4")));
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let result: std::result::Result<Config, _> = toml::from_str(
            r#"
            chip = "STM32G474CE"
            typo = true
            "#,
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_option() {
        let config: Config = toml::from_str(
            r#"
            [options]
            not-an-option = true
            "#,
        )
        .unwrap();

        assert!(config.to_option_set(&G4).is_err());
    }

    #[test]
    fn rejects_non_integer_level() {
        let config: Config = toml::from_str(
            r#"
            [options]
            rdp = "level1"
            "#,
        )
        .unwrap();

        assert!(config.to_option_set(&G4).is_err());
    }

    #[test]
    fn rejects_out_of_range_rdp() {
        let config: Config = toml::from_str(
            r#"
            [options]
            rdp = 3
            "#,
        )
        .unwrap();

        assert!(config.to_option_set(&G4).is_err());
    }

    #[test]
    fn rejects_out_of_range_level() {
        let config: Config = toml::from_str(
            r#"
            [options]
            brownout = 9
            "#,
        )
        .unwrap();

        assert!(config.to_option_set(&G4).is_err());
    }

    #[test]
    fn level_variants_are_ordered() {
        fn check(family: &dyn Family, field: &str, names: &[&str]) {
            let field = family.field(field).unwrap();
            let variants = field.variants().unwrap();
            let actual: Vec<_> = variants.iter().map(|variant| variant.name).collect();

            assert_eq!(actual, names, "unexpected variant order for {field:?}");
        }

        check(&G4, "rdp", &["level0", "level1", "level2"]);
        check(
            &G4,
            "brownout",
            &["level0", "level1", "level2", "level3", "level4"],
        );
    }

    #[test]
    fn config_enum_names_match_family_variants() {
        fn check(family: &dyn Family, field: &str, names: &[&str]) {
            let field = family.field(field).unwrap();
            let variants = field.variants().unwrap();

            for name in names {
                assert!(
                    variants.iter().any(|variant| variant.name == *name),
                    "config variant {name:?} is missing from family field {field:?}"
                );
            }
        }

        check(
            &G4,
            "bank-mode",
            &[BankMode::Single.name(), BankMode::Dual.name()],
        );
        check(
            &G4,
            "boot",
            &[
                Boot::MainFlash.name(),
                Boot::Pin.name(),
                Boot::SystemMemory.name(),
                Boot::Sram.name(),
            ],
        );
    }
}
