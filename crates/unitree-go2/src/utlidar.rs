//! Publisher for **`rt/utlidar/switch`** — the Go2 L1 LIDAR on/off control
//! topic.
//!
//! The topic type is `std_msgs::msg::dds_::String_` (a single `string data`).
//! Being variable-length it is *not* a POD `unitree-dds` topic type, so — like
//! [`unitree-rpc`](https://docs.rs/unitree-rpc) does for the RPC envelopes — we
//! link the idlc-generated descriptor (committed under `csrc/std_msgs.{idl,c,h}`,
//! symbol `std_msgs_msg_dds__String__desc`) and marshal a `#[repr(C)]` mirror
//! over `cyclonedds-sys` directly. The Rust message type itself lives in
//! [`unitree_msgs::std_msgs::String`].
//!
//! Publishing the string `"ON"` / `"OFF"` toggles the LIDAR.
//!
//! ```no_run
//! # fn main() -> Result<(), unitree_go2::utlidar::UtlidarError> {
//! let sw = unitree_go2::utlidar::UtlidarSwitch::new("eth0")?;
//! sw.set_on(false)?;          // turn the L1 LIDAR off
//! # Ok(()) }
//! ```

use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::ptr;
use std::time::{Duration, Instant};

use cyclonedds_sys as dds;

/// `DDS_INFINITY` (`INT64_MAX` ns) is a `#define`, not emitted by bindgen.
const DDS_INFINITY: dds::dds_duration_t = i64::MAX;

/// The Go2 LIDAR switch topic name.
pub const UTLIDAR_SWITCH_TOPIC: &str = "rt/utlidar/switch";

/// Errors from the LIDAR-switch publisher.
#[derive(Debug, thiserror::Error)]
pub enum UtlidarError {
    /// A Cyclone DDS C call returned a negative code.
    #[error("DDS call {op} failed (code {code})")]
    Native {
        /// The C function that failed.
        op: &'static str,
        /// The negative `dds_return_t`.
        code: i32,
    },
    /// The topic name or payload contained an interior NUL byte.
    #[error("string contained an interior NUL byte")]
    NulString,
    /// No subscriber (the Go2 utlidar node) matched within the timeout —
    /// check the interface name and that the robot is reachable.
    #[error("no rt/utlidar/switch subscriber matched within {0:?}")]
    NoSubscriber(Duration),
}

type Result<T> = std::result::Result<T, UtlidarError>;

fn check(code: dds::dds_return_t, op: &'static str) -> Result<()> {
    if code < 0 {
        Err(UtlidarError::Native { op, code })
    } else {
        Ok(())
    }
}

/// FFI mirror of `csrc/std_msgs.h`'s `std_msgs_msg_dds__String_` (`{ char* data; }`).
/// A pointer to this is a valid sample pointer for `dds_write` with the linked
/// descriptor.
#[repr(C)]
struct CStringMsg {
    data: *mut c_char,
}

extern "C" {
    static std_msgs_msg_dds__String__desc: dds::dds_topic_descriptor_t;
}

/// Publisher bound to `rt/utlidar/switch` on one interface.
///
/// Owns its own DDS participant; dropping it tears down the participant, topic
/// and writer.
pub struct UtlidarSwitch {
    participant: dds::dds_entity_t,
    writer: dds::dds_entity_t,
}

