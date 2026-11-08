//! Read and modify STM32 flash option bytes over probe-rs.
//!
//! The library is split into three layers:
//!
//! - [`probe`] opens a probe and attaches to a target using the same options
//!   as probe-rs.
//! - [`family`] describes the option-byte layout of a device family. The
//!   [`family::g4`] module implements STM32G4; new families can be added by
//!   implementing [`family::Family`] and registering the family in
//!   [`family::all`].
//! - [`device`] combines a session with a family and provides high-level
//!   read/write operations, including RDP regression and verification.
//!
//! [`config`] provides the TOML configuration format used by the CLI.

#![warn(missing_docs)]

/// TOML configuration format used by the CLI.
pub mod config;
/// Low-level option-byte controller helpers.
pub mod controller;
/// High-level device operations.
pub mod device;
/// Error and result types.
pub mod error;
/// Device-family definitions.
pub mod family;
/// Option-byte field and value model.
pub mod options;
/// Probe discovery and session handling.
pub mod probe;

pub use config::{Config, ConfigValue};
pub use device::{Device, WriteOptions, WriteReport};
pub use error::{Error, Result};
pub use family::Family;
pub use options::{
    EnumVariant, FieldKind, OptionField, OptionSet, OptionValue, RegisterDescriptor, RegisterMap,
};
pub use probe::{ProbeInfo, ProbeOptions, ProbeSelector, Protocol};
