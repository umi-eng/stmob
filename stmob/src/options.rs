use std::collections::BTreeMap;
use std::fmt;

use crate::error::{Error, Result};

/// A named register in a family's option-byte register map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterDescriptor {
    /// Register name, e.g. `OPTR`.
    pub name: &'static str,
    /// Absolute register address.
    pub address: u64,
}

/// Raw register values keyed by register name.
pub type RegisterMap = BTreeMap<&'static str, u32>;

/// A single configurable flash option.
#[derive(Clone, Copy, Debug)]
pub struct OptionField {
    /// Stable option name used on the command line and in configuration files.
    pub name: &'static str,
    /// Human-readable description.
    pub description: &'static str,
    /// Name of the register that holds this option.
    pub register: &'static str,
    /// Encoding of the option within the register.
    pub kind: FieldKind,
}

/// How an option is encoded in its register.
#[derive(Clone, Copy, Debug)]
pub enum FieldKind {
    /// A single bit. `inverted` flips the logical value relative to the bit.
    Bool {
        /// Bit position within the register.
        bit: u8,
        /// Whether the logical value is inverted relative to the bit.
        inverted: bool,
    },
    /// A contiguous bit field with named values.
    Enum {
        /// Offset of the field within the register.
        offset: u8,
        /// Width of the field in bits.
        width: u8,
        /// Named values for the field.
        variants: &'static [EnumVariant],
        /// Maps a raw field value to a canonical variant value for display.
        normalize: Option<fn(u32) -> u32>,
    },
    /// A non-contiguous set of bits with named values.
    MaskedEnum {
        /// Bit mask selecting the field.
        mask: u32,
        /// Named values for the field.
        variants: &'static [EnumVariant],
        /// Maps a raw masked value to a canonical variant value for display.
        normalize: Option<fn(u32) -> u32>,
    },
    /// A contiguous bit field holding a raw integer.
    Integer {
        /// Offset of the field within the register.
        offset: u8,
        /// Width of the field in bits.
        width: u8,
        /// Minimum accepted value.
        min: u32,
        /// Maximum accepted value.
        max: u32,
    },
}

/// A named value of an enum-like option.
#[derive(Clone, Copy, Debug)]
pub struct EnumVariant {
    /// Stable value name used on the command line and in configuration files.
    pub name: &'static str,
    /// Raw encoded value.
    pub value: u32,
    /// Human-readable description.
    pub description: &'static str,
}

/// A concrete option value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionValue {
    /// A boolean option.
    Bool(bool),
    /// A named enum variant.
    Enum(&'static str),
    /// A raw integer value.
    Integer(u32),
}

impl fmt::Display for OptionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => write!(f, "{value}"),
            Self::Enum(name) => write!(f, "{name}"),
            Self::Integer(value) => write!(f, "{value}"),
        }
    }
}

impl From<bool> for OptionValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// A set of option values keyed by option name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptionSet {
    values: BTreeMap<&'static str, OptionValue>,
}

impl OptionSet {
    /// Creates an empty option set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the value of an option, if present.
    pub fn get(&self, name: &str) -> Option<&OptionValue> {
        self.values.get(name)
    }

    /// Inserts or replaces an option value.
    pub fn insert(&mut self, name: &'static str, value: OptionValue) {
        self.values.insert(name, value);
    }

    /// Iterates over the options in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &OptionValue)> {
        self.values.iter().map(|(name, value)| (*name, value))
    }

    /// Returns true if no options are present.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the number of options present.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Merges another option set into this one, replacing duplicate names.
    pub fn merge(&mut self, other: &OptionSet) {
        for (name, value) in other.iter() {
            self.insert(name, value.clone());
        }
    }
}

