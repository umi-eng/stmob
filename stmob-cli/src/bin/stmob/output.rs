use stmob::{Family, OptionSet, OptionValue, RegisterMap, WriteReport};

/// Prints the options supported by a family.
pub fn print_family(family: &dyn Family) {
    println!("{}", family.name());

    for field in family.fields() {
        let values = match field.variants() {
            Some(variants) => format!(
                " [{}]",
                variants
                    .iter()
                    .map(|variant| variant.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => String::new(),
        };

        println!("  {:<24} {}{}", field.name, field.description, values);
    }
}

/// Prints the current option values.
pub fn print_options(
    chip: &str,
    family: &dyn Family,
    options: &OptionSet,
    raw: Option<&RegisterMap>,
) {
    println!("Chip: {chip} ({})", family.name());

    if let Some(raw) = raw {
        for (name, value) in raw {
            println!("FLASH_{name} = {value:#010x}");
        }
    }

    println!();
    print_option_table(family, options);
}

/// Prints a single option value.
pub fn print_value(family: &dyn Family, name: &str, value: &OptionValue) {
    let field = family
        .field(name)
        .expect("the caller resolved the field before printing");

    println!("{} = {}", field.name, field.format_value(value));
}

/// Prints the result of a write operation.
pub fn print_report(family: &dyn Family, report: &WriteReport) {
    println!("Chip: {} ({})", report.chip, family.name());
    println!();
    println!("Current flash options:");
    print_option_table(family, &report.before);
    println!();
    println!("Requested changes:");
    print_option_table(family, &report.desired);
    println!();

    if report.changed {
        println!("Resulting flash options:");
        print_option_table(family, &report.after);
        println!();
        println!("Register changes:");
        for (name, before) in &report.raw_before {
            if let Some(after) = report.raw_after.get(name)
                && before != after
            {
                println!("  FLASH_{name}: {before:#010x} -> {after:#010x}");
            }
        }
        println!();
    }

    if report.written {
        println!("Flash options programmed successfully.");
    } else if report.dry_run {
        if report.changed {
            println!("Dry run: no changes were written.");
        } else {
            println!("Flash options already match the requested configuration; nothing to do.");
        }
    } else if report.regressed_rdp {
        println!("RDP regression complete; no further changes were needed.");
    } else {
        println!("Flash options already match the requested configuration; nothing to do.");
    }

    for note in &report.notes {
        println!("Note: {note}");
    }
}

fn print_option_table(family: &dyn Family, options: &OptionSet) {
    for field in family.fields() {
        if let Some(value) = options.get(field.name) {
            println!("  {:<24} {}", field.name, field.format_value(value));
        }
    }
}
