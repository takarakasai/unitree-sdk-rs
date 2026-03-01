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

// Generated message modules (`unitree_api`, `unitree_go`, `unitree_hg`).
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
