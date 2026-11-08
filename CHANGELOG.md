# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Configuration options are now strongly typed. Unknown option names and
  invalid values are rejected while the file is parsed, and the loose
  `ConfigValue` type has been replaced by a family-specific option schema
  (`G4Options` for STM32G4) parsed through `Family::parse_config`.
- Configuration values now use a single representation per option: named
  enums for categorical options, integers for numeric levels, and booleans
  for flags. The previous string-or-raw-value forms are no longer accepted.
- `rdp` and `brownout` are integer levels (`0`-`2` and `0`-`4`
  respectively); `bank-mode` and `boot` are named enums.

## [0.1.0] - 2026-11-09

### Added

- Initial release of the `stmob` library and `stmob-cli` binary.
- STM32G4 option-byte support.
- TOML configuration parsing.
- Probe selection and `list`, `read`, `get`, `set`, `apply`, and `probes`
  commands.
- RDP regression and write verification.
