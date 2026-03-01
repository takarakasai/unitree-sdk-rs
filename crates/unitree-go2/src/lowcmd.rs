//! Low-level command (`rt/lowcmd`) helpers: initialization and CRC.
//!
//! The Go2 validates every `LowCmd` with a CRC32 over its raw `#[repr(C)]`
//! bytes (all words except the trailing `crc` field). A command with a wrong
//! CRC is silently dropped by the robot, so [`set_crc`] must be called on each
//! command right before publishing.
//!
//! These mirror `InitLowCmd()` / `crc32_core()` in the `unitree_sdk2` C++
//! example `go2/go2_stand_example.cpp`.

use crate::{LowCmd, POS_STOP_F, VEL_STOP_F};

/// Build a `LowCmd` initialized for low-level (direct motor) control.
///
/// Sets the protocol header (`0xFE 0xEF`), `level_flag = 0xFF`, and puts every
/// one of the 20 motor slots into servo (PMSM) mode with position/velocity
/// control disabled (`q = PosStopF`, `dq = VelStopF`, gains zero). Callers then
/// fill in `motor_cmd[..12]` (`q`/`kp`/`kd`) for the leg joints each tick.
///
/// The struct is zero-initialized first so that *padding* bytes are
/// deterministic — the CRC in [`set_crc`] is computed over the raw bytes that
/// are also placed on the wire, so both sides must agree on padding.
pub fn init_lowcmd() -> LowCmd {
    // SAFETY: `LowCmd` is a `DdsPod` (`#[repr(C)]`, only scalars / fixed arrays):
    // the all-zero bit pattern is a valid value, and zeroing makes padding bytes
    // deterministic for the CRC/wire-byte agreement described above.
    let mut cmd: LowCmd = unsafe { core::mem::zeroed() };
    cmd.head = [0xFE, 0xEF];
    cmd.level_flag = 0xFF;
    for m in cmd.motor_cmd.iter_mut() {
        m.mode = 0x01; // servo (PMSM) mode
        m.q = POS_STOP_F;
        m.dq = VEL_STOP_F;
        m.kp = 0.0;
        m.kd = 0.0;
        m.tau = 0.0;
    }
    cmd
}

/// Unitree's CRC32 (poly `0x04C11DB7`, init `0xFFFFFFFF`, MSB-first, no final
/// XOR or reflection), computed word-by-word over a `u32` slice.
///
/// Bit-for-bit identical to `crc32_core()` in the `unitree_sdk2` examples.
pub fn crc32_core(data: &[u32]) -> u32 {
    const POLY: u32 = 0x04c1_1db7;
    let mut crc: u32 = 0xFFFF_FFFF;
    for &word in data {
        let mut xbit: u32 = 1 << 31;
        for _ in 0..32 {
            if crc & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ POLY;
            } else {
                crc <<= 1;
            }
            if word & xbit != 0 {
                crc ^= POLY;
            }
            xbit >>= 1;
        }
    }
    crc
}

/// Compute and store the CRC over `cmd`, exactly as the robot expects.
///
/// The CRC covers every 32-bit word of the `#[repr(C)]` struct except the final
/// `crc` word, matching the C++ `(sizeof(LowCmd_) >> 2) - 1`.
pub fn set_crc(cmd: &mut LowCmd) {
    let len = core::mem::size_of::<LowCmd>() / 4;
    debug_assert_eq!(
        core::mem::size_of::<LowCmd>() % 4,
        0,
        "LowCmd size must be a multiple of 4 for word-wise CRC"
    );
    // SAFETY: `LowCmd` is `#[repr(C)]` with `u32` fields (align >= 4) and a size
    // that is a multiple of 4, so it can be read as `[u32; size/4]`. We only read
    // the bytes (computing into a local) before writing back `cmd.crc`.
    let words: &[u32] =
        unsafe { core::slice::from_raw_parts(cmd as *const LowCmd as *const u32, len) };
    let crc = crc32_core(&words[..len - 1]);
    cmd.crc = crc;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowcmd_size_is_word_aligned() {
        assert_eq!(core::mem::size_of::<LowCmd>() % 4, 0);
    }

    #[test]
    fn crc_is_deterministic_and_set() {
        let mut a = init_lowcmd();
        let mut b = init_lowcmd();
        set_crc(&mut a);
        set_crc(&mut b);
        assert_eq!(a.crc, b.crc);
        assert_ne!(a.crc, 0);
    }

    #[test]
    fn crc_changes_with_payload() {
        let mut a = init_lowcmd();
        set_crc(&mut a);
        let mut b = init_lowcmd();
        b.motor_cmd[0].q = 0.5;
        set_crc(&mut b);
        assert_ne!(a.crc, b.crc);
    }
}
