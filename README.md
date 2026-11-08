# stmob

[![CI](https://github.com/umi-eng/stmob/actions/workflows/ci.yml/badge.svg)](https://github.com/umi-eng/stmob/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/stmob.svg)](https://crates.io/crates/stmob)
[![crates.io](https://img.shields.io/crates/v/stmob-cli.svg)](https://crates.io/crates/stmob-cli)
[![docs.rs](https://docs.rs/stmob/badge.svg)](https://docs.rs/stmob)

A command-line tool for reading and modifying STM32 flash option bytes over
[probe-rs](https://probe.rs/).

The workspace is split into two crates:

- `stmob` — the library. It contains the probe/session handling, the
  option-byte model, the STM32G4 family definition, and the TOML configuration
  parser.
- `stmob-cli` — the `stmob` binary. It contains the clap command-line
  interface and output formatting.

## Installation

Install the command-line tool from crates.io:

```sh
cargo install stmob-cli
```

This installs the `stmob` binary. To use the library in another crate:

```sh
cargo add stmob
```

The minimum supported Rust version is 1.89.

## Usage

```text
stmob [OPTIONS] <COMMAND>
```

### Probe options

These mirror the relevant probe-rs options:

| Option                  | Description                                                            |
| ----------------------- | ---------------------------------------------------------------------- |
| `-c, --chip <CHIP>`     | Target chip, e.g. `STM32G474CE`. If omitted, probe-rs auto-detects it. |
| `-p, --probe <PROBE>`   | Probe index (`0`) or USB selector (`VID:PID[:SERIAL]`).                |
| `--speed <KHZ>`         | Debug probe speed in kHz.                                              |
| `--protocol <PROTOCOL>` | Wire protocol: `swd` or `jtag`.                                        |
| `--connect-under-reset` | Attach to the target under reset.                                      |
| `--allow-erase-all`     | Allow probe-rs to erase all memory.                                    |

These options use the same environment variables as probe-rs, so existing
scripts work unchanged: `PROBE_RS_CHIP`, `PROBE_RS_PROBE`, `PROBE_RS_SPEED`,
`PROBE_RS_PROTOCOL`, `PROBE_RS_CONNECT_UNDER_RESET`, and
`PROBE_RS_ALLOW_ERASE_ALL`.

## Configuration files

```toml
# Target chip. Can be overridden with --chip.
chip = "STM32G474CE"

[options]
rdp = 1
brownout = 4
bank-mode = "dual"
boot = "main-flash"
boot-lock = true
```

The `chip` field is optional. If it is omitted and `--chip` is not given,
probe-rs auto-detects the target. Only the options present in `[options]` are
changed; every other option bit is preserved. The `[options]` table is parsed
by the selected family, so each family defines its own option names and value
types. Option names and values are validated while the file is parsed, so typos
and unsupported values are reported before a probe is opened. Enum options
accept their name, numeric levels are integers, and boolean options accept
`true` or `false`:

```toml
[options]
rdp = 1                 # 0-2
bank-mode = "dual"
boot-lock = true
brownout = 4            # 0-4
```

See the [`examples/` directory](https://github.com/umi-eng/stmob/tree/main/examples)
for complete files.

## Safety notes

- **RDP Level 2 is never programmed.** It is irreversible and permanently
  disables debug access. The tool rejects it before touching the device.
- **RDP Level 1 requires regression to change options.** A device at Level 1
  must first be regressed to Level 0, which performs a full chip erase. Pass
  `--allow-rdp-regression` to permit this. The tool refuses to do it
  implicitly.
- **Changing the bank mode changes the flash layout.** Existing firmware may
  become unreadable. The tool checks that PCROP is disabled before allowing a
  bank-mode change, but you should still mass-erase and reflash afterwards.
- **Power cycle after programming.** The new option bytes take effect after a
  reset or power cycle.
- **Verification is on by default.** After programming, the option registers are
  read back and compared with the values that were written. Use `--no-verify`
  to skip this check.

## Adding a new STM32 family

1. Create a new module under the library crate's `src/family/` directory
   (`stmob/src/family/` in the repository).
2. Define the option fields with `OptionField` and `FieldKind`, and the
   registers with `RegisterDescriptor`.
3. Define the family's typed `[options]` schema (see `G4Options`) and implement
   `Family::parse_config` to convert it into an `OptionSet`.
4. Implement the `Family` trait. If the family uses the common STM32
   option-byte programming sequence, reuse `OptionByteController` with a
   `ControllerLayout`; otherwise implement `read_registers` and
   `write_registers` directly.
5. Register the family in the library crate's `src/family/mod.rs`
   (`stmob/src/family/mod.rs` in the repository).

The default `Family::decode` and `Family::encode` implementations handle all
field types, so most families only need a declarative field list and the
register read/write primitives.

## Changelog

See the [changelog](https://github.com/umi-eng/stmob/blob/main/CHANGELOG.md).

## License

Licensed under the [Mozilla Public License 2.0](LICENSE).
