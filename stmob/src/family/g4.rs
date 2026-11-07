use std::time::Duration;

use probe_rs::Core;

use crate::controller::{ControllerLayout, OptionByteController};
use crate::error::{Error, Result};
use crate::family::Family;
use crate::options::{
    EnumVariant, FieldKind, OptionField, OptionSet, OptionValue, RegisterDescriptor, RegisterMap,
};

/// FLASH peripheral base address.
const FLASH_BASE: u64 = 0x4002_2000;

const FLASH_ACR: u64 = FLASH_BASE;
const FLASH_KEYR: u64 = FLASH_BASE + 0x08;
const FLASH_OPTKEYR: u64 = FLASH_BASE + 0x0C;
const FLASH_SR: u64 = FLASH_BASE + 0x10;
const FLASH_CR: u64 = FLASH_BASE + 0x14;
const FLASH_OPTR: u64 = FLASH_BASE + 0x20;
const FLASH_PCROP1SR: u64 = FLASH_BASE + 0x24;
const FLASH_PCROP1ER: u64 = FLASH_BASE + 0x28;
const FLASH_PCROP2SR: u64 = FLASH_BASE + 0x44;
const FLASH_PCROP2ER: u64 = FLASH_BASE + 0x48;
const FLASH_SEC1R: u64 = FLASH_BASE + 0x70;

/// FLASH_KEYR unlock keys.
const FLASH_KEY1: u32 = 0x4567_0123;
const FLASH_KEY2: u32 = 0xCDEF_89AB;

/// FLASH_OPTKEYR unlock keys.
const OPTKEY1: u32 = 0x0819_2A3B;
const OPTKEY2: u32 = 0x4C5D_6E7F;

/// FLASH_ACR bits.
const ACR_PRFTEN: u32 = 1 << 8;
const ACR_ICEN: u32 = 1 << 9;
const ACR_DCEN: u32 = 1 << 10;
const ACR_ICRST: u32 = 1 << 11;
const ACR_DCRST: u32 = 1 << 12;

/// FLASH_CR bits.
const CR_LOCK: u32 = 1 << 31;
const CR_OPTLOCK: u32 = 1 << 30;
const CR_OPTSTRT: u32 = 1 << 17;

/// FLASH_SR bits.
const SR_EOP: u32 = 1 << 0;
const SR_OPERR: u32 = 1 << 1;
const SR_PROGERR: u32 = 1 << 3;
const SR_WRPERR: u32 = 1 << 4;
const SR_PGAERR: u32 = 1 << 5;
const SR_SIZERR: u32 = 1 << 6;
const SR_PGSERR: u32 = 1 << 7;
const SR_MISSERR: u32 = 1 << 8;
const SR_FASTERR: u32 = 1 << 9;
const SR_RDERR: u32 = 1 << 14;
const SR_OPTVERR: u32 = 1 << 15;
const SR_BSY: u32 = 1 << 16;

/// All FLASH_SR error flags. Writing 1 to a flag clears it.
const SR_ERRORS: u32 = SR_OPERR
    | SR_PROGERR
    | SR_WRPERR
    | SR_PGAERR
    | SR_SIZERR
    | SR_PGSERR
    | SR_MISSERR
    | SR_FASTERR
    | SR_RDERR
    | SR_OPTVERR;

/// PCROP start/end offset field mask.
const PCROP_OFFSET_MASK: u32 = 0x7FFF;

/// FLASH_OPTR bits managed by the `boot` option.
const OPTR_NBOOT1: u32 = 1 << 23;
const OPTR_NSWBOOT0: u32 = 1 << 26;
const OPTR_NBOOT0: u32 = 1 << 27;
const OPTR_BOOT_MASK: u32 = OPTR_NBOOT1 | OPTR_NSWBOOT0 | OPTR_NBOOT0;

