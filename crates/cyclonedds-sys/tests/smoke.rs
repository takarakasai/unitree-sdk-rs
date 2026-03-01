//! Smoke test: exercise the generated FFI by creating and deleting a DDS
//! participant on domain 0. This proves bindgen output, linking against the
//! vendored `libddsc`, and runtime loading (via the OUT_DIR rpath) all work.

use cyclonedds_sys as dds;

// `DDS_DOMAIN_DEFAULT` is a plain `#define` macro, which bindgen does not emit
// as a constant. Its value is 0xFFFF_FFFF (dds_domainid_t == u32).
const DDS_DOMAIN_DEFAULT: dds::dds_domainid_t = 0xFFFF_FFFF;

#[test]
fn create_and_delete_participant() {
    unsafe {
        let participant = dds::dds_create_participant(
            DDS_DOMAIN_DEFAULT,
            std::ptr::null(),
            std::ptr::null(),
        );
        assert!(
            participant > 0,
            "dds_create_participant returned non-positive handle {participant} \
             (negative is a dds_return_t error code)"
        );

        let rc = dds::dds_delete(participant);
        assert_eq!(rc, dds::DDS_RETCODE_OK as dds::dds_return_t, "dds_delete failed");
    }
}
