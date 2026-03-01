//! Round-trip and wire-layout tests for the generated Unitree message types.

use unitree_msgs::cdr::{CdrDeserializer, CdrSerializer};
use unitree_msgs::unitree_go::{HeightMap, LowCmd, LowState, MotorCmd};
use unitree_msgs::{CdrSerialize, DdsType};

/// Helper: assert a value round-trips through a full CDR payload.
fn assert_roundtrip<T: DdsType + PartialEq + std::fmt::Debug>(v: &T) {
    let payload = v.to_cdr();
    assert_eq!(
        &payload[..4],
        &[0x00, 0x07, 0x00, 0x00],
        "encapsulation header"
    );
    let back = T::from_cdr(&payload).expect("decode");
    assert_eq!(&back, v, "round-trip mismatch");
}

#[test]
fn roundtrip_defaults() {
    assert_roundtrip(&MotorCmd::default());
    assert_roundtrip(&LowCmd::default());
    assert_roundtrip(&LowState::default());
    assert_roundtrip(&HeightMap::default());
}

#[test]
fn roundtrip_populated_lowcmd() {
    let mut cmd = LowCmd {
        head: [0xFE, 0xEF],
        level_flag: 0xFF,
        bandwidth: 1234,
        crc: 0xDEAD_BEEF,
        ..Default::default()
    };
    for (i, m) in cmd.motor_cmd.iter_mut().enumerate() {
        m.mode = 0x01;
        m.q = i as f32 * 0.5;
        m.kp = 25.0;
        m.kd = 0.5;
    }
    assert_roundtrip(&cmd);
}

#[test]
fn roundtrip_string_and_sequence() {
    let hm = HeightMap {
        frame_id: "world".to_string(),
        width: 3,
        height: 2,
        resolution: 0.05,
        data: vec![0.0, 1.5, -2.0, 3.25, 4.0, 5.0],
        ..Default::default()
    };
    assert_roundtrip(&hm);
}

/// MotorCmd exercises 4-byte alignment: `uint8 mode` is followed by 3 pad
/// bytes before the first `float32`.
#[test]
fn motorcmd_wire_layout() {
    let m = MotorCmd {
        mode: 0x01,
        q: 2.0,
        ..Default::default()
    };

    let mut s = CdrSerializer::new();
    m.serialize(&mut s);
    let body = s.into_body();

    // mode(1) + pad(3) + q,dq,tau,kp,kd(5*4) + reserve[3](3*4) = 36
    assert_eq!(body.len(), 36, "MotorCmd body size");
    assert_eq!(&body[0..4], &[0x01, 0x00, 0x00, 0x00], "mode + 3 pad bytes");
    // 2.0_f32 == 0x4000_0000, little-endian
    assert_eq!(&body[4..8], &2.0_f32.to_le_bytes(), "q at offset 4");
}

/// Golden bytes for a small message, computed by hand from the XCDR2 rules.
#[test]
fn timespec_golden() {
    use unitree_msgs::unitree_go::TimeSpec;
    let ts = TimeSpec { sec: 1, nanosec: 2 };
    let payload = ts.to_cdr();
    assert_eq!(
        payload,
        vec![
            0x00, 0x07, 0x00, 0x00, // encapsulation header (CDR2_LE)
            0x01, 0x00, 0x00, 0x00, // sec = 1
            0x02, 0x00, 0x00, 0x00, // nanosec = 2
        ]
    );
}

/// A truncated payload must error rather than panic.
#[test]
fn truncated_payload_errors() {
    let payload = MotorCmd::default().to_cdr();
    let truncated = &payload[..payload.len() - 4];
    assert!(MotorCmd::from_cdr(truncated).is_err());
}

/// A bad encapsulation header is rejected.
#[test]
fn bad_header_errors() {
    let mut payload = MotorCmd::default().to_cdr();
    payload[1] = 0x99;
    assert!(MotorCmd::from_cdr(&payload).is_err());
}

/// Deserializing from an explicit body (no header) via the low-level API.
#[test]
fn body_level_roundtrip() {
    let mut s = CdrSerializer::new();
    let m = MotorCmd {
        mode: 7,
        q: 1.0,
        ..Default::default()
    };
    m.serialize(&mut s);
    let body = s.into_body();
    let mut d = CdrDeserializer::new(&body);
    let back = <MotorCmd as unitree_msgs::CdrDeserialize>::deserialize(&mut d).unwrap();
    assert_eq!(back, m);
}