const OPTR: &str = "OPTR";
const SEC1R: &str = "SEC1R";
const PCROP1SR: &str = "PCROP1SR";
const PCROP1ER: &str = "PCROP1ER";
const PCROP2SR: &str = "PCROP2SR";
const PCROP2ER: &str = "PCROP2ER";

static REGISTERS: &[RegisterDescriptor] = &[
    RegisterDescriptor {
        name: OPTR,
        address: FLASH_OPTR,
    },
    RegisterDescriptor {
        name: SEC1R,
        address: FLASH_SEC1R,
    },
    RegisterDescriptor {
        name: PCROP1SR,
        address: FLASH_PCROP1SR,
    },
    RegisterDescriptor {
        name: PCROP1ER,
        address: FLASH_PCROP1ER,
    },
    RegisterDescriptor {
        name: PCROP2SR,
        address: FLASH_PCROP2SR,
    },
    RegisterDescriptor {
        name: PCROP2ER,
        address: FLASH_PCROP2ER,
    },
];

static WRITABLE_REGISTERS: &[&str] = &[OPTR, SEC1R];

static CONTROLLER: ControllerLayout = ControllerLayout {
    acr: FLASH_ACR,
    keyr: FLASH_KEYR,
    optkeyr: FLASH_OPTKEYR,
    sr: FLASH_SR,
    cr: FLASH_CR,
    flash_key: (FLASH_KEY1, FLASH_KEY2),
    option_key: (OPTKEY1, OPTKEY2),
    cr_lock: CR_LOCK,
    cr_optlock: CR_OPTLOCK,
    cr_optstrt: CR_OPTSTRT,
    sr_eop: SR_EOP,
    sr_errors: SR_ERRORS,
    sr_bsy: SR_BSY,
    acr_cache_bits: ACR_PRFTEN | ACR_ICEN | ACR_DCEN,
    acr_icrst: ACR_ICRST,
    acr_dcrst: ACR_DCRST,
    timeout: Duration::from_secs(5),
};

static RDP_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "level0",
        value: 0xAA,
        description: "level 0 (unprotected)",
    },
    EnumVariant {
        name: "level1",
        value: 0xBB,
        description: "level 1 (read-protected)",
    },
    EnumVariant {
        name: "level2",
        value: 0xCC,
        description: "level 2 (chip-protected)",
    },
];

static BOR_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "level0",
        value: 0,
        description: "level 0",
    },
    EnumVariant {
        name: "level1",
        value: 1,
        description: "level 1",
    },
    EnumVariant {
        name: "level2",
        value: 2,
        description: "level 2",
    },
    EnumVariant {
        name: "level3",
        value: 3,
        description: "level 3",
    },
    EnumVariant {
        name: "level4",
        value: 4,
        description: "level 4",
    },
];

static BANK_MODE_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "single",
        value: 0,
        description: "single-bank",
    },
    EnumVariant {
        name: "dual",
        value: 1,
        description: "dual-bank",
    },
];

static BOOT_VARIANTS: &[EnumVariant] = &[
    EnumVariant {
        name: "main-flash",
        value: OPTR_NBOOT1 | OPTR_NBOOT0,
        description: "main flash (BOOT0 forced low)",
    },
    EnumVariant {
        name: "pin",
        value: OPTR_NBOOT1 | OPTR_NSWBOOT0 | OPTR_NBOOT0,
        description: "BOOT0 pin",
    },
    EnumVariant {
        name: "system-memory",
        value: OPTR_NBOOT1,
        description: "system memory (BOOT0 option bit high)",
    },
    EnumVariant {
        name: "sram",
        value: 0,
        description: "SRAM",
    },
];

