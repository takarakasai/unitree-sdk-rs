//! Native-Rust client for the Unitree request/response RPC services that ride
//! on top of DDS — in particular the **`motion_switcher`** service used to turn
//! the onboard high-level motion controller (`sport_mode`) on and off.
//!
//! This is the piece the low-level demos (`go2_stand`, `go2-gait-runner`) need
//! before they can drive `rt/lowcmd`: while `sport_mode` is active the onboard
//! controller fights low-level commands (the joints visibly oscillate), so it
//! must be released first. Until now that was done by shelling out to the C++
//! `go2_motion_ctrl` helper; this crate replaces it with a pure-Rust path.
//!
//! # Why a separate crate
//! The RPC envelope types `unitree_api::msg::dds_::Request_` / `Response_` carry
//! a `string` and a `sequence<octet>`, so they are *not* POD. The `unitree-dds`
//! v0.1 data path only exchanges `#[repr(C)]` POD C structs, so it cannot carry
//! them. Here we link the idlc-generated descriptors for those two types
//! (committed under `csrc/`) and marshal them over `cyclonedds-sys` directly.
//!
//! # Wire protocol
//! A request is published on `rt/api/<service>/request` (type `Request_`); the
//! service replies on `rt/api/<service>/response` (type `Response_`) echoing the
//! request's `identity.id`. The `parameter` / `data` fields carry JSON. The
//! `motion_switcher` service needs no lease.
//!
//! ```no_run
//! # fn main() -> Result<(), unitree_rpc::RpcError> {
//! let sw = unitree_rpc::MotionSwitcher::new("eth0")?;
//! sw.release()?;            // deactivate sport_mode (safe for low-level control)
//! // ... run low-level control ...
//! sw.restore()?;            // hand control back to the onboard controller
//! # Ok(()) }
//! ```

use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cyclonedds_sys as dds;

/// `DDS_INFINITY` (`INT64_MAX` ns) is a `#define`, not emitted by bindgen.
const DDS_INFINITY: dds::dds_duration_t = i64::MAX;

// ---------------------------------------------------------------------------
// motion_switcher service API (see unitree_sdk2 motion_switcher_api.hpp).
// ---------------------------------------------------------------------------

/// `motion_switcher` service name (channel = `rt/api/<name>/{request,response}`).
pub const MOTION_SWITCHER_SERVICE: &str = "motion_switcher";

/// `CheckMode` — query the currently selected mode (`{form, name}`).
pub const API_ID_CHECK_MODE: i64 = 1001;
/// `SelectMode` — select a mode by name/alias (parameter `{"name": "..."}`).
pub const API_ID_SELECT_MODE: i64 = 1002;
/// `ReleaseMode` — release the active mode (deactivate sport_mode).
pub const API_ID_RELEASE_MODE: i64 = 1003;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from the RPC layer.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// A Cyclone DDS C call returned a negative code.
    #[error("DDS call {op} failed (code {code})")]
    Native {
        /// The C function that failed.
        op: &'static str,
        /// The negative `dds_return_t`.
        code: i32,
    },
    /// No response arrived within the timeout (service not reachable, wrong
    /// interface, or `sport_mode` service not running).
    #[error("RPC {api_id} timed out after {waited:?} (no response on the service)")]
    Timeout {
        /// API id of the call that timed out.
        api_id: i64,
        /// How long we waited overall.
        waited: Duration,
    },
    /// The service replied with a non-zero status code.
    #[error("RPC {api_id} returned error status {code}")]
    Status {
        /// API id of the call.
        api_id: i64,
        /// The `ResponseStatus.code` reported by the service.
        code: i32,
    },
    /// An interior NUL prevented building a C string (topic name / parameter).
    #[error("string contained an interior NUL byte")]
    NulString,
}

type Result<T> = std::result::Result<T, RpcError>;

