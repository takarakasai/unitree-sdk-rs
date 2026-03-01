//! Pluggable DDS backend.
//!
//! Exactly one backend is selected at compile time via cargo features
//! (`backend-cyclonedds` by default, `backend-rustdds` as a Phase-2 stub). The
//! public API in [`crate`] is backend-independent; only [`ActiveBackend`]
//! changes.
//!
//! The data path is the **C-struct path** (see `doc/design.md` §5.3): each
//! message is a `#[repr(C)]` POD ([`unitree_msgs::DdsPod`]) matching the
//! idlc-generated C struct, so writers pass `&T` straight to `dds_write` and
//! readers receive into `T` via `dds_take` with no conversion layer.

use crate::error::Result;
use crate::qos::{ReaderQos, WriterQos};

#[cfg(feature = "backend-cyclonedds")]
#[doc(hidden)]
pub mod cyclonedds;
#[cfg(feature = "backend-rustdds")]
pub(crate) mod rustdds;

/// A DDS backend: an I/O layer over typed POD samples.
pub(crate) trait DdsBackend: 'static {
    /// Opaque participant handle.
    type Participant;
    /// Opaque topic handle.
    type Topic;
    /// Opaque writer handle.
    type Writer;
    /// Opaque reader handle.
    type Reader;

    /// Create a participant on `domain`, optionally bound to a network `iface`.
    fn create_participant(domain: u32, iface: Option<&str>) -> Result<Self::Participant>;

    /// Create a topic named `name` for message type `T`.
    fn create_topic<T: crate::Message>(
        participant: &Self::Participant,
        name: &str,
    ) -> Result<Self::Topic>;

    /// Create a writer on `topic`.
    fn create_writer(
        participant: &Self::Participant,
        topic: &Self::Topic,
        qos: &WriterQos,
    ) -> Result<Self::Writer>;

    /// Create a reader on `topic`.
    fn create_reader(
        participant: &Self::Participant,
        topic: &Self::Topic,
        qos: &ReaderQos,
    ) -> Result<Self::Reader>;

    /// Publish one sample.
    fn write<T: crate::Message>(writer: &Self::Writer, sample: &T) -> Result<()>;

    /// Take at most one sample, non-blocking.
    fn take<T: crate::Message>(reader: &Self::Reader) -> Result<Option<T>>;
}

/// The backend selected by cargo features.
#[cfg(feature = "backend-cyclonedds")]
pub(crate) type ActiveBackend = cyclonedds::CycloneBackend;

/// The backend selected by cargo features.
#[cfg(all(feature = "backend-rustdds", not(feature = "backend-cyclonedds")))]
pub(crate) type ActiveBackend = rustdds::RustddsBackend;