static FIELDS: &[OptionField] = &[
    OptionField {
        name: "rdp",
        description: "Readout protection level",
        register: OPTR,
        kind: FieldKind::Enum {
            offset: 0,
            width: 8,
            variants: RDP_VARIANTS,
            normalize: Some(normalize_rdp),
        },
    },
    OptionField {
        name: "brownout",
        description: "Brownout reset threshold level",
        register: OPTR,
        kind: FieldKind::Enum {
            offset: 8,
            width: 3,
            variants: BOR_VARIANTS,
            normalize: None,
        },
    },
    OptionField {
        name: "bank-mode",
        description: "Flash bank organization",
        register: OPTR,
        kind: FieldKind::Enum {
            offset: 22,
            width: 1,
            variants: BANK_MODE_VARIANTS,
            normalize: None,
        },
    },
    OptionField {
        name: "boot",
        description: "Boot source",
        register: OPTR,
        kind: FieldKind::MaskedEnum {
            mask: OPTR_BOOT_MASK,
            variants: BOOT_VARIANTS,
            normalize: Some(normalize_boot),
        },
    },
    OptionField {
        name: "boot-lock",
        description: "Force boot from main flash, ignoring BOOT0",
        register: SEC1R,
        kind: FieldKind::Bool {
            bit: 16,
            inverted: false,
        },
    },
    OptionField {
        name: "reset-in-stop",
        description: "Generate a reset in Stop mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 12,
            inverted: true,
        },
    },
    OptionField {
        name: "reset-in-standby",
        description: "Generate a reset in Standby mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 13,
            inverted: true,
        },
    },
    OptionField {
        name: "reset-in-shutdown",
        description: "Generate a reset in Shutdown mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 14,
            inverted: true,
        },
    },
    OptionField {
        name: "iwdg-software",
        description: "Independent watchdog controlled by software",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 16,
            inverted: false,
        },
    },
    OptionField {
        name: "iwdg-freeze-stop",
        description: "Freeze the independent watchdog in Stop mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 17,
            inverted: false,
        },
    },
    OptionField {
        name: "iwdg-freeze-standby",
        description: "Freeze the independent watchdog in Standby mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 18,
            inverted: false,
        },
    },
    OptionField {
        name: "wwdg-software",
        description: "Window watchdog controlled by software",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 19,
            inverted: false,
        },
    },
    OptionField {
        name: "boot-from-bank2",
        description: "Boot from flash bank 2 in dual-bank mode",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 20,
            inverted: false,
        },
    },
    OptionField {
        name: "sram2-parity",
        description: "Enable SRAM2 parity checking",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 24,
            inverted: false,
        },
    },
    OptionField {
        name: "sram2-erase-on-reset",
        description: "Erase SRAM2 on system reset",
        register: OPTR,
        kind: FieldKind::Bool {
            bit: 25,
            inverted: false,
        },
    },
];

/// STM32G4 family.
pub struct G4Family;

/// The registered STM32G4 family instance.
pub static G4: G4Family = G4Family;

impl Family for G4Family {
    fn name(&self) -> &'static str {
        "STM32G4"
    }

    fn matches_chip(&self, chip: &str) -> bool {
        chip.trim().to_ascii_uppercase().starts_with("STM32G4")
    }

    fn fields(&self) -> &'static [OptionField] {
        FIELDS
    }

    fn read_registers(&self, core: &mut Core<'_>) -> Result<RegisterMap> {
        OptionByteController::read_registers(core, REGISTERS)
    }

    fn write_registers(&self, core: &mut Core<'_>, after: &RegisterMap) -> Result<()> {
        OptionByteController::write_registers(
            core,
            &CONTROLLER,
            REGISTERS,
            WRITABLE_REGISTERS,
            after,
        )
    }

    fn validate(&self, desired: &OptionSet) -> Result<()> {
        self.validate_names(desired)?;

        // Level 2 is irreversible and permanently disables debug access, so it
        // must never be programmed by this tool.
        if matches!(desired.get("rdp"), Some(OptionValue::Enum("level2"))) {
            return Err(Error::RdpLevel2Write);
        }

        Ok(())
    }

    fn check_preconditions(
        &self,
        registers: &RegisterMap,
        current: &OptionSet,
        desired: &OptionSet,
    ) -> Result<()> {
        let changing_bank_mode = match (current.get("bank-mode"), desired.get("bank-mode")) {
            (Some(current), Some(desired)) => current != desired,
            _ => false,
        };

        if !changing_bank_mode {
            return Ok(());
        }

        // PCROP is disabled when the start offset is greater than the end
        // offset.
        let pcrop1_start = registers.get(PCROP1SR).copied().unwrap_or(0) & PCROP_OFFSET_MASK;
        let pcrop1_end = registers.get(PCROP1ER).copied().unwrap_or(0) & PCROP_OFFSET_MASK;
        let pcrop2_start = registers.get(PCROP2SR).copied().unwrap_or(0) & PCROP_OFFSET_MASK;
        let pcrop2_end = registers.get(PCROP2ER).copied().unwrap_or(0) & PCROP_OFFSET_MASK;

        if pcrop1_start <= pcrop1_end || pcrop2_start <= pcrop2_end {
            return Err(Error::PcropEnabled);
        }

        Ok(())
    }
}