fn check(code: dds::dds_return_t, op: &'static str) -> Result<()> {
    if code < 0 {
        Err(RpcError::Native { op, code })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FFI mirrors of the idlc-generated C structs (csrc/rpc.h). Field order, types
// and natural alignment match idlc's output exactly, so a pointer to these is a
// valid sample pointer for dds_write / dds_take with the linked descriptors.
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
struct RequestIdentity {
    id: i64,
    api_id: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RequestLease {
    id: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RequestPolicy {
    priority: i32,
    noreply: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RequestHeader {
    identity: RequestIdentity,
    lease: RequestLease,
    policy: RequestPolicy,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ResponseStatus {
    code: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ResponseHeader {
    identity: RequestIdentity,
    status: ResponseStatus,
}

/// `dds_sequence_octet`: a Cyclone DDS variable-length octet sequence.
#[repr(C)]
struct DdsSequenceOctet {
    maximum: u32,
    length: u32,
    buffer: *mut u8,
    release: bool,
}

impl DdsSequenceOctet {
    /// An empty (zero-length, no buffer) sequence — what every request we send
    /// uses for `binary`.
    fn empty() -> Self {
        Self {
            maximum: 0,
            length: 0,
            buffer: ptr::null_mut(),
            release: false,
        }
    }
}

#[repr(C)]
struct CRequest {
    header: RequestHeader,
    parameter: *mut c_char,
    binary: DdsSequenceOctet,
}

#[repr(C)]
struct CResponse {
    header: ResponseHeader,
    data: *mut c_char,
    binary: DdsSequenceOctet,
}

extern "C" {
    static unitree_api_msg_dds__Request__desc: dds::dds_topic_descriptor_t;
    static unitree_api_msg_dds__Response__desc: dds::dds_topic_descriptor_t;
}

// ---------------------------------------------------------------------------
// A decoded response handed back to callers.
// ---------------------------------------------------------------------------

/// A decoded service reply.
#[derive(Debug, Clone)]
pub struct Reply {
    /// `ResponseStatus.code` (0 = success).
    pub code: i32,
    /// The `data` JSON payload (may be empty).
    pub data: String,
}

// ---------------------------------------------------------------------------
// Generic RPC client
// ---------------------------------------------------------------------------

/// A request/response client bound to one Unitree RPC service on one interface.
///
/// Owns its own DDS participant; dropping it tears down the participant and all
/// endpoints. Create it, make the calls, and drop it before bringing up the
/// low-level control participant.
pub struct RpcClient {
    participant: dds::dds_entity_t,
    request_writer: dds::dds_entity_t,
    response_reader: dds::dds_entity_t,
    next_id: AtomicI64,
}

impl RpcClient {
    /// Connect to `service` on domain 0 over network interface `iface`.
    pub fn new(service: &str, iface: &str) -> Result<Self> {
        // Cyclone selects the interface from the process-wide CYCLONEDDS_URI.
        // Mirrors unitree-dds's participant setup. Safe here because the RPC
        // client is created and dropped before the low-level participant.
        let xml = format!(
            "<CycloneDDS><Domain><General><Interfaces>\
             <NetworkInterface name=\"{iface}\" priority=\"default\" multicast=\"default\"/>\
             </Interfaces></General></Domain></CycloneDDS>"
        );
        std::env::set_var("CYCLONEDDS_URI", xml);

        let participant = unsafe { dds::dds_create_participant(0, ptr::null(), ptr::null()) };
        check(participant, "dds_create_participant")?;
        let mut this = RpcClient {
            participant,
            request_writer: 0,
            response_reader: 0,
            // Seed ids from the wall clock so they don't collide with other
            // clients sharing the service's response topic; the value only has
            // to be echoed back to us, never interpreted.
            next_id: AtomicI64::new(seed_id()),
        };

        let req_topic = this.create_topic(
            &format!("rt/api/{service}/request"),
            unsafe { &unitree_api_msg_dds__Request__desc },
        )?;
        let resp_topic = this.create_topic(
            &format!("rt/api/{service}/response"),
            unsafe { &unitree_api_msg_dds__Response__desc },
        )?;

        // Reliable + volatile: the service's endpoints are reliable, so the
        // request writer must be too or it won't match. Keep a little history.
        let wq = build_qos(8);
        this.request_writer =
            unsafe { dds::dds_create_writer(participant, req_topic, wq, ptr::null()) };
        unsafe { dds::dds_delete_qos(wq) };
        check(this.request_writer, "dds_create_writer")?;

        let rq = build_qos(64);
        this.response_reader =
            unsafe { dds::dds_create_reader(participant, resp_topic, rq, ptr::null()) };
        unsafe { dds::dds_delete_qos(rq) };
        check(this.response_reader, "dds_create_reader")?;

        Ok(this)
    }

    fn create_topic(
        &self,
        name: &str,
        desc: *const dds::dds_topic_descriptor_t,
    ) -> Result<dds::dds_entity_t> {
        let cname = CString::new(name).map_err(|_| RpcError::NulString)?;
        let topic = unsafe {
            dds::dds_create_topic(self.participant, desc, cname.as_ptr(), ptr::null(), ptr::null())
        };
        check(topic, "dds_create_topic")?;
        Ok(topic)
    }

    /// Issue one RPC call: publish a request with a fresh id and wait for the
    /// matching response. Retries the publish to ride out discovery latency
    /// (the writer/reader must first match the service's endpoints).
    pub fn call(&self, api_id: i64, parameter: &str) -> Result<Reply> {
        const ATTEMPTS: u32 = 10;
        const PER_ATTEMPT: Duration = Duration::from_millis(800);
        let overall_start = Instant::now();

        for _ in 0..ATTEMPTS {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            self.write_request(id, api_id, parameter)?;

            let deadline = Instant::now() + PER_ATTEMPT;
            while Instant::now() < deadline {
                match self.take_matching(id)? {
                    Some(reply) => {
                        if reply.code != 0 {
                            return Err(RpcError::Status {
                                api_id,
                                code: reply.code,
                            });
                        }
                        return Ok(reply);
                    }
                    None => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        }
        Err(RpcError::Timeout {
            api_id,
            waited: overall_start.elapsed(),
        })
    }

    fn write_request(&self, id: i64, api_id: i64, parameter: &str) -> Result<()> {
        let param = CString::new(parameter).map_err(|_| RpcError::NulString)?;
        let mut req = CRequest {
            header: RequestHeader {
                identity: RequestIdentity { id, api_id },
                lease: RequestLease { id: 0 },
                policy: RequestPolicy {
                    priority: 0,
                    noreply: false,
                },
            },
            // dds_write serializes synchronously, so `param` only has to live
            // until the call returns.
            parameter: param.as_ptr() as *mut c_char,
            binary: DdsSequenceOctet::empty(),
        };
        let rc = unsafe {
            dds::dds_write(self.request_writer, (&mut req as *mut CRequest).cast::<c_void>())
        };
        check(rc, "dds_write")
    }

    /// Take responses; return the first whose `identity.id` matches `want_id`.
    /// Responses for other ids (other in-flight calls or other clients sharing
    /// the service's response topic) are discarded.
    fn take_matching(&self, want_id: i64) -> Result<Option<Reply>> {
        loop {
            let mut samples: [*mut c_void; 1] = [ptr::null_mut()];
            let mut infos: [dds::dds_sample_info_t; 1] = unsafe { std::mem::zeroed() };
            let n = unsafe {
                dds::dds_take(
                    self.response_reader,
                    samples.as_mut_ptr(),
                    infos.as_mut_ptr(),
                    1,
                    1,
                )
            };
            check(n, "dds_take")?;
            if n <= 0 {
                return Ok(None);
            }

            let mut matched = None;
            if infos[0].valid_data {
                let resp = unsafe { &*samples[0].cast::<CResponse>() };
                if resp.header.identity.id == want_id {
                    let data = if resp.data.is_null() {
                        String::new()
                    } else {
                        unsafe { CStr::from_ptr(resp.data) }
                            .to_string_lossy()
                            .into_owned()
                    };
                    matched = Some(Reply {
                        code: resp.header.status.code,
                        data,
                    });
                }
            }
            let rc =
                unsafe { dds::dds_return_loan(self.response_reader, samples.as_mut_ptr(), n) };
            check(rc, "dds_return_loan")?;

            if matched.is_some() {
                return Ok(matched);
            }
            // Non-matching sample consumed; keep draining what's queued.
        }
    }
}

impl Drop for RpcClient {
    fn drop(&mut self) {
        // Deleting the participant cascades to topics/reader/writer.
        unsafe {
            dds::dds_delete(self.participant);
        }
    }
}

// ---------------------------------------------------------------------------
// motion_switcher high-level client
// ---------------------------------------------------------------------------

/// High-level client for the `motion_switcher` service: turn the onboard
/// `sport_mode` controller off (`release`) or back on (`restore`).
pub struct MotionSwitcher {
    rpc: RpcClient,
}

impl MotionSwitcher {
    /// Connect to the `motion_switcher` service over `iface` (e.g. `"eth0"`).
    pub fn new(iface: &str) -> Result<Self> {
        Ok(Self {
            rpc: RpcClient::new(MOTION_SWITCHER_SERVICE, iface)?,
        })
    }

    /// Query the active mode as `(form, name)`. An empty `name` means no mode is
    /// active (i.e. `sport_mode` is already released).
    pub fn check_mode(&self) -> Result<(String, String)> {
        let reply = self.rpc.call(API_ID_CHECK_MODE, "")?;
        let name = json_string_field(&reply.data, "name").unwrap_or_default();
        let form = json_string_field(&reply.data, "form").unwrap_or_default();
        Ok((form, name))
    }

    /// Select a mode by name or alias (e.g. `"normal"`).
    pub fn select_mode(&self, name: &str) -> Result<()> {
        let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
        self.rpc
            .call(API_ID_SELECT_MODE, &format!("{{\"name\":\"{escaped}\"}}"))?;
        Ok(())
    }

    /// Release the currently active mode once.
    pub fn release_mode(&self) -> Result<()> {
        self.rpc.call(API_ID_RELEASE_MODE, "")?;
        Ok(())
    }

    /// Deactivate `sport_mode` so low-level `rt/lowcmd` control works: release
    /// the active mode, then confirm via `check_mode` until no mode is active
    /// (mirrors the C++ `go2_motion_ctrl release` loop). Returns the number of
    /// modes released.
    pub fn release(&self) -> Result<u32> {
        const MAX_ITERS: u32 = 5;
        let mut released = 0;
        for _ in 0..MAX_ITERS {
            let (_form, name) = self.check_mode()?;
            if name.is_empty() {
                return Ok(released);
            }
            self.release_mode()?;
            released += 1;
            std::thread::sleep(Duration::from_secs(2));
        }
        // Best effort: report what we did even if the service is slow to settle.
        Ok(released)
    }

    /// Hand control back to the onboard controller by selecting `"normal"`.
    pub fn restore(&self) -> Result<()> {
        self.select_mode("normal")
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

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

/// Seed for request ids: low 48 bits of the wall-clock nanoseconds, so distinct
/// processes/clients on the shared response topic don't reuse the same ids.
fn seed_id() -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(1);
    (nanos & 0x0000_FFFF_FFFF_FFFF).max(1)
}

/// Extract a string field's value from a flat JSON object, e.g. `"name"` from
/// `{"name":"normal","form":"0"}`. Minimal — handles the small, well-formed
/// payloads the motion_switcher service returns; returns `None` if absent.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = &json[json.find(&needle)? + needle.len()..];
    let after_colon = &after_key[after_key.find(':')? + 1..];
    let rest = after_colon.trim_start();
    let mut chars = rest.char_indices();
    // Expect an opening quote.
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escaped = false;
    for (_, c) in chars {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_string_fields() {
        let s = r#"{"name":"normal","form":"0"}"#;
        assert_eq!(json_string_field(s, "name").as_deref(), Some("normal"));
        assert_eq!(json_string_field(s, "form").as_deref(), Some("0"));
        assert_eq!(json_string_field(s, "missing"), None);
    }

    #[test]
    fn empty_name_when_no_mode() {
        let s = r#"{"name":"","form":""}"#;
        assert_eq!(json_string_field(s, "name").as_deref(), Some(""));
    }

    #[test]
    fn handles_escapes() {
        let s = r#"{"name":"a\"b"}"#;
        assert_eq!(json_string_field(s, "name").as_deref(), Some("a\"b"));
    }
}
