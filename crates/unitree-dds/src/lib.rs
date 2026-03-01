//! Safe DDS pub/sub layer for Unitree robots.
//!
//! A thin, backend-independent API ([`Participant`], [`Topic`], [`Writer`],
//! [`Reader`]) over a DDS implementation selected at compile time:
//! `backend-cyclonedds` (default, via [`cyclonedds_sys`]) or `backend-rustdds`
//! (Phase-2 stub). See `doc/design.md` §5.3.
//!
//! Messages are the `#[repr(C)]` POD types from [`unitree_msgs`] (the
//! [`Message`] alias), exchanged with Cyclone DDS as C structs (the committed
//! idlc descriptors). Variable-length types (with `string`/sequence fields) are
//! not POD and cannot be used as topic types in v0.1.
//!
//! ```no_run
//! use unitree_dds::{Participant, ReaderQos, WriterQos};
//! use unitree_msgs::unitree_go::LowState;
//!
//! # fn main() -> Result<(), unitree_dds::DdsError> {
//! let dp = Participant::new(0, Some("eth0"))?;
//! let topic = dp.create_topic::<LowState>("rt/lowstate")?;
//! let reader = dp.create_reader(&topic, ReaderQos::low_level_default())?;
//! while let Some(state) = reader.poll()? {
//!     println!("tick = {}", state.tick);
//! }
//! # Ok(()) }
//! ```

#[doc(hidden)]
pub mod backend;
mod error;
mod qos;

use std::marker::PhantomData;
use std::time::{Duration, Instant};

pub use error::{DdsError, Result};
pub use qos::{Durability, History, ReaderQos, Reliability, WriterQos};

use backend::{ActiveBackend, DdsBackend};

#[cfg(feature = "backend-cyclonedds")]
#[doc(hidden)]
pub use backend::cyclonedds::CycloneType;

/// A type usable as a DDS topic type with the active backend.
///
/// With `backend-cyclonedds` this requires the type to be a POD message
/// ([`unitree_msgs::DdsPod`]) with a linked C topic descriptor
/// ([`CycloneType`]); both are satisfied automatically by the generated types.
#[cfg(feature = "backend-cyclonedds")]
pub trait Message: unitree_msgs::DdsPod + CycloneType {}
#[cfg(feature = "backend-cyclonedds")]
impl<T: unitree_msgs::DdsPod + CycloneType> Message for T {}

/// A type usable as a DDS topic type with the active backend.
#[cfg(all(feature = "backend-rustdds", not(feature = "backend-cyclonedds")))]
pub trait Message: unitree_msgs::DdsPod {}
#[cfg(all(feature = "backend-rustdds", not(feature = "backend-cyclonedds")))]
impl<T: unitree_msgs::DdsPod> Message for T {}

/// A DDS domain participant. Owns the underlying entity; dropping it tears down
/// all topics/readers/writers created from it.
pub struct Participant {
    inner: <ActiveBackend as DdsBackend>::Participant,
}

impl Participant {
    /// Create a participant on `domain`, optionally bound to network interface
    /// `iface` (e.g. `"eth0"`).
    ///
    /// # Note
    /// The interface is selected via the process-wide `CYCLONEDDS_URI`
    /// environment variable, so create all participants from one thread before
    /// spawning workers, and avoid mixing different interfaces in one process.
    pub fn new(domain: u32, iface: Option<&str>) -> Result<Self> {
        Ok(Self {
            inner: ActiveBackend::create_participant(domain, iface)?,
        })
    }

    /// Create a topic named `name` carrying messages of type `T`.
    pub fn create_topic<T: Message>(&self, name: &str) -> Result<Topic<T>> {
        Ok(Topic {
            inner: ActiveBackend::create_topic::<T>(&self.inner, name)?,
            _t: PhantomData,
        })
    }

    /// Create a writer on `topic`.
    pub fn create_writer<T: Message>(
        &self,
        topic: &Topic<T>,
        qos: WriterQos,
    ) -> Result<Writer<T>> {
        Ok(Writer {
            inner: ActiveBackend::create_writer(&self.inner, &topic.inner, &qos)?,
            _t: PhantomData,
        })
    }

    /// Create a reader on `topic`.
    pub fn create_reader<T: Message>(
        &self,
        topic: &Topic<T>,
        qos: ReaderQos,
    ) -> Result<Reader<T>> {
        Ok(Reader {
            inner: ActiveBackend::create_reader(&self.inner, &topic.inner, &qos)?,
            _t: PhantomData,
        })
    }
}

/// A typed topic.
pub struct Topic<T: Message> {
    inner: <ActiveBackend as DdsBackend>::Topic,
    _t: PhantomData<fn() -> T>,
}

/// A typed writer.
pub struct Writer<T: Message> {
    inner: <ActiveBackend as DdsBackend>::Writer,
    _t: PhantomData<fn() -> T>,
}

impl<T: Message> Writer<T> {
    /// Publish one sample.
    pub fn write(&self, sample: &T) -> Result<()> {
        ActiveBackend::write(&self.inner, sample)
    }
}

/// A typed reader.
pub struct Reader<T: Message> {
    inner: <ActiveBackend as DdsBackend>::Reader,
    _t: PhantomData<fn() -> T>,
}

impl<T: Message> Reader<T> {
    /// Take at most one sample, non-blocking. Returns `Ok(None)` if no data is
    /// currently available.
    pub fn poll(&self) -> Result<Option<T>> {
        ActiveBackend::take(&self.inner)
    }

    /// Block until a sample arrives or `timeout` elapses.
    ///
    /// Polls on a short interval; returns [`DdsError::Timeout`] if nothing
    /// arrives in time.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(sample) = self.poll()? {
                return Ok(sample);
            }
            if Instant::now() >= deadline {
                return Err(DdsError::Timeout);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
