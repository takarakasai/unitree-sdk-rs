//! Same-process Pub/Sub round-trip ("echo") over the raw Cyclone DDS C API.
//!
//! This is the M2 exit check: prove we can create a topic from a committed
//! idlc-generated descriptor, publish a sample, and receive it back in the same
//! process. The descriptor C (`csrc/echo.c`) is compiled by `build.rs`.

use cyclonedds_sys as dds;
use std::ffi::{c_void, CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::time::{Duration, Instant};

// `DDS_DOMAIN_DEFAULT` is a #define, not emitted by bindgen.
const DDS_DOMAIN_DEFAULT: dds::dds_domainid_t = 0xFFFF_FFFF;

/// Rust mirror of the idlc-generated `cyclonedds_sys_test_Echo` C struct.
#[repr(C)]
struct Echo {
    content: *const c_char,
    seq: i32,
}

extern "C" {
    /// Topic descriptor emitted by idlc into `csrc/echo.c`.
    static cyclonedds_sys_test_Echo_desc: dds::dds_topic_descriptor_t;
}

#[test]
fn same_process_echo() {
    unsafe {
        let participant =
            dds::dds_create_participant(DDS_DOMAIN_DEFAULT, ptr::null(), ptr::null());
        assert!(participant > 0, "create_participant failed: {participant}");

        let topic_name = CString::new("cyclonedds_sys_EchoTopic").unwrap();
        let topic = dds::dds_create_topic(
            participant,
            &cyclonedds_sys_test_Echo_desc,
            topic_name.as_ptr(),
            ptr::null(),
            ptr::null(),
        );
        assert!(topic > 0, "create_topic failed: {topic}");

        let writer = dds::dds_create_writer(participant, topic, ptr::null(), ptr::null());
        assert!(writer > 0, "create_writer failed: {writer}");
        let reader = dds::dds_create_reader(participant, topic, ptr::null(), ptr::null());
        assert!(reader > 0, "create_reader failed: {reader}");

        // Publish one sample.
        let content = CString::new("hello-dds").unwrap();
        let sample = Echo {
            content: content.as_ptr(),
            seq: 42,
        };
        let rc = dds::dds_write(writer, (&sample as *const Echo).cast::<c_void>());
        assert_eq!(
            rc,
            dds::DDS_RETCODE_OK as dds::dds_return_t,
            "dds_write failed"
        );

        // Take it back (loaned buffer). Retry to allow local match to settle.
        let mut samples: [*mut c_void; 1] = [ptr::null_mut()];
        let mut infos: [dds::dds_sample_info_t; 1] = std::mem::zeroed();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = None;
        while Instant::now() < deadline {
            let n = dds::dds_take(
                reader,
                samples.as_mut_ptr(),
                infos.as_mut_ptr(),
                1,
                1,
            );
            assert!(n >= 0, "dds_take error: {n}");
            if n > 0 && infos[0].valid_data {
                let echo = &*samples[0].cast::<Echo>();
                let text = CStr::from_ptr(echo.content).to_string_lossy().into_owned();
                let seq = echo.seq;
                // Return the loaned sample before asserting.
                dds::dds_return_loan(reader, samples.as_mut_ptr(), n);
                got = Some((text, seq));
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        dds::dds_delete(participant);

        let (text, seq) = got.expect("did not receive the published sample within timeout");
        assert_eq!(text, "hello-dds");
        assert_eq!(seq, 42);
    }
}
