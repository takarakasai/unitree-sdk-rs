//! High-level API for the Unitree Go2 robot.
//!
//! Thin Go2-specific conveniences over [`unitree_dds`]: topic-name and joint
//! constants, re-exported message types, and (from M5) `LowCmd` initialization
//! and CRC helpers. The DDS plumbing lives in [`unitree_dds`]; this crate adds
//! only Go2 domain knowledge.
//!
//! # Receiving robot state
//! ```no_run
//! use unitree_dds::{Participant, ReaderQos};
//! use unitree_go2::{topics, LowState};
//!
//! # fn main() -> Result<(), unitree_dds::DdsError> {
//! // domain 0, wired interface to the robot (e.g. "eth0")
//! let dp = Participant::new(0, Some("eth0"))?;
//! let topic = dp.create_topic::<LowState>(topics::LOW_STATE)?;
//! let reader = dp.create_reader(&topic, ReaderQos::low_level_default())?;
//! loop {
//!     if let Some(state) = reader.poll()? {
//!         println!("tick={} q0={}", state.tick, state.motor_state[0].q);
//!     }
//! }
//! # }
//! ```

pub use unitree_dds::{DdsError, Participant, Reader, ReaderQos, Result, Topic, Writer, WriterQos};
pub use unitree_msgs::unitree_go::{
    BmsCmd, BmsState, IMUState, LowCmd, LowState, MotorCmd, MotorState, WirelessController,
};

pub mod lowcmd;
pub use lowcmd::{crc32_core, init_lowcmd, set_crc};

pub mod utlidar;
pub use utlidar::{UtlidarError, UtlidarSwitch};

/// DDS topic names used by the Go2 (domain 0).
pub mod topics {
    /// Low-level command topic (`LowCmd`).
    pub const LOW_CMD: &str = "rt/lowcmd";
    /// Low-level state topic (`LowState`).
    pub const LOW_STATE: &str = "rt/lowstate";
    /// Sport-mode high-level state topic (`SportModeState`).
    pub const SPORT_MODE_STATE: &str = "rt/sportmodestate";
    /// Wireless controller (remote) topic (`WirelessController`).
    pub const WIRELESS_CONTROLLER: &str = "rt/wirelesscontroller";
}

/// Joint (motor) indices into `LowCmd.motor_cmd` / `LowState.motor_state`.
///
/// Naming: `<leg><joint>` where leg ∈ {FR, FL, RR, RL} (front/rear,
/// right/left) and joint 0/1/2 = hip / thigh / calf.
pub mod joint {
    /// Front-right hip.
    pub const FR_0: usize = 0;
    /// Front-right thigh.
    pub const FR_1: usize = 1;
    /// Front-right calf.
    pub const FR_2: usize = 2;

    /// Front-left hip.
    pub const FL_0: usize = 3;
    /// Front-left thigh.
    pub const FL_1: usize = 4;
    /// Front-left calf.
    pub const FL_2: usize = 5;

    /// Rear-right hip.
    pub const RR_0: usize = 6;
    /// Rear-right thigh.
    pub const RR_1: usize = 7;
    /// Rear-right calf.
    pub const RR_2: usize = 8;

    /// Rear-left hip.
    pub const RL_0: usize = 9;
    /// Rear-left thigh.
    pub const RL_1: usize = 10;
    /// Rear-left calf.
    pub const RL_2: usize = 11;

    /// Number of leg joints (the first 12 entries of the 20-element arrays).
    pub const NUM_LEG_JOINTS: usize = 12;
}

/// Sentinel position feedforward value that disables position control on a motor
/// (`PosStopF` in `unitree_sdk2`). Used for velocity/torque-only control.
pub const POS_STOP_F: f32 = 2.146e9;

/// Sentinel velocity feedforward value that disables velocity control on a motor
/// (`VelStopF` in `unitree_sdk2`).
pub const VEL_STOP_F: f32 = 16000.0;
