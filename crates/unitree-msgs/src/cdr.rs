//! XCDR2 (plain, `@final`) serialization for Unitree ROS2 messages.
//!
//! Unitree's `.msg` types are all effectively `@final`: there is no DHEADER and
//! no per-member EMHEADER. The wire format is therefore plain XCDR2:
//!
//! - little-endian
//! - maximum alignment of 4 bytes (8-byte primitives align to 4, not 8)
//! - strings are `u32 length` (including the trailing NUL) + bytes + `\0`
//! - sequences (`T[]`) are `u32 length` + elements
//! - fixed arrays (`T[N]`) are `N` elements back-to-back, no length prefix
//!
//! A serialized payload is prefixed with the 4-byte RTPS encapsulation header
//! `00 07 00 00` (CDR2_LE, no options). Alignment is tracked relative to the
//! start of the body; since the header is exactly 4 bytes the two conventions
//! coincide for a max alignment of 4.

use core::convert::TryInto;

/// RTPS encapsulation header for plain XCDR2, little-endian (`CDR2_LE`).
pub const ENCAPSULATION_HEADER: [u8; 4] = [0x00, 0x07, 0x00, 0x00];

/// Errors that can occur while decoding a CDR payload.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CdrError {
    /// The buffer ended before the expected number of bytes was available.
    #[error("unexpected end of CDR buffer: needed {needed} more byte(s) at offset {offset}")]
    Eof { offset: usize, needed: usize },
    /// The encapsulation header was missing or not a supported representation.
    #[error("unsupported CDR encapsulation header: {0:02x?}")]
    BadEncapsulation([u8; 4]),
    /// A string field was not valid UTF-8.
    #[error("invalid UTF-8 in string field")]
    Utf8,
    /// A length field exceeded the remaining buffer (corrupt or hostile input).
    #[error("declared length {len} exceeds remaining buffer at offset {offset}")]
    LengthOverflow { offset: usize, len: usize },
}

/// A growable XCDR2 writer. Tracks the body offset for alignment.
#[derive(Debug, Default, Clone)]
pub struct CdrSerializer {
    buf: Vec<u8>,
}

impl CdrSerializer {
    /// Create an empty serializer.
    #[must_use]
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Current body length (also the alignment offset).
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing has been written yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Pad with zero bytes until the body offset is a multiple of `min(n, 4)`.
    pub fn align(&mut self, n: usize) {
        let a = n.min(4);
        while self.buf.len() % a != 0 {
            self.buf.push(0);
        }
    }

    /// Append raw bytes with no alignment.
    pub fn write_bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// Consume the serializer and return the body (without encapsulation header).
    #[must_use]
    pub fn into_body(self) -> Vec<u8> {
        self.buf
    }

    /// Consume the serializer and return a full payload with the 4-byte
    /// encapsulation header prepended.
    #[must_use]
    pub fn into_payload(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len() + 4);
        out.extend_from_slice(&ENCAPSULATION_HEADER);
        out.extend_from_slice(&self.buf);
        out
    }
}

