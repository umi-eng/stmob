/// Result type used throughout the library.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors returned by the flash-option library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A probe-level error occurred.
    #[error(transparent)]
    Probe(#[from] probe_rs::probe::DebugProbeError),

    /// A probe-rs session or target error occurred.
    #[error(transparent)]
    ProbeRs(#[from] probe_rs::Error),

    /// No debug probes were found.
    #[error("no debug probes found")]
    NoProbesFound,

    /// A probe index was requested that does not exist.
    #[error("probe index {index} is out of range; only {count} probe(s) found")]
    ProbeIndexOutOfRange { index: usize, count: usize },

    /// A probe selector could not be parsed.
    #[error("invalid probe selector {selector:?}; expected an index or VID:PID[:SERIAL]")]
    InvalidProbeSelector { selector: String },

    /// The chip is not handled by any registered family.
    #[error("unsupported chip {0:?}; no STM32 flash-option family is registered for it")]
    UnsupportedChip(String),

    /// An option name was not recognised.
    #[error("unknown flash option {name:?}; available options: {available}")]
    UnknownOption { name: String, available: String },

    /// An option value was not valid for the field.
    #[error("invalid value {value:?} for option {option:?}; expected {expected}")]
    InvalidValue {
        option: String,
        value: String,
        expected: String,
    },

    /// A register referenced by a field is missing from the family register map.
    #[error("register {0:?} is missing from the family register map")]
    MissingRegister(&'static str),

    /// The device is read-protected and regression was not allowed.
    #[error(
        "device is read-protected (RDP level 1); pass --allow-rdp-regression to regress to \
         Level 0 (full chip erase) before modifying option bytes"
    )]
    RdpLevel1,

    /// The device is permanently protected.
    #[error("device is permanently protected (RDP level 2); option bytes cannot be modified")]
    RdpLevel2,

    /// Programming RDP level 2 was requested.
    #[error(
        "refusing to program RDP Level 2: it is irreversible and permanently disables debug access"
    )]
    RdpLevel2Write,

    /// PCROP is enabled and the bank mode cannot be changed.
    #[error("PCROP is enabled; DBANK can only be changed when PCROP is disabled")]
    PcropEnabled,

    /// The flash controller reported an error while programming option bytes.
    #[error("option byte programming failed (FLASH_SR = {0:#010x})")]
    FlashProgramming(u32),

    /// FLASH_CR could not be unlocked.
    #[error("failed to unlock FLASH_CR (CR = {0:#010x})")]
    FlashUnlock(u32),

    /// The option bytes could not be unlocked.
    #[error("failed to unlock option bytes (CR = {0:#010x})")]
    OptionUnlock(u32),

    /// FLASH_CR could not be locked again.
    #[error("failed to lock FLASH_CR/option bytes (CR = {0:#010x})")]
    FlashLock(u32),

    /// The flash controller stayed busy for too long.
    #[error("timed out waiting for FLASH_SR.BSY to clear (FLASH_SR = {0:#010x})")]
    FlashTimeout(u32),

    /// The values read back after programming did not match what was written.
    #[error("verification failed: expected {expected}, read back {actual}")]
    VerificationFailed { expected: String, actual: String },

    /// RDP regression did not leave the device at Level 0.
    #[error("RDP regression failed: device is still not at Level 0")]
    RdpRegressionFailed,

    /// The configuration file could not be read.
    #[error("failed to read configuration file {path:?}: {source}")]
    ConfigRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The configuration file could not be parsed.
    #[error("failed to parse configuration file {path:?}: {source}")]
    ConfigParse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}