impl UtlidarSwitch {
    /// Create a participant on domain 0 bound to network interface `iface`
    /// (e.g. `"eth0"`) and a writer for `rt/utlidar/switch`.
    pub fn new(iface: &str) -> Result<Self> {
        // Cyclone selects the interface from the process-wide CYCLONEDDS_URI
        // (mirrors `unitree-rpc` / `unitree-dds`).
        let xml = format!(
            "<CycloneDDS><Domain><General><Interfaces>\
             <NetworkInterface name=\"{iface}\" priority=\"default\" multicast=\"default\"/>\
             </Interfaces></General></Domain></CycloneDDS>"
        );
        std::env::set_var("CYCLONEDDS_URI", xml);

        let participant = unsafe { dds::dds_create_participant(0, ptr::null(), ptr::null()) };
        check(participant, "dds_create_participant")?;

        let cname = CString::new(UTLIDAR_SWITCH_TOPIC).map_err(|_| UtlidarError::NulString)?;
        let topic = unsafe {
            dds::dds_create_topic(
                participant,
                &std_msgs_msg_dds__String__desc,
                cname.as_ptr(),
                ptr::null(),
                ptr::null(),
            )
        };
        if topic < 0 {
            unsafe { dds::dds_delete(participant) };
        }
        check(topic, "dds_create_topic")?;

        // Reliable + keep-last + volatile: the Go2 utlidar reader is reliable,
        // so the writer must be too or it won't match.
        let qos = build_qos(4);
        let writer = unsafe { dds::dds_create_writer(participant, topic, qos, ptr::null()) };
        unsafe { dds::dds_delete_qos(qos) };
        if writer < 0 {
            unsafe { dds::dds_delete(participant) };
        }
        check(writer, "dds_create_writer")?;

        Ok(Self {
            participant,
            writer,
        })
    }

    /// Publish an arbitrary `std_msgs/String` payload to `rt/utlidar/switch`.
    ///
    /// `dds_write` serializes synchronously, so the temporary C string only has
    /// to live until the call returns. Does **not** wait for a subscriber — use
    /// [`Self::wait_for_subscriber`] or [`Self::set_on`] for that.
    pub fn publish(&self, data: &str) -> Result<()> {
        let c = CString::new(data).map_err(|_| UtlidarError::NulString)?;
        let mut msg = CStringMsg {
            data: c.as_ptr() as *mut c_char,
        };
        let rc = unsafe {
            dds::dds_write(self.writer, (&mut msg as *mut CStringMsg).cast::<c_void>())
        };
        check(rc, "dds_write")
    }

    /// Block until at least one subscriber (the Go2 utlidar node) has matched
    /// the writer, or `timeout` elapses. DDS discovery is asynchronous, so a
    /// write issued immediately after [`Self::new`] can be dropped before the
    /// reader is known; call this first.
    pub fn wait_for_subscriber(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let mut status: dds::dds_publication_matched_status_t =
                unsafe { std::mem::zeroed() };
            let rc =
                unsafe { dds::dds_get_publication_matched_status(self.writer, &mut status) };
            check(rc, "dds_get_publication_matched_status")?;
            if status.current_count > 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(UtlidarError::NoSubscriber(timeout));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Turn the L1 LIDAR on (`"ON"`) or off (`"OFF"`). Waits up to 2 s for the
    /// utlidar subscriber to match before publishing so the command isn't lost
    /// to discovery latency.
    pub fn set_on(&self, on: bool) -> Result<()> {
        self.wait_for_subscriber(Duration::from_secs(2))?;
        self.publish(if on { "ON" } else { "OFF" })
    }
}

impl Drop for UtlidarSwitch {
    fn drop(&mut self) {
        // Deleting the participant cascades to the topic and writer.
        unsafe {
            dds::dds_delete(self.participant);
        }
    }
}

/// Build a reliable / keep-last(`depth`) / volatile QoS. Caller owns the result
/// and must `dds_delete_qos` it.
fn build_qos(depth: i32) -> *mut dds::dds_qos_t {
    unsafe {
        let q = dds::dds_create_qos();
        dds::dds_qset_reliability(
            q,
            dds::dds_reliability_kind::DDS_RELIABILITY_RELIABLE,
            DDS_INFINITY,
        );
        dds::dds_qset_history(q, dds::dds_history_kind::DDS_HISTORY_KEEP_LAST, depth);
        dds::dds_qset_durability(q, dds::dds_durability_kind::DDS_DURABILITY_VOLATILE);
        q
    }
}
