//! Error type for the DDS layer.

/// Result alias for the DDS layer.
pub type Result<T> = core::result::Result<T, DdsError>;

/// Errors surfaced by the safe DDS API.
#[derive(Debug, thiserror::Error)]
pub enum DdsError {
    /// A Cyclone DDS C call returned a negative `dds_return_t`.
    #[error("Cyclone DDS call `{op}` failed: {code}")]
    Native {
        /// The C function that failed.
        op: &'static str,
        /// The raw `dds_return_t` value (negative).
        code: i32,
    },

    /// Creating a topic failed.
    #[error("failed to create topic `{name}`")]
    TopicCreate {
        /// Topic name.
        name: String,
    },

    /// A blocking receive exceeded its timeout.
    #[error("receive timed out")]
    Timeout,

    /// The selected backend does not implement this operation yet.
    #[error("operation not supported by the active DDS backend: {0}")]
    Unsupported(&'static str),
}
