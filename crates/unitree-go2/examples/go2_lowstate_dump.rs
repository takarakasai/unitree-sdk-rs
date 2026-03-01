//! Connect to a Go2 and dump `LowState` for a few seconds as CSV.
//!
//! Equivalent in spirit to reading `rt/lowstate` with `unitree_sdk2`. The robot
//! must be reachable on the wired LAN (Go2 default `192.168.123.161`); the dev
//! machine needs an address on `192.168.123.x`.
//!
//! Usage:
//! ```text
//! cargo run -p unitree-go2 --example go2_lowstate_dump -- <iface> [seconds]
//! # e.g.
//! cargo run -p unitree-go2 --example go2_lowstate_dump -- eth0 3
//! ```
//!
//! Output columns: `t_s,tick,quat_w,quat_x,quat_y,quat_z,q0..q11`.

use std::time::{Duration, Instant};

use unitree_go2::{joint, topics, LowState, Participant, ReaderQos};

fn main() {
    let mut args = std::env::args().skip(1);
    let iface = match args.next() {
        Some(s) => s,
        None => {
            eprintln!(
                "usage: go2_lowstate_dump <iface> [seconds]\n\
                 e.g.   go2_lowstate_dump eth0 3"
            );
            std::process::exit(2);
        }
    };
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);

    if let Err(e) = run(&iface, secs) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(iface: &str, secs: u64) -> unitree_go2::Result<()> {
    eprintln!("opening participant on domain 0, iface {iface} ...");
    let dp = Participant::new(0, Some(iface))?;
    let topic = dp.create_topic::<LowState>(topics::LOW_STATE)?;
    let reader = dp.create_reader(&topic, ReaderQos::low_level_default())?;

    // CSV header.
    print!("t_s,tick,quat_w,quat_x,quat_y,quat_z");
    for i in 0..joint::NUM_LEG_JOINTS {
        print!(",q{i}");
    }
    println!();

    let start = Instant::now();
    let deadline = start + Duration::from_secs(secs);
    let mut count: u64 = 0;
    let mut last_warn = start;

    while Instant::now() < deadline {
        match reader.poll()? {
            Some(s) => {
                count += 1;
                emit_row(start, &s);
            }
            None => {
                // Nothing yet — if we never hear from the robot, hint at why.
                if count == 0 && last_warn.elapsed() >= Duration::from_secs(1) {
                    eprintln!(
                        "... no LowState yet (check cabling / 192.168.123.x / iface {iface})"
                    );
                    last_warn = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    eprintln!("received {count} samples in {secs}s");
    if count == 0 {
        return Err(unitree_go2::DdsError::Timeout);
    }
    Ok(())
}

fn emit_row(start: Instant, s: &LowState) {
    let t = start.elapsed().as_secs_f64();
    let q = s.imu_state.quaternion;
    print!("{t:.4},{},{},{},{},{}", s.tick, q[0], q[1], q[2], q[3]);
    for i in 0..joint::NUM_LEG_JOINTS {
        print!(",{}", s.motor_state[i].q);
    }
    println!();
}
