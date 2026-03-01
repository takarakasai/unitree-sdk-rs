//! Offline self-check: print the CRC of a fixed `LowCmd`, no robot needed.
//!
//! Builds the exact command the C++ helper `go2_motion_ctrl crc` builds
//! (init + the 12 leg joints set to the standing pose with Kp=60/Kd=5) and
//! prints `sizeof` and CRC. The two must match bit-for-bit, which validates
//! that the Rust `#[repr(C)]` layout and the CRC port agree with `unitree_sdk2`
//! before any low-level command is sent to the robot.

use unitree_go2::{init_lowcmd, joint, set_crc, LowCmd};

const STAND_POS: [f32; 12] = [
    0.0, 0.67, -1.3, 0.0, 0.67, -1.3, 0.0, 0.67, -1.3, 0.0, 0.67, -1.3,
];

fn main() {
    let mut cmd = init_lowcmd();
    for j in 0..joint::NUM_LEG_JOINTS {
        let m = &mut cmd.motor_cmd[j];
        m.q = STAND_POS[j];
        m.dq = 0.0;
        m.kp = 60.0;
        m.kd = 5.0;
        m.tau = 0.0;
    }
    set_crc(&mut cmd);
    let words = core::mem::size_of::<LowCmd>() / 4;
    println!(
        "sizeof(LowCmd)={} words={} crc=0x{:x}",
        core::mem::size_of::<LowCmd>(),
        words,
        cmd.crc
    );
}
