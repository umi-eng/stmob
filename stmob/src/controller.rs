use std::time::{Duration, Instant};

use probe_rs::{Core, MemoryInterface};

use crate::error::{Error, Result};
use crate::options::{RegisterDescriptor, RegisterMap};

/// Register addresses and unlock keys for the STM32 option-byte controller.
///
/// This describes the common STM32 flash-controller programming sequence. A
/// family that uses a different sequence can implement [`crate::family::Family`]
/// directly instead of using this controller.
#[derive(Clone, Copy, Debug)]
pub struct ControllerLayout {
    /// Flash access control register.
    pub acr: u64,
    /// Flash key register.
    pub keyr: u64,
    /// Option-byte key register.
    pub optkeyr: u64,
    /// Flash status register.
    pub sr: u64,
    /// Flash control register.
    pub cr: u64,
    /// The two keys written to `keyr` to unlock the flash.
    pub flash_key: (u32, u32),
    /// The two keys written to `optkeyr` to unlock the option bytes.
    pub option_key: (u32, u32),
    /// `CR` bit that indicates the flash is locked.
    pub cr_lock: u32,
    /// `CR` bit that indicates the option bytes are locked.
    pub cr_optlock: u32,
    /// `CR` bit that starts an option-byte programming operation.
    pub cr_optstrt: u32,
    /// `SR` bit that indicates the end of a programming operation.
    pub sr_eop: u32,
    /// All `SR` error bits. Writing 1 to a flag clears it.
    pub sr_errors: u32,
    /// `SR` bit that indicates the controller is busy.
    pub sr_bsy: u32,
    /// `ACR` bits that enable the instruction/data/prefetch caches.
    pub acr_cache_bits: u32,
    /// `ACR` bit that resets the instruction cache.
    pub acr_icrst: u32,
    /// `ACR` bit that resets the data cache.
    pub acr_dcrst: u32,
    /// Maximum time to wait for the controller to become idle.
    pub timeout: Duration,
}

/// Generic STM32 option-byte programming sequence.
pub struct OptionByteController;

impl OptionByteController {
    /// Reads every register in `registers`.
    pub fn read_registers(
        core: &mut Core<'_>,
        registers: &[RegisterDescriptor],
    ) -> Result<RegisterMap> {
        let mut map = RegisterMap::new();

        for register in registers {
            map.insert(register.name, core.read_word_32(register.address)?);
        }

        Ok(map)
    }

    /// Programs the writable option registers in `after`.
    ///
    /// The caches are disabled and flushed for the duration of the operation,
    /// and the flash is locked again afterwards even if programming fails.
    pub fn write_registers(
        core: &mut Core<'_>,
        layout: &ControllerLayout,
        registers: &[RegisterDescriptor],
        writable: &[&'static str],
        after: &RegisterMap,
    ) -> Result<()> {
        let original_acr = disable_and_flush_caches(core, layout)?;

        let program_result = (|| -> Result<()> {
            unlock_flash(core, layout)?;
            unlock_options(core, layout)?;
            program_options(core, layout, registers, writable, after)?;
            Ok(())
        })();

        let restore_result = restore_caches(core, layout, original_acr);
        let lock_result = lock_flash(core, layout);

        program_result?;
        restore_result?;
        lock_result?;

        Ok(())
    }
}

fn program_options(
    core: &mut Core<'_>,
    layout: &ControllerLayout,
    registers: &[RegisterDescriptor],
    writable: &[&'static str],
    after: &RegisterMap,
) -> Result<()> {
    wait_not_busy(core, layout)?;

    // Clear any stale error flags before starting a new option operation.
    core.write_word_32(layout.sr, layout.sr_errors | layout.sr_eop)?;

    // All option bytes are programmed in one go, so write every managed
    // register before starting the operation.
    for name in writable {
        let register = registers
            .iter()
            .find(|register| register.name == *name)
            .ok_or(Error::MissingRegister(name))?;
        let value = after
            .get(*name)
            .copied()
            .ok_or(Error::MissingRegister(name))?;
        core.write_word_32(register.address, value)?;
    }

    let cr = core.read_word_32(layout.cr)?;
    core.write_word_32(layout.cr, cr | layout.cr_optstrt)?;

    wait_not_busy(core, layout)?;

    let sr = core.read_word_32(layout.sr)?;
    if sr & layout.sr_errors != 0 {
        return Err(Error::FlashProgramming(sr));
    }

    Ok(())
}

fn disable_and_flush_caches(core: &mut Core<'_>, layout: &ControllerLayout) -> Result<u32> {
    wait_not_busy(core, layout)?;

    let acr = core.read_word_32(layout.acr)?;
    let acr_disabled = acr & !layout.acr_cache_bits;
    core.write_word_32(layout.acr, acr_disabled)?;
    core.write_word_32(
        layout.acr,
        acr_disabled | layout.acr_icrst | layout.acr_dcrst,
    )?;

    Ok(acr)
}

fn restore_caches(core: &mut Core<'_>, layout: &ControllerLayout, acr: u32) -> Result<()> {
    core.write_word_32(layout.acr, acr)?;
    Ok(())
}

fn unlock_flash(core: &mut Core<'_>, layout: &ControllerLayout) -> Result<()> {
    wait_not_busy(core, layout)?;

    let cr = core.read_word_32(layout.cr)?;
    if cr & layout.cr_lock != 0 {
        core.write_word_32(layout.keyr, layout.flash_key.0)?;
        core.write_word_32(layout.keyr, layout.flash_key.1)?;

        let cr = core.read_word_32(layout.cr)?;
        if cr & layout.cr_lock != 0 {
            return Err(Error::FlashUnlock(cr));
        }
    }

    Ok(())
}

fn unlock_options(core: &mut Core<'_>, layout: &ControllerLayout) -> Result<()> {
    let cr = core.read_word_32(layout.cr)?;
    if cr & layout.cr_optlock != 0 {
        core.write_word_32(layout.optkeyr, layout.option_key.0)?;
        core.write_word_32(layout.optkeyr, layout.option_key.1)?;

        let cr = core.read_word_32(layout.cr)?;
        if cr & layout.cr_optlock != 0 {
            return Err(Error::OptionUnlock(cr));
        }
    }

    Ok(())
}

fn lock_flash(core: &mut Core<'_>, layout: &ControllerLayout) -> Result<()> {
    wait_not_busy(core, layout)?;

    let cr = core.read_word_32(layout.cr)?;
    let cr = (cr & !layout.cr_optstrt) | layout.cr_lock | layout.cr_optlock;
    core.write_word_32(layout.cr, cr)?;

    let cr = core.read_word_32(layout.cr)?;
    if cr & layout.cr_lock == 0 || cr & layout.cr_optlock == 0 {
        return Err(Error::FlashLock(cr));
    }

    Ok(())
}

fn wait_not_busy(core: &mut Core<'_>, layout: &ControllerLayout) -> Result<()> {
    let start = Instant::now();

    loop {
        let sr = core.read_word_32(layout.sr)?;
        if sr & layout.sr_bsy == 0 {
            return Ok(());
        }

        if start.elapsed() > layout.timeout {
            return Err(Error::FlashTimeout(sr));
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
