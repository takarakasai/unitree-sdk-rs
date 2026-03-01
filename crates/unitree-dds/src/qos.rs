//! QoS settings for writers and readers.
//!
//! Unitree's low-level control topics (`rt/lowcmd`, `rt/lowstate`) are
//! loss-tolerant and high-rate, so the default profile mirrors `unitree_sdk2`:
//! best-effort reliability, keep-last(1) history, volatile durability.

/// Reliability kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    /// Best-effort: samples may be dropped (default for low-level control).
    BestEffort,
    /// Reliable: lost samples are retransmitted.
    Reliable,
}

/// History kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum History {
    /// Keep only the last `depth` samples.
    KeepLast(u32),
    /// Keep all samples (bounded by resource limits).
    KeepAll,
}

/// Durability kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// No samples delivered to late-joining readers.
    Volatile,
    /// Locally retained samples delivered to late-joining readers.
    TransientLocal,
}

/// Writer QoS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterQos {
    /// Reliability policy.
    pub reliability: Reliability,
    /// History policy.
    pub history: History,
    /// Durability policy.
    pub durability: Durability,
}

/// Reader QoS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderQos {
    /// Reliability policy.
    pub reliability: Reliability,
    /// History policy.
    pub history: History,
    /// Durability policy.
    pub durability: Durability,
}

impl WriterQos {
    /// QoS matching `unitree_sdk2` for low-level control: best-effort,
    /// keep-last(1), volatile.
    #[must_use]
    pub fn low_level_default() -> Self {
        Self {
            reliability: Reliability::BestEffort,
            history: History::KeepLast(1),
            durability: Durability::Volatile,
        }
    }
}

impl Default for WriterQos {
    fn default() -> Self {
        Self::low_level_default()
    }
}

impl ReaderQos {
    /// QoS matching `unitree_sdk2` for low-level control: best-effort,
    /// keep-last(1), volatile.
    #[must_use]
    pub fn low_level_default() -> Self {
        Self {
            reliability: Reliability::BestEffort,
            history: History::KeepLast(1),
            durability: Durability::Volatile,
        }
    }
}

impl Default for ReaderQos {
    fn default() -> Self {
        Self::low_level_default()
    }
}