impl OptionField {
    /// Decodes this field from a raw register value.
    pub fn decode(&self, register: u32) -> Result<OptionValue> {
        match self.kind {
            FieldKind::Bool { bit, inverted } => {
                let bit_value = (register >> bit) & 1 != 0;
                Ok(OptionValue::Bool(if inverted {
                    !bit_value
                } else {
                    bit_value
                }))
            }
            FieldKind::Enum {
                offset,
                width,
                variants,
                normalize,
            } => {
                let raw = (register >> offset) & bit_mask(width);
                let normalized = normalize.map_or(raw, |normalize| normalize(raw));
                Ok(
                    match variants.iter().find(|variant| variant.value == normalized) {
                        Some(variant) => OptionValue::Enum(variant.name),
                        None => OptionValue::Integer(normalized),
                    },
                )
            }
            FieldKind::MaskedEnum {
                mask,
                variants,
                normalize,
            } => {
                let raw = register & mask;
                let normalized = normalize.map_or(raw, |normalize| normalize(raw));
                Ok(
                    match variants.iter().find(|variant| variant.value == normalized) {
                        Some(variant) => OptionValue::Enum(variant.name),
                        None => OptionValue::Integer(normalized),
                    },
                )
            }
            FieldKind::Integer { offset, width, .. } => {
                let raw = (register >> offset) & bit_mask(width);
                Ok(OptionValue::Integer(raw))
            }
        }
    }

    /// Encodes a value into a raw register value, preserving unrelated bits.
    pub fn encode(&self, register: &mut u32, value: &OptionValue) -> Result<()> {
        match self.kind {
            FieldKind::Bool { bit, inverted } => {
                let value = self.expect_bool(value)?;
                let bit_value = if inverted { !value } else { value };
                if bit_value {
                    *register |= 1 << bit;
                } else {
                    *register &= !(1 << bit);
                }
            }
            FieldKind::Enum {
                offset,
                width,
                variants,
                ..
            } => {
                let variant = self.expect_variant(value, variants)?;
                let field_mask = bit_mask(width) << offset;
                *register =
                    (*register & !field_mask) | ((variant.value & bit_mask(width)) << offset);
            }
            FieldKind::MaskedEnum { mask, variants, .. } => {
                let variant = self.expect_variant(value, variants)?;
                *register = (*register & !mask) | (variant.value & mask);
            }
            FieldKind::Integer {
                offset,
                width,
                min,
                max,
            } => {
                let value = self.expect_integer(value)?;
                if value < min || value > max {
                    return Err(Error::InvalidValue {
                        option: self.name.to_string(),
                        value: value.to_string(),
                        expected: format!("an integer between {min} and {max}"),
                    });
                }
                let field_mask = bit_mask(width) << offset;
                *register = (*register & !field_mask) | ((value & bit_mask(width)) << offset);
            }
        }

        Ok(())
    }

    /// Parses a command-line string into a value for this field.
    pub fn parse_str(&self, input: &str) -> Result<OptionValue> {
        match self.kind {
            FieldKind::Bool { .. } => {
                parse_bool(input)
                    .map(OptionValue::Bool)
                    .ok_or_else(|| Error::InvalidValue {
                        option: self.name.to_string(),
                        value: input.to_string(),
                        expected: "a boolean (true/false, on/off, enabled/disabled)".to_string(),
                    })
            }
            FieldKind::Enum { variants, .. } | FieldKind::MaskedEnum { variants, .. } => {
                let normalized = normalize_name(input);

                if let Some(variant) = variants
                    .iter()
                    .find(|variant| normalize_name(variant.name) == normalized)
                {
                    return Ok(OptionValue::Enum(variant.name));
                }

                if let Some(raw) = parse_u32(input)
                    && let Some(variant) = variants.iter().find(|variant| variant.value == raw)
                {
                    return Ok(OptionValue::Enum(variant.name));
                }

                Err(Error::InvalidValue {
                    option: self.name.to_string(),
                    value: input.to_string(),
                    expected: expected_variants(variants),
                })
            }
            FieldKind::Integer { min, max, .. } => {
                let value = parse_u32(input).ok_or_else(|| Error::InvalidValue {
                    option: self.name.to_string(),
                    value: input.to_string(),
                    expected: format!("an integer between {min} and {max}"),
                })?;

                if value < min || value > max {
                    return Err(Error::InvalidValue {
                        option: self.name.to_string(),
                        value: input.to_string(),
                        expected: format!("an integer between {min} and {max}"),
                    });
                }

                Ok(OptionValue::Integer(value))
            }
        }
    }