fn normalize_rdp(raw: u32) -> u32 {
    match raw {
        0xAA => 0xAA,
        0xCC => 0xCC,
        // Level 1 is any value other than the Level 0 and Level 2 magic
        // values. Normalise it to the conventional 0xBB.
        _ => 0xBB,
    }
}

fn normalize_boot(raw: u32) -> u32 {
    // nBOOT1 = 0 selects SRAM regardless of the other bits.
    if raw & OPTR_NBOOT1 == 0 {
        return 0;
    }

    // nSWBOOT0 = 1 selects the BOOT0 pin; nBOOT0 is ignored.
    if raw & OPTR_NSWBOOT0 != 0 {
        return OPTR_NBOOT1 | OPTR_NSWBOOT0 | OPTR_NBOOT0;
    }

    // nSWBOOT0 = 0: nBOOT0 selects main flash (1) or system memory (0).
    if raw & OPTR_NBOOT0 != 0 {
        OPTR_NBOOT1 | OPTR_NBOOT0
    } else {
        OPTR_NBOOT1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registers(optr: u32, sec1r: u32) -> RegisterMap {
        let mut registers = RegisterMap::new();
        registers.insert(OPTR, optr);
        registers.insert(SEC1R, sec1r);
        registers.insert(PCROP1SR, 0x7FFF);
        registers.insert(PCROP1ER, 0);
        registers.insert(PCROP2SR, 0x7FFF);
        registers.insert(PCROP2ER, 0);
        registers
    }

    fn options(pairs: &[(&'static str, OptionValue)]) -> OptionSet {
        let mut options = OptionSet::new();
        for (name, value) in pairs {
            options.insert(name, value.clone());
        }
        options
    }

    #[test]
    fn rdp_levels_are_normalized() {
        let field = G4
            .fields()
            .iter()
            .find(|field| field.name == "rdp")
            .unwrap();

        assert_eq!(
            field.parse_str("level0").unwrap(),
            OptionValue::Enum("level0")
        );
        assert_eq!(
            field.parse_str("level1").unwrap(),
            OptionValue::Enum("level1")
        );
        assert_eq!(
            field.parse_str("level2").unwrap(),
            OptionValue::Enum("level2")
        );

        assert_eq!(
            G4.decode(&registers(0xAA, 0)).unwrap().get("rdp"),
            Some(&OptionValue::Enum("level0"))
        );
        assert_eq!(
            G4.decode(&registers(0xBB, 0)).unwrap().get("rdp"),
            Some(&OptionValue::Enum("level1"))
        );
        assert_eq!(
            G4.decode(&registers(0x55, 0)).unwrap().get("rdp"),
            Some(&OptionValue::Enum("level1"))
        );
        assert_eq!(
            G4.decode(&registers(0xCC, 0)).unwrap().get("rdp"),
            Some(&OptionValue::Enum("level2"))
        );
    }

    #[test]
    fn brownout_levels_round_trip() {
        for (bits, name) in [
            (0, "level0"),
            (1, "level1"),
            (2, "level2"),
            (3, "level3"),
            (4, "level4"),
        ] {
            let optr = bits << 8;
            let decoded = G4.decode(&registers(optr, 0)).unwrap();
            assert_eq!(decoded.get("brownout"), Some(&OptionValue::Enum(name)));

            let mut raw = registers(0, 0);
            G4.encode(&mut raw, &options(&[("brownout", OptionValue::Enum(name))]))
                .unwrap();
            assert_eq!(raw[OPTR] & (0x7 << 8), bits << 8);
        }
    }

    #[test]
    fn bank_mode_round_trips() {
        assert_eq!(
            G4.decode(&registers(0, 0)).unwrap().get("bank-mode"),
            Some(&OptionValue::Enum("single"))
        );
        assert_eq!(
            G4.decode(&registers(1 << 22, 0)).unwrap().get("bank-mode"),
            Some(&OptionValue::Enum("dual"))
        );

        let mut raw = registers(0, 0);
        G4.encode(
            &mut raw,
            &options(&[("bank-mode", OptionValue::Enum("dual"))]),
        )
        .unwrap();
        assert_eq!(raw[OPTR] & (1 << 22), 1 << 22);
    }

    #[test]
    fn boot_config_round_trips() {
        for (optr, name) in [
            (OPTR_NBOOT1 | OPTR_NBOOT0, "main-flash"),
            (OPTR_NBOOT1 | OPTR_NSWBOOT0 | OPTR_NBOOT0, "pin"),
            (OPTR_NBOOT1 | OPTR_NSWBOOT0, "pin"),
            (OPTR_NBOOT1, "system-memory"),
            (0, "sram"),
        ] {
            let decoded = G4.decode(&registers(optr, 0)).unwrap();
            assert_eq!(decoded.get("boot"), Some(&OptionValue::Enum(name)));
        }
    }

    #[test]
    fn main_flash_boot_forces_boot0_low() {
        for optr in [
            0,
            OPTR_NSWBOOT0,
            OPTR_NBOOT0,
            OPTR_NSWBOOT0 | OPTR_NBOOT0,
            OPTR_NBOOT1 | OPTR_NSWBOOT0 | OPTR_NBOOT0,
        ] {
            let mut raw = registers(optr, 0);
            G4.encode(
                &mut raw,
                &options(&[("boot", OptionValue::Enum("main-flash"))]),
            )
            .unwrap();

            assert_eq!(raw[OPTR] & OPTR_NSWBOOT0, 0);
            assert_eq!(raw[OPTR] & OPTR_NBOOT0, OPTR_NBOOT0);
            assert_eq!(raw[OPTR] & OPTR_NBOOT1, OPTR_NBOOT1);
            assert_eq!(
                G4.decode(&raw).unwrap().get("boot"),
                Some(&OptionValue::Enum("main-flash"))
            );
        }
    }

    #[test]
    fn boot_lock_round_trips() {
        assert_eq!(
            G4.decode(&registers(0, 0)).unwrap().get("boot-lock"),
            Some(&OptionValue::Bool(false))
        );
        assert_eq!(
            G4.decode(&registers(0, 1 << 16)).unwrap().get("boot-lock"),
            Some(&OptionValue::Bool(true))
        );

        let mut raw = registers(0, 0);
        G4.encode(
            &mut raw,
            &options(&[("boot-lock", OptionValue::Bool(true))]),
        )
        .unwrap();
        assert_eq!(raw[SEC1R] & (1 << 16), 1 << 16);
    }

    #[test]
    fn apply_preserves_unrelated_bits() {
        let managed = (0xFF)
            | (0x7 << 8)
            | (1 << 22)
            | OPTR_BOOT_MASK
            | (1 << 12)
            | (1 << 13)
            | (1 << 14)
            | (1 << 16)
            | (1 << 17)
            | (1 << 18)
            | (1 << 19)
            | (1 << 20)
            | (1 << 24)
            | (1 << 25);
        let optr = !managed;
        let sec1r = !(1 << 16);

        let mut raw = registers(optr, sec1r);
        G4.encode(
            &mut raw,
            &options(&[
                ("rdp", OptionValue::Enum("level1")),
                ("brownout", OptionValue::Enum("level4")),
                ("bank-mode", OptionValue::Enum("dual")),
                ("boot", OptionValue::Enum("main-flash")),
                ("boot-lock", OptionValue::Bool(true)),
            ]),
        )
        .unwrap();

        assert_eq!(raw[OPTR] & !managed, optr & !managed);
        assert_eq!(raw[SEC1R] & !(1 << 16), sec1r & !(1 << 16));
    }

    #[test]
    fn apply_is_idempotent() {
        let desired = options(&[
            ("rdp", OptionValue::Enum("level1")),
            ("brownout", OptionValue::Enum("level4")),
            ("bank-mode", OptionValue::Enum("dual")),
            ("boot", OptionValue::Enum("main-flash")),
            ("boot-lock", OptionValue::Bool(true)),
        ]);

        let mut once = registers(0x1234_5678, 0x1234_5678);
        G4.encode(&mut once, &desired).unwrap();

        let mut twice = once.clone();
        G4.encode(&mut twice, &desired).unwrap();

        assert_eq!(once, twice);
    }

    #[test]
    fn apply_can_clear_managed_bits() {
        let desired = options(&[
            ("rdp", OptionValue::Enum("level0")),
            ("brownout", OptionValue::Enum("level0")),
            ("bank-mode", OptionValue::Enum("single")),
            ("boot", OptionValue::Enum("sram")),
            ("boot-lock", OptionValue::Bool(false)),
        ]);

        let mut raw = registers(
            (4 << 8) | (1 << 22) | OPTR_NBOOT1 | OPTR_NBOOT0 | 0xBB,
            1 << 16,
        );
        G4.encode(&mut raw, &desired).unwrap();

        assert_eq!(raw[OPTR] & 0xFF, 0xAA);
        assert_eq!(raw[OPTR] & (0x7 << 8), 0);
        assert_eq!(raw[OPTR] & (1 << 22), 0);
        assert_eq!(raw[OPTR] & OPTR_BOOT_MASK, 0);
        assert_eq!(raw[SEC1R] & (1 << 16), 0);
    }

    #[test]
    fn validate_refuses_rdp_level_2() {
        let desired = options(&[("rdp", OptionValue::Enum("level2"))]);
        assert!(matches!(G4.validate(&desired), Err(Error::RdpLevel2Write)));
    }

    #[test]
    fn changing_bank_mode_requires_pcrop_disabled() {
        let current = G4.decode(&registers(0, 0)).unwrap();
        let desired = options(&[("bank-mode", OptionValue::Enum("dual"))]);

        // PCROP disabled: start > end.
        assert!(
            G4.check_preconditions(&registers(0, 0), &current, &desired)
                .is_ok()
        );

        // PCROP enabled: start <= end.
        let mut raw = registers(0, 0);
        raw.insert(PCROP1SR, 0);
        raw.insert(PCROP1ER, 0x100);
        assert!(matches!(
            G4.check_preconditions(&raw, &current, &desired),
            Err(Error::PcropEnabled)
        ));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let mut raw = registers(0, 0);
        let desired = options(&[("not-an-option", OptionValue::Bool(true))]);
        assert!(matches!(
            G4.encode(&mut raw, &desired),
            Err(Error::UnknownOption { .. })
        ));
    }
}
