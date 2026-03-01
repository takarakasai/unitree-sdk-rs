//! Low-level stand-up / lie-down / hold for the Go2, selectable per run.
//!
//! Equivalent in spirit to `unitree_sdk2`'s `go2/go2_stand_example.cpp`, but
//! instead of cycling through a fixed sequence it performs **one** action
//! chosen on the command line:
//!
//! - `up`   — ramp the 12 leg joints from their current pose to a standing pose
//! - `down` — ramp them to a folded lying pose
//! - `hold` — command the *current* pose back (no target motion); meant for
//!            incremental bring-up: start at zero stiffness (pure damping) and
//!            raise `kp` gradually to confirm the command path before any
//!            real transition
//!
//! It reads the current joint angles from `rt/lowstate`, then publishes
//! position-controlled `rt/lowcmd` at 500 Hz, linearly interpolating to the
//! target over `secs` seconds and holding there briefly.
//!
//! Usage:
//! ```text
//! cargo run -p unitree-go2 --example go2_stand -- <iface> <up|down|hold> [secs] [kp] [kd]
//! # incremental bring-up (robot lying, sport_mode already OFF):
//! cargo run -p unitree-go2 --example go2_stand -- eth0 hold 3 0  2   # pure damping, ~no motion
//! cargo run -p unitree-go2 --example go2_stand -- eth0 hold 3 10 3   # gentle hold, tiny motion
//! cargo run -p unitree-go2 --example go2_stand -- eth0 up   1.5      # full stand-up
//! ```
//!
//! # ⚠️ Safety
//! This is **low-level** control that bypasses the onboard motion controller.
//! Before running, the robot's motion-control service (`sport_mode`) MUST be
//! deactivated, otherwise it fights these commands. The Rust SDK cannot toggle
//! that RPC service (out of scope for v0.1) — use the bundled C++ helper
//! `go2_motion_ctrl release <iface>`. While a low-level program is running the
//! remote controller's high-level buttons do NOT work; the emergency action is
//! Ctrl-C plus physically supporting the robot. Keep clear space around it.

use std::time::{Duration, Instant};

use unitree_go2::{
    init_lowcmd, joint, set_crc, topics, LowState, Participant, ReaderQos, WriterQos,
};

/// Standing pose (hip, thigh, calf) per leg, order FR, FL, RR, RL.
/// Matches `_targetPos_2` in `go2_stand_example.cpp`.
const STAND_POS: [f32; 12] = [
    0.0, 0.67, -1.3, // FR
    0.0, 0.67, -1.3, // FL
    0.0, 0.67, -1.3, // RR
    0.0, 0.67, -1.3, // RL
];

/// Folded lying pose. Matches `_targetPos_1` in `go2_stand_example.cpp`.
const LIE_POS: [f32; 12] = [
    0.0, 1.36, -2.65, // FR
    0.0, 1.36, -2.65, // FL
    -0.2, 1.36, -2.65, // RR
    0.2, 1.36, -2.65, // RL
];

/// Default gains for a real transition (`up`/`down`), as in the C++ example.
const KP_MOVE: f32 = 60.0;
const KD_MOVE: f32 = 5.0;
/// Default gains for `hold`: zero stiffness, light damping — essentially no
/// commanded motion, the safest way to confirm the command path.
const KP_HOLD: f32 = 0.0;
const KD_HOLD: f32 = 2.0;