/// A borrowed XCDR2 reader over a message body (header already stripped).
#[derive(Debug)]
pub struct CdrDeserializer<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> CdrDeserializer<'a> {
    /// Create a deserializer over a body slice (no encapsulation header).
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Create a deserializer over a full payload, validating and skipping the
    /// 4-byte encapsulation header.
    pub fn from_payload(payload: &'a [u8]) -> Result<Self, CdrError> {
        if payload.len() < 4 {
            return Err(CdrError::Eof {
                offset: payload.len(),
                needed: 4 - payload.len(),
            });
        }
        let hdr: [u8; 4] = payload[..4].try_into().unwrap();
        // Accept CDR2_LE (00 07) and plain CDR_LE (00 01); both are LE plain.
        if hdr[0] != 0x00 || (hdr[1] != 0x07 && hdr[1] != 0x01) {
            return Err(CdrError::BadEncapsulation(hdr));
        }
        Ok(Self {
            buf: &payload[4..],
            pos: 0,
        })
    }

    /// Current offset within the body.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Skip padding until the offset is a multiple of `min(n, 4)`.
    pub fn align(&mut self, n: usize) {
        let a = n.min(4);
        while self.pos % a != 0 && self.pos < self.buf.len() {
            self.pos += 1;
        }
    }

    /// Read exactly `n` bytes.
    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], CdrError> {
        if self.pos + n > self.buf.len() {
            return Err(CdrError::Eof {
                offset: self.pos,
                needed: (self.pos + n) - self.buf.len(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
}

/// A type that can be written to an XCDR2 stream.
pub trait CdrSerialize {
    /// Append this value to the serializer.
    fn serialize(&self, s: &mut CdrSerializer);
}

/// A type that can be read from an XCDR2 stream.
pub trait CdrDeserialize: Sized {
    /// Read one value from the deserializer.
    fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError>;
}

macro_rules! impl_primitive {
    ($t:ty, $size:expr) => {
        impl CdrSerialize for $t {
            fn serialize(&self, s: &mut CdrSerializer) {
                s.align($size);
                s.write_bytes(&self.to_le_bytes());
            }
        }
        impl CdrDeserialize for $t {
            fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError> {
                d.align($size);
                let b = d.read_bytes($size)?;
                Ok(<$t>::from_le_bytes(b.try_into().unwrap()))
            }
        }
    };
}

impl_primitive!(u8, 1);
impl_primitive!(i8, 1);
impl_primitive!(u16, 2);
impl_primitive!(i16, 2);
impl_primitive!(u32, 4);
impl_primitive!(i32, 4);
impl_primitive!(u64, 8);
impl_primitive!(i64, 8);
impl_primitive!(f32, 4);
impl_primitive!(f64, 8);

impl CdrSerialize for bool {
    fn serialize(&self, s: &mut CdrSerializer) {
        s.write_bytes(&[u8::from(*self)]);
    }
}

impl CdrDeserialize for bool {
    fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError> {
        Ok(d.read_bytes(1)?[0] != 0)
    }
}

impl CdrSerialize for String {
    fn serialize(&self, s: &mut CdrSerializer) {
        // length includes the trailing NUL
        let len = u32::try_from(self.len() + 1).unwrap_or(u32::MAX);
        len.serialize(s);
        s.write_bytes(self.as_bytes());
        s.write_bytes(&[0u8]);
    }
}

impl CdrDeserialize for String {
    fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError> {
        let len = u32::deserialize(d)? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        let bytes = d.read_bytes(len)?;
        // strip the trailing NUL
        let end = if bytes.last() == Some(&0) {
            bytes.len() - 1
        } else {
            bytes.len()
        };
        core::str::from_utf8(&bytes[..end])
            .map(ToOwned::to_owned)
            .map_err(|_| CdrError::Utf8)
    }
}

impl<T: CdrSerialize> CdrSerialize for Vec<T> {
    fn serialize(&self, s: &mut CdrSerializer) {
        let len = u32::try_from(self.len()).unwrap_or(u32::MAX);
        len.serialize(s);
        for item in self {
            item.serialize(s);
        }
    }
}

impl<T: CdrDeserialize> CdrDeserialize for Vec<T> {
    fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError> {
        let len = u32::deserialize(d)? as usize;
        let mut v = Vec::with_capacity(len.min(1024));
        for _ in 0..len {
            v.push(T::deserialize(d)?);
        }
        Ok(v)
    }
}

impl<T: CdrSerialize, const N: usize> CdrSerialize for [T; N] {
    fn serialize(&self, s: &mut CdrSerializer) {
        for item in self {
            item.serialize(s);
        }
    }
}

impl<T: CdrDeserialize, const N: usize> CdrDeserialize for [T; N] {
    fn deserialize(d: &mut CdrDeserializer<'_>) -> Result<Self, CdrError> {
        let mut v = Vec::with_capacity(N);
        for _ in 0..N {
            v.push(T::deserialize(d)?);
        }
        // `v` has exactly N elements, so the conversion cannot fail.
        Ok(v.try_into().unwrap_or_else(|_| unreachable!()))
    }
}
