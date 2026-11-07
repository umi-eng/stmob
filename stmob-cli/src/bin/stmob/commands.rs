use std::path::Path;

use anyhow::{Context, Result, bail};
use stmob::{Config, Device, Family, OptionField, OptionSet, ProbeOptions, WriteOptions};

use crate::args::{ProbeArgs, WriteArgs};
use crate::output;

/// Lists the options supported by a family.
pub fn list(probe_args: &ProbeArgs, family: Option<&str>) -> Result<()> {
    let family = match family {
        Some(name) => Some(
            stmob::family::find_by_name(name)
                .with_context(|| format!("unknown family {name:?}"))?,
        ),
        None => match probe_args.chip.as_deref() {
            Some(chip) => Some(
                stmob::family::find(chip).with_context(|| format!("unsupported chip {chip:?}"))?,
            ),
            None => None,
        },
    };

    match family {
        Some(family) => output::print_family(family),
        None => {
            for (index, family) in stmob::family::all().iter().enumerate() {
                if index > 0 {
                    println!();
                }
                output::print_family(*family);
            }
        }
    }

    Ok(())
}

/// Lists the connected debug probes.
pub fn probes() -> Result<()> {
    let probes = stmob::probe::list_probes();

    if probes.is_empty() {
        println!("No debug probes found.");
        return Ok(());
    }

    for probe in probes {
        println!("{probe}");
    }

    Ok(())
}

/// Reads and prints all flash options.
pub fn read(probe: &ProbeArgs, options: &ProbeOptions) -> Result<()> {
    let mut device = connect(probe, options)?;
    let raw = device
        .read_raw()
        .context("failed to read the option-byte registers")?;
    let values = device
        .family()
        .decode(&raw)
        .context("failed to decode the option bytes")?;

    output::print_options(device.chip(), device.family(), &values, Some(&raw));

    Ok(())
}

/// Reads and prints a single flash option.
pub fn get(probe: &ProbeArgs, options: &ProbeOptions, option: &str) -> Result<()> {
    // If the chip is known, resolve the option before opening a probe so that
    // typos are reported without touching the device.
    let field = match probe.chip.as_deref() {
        Some(chip) => {
            let family =
                stmob::family::find(chip).with_context(|| format!("unsupported chip {chip:?}"))?;
            Some(resolve_field(family, option)?)
        }
        None => None,
    };

    let mut device = connect(probe, options)?;
    let field = match field {
        Some(field) => field,
        None => resolve_field(device.family(), option)?,
    };
    let values = device.read().context("failed to read the option bytes")?;
    let value = values
        .get(field.name)
        .expect("decoded option set contains every field");

    output::print_value(device.family(), field.name, value);

    Ok(())
}

/// Sets a single flash option.
pub fn set(
    probe: &ProbeArgs,
    options: &ProbeOptions,
    option: &str,
    value: &str,
    write_args: WriteArgs,
) -> Result<()> {
    // If the chip is known, resolve and validate the option before opening a
    // probe so that typos and unsafe values are reported without touching the
    // device.
    let desired = match probe.chip.as_deref() {
        Some(chip) => {
            let family =
                stmob::family::find(chip).with_context(|| format!("unsupported chip {chip:?}"))?;
            Some(build_single_option(family, option, value)?)
        }
        None => None,
    };

    let mut device = connect(probe, options)?;
    let desired = match desired {
        Some(desired) => desired,
        None => build_single_option(device.family(), option, value)?,
    };

    write_and_report(&mut device, &desired, write_args)
}

/// Applies all flash options from a TOML file.
pub fn apply(
    probe: &ProbeArgs,
    options: &ProbeOptions,
    file: &Path,
    write_args: WriteArgs,
) -> Result<()> {
    let config = Config::from_path(file)
        .with_context(|| format!("failed to load configuration from {}", file.display()))?;

    let chip = probe.chip.as_deref().or(config.chip.as_deref());

    // If the chip is known, validate the configuration before opening a probe
    // so that configuration errors are reported without touching the device.
    let desired = match chip {
        Some(chip) => {
            let family =
                stmob::family::find(chip).with_context(|| format!("unsupported chip {chip:?}"))?;
            let desired = config
                .into_option_set(family)
                .context("invalid flash-option configuration")?;
            family.validate(&desired)?;
            Some(desired)
        }
        None => None,
    };

    let mut device = connect_with_chip(chip, options)?;
    let desired = match desired {
        Some(desired) => desired,
        None => {
            let desired = config
                .into_option_set(device.family())
                .context("invalid flash-option configuration")?;
            device.family().validate(&desired)?;
            desired
        }
    };

    write_and_report(&mut device, &desired, write_args)
}

fn connect(probe: &ProbeArgs, options: &ProbeOptions) -> Result<Device> {
    connect_with_chip(probe.chip.as_deref(), options)
}

fn connect_with_chip(chip: Option<&str>, options: &ProbeOptions) -> Result<Device> {
    Device::connect(chip, options).context("failed to attach to the target")
}

fn resolve_field(family: &dyn Family, option: &str) -> Result<&'static OptionField> {
    family.field(option).with_context(|| {
        format!(
            "unknown flash option {option:?}; available options: {}",
            family.available_options()
        )
    })
}

fn build_single_option(family: &dyn Family, option: &str, value: &str) -> Result<OptionSet> {
    let field = resolve_field(family, option)?;
    let value = field
        .parse_str(value)
        .with_context(|| format!("invalid value for option {:?}", field.name))?;

    let mut desired = OptionSet::new();
    desired.insert(field.name, value);
    family.validate(&desired)?;

    Ok(desired)
}

fn write_and_report(device: &mut Device, desired: &OptionSet, write_args: WriteArgs) -> Result<()> {
    let check = write_args.check;
    let options: WriteOptions = write_args.to_options();

    let report = device
        .write(desired, options)
        .context("failed to write the flash options")?;

    output::print_report(device.family(), &report);

    if check && report.changed {
        bail!("flash options do not match the requested configuration");
    }

    Ok(())
}