/// 500 Hz control loop (`dt = 0.002 s`).
const CONTROL_DT: Duration = Duration::from_millis(2);
/// How long to keep holding the target pose after the ramp completes.
const HOLD: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
enum Mode {
    Up,
    Down,
    Hold,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let iface = args.next();
    let mode = args.next();
    let secs: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.5);
    let kp_arg: Option<f32> = args.next().and_then(|s| s.parse().ok());
    let kd_arg: Option<f32> = args.next().and_then(|s| s.parse().ok());

    let (iface, mode) = match (iface, mode.as_deref()) {
        (Some(i), Some("up")) => (i, Mode::Up),
        (Some(i), Some("down")) => (i, Mode::Down),
        (Some(i), Some("hold")) => (i, Mode::Hold),
        _ => {
            eprintln!(
                "usage: go2_stand <iface> <up|down|hold> [secs] [kp] [kd]\n\
                 e.g.   go2_stand eth0 hold 3 0 2   # confirm command path, ~no motion\n\
                 e.g.   go2_stand eth0 up   1.5     # full stand-up\n\n\
                 SAFETY: deactivate sport_mode first (go2_motion_ctrl release <iface>);\n\
                 keep space around the robot; Ctrl-C + support it to abort."
            );
            std::process::exit(2);
        }
    };

    let (kp, kd) = match mode {
        Mode::Hold => (kp_arg.unwrap_or(KP_HOLD), kd_arg.unwrap_or(KD_HOLD)),
        _ => (kp_arg.unwrap_or(KP_MOVE), kd_arg.unwrap_or(KD_MOVE)),
    };
    let what = match mode {
        Mode::Up => "STAND UP (lie -> stand)",
        Mode::Down => "LIE DOWN (stand -> lie)",
        Mode::Hold => "HOLD current pose",
    };
    eprintln!("go2_stand: {what}  secs={secs:.2} kp={kp} kd={kd} iface={iface}");
    eprintln!("           ensure sport_mode is OFF and the area is clear ...");

    if let Err(e) = run(&iface, mode, secs, kp, kd) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(iface: &str, mode: Mode, secs: f32, kp: f32, kd: f32) -> unitree_go2::Result<()> {
    let dp = Participant::new(0, Some(iface))?;

    let cmd_topic = dp.create_topic::<unitree_go2::LowCmd>(topics::LOW_CMD)?;
    let writer = dp.create_writer(&cmd_topic, WriterQos::low_level_default())?;

    let state_topic = dp.create_topic::<LowState>(topics::LOW_STATE)?;
    let reader = dp.create_reader(&state_topic, ReaderQos::low_level_default())?;

    // 1. Capture the current leg-joint angles as the ramp start.
    let start = wait_for_start_pose(&reader)?;
    eprintln!(
        "start pose captured: q[0..3]=[{:.3},{:.3},{:.3}] ...",
        start[0], start[1], start[2]
    );

    // Target: a canonical pose for up/down, or the captured pose for hold
    // (so the ramp commands a constant pose and the robot should not move).
    let target = match mode {
        Mode::Up => STAND_POS,
        Mode::Down => LIE_POS,
        Mode::Hold => start,
    };

    let mut cmd = init_lowcmd();
    let total_ticks = ((secs / CONTROL_DT.as_secs_f32()).round() as u64).max(1);
    let hold_ticks = (HOLD.as_secs_f32() / CONTROL_DT.as_secs_f32()).round() as u64;

    // 2. Fixed-rate loop: ramp start -> target, then hold target.
    let loop_start = Instant::now();
    for tick in 0..(total_ticks + hold_ticks) {
        let p = ((tick as f32) / (total_ticks as f32)).min(1.0); // 0 -> 1, then clamped
        for j in 0..joint::NUM_LEG_JOINTS {
            let q = (1.0 - p) * start[j] + p * target[j];
            let m = &mut cmd.motor_cmd[j];
            m.q = q;
            m.dq = 0.0;
            m.kp = kp;
            m.kd = kd;
            m.tau = 0.0;
        }
        set_crc(&mut cmd);
        writer.write(&cmd)?;

        // Sleep to the next 2 ms boundary (keeps the 500 Hz cadence steady).
        let next = loop_start + CONTROL_DT * ((tick + 1) as u32);
        if let Some(d) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(d);
        }
    }

    eprintln!("done: reached target and held {:.1}s", HOLD.as_secs_f32());
    Ok(())
}

/// Block until a `LowState` with joint feedback arrives, returning the first 12
/// leg-joint positions.
fn wait_for_start_pose(reader: &unitree_go2::Reader<LowState>) -> unitree_go2::Result<[f32; 12]> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut warned = false;
    loop {
        if let Some(s) = reader.poll()? {
            let mut q = [0.0f32; 12];
            for j in 0..joint::NUM_LEG_JOINTS {
                q[j] = s.motor_state[j].q;
            }
            return Ok(q);
        }
        if Instant::now() >= deadline {
            return Err(unitree_go2::DdsError::Timeout);
        }
        if !warned {
            eprintln!("... waiting for LowState (check cabling / 192.168.123.x / iface)");
            warned = true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
