use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::family::Family;
use crate::options::OptionSet;

/// A flash-option configuration file.
///
/// ```toml
/// chip = "STM32G474CE"
///
/// [options]
/// rdp = "level1"
/// bank-mode = "dual"
/// boot = "main-flash"
/// boot-lock = true
/// ```
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Target chip, e.g. `STM32G474CE`. Can be overridden with `--chip`.
    pub chip: Option<String>,
    /// Flash options to apply.
    #[serde(default)]
    pub options: std::collections::BTreeMap<String, ConfigValue>,
}

/// A raw TOML value for a flash option.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    /// A boolean value.
    Bool(bool),
    /// An integer value.
    Integer(i64),
    /// A string value.
    String(String),
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

    /// Converts the configured options into a typed option set.
    pub fn into_option_set(&self, family: &dyn Family) -> Result<OptionSet> {
        let mut set = OptionSet::new();

        for (name, value) in &self.options {
            let field = family.field(name).ok_or_else(|| Error::UnknownOption {
                name: name.clone(),
                available: family.available_options(),
            })?;
            set.insert(field.name, field.parse_config_value(value)?);
        }

        Ok(set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::g4::G4;
    use crate::options::OptionValue;

    #[test]
    fn parses_config() {
        let config: Config = toml::from_str(
            r#"
            chip = "STM32G474CE"

            [options]
            rdp = "level1"
            bank-mode = "dual"
            boot-lock = true
            brownout = 4
            "#,
        )
        .unwrap();

        assert_eq!(config.chip.as_deref(), Some("STM32G474CE"));

        let options = config.into_option_set(&G4).unwrap();
        assert_eq!(options.get("rdp"), Some(&OptionValue::Enum("level1")));
        assert_eq!(options.get("bank-mode"), Some(&OptionValue::Enum("dual")));
        assert_eq!(options.get("boot-lock"), Some(&OptionValue::Bool(true)));
        assert_eq!(options.get("brownout"), Some(&OptionValue::Enum("level4")));
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

        assert!(matches!(
            config.into_option_set(&G4),
            Err(Error::UnknownOption { .. })
        ));
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
}