    /// Formats a value for display.
    pub fn format_value(&self, value: &OptionValue) -> String {
        match (self.kind, value) {
            (FieldKind::Bool { .. }, OptionValue::Bool(value)) => {
                if *value {
                    "enabled".to_string()
                } else {
                    "disabled".to_string()
                }
            }
            (
                FieldKind::Enum { variants, .. } | FieldKind::MaskedEnum { variants, .. },
                OptionValue::Enum(name),
            ) => variants
                .iter()
                .find(|variant| variant.name == *name)
                .map_or_else(
                    || name.to_string(),
                    |variant| variant.description.to_string(),
                ),
            (_, value) => value.to_string(),
        }
    }

    /// Returns the named variants of an enum-like field.
    pub fn variants(&self) -> Option<&'static [EnumVariant]> {
        match self.kind {
            FieldKind::Enum { variants, .. } | FieldKind::MaskedEnum { variants, .. } => {
                Some(variants)
            }
            _ => None,
        }
    }

    fn expect_bool(&self, value: &OptionValue) -> Result<bool> {
        match value {
            OptionValue::Bool(value) => Ok(*value),
            _ => Err(self.invalid_value(value, "a boolean")),
        }
    }

    fn expect_variant(
        &self,
        value: &OptionValue,
        variants: &'static [EnumVariant],
    ) -> Result<&'static EnumVariant> {
        match value {
            OptionValue::Enum(name) => variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or_else(|| self.invalid_value(value, &expected_variants(variants))),
            OptionValue::Integer(raw) => variants
                .iter()
                .find(|variant| variant.value == *raw)
                .ok_or_else(|| self.invalid_value(value, &expected_variants(variants))),
            _ => Err(self.invalid_value(value, &expected_variants(variants))),
        }
    }

    fn expect_integer(&self, value: &OptionValue) -> Result<u32> {
        match value {
            OptionValue::Integer(value) => Ok(*value),
            _ => Err(self.invalid_value(value, "an integer")),
        }
    }

    fn invalid_value(&self, value: &OptionValue, expected: &str) -> Error {
        Error::InvalidValue {
            option: self.name.to_string(),
            value: value.to_string(),
            expected: expected.to_string(),
        }
    }
}

fn bit_mask(width: u8) -> u32 {
    if width >= 32 {
        u32::MAX
    } else {
        (1 << width) - 1
    }
}

fn expected_variants(variants: &[EnumVariant]) -> String {
    let names: Vec<_> = variants.iter().map(|variant| variant.name).collect();
    format!("one of {}", names.join(", "))
}

fn normalize_name(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace('_', "-")
}

fn parse_bool(input: &str) -> Option<bool> {
    match normalize_name(input).as_str() {
        "true" | "on" | "enabled" | "yes" | "1" => Some(true),
        "false" | "off" | "disabled" | "no" | "0" => Some(false),
        _ => None,
    }
}

