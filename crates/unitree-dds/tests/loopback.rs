//! Same-process loopback: publish a `LowState` and receive it back through the
//! safe API. This is the M3 exit check (design.md §5.3) and also guards the
//! `#[repr(C)]` ⇄ idlc C-struct layout contract at runtime.

#![cfg(feature = "backend-cyclonedds")]

use std::time::Duration;

use unitree_dds::{Participant, ReaderQos, WriterQos};
use unitree_msgs::unitree_go::LowState;

#[test]
fn lowstate_loopback() {
    let dp = Participant::new(0, None).expect("participant");
    let topic = dp
        .create_topic::<LowState>("rt/lowstate_test")
        .expect("topic");
    let writer = dp
        .create_writer(&topic, WriterQos::low_level_default())
        .expect("writer");
    let reader = dp
        .create_reader(&topic, ReaderQos::low_level_default())
        .expect("reader");

    let mut sample = LowState {
        tick: 12345,
        foot_force: [10, 20, 30, 40],
        ..Default::default()
    };
    sample.imu_state.quaternion = [1.0, 0.0, 0.0, 0.0];
    sample.motor_state[0].q = 1.5;
    sample.motor_state[11].q = -0.75;

    writer.write(&sample).expect("write");

    let got = reader
        .recv_timeout(Duration::from_secs(5))
        .expect("receive within timeout");

    assert_eq!(got.tick, 12345);
    assert_eq!(got.imu_state.quaternion, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(got.motor_state[0].q, 1.5);
    assert_eq!(got.motor_state[11].q, -0.75);
    assert_eq!(got.foot_force, [10, 20, 30, 40]);
}
