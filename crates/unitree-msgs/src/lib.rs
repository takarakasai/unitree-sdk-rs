//! Rust types for Unitree ROS2 messages (`unitree_api`, `unitree_go`,
//! `unitree_hg`) with XCDR2 (de)serialization.
//!
//! The concrete message structs in the [`unitree_api`], [`unitree_go`], and
//! [`unitree_hg`] modules are generated at build time from the `.msg` snapshots
//! under `msgs/`. Each generated struct implements [`DdsType`], giving it a
//! stable DDS type name and CDR round-tripping via [`DdsType::to_cdr`] /
//! [`DdsType::from_cdr`].

pub mod cdr;

pub use cdr::{CdrDeserialize, CdrDeserializer, CdrError, CdrSerialize, CdrSerializer};

/// A DDS message type: a CDR-serializable struct with a DDS type name.
pub trait DdsType: CdrSerialize + CdrDeserialize + Default + Clone {
    /// Fully-qualified DDS type name, e.g. `unitree_go::msg::dds_::LowCmd_`.
    const TYPE_NAME: &'static str;
    /// Whether the topic type has no key fields (true for all Unitree types).
    const IS_KEYLESS: bool;

    /// Serialize to a complete CDR payload including the encapsulation header.
    fn to_cdr(&self) -> Vec<u8> {
        let mut s = CdrSerializer::new();
        self.serialize(&mut s);
        s.into_payload()
    }

    /// Deserialize from a complete CDR payload (with encapsulation header).
    fn from_cdr(payload: &[u8]) -> Result<Self, CdrError> {
        let mut d = CdrDeserializer::from_payload(payload)?;
        Self::deserialize(&mut d)
    }
}

/// A "plain old data" DDS type: one that contains only scalars, fixed-size
/// arrays, and references to other [`DdsPod`] types — no `string` and no
/// variable-length sequence.
///
/// Such a type is generated as `#[repr(C)]` with a memory layout matching the
/// C struct that idlc emits for the same `.msg`, so the Cyclone DDS backend can
/// pass `&Self` directly to `dds_write` and receive into `Self` via `dds_take`
/// with no conversion. [`DESCRIPTOR_SYMBOL`](DdsPod::DESCRIPTOR_SYMBOL) names
/// the `extern` C topic descriptor (committed under `unitree-dds`).
///
/// # Safety / layout contract
/// The `#[repr(C)]` field order and types must match the idlc-generated C
/// struct field-for-field. The committed descriptors are regenerated from the
/// same `.msg` via `.idl`, so this holds by construction; the loopback tests in
/// `unitree-dds` guard it at runtime.
pub trait DdsPod: DdsType + Copy {
    /// Name of the `extern` C `dds_topic_descriptor_t` symbol emitted by idlc,
    /// e.g. `unitree_go_msg_dds__LowState__desc`.
    const DESCRIPTOR_SYMBOL: &'static str;
}

// Generated message modules (`unitree_api`, `unitree_go`, `unitree_hg`).
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
