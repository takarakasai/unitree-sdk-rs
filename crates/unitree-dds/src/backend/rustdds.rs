//! Phase-2 placeholder backend.
//!
//! This is a compile-only stub so that `cargo check --no-default-features
//! --features backend-rustdds` succeeds and the workspace keeps a slot for the
//! pure-Rust backend. Every operation returns [`DdsError::Unsupported`].

use crate::error::{DdsError, Result};
use crate::qos::{ReaderQos, WriterQos};

use super::DdsBackend;

/// Pure-Rust DDS backend (not yet implemented).
pub(crate) struct RustddsBackend;

impl DdsBackend for RustddsBackend {
    type Participant = ();
    type Topic = ();
    type Writer = ();
    type Reader = ();

    fn create_participant(_domain: u32, _iface: Option<&str>) -> Result<Self::Participant> {
        Err(DdsError::Unsupported("rustdds::create_participant"))
    }

    fn create_topic<T: crate::Message>(
        _participant: &Self::Participant,
        _name: &str,
    ) -> Result<Self::Topic> {
        Err(DdsError::Unsupported("rustdds::create_topic"))
    }

    fn create_writer(
        _participant: &Self::Participant,
        _topic: &Self::Topic,
        _qos: &WriterQos,
    ) -> Result<Self::Writer> {
        Err(DdsError::Unsupported("rustdds::create_writer"))
    }

    fn create_reader(
        _participant: &Self::Participant,
        _topic: &Self::Topic,
        _qos: &ReaderQos,
    ) -> Result<Self::Reader> {
        Err(DdsError::Unsupported("rustdds::create_reader"))
    }

    fn write<T: crate::Message>(_writer: &Self::Writer, _sample: &T) -> Result<()> {
        Err(DdsError::Unsupported("rustdds::write"))
    }

    fn take<T: crate::Message>(_reader: &Self::Reader) -> Result<Option<T>> {
        Err(DdsError::Unsupported("rustdds::take"))
    }
}