fn parse_u32(input: &str) -> Option<u32> {
    let input = input.trim();

    if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        input.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static VARIANTS: &[EnumVariant] = &[
        EnumVariant {
            name: "zero",
            value: 0,
            description: "zero",
        },
        EnumVariant {
            name: "one",
            value: 1,
            description: "one",
        },
    ];

    static MASKED_VARIANTS: &[EnumVariant] = &[
        EnumVariant {
            name: "zero",
            value: 0,
            description: "zero",
        },
        EnumVariant {
            name: "one",
            value: 0b1010,
            description: "one",
        },
    ];

    #[test]
    fn bool_round_trips() {
        let field = OptionField {
            name: "flag",
            description: "test flag",
            register: "TEST",
            kind: FieldKind::Bool {
                bit: 3,
                inverted: false,
            },
        };

        let mut register = 0;
        field
            .encode(&mut register, &OptionValue::Bool(true))
            .unwrap();
        assert_eq!(register, 1 << 3);
        assert_eq!(field.decode(register).unwrap(), OptionValue::Bool(true));

        field
            .encode(&mut register, &OptionValue::Bool(false))
            .unwrap();
        assert_eq!(register, 0);
        assert_eq!(OptionValue::Bool(false), field.decode(register).unwrap());
    }

    #[test]
    fn inverted_bool_round_trips() {
        let field = OptionField {
            name: "flag",
            description: "test flag",
            register: "TEST",
            kind: FieldKind::Bool {
                bit: 3,
                inverted: true,
            },
        };

        let mut register = 0;
        field
            .encode(&mut register, &OptionValue::Bool(true))
            .unwrap();
        assert_eq!(register, 0);
        assert_eq!(field.decode(register).unwrap(), OptionValue::Bool(true));

        field
            .encode(&mut register, &OptionValue::Bool(false))
            .unwrap();
        assert_eq!(register, 1 << 3);
        assert_eq!(OptionValue::Bool(false), field.decode(register).unwrap());
    }

    #[test]
    fn enum_round_trips_and_preserves_other_bits() {
        let field = OptionField {
            name: "mode",
            description: "test mode",
            register: "TEST",
            kind: FieldKind::Enum {
                offset: 4,
                width: 2,
                variants: VARIANTS,
                normalize: None,
            },
        };

        let mut register = 0xF0F0;
        field
            .encode(&mut register, &OptionValue::Enum("one"))
            .unwrap();

        assert_eq!(register & 0x3F, 0x10);
        assert_eq!(register & !0x3F, 0xF0F0 & !0x3F);
        assert_eq!(field.decode(register).unwrap(), OptionValue::Enum("one"));
    }

    #[test]
    fn masked_enum_round_trips() {
        let field = OptionField {
            name: "boot",
            description: "test boot",
            register: "TEST",
            kind: FieldKind::MaskedEnum {
                mask: 0b1010,
                variants: MASKED_VARIANTS,
                normalize: None,
            },
        };

        let mut register = 0b0101;
        field
            .encode(&mut register, &OptionValue::Enum("one"))
            .unwrap();

        assert_eq!(register, 0b1111);
        assert_eq!(field.decode(register).unwrap(), OptionValue::Enum("one"));
    }

    #[test]
    fn enum_rejects_unknown_variant() {
        let field = OptionField {
            name: "mode",
            description: "test mode",
            register: "TEST",
            kind: FieldKind::Enum {
                offset: 0,
                width: 2,
                variants: VARIANTS,
                normalize: None,
            },
        };

        assert!(field.parse_str("two").is_err());
        assert!(field.parse_str("2").is_err());
    }

    #[test]
    fn parse_bool_accepts_common_spellings() {
        let field = OptionField {
            name: "flag",
            description: "test flag",
            register: "TEST",
            kind: FieldKind::Bool {
                bit: 0,
                inverted: false,
            },
        };

        for input in ["true", "on", "enabled", "yes", "1", "TRUE"] {
            assert_eq!(field.parse_str(input).unwrap(), OptionValue::Bool(true));
        }

        for input in ["false", "off", "disabled", "no", "0", "FALSE"] {
            assert_eq!(field.parse_str(input).unwrap(), OptionValue::Bool(false));
        }
    }

    #[test]
    fn parse_enum_accepts_dashes_and_underscores() {
        let field = OptionField {
            name: "mode",
            description: "test mode",
            register: "TEST",
            kind: FieldKind::Enum {
                offset: 0,
                width: 2,
                variants: VARIANTS,
                normalize: None,
            },
        };

        assert_eq!(field.parse_str("ONE").unwrap(), OptionValue::Enum("one"));
    }
}
