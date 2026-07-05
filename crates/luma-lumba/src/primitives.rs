//! Primitive value objects and binary helpers shared across the crate.

use crate::error::{LumbaError, Result};
use core::str;

/// Stable identifier wrapper used by higher-level model types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Identifier(String);

impl Identifier {
    /// Creates a new identifier.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the identifier and returns the owned string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl From<String> for Identifier {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Identifier {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Byte-oriented payload used for opaque content.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BinaryBlob(pub Vec<u8>);

impl BinaryBlob {
    /// Creates a new blob.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }
}

/// Minimal unsigned LEB128 value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct UVar(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VarintCanonicality {
    Relaxed,
    Canonical,
}

impl UVar {
    /// Encodes the value into the provided output buffer.
    pub fn encode_into(self, output: &mut Vec<u8>) {
        let mut value = self.0;

        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;

            if value != 0 {
                byte |= 0x80;
            }

            output.push(byte);

            if value == 0 {
                break;
            }
        }
    }

    /// Encodes the value into a new byte vector.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut output = Vec::new();
        self.encode_into(&mut output);
        output
    }

    /// Decodes an unsigned LEB128 value from the input at the current offset.
    pub fn decode(input: &[u8], offset: &mut usize) -> Result<Self> {
        Self::decode_with_canonicality(input, offset, VarintCanonicality::Relaxed)
    }

    /// Decodes a canonically-encoded unsigned LEB128 value from the input at the current offset.
    pub fn decode_canonical(input: &[u8], offset: &mut usize) -> Result<Self> {
        Self::decode_with_canonicality(input, offset, VarintCanonicality::Canonical)
    }

    fn decode_with_canonicality(
        input: &[u8],
        offset: &mut usize,
        canonicality: VarintCanonicality,
    ) -> Result<Self> {
        let start = *offset;
        let at_offset = |error: LumbaError| {
            let context = error.context().clone().with_byte_offset(start);
            error.with_context(context)
        };
        let mut value = 0_u64;
        let mut shift = 0_u32;
        let mut encoded = [0_u8; 10];
        let mut len = 0_usize;

        loop {
            let byte = *input.get(*offset).ok_or_else(|| {
                at_offset(LumbaError::invalid_varint("truncated unsigned varint"))
            })?;

            if len == encoded.len() {
                return Err(at_offset(LumbaError::invalid_varint(
                    "unsigned varint exceeds 10-byte u64 limit",
                )));
            }

            *offset += 1;
            encoded[len] = byte;
            len += 1;

            if shift == 63 && byte > 0x01 {
                return Err(at_offset(LumbaError::invalid_varint(
                    "unsigned varint overflows u64",
                )));
            }

            value |= u64::from(byte & 0x7f) << shift;

            if byte & 0x80 == 0 {
                if canonicality == VarintCanonicality::Canonical {
                    let canonical = UVar(value).encode();

                    if canonical.as_slice() != &encoded[..len] {
                        return Err(at_offset(LumbaError::non_canonical_encoding(
                            "unsigned varint was not minimally encoded",
                        )));
                    }
                }

                return Ok(Self(value));
            }

            shift += 7;

            if shift >= 70 {
                return Err(at_offset(LumbaError::invalid_varint(
                    "unsigned varint exceeds 10-byte u64 limit",
                )));
            }
        }
    }
}

/// Zigzag-encoded signed LEB128 value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SVar(pub i64);

impl SVar {
    /// Converts the signed value to its zigzag-encoded unsigned representation.
    #[must_use]
    pub const fn to_zigzag(self) -> u64 {
        ((self.0 as u64) << 1) ^ ((self.0 >> 63) as u64)
    }

    /// Converts a zigzag-encoded unsigned representation into a signed value.
    #[must_use]
    pub const fn from_zigzag(value: u64) -> Self {
        Self(((value >> 1) as i64) ^ -((value & 1) as i64))
    }

    /// Encodes the value into the provided output buffer.
    pub fn encode_into(self, output: &mut Vec<u8>) {
        UVar(self.to_zigzag()).encode_into(output);
    }

    /// Encodes the value into a new byte vector.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        UVar(self.to_zigzag()).encode()
    }

    /// Decodes a zigzag-encoded signed value from the input at the current offset.
    pub fn decode(input: &[u8], offset: &mut usize) -> Result<Self> {
        UVar::decode(input, offset).map(|value| Self::from_zigzag(value.0))
    }

    /// Decodes a canonically-encoded zigzag-encoded signed value from the input.
    pub fn decode_canonical(input: &[u8], offset: &mut usize) -> Result<Self> {
        UVar::decode_canonical(input, offset).map(|value| Self::from_zigzag(value.0))
    }
}

/// Appends a little-endian `u16` to the output buffer.
pub fn write_u16_le(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian `u32` to the output buffer.
pub fn write_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Appends a little-endian `u64` to the output buffer.
pub fn write_u64_le(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

/// Reads a little-endian `u16` from the input at the current offset.
pub fn read_u16_le(input: &[u8], offset: &mut usize) -> Result<u16> {
    read_fixed::<2>(input, offset).map(u16::from_le_bytes)
}

/// Reads a little-endian `u32` from the input at the current offset.
pub fn read_u32_le(input: &[u8], offset: &mut usize) -> Result<u32> {
    read_fixed::<4>(input, offset).map(u32::from_le_bytes)
}

/// Reads a little-endian `u64` from the input at the current offset.
pub fn read_u64_le(input: &[u8], offset: &mut usize) -> Result<u64> {
    read_fixed::<8>(input, offset).map(u64::from_le_bytes)
}

/// Reads a bounded byte slice.
pub fn read_bounded_bytes<'a>(
    input: &'a [u8],
    offset: &mut usize,
    len: usize,
    max_len: usize,
) -> Result<&'a [u8]> {
    let start = *offset;
    if len > max_len {
        return Err(at_offset(
            LumbaError::resource_limit_exceeded(format!(
                "byte slice length {len} exceeds configured maximum {max_len}",
            )),
            start,
        ));
    }

    let end = (*offset).checked_add(len).ok_or_else(|| {
        at_offset(
            LumbaError::offset_outside_file("byte slice length overflowed offset"),
            start,
        )
    })?;

    let bytes = input.get(*offset..end).ok_or_else(|| {
        at_offset(
            LumbaError::offset_outside_file("byte slice extends beyond available input"),
            start,
        )
    })?;

    *offset = end;
    Ok(bytes)
}

/// Reads a bounded UTF-8 string slice.
pub fn read_bounded_str<'a>(
    input: &'a [u8],
    offset: &mut usize,
    len: usize,
    max_len: usize,
) -> Result<&'a str> {
    let bytes = read_bounded_bytes(input, offset, len, max_len)?;
    str::from_utf8(bytes).map_err(|_| {
        at_offset(
            LumbaError::invalid_utf8("string bytes were not valid UTF-8"),
            *offset - len,
        )
    })
}

/// Returns the zero-padding needed to align the length to an 8-byte boundary.
#[must_use]
pub const fn padding_to_eight(len: usize) -> usize {
    (8 - (len % 8)) % 8
}

/// Returns the next 8-byte aligned length.
#[must_use]
pub const fn align_to_eight(len: usize) -> usize {
    len + padding_to_eight(len)
}

/// Appends zero padding so the output length becomes 8-byte aligned.
pub fn pad_to_eight(output: &mut Vec<u8>) {
    output.resize(output.len() + padding_to_eight(output.len()), 0);
}

/// Validates and consumes explicit zero padding bytes.
pub fn read_zero_padding(input: &[u8], offset: &mut usize, len: usize) -> Result<()> {
    let start = *offset;
    let padding = read_bounded_bytes(input, offset, len, len)?;

    if padding.iter().all(|byte| *byte == 0) {
        Ok(())
    } else {
        Err(at_offset(
            LumbaError::invalid_reserved_flags("padding bytes must be zero"),
            start,
        ))
    }
}

/// Validates and consumes the zero padding required to reach the next 8-byte boundary.
pub fn read_alignment_padding(input: &[u8], offset: &mut usize) -> Result<()> {
    read_zero_padding(input, offset, padding_to_eight(*offset))
}

fn read_fixed<const N: usize>(input: &[u8], offset: &mut usize) -> Result<[u8; N]> {
    let start = *offset;
    let end = (*offset).checked_add(N).ok_or_else(|| {
        at_offset(
            LumbaError::offset_outside_file("fixed-width integer length overflowed offset"),
            start,
        )
    })?;
    let bytes = input.get(*offset..end).ok_or_else(|| {
        at_offset(
            LumbaError::offset_outside_file("fixed-width integer extends beyond available input"),
            start,
        )
    })?;

    *offset = end;
    Ok(bytes.try_into().expect("slice length already validated"))
}

fn at_offset(error: LumbaError, byte_offset: usize) -> LumbaError {
    let context = error.context().clone().with_byte_offset(byte_offset);
    error.with_context(context)
}

#[cfg(test)]
mod tests {
    use super::{
        SVar, UVar, align_to_eight, pad_to_eight, padding_to_eight, read_alignment_padding,
        read_bounded_bytes, read_bounded_str, read_u16_le, read_u32_le, read_u64_le,
        read_zero_padding, write_u16_le, write_u32_le, write_u64_le,
    };
    use crate::error::LumbaError;

    #[test]
    fn primitives_fixed_width_little_endian_round_trip() {
        let mut bytes = Vec::new();
        write_u16_le(&mut bytes, 0x3412);
        write_u32_le(&mut bytes, 0x7856_3412);
        write_u64_le(&mut bytes, 0xefcd_ab89_6745_2301);

        let mut offset = 0;
        assert_eq!(read_u16_le(&bytes, &mut offset).unwrap(), 0x3412);
        assert_eq!(read_u32_le(&bytes, &mut offset).unwrap(), 0x7856_3412);
        assert_eq!(
            read_u64_le(&bytes, &mut offset).unwrap(),
            0xefcd_ab89_6745_2301
        );
        assert_eq!(offset, bytes.len());
    }

    #[test]
    fn primitives_uvar_round_trips_canonical_values() {
        let cases = [0, 1, 127, 128, 255, 300, 16_384, 1 << 20, u64::MAX];

        for expected in cases {
            let encoded = UVar(expected).encode();
            let mut offset = 0;
            let decoded = UVar::decode(&encoded, &mut offset).unwrap();
            let mut canonical_offset = 0;
            let canonical_decoded =
                UVar::decode_canonical(&encoded, &mut canonical_offset).unwrap();

            assert_eq!(decoded, UVar(expected));
            assert_eq!(canonical_decoded, UVar(expected));
            assert_eq!(offset, encoded.len());
            assert_eq!(canonical_offset, encoded.len());
            assert_eq!(encoded, decoded.encode());
        }
    }

    #[test]
    fn primitives_uvar_rejects_more_than_ten_bytes() {
        let mut offset = 0;
        let error = UVar::decode(&[0x80; 11], &mut offset).unwrap_err();

        assert!(matches!(error, LumbaError::InvalidVarint(_)));
        assert_eq!(error.code().as_str(), "LB0012");
    }

    #[test]
    fn primitives_uvar_relaxed_decode_accepts_non_minimal_encoding() {
        let mut offset = 0;
        let decoded = UVar::decode(&[0x80, 0x00], &mut offset).unwrap();

        assert_eq!(decoded, UVar(0));
        assert_eq!(offset, 2);
    }

    #[test]
    fn primitives_uvar_canonical_decode_rejects_non_minimal_encoding() {
        let mut offset = 0;
        let error = UVar::decode_canonical(&[0x80, 0x00], &mut offset).unwrap_err();

        assert!(matches!(error, LumbaError::NonCanonicalEncoding(_)));
        assert_eq!(error.code().as_str(), "LB0017");
    }

    #[test]
    fn primitives_svar_round_trips_signed_edge_cases() {
        let cases = [i64::MIN, -65, -64, -1, 0, 1, 64, 65, i64::MAX];

        for expected in cases {
            let encoded = SVar(expected).encode();
            let mut offset = 0;
            let decoded = SVar::decode(&encoded, &mut offset).unwrap();
            let mut canonical_offset = 0;
            let canonical_decoded =
                SVar::decode_canonical(&encoded, &mut canonical_offset).unwrap();

            assert_eq!(decoded, SVar(expected));
            assert_eq!(canonical_decoded, SVar(expected));
            assert_eq!(offset, encoded.len());
            assert_eq!(canonical_offset, encoded.len());
        }
    }

    #[test]
    fn primitives_svar_canonical_decode_rejects_non_minimal_encoding() {
        let mut offset = 0;
        let error = SVar::decode_canonical(&[0x80, 0x00], &mut offset).unwrap_err();

        assert!(matches!(error, LumbaError::NonCanonicalEncoding(_)));
        assert_eq!(error.code().as_str(), "LB0017");
    }

    #[test]
    fn primitives_bounded_reads_enforce_limits_and_utf8() {
        let input = b"hello\xff";

        let mut offset = 0;
        assert_eq!(
            read_bounded_bytes(input, &mut offset, 5, 5).unwrap(),
            b"hello"
        );
        assert_eq!(offset, 5);

        let mut offset = 0;
        let error = read_bounded_bytes(input, &mut offset, 6, 5).unwrap_err();
        assert!(matches!(error, LumbaError::ResourceLimitExceeded(_)));
        assert_eq!(error.code().as_str(), "LB0018");

        let mut offset = 0;
        let error = read_bounded_str(input, &mut offset, 6, 6).unwrap_err();
        assert!(matches!(error, LumbaError::InvalidUtf8(_)));
        assert_eq!(error.code().as_str(), "LB0013");
    }

    #[test]
    fn primitives_alignment_and_padding_helpers_work() {
        assert_eq!(padding_to_eight(0), 0);
        assert_eq!(padding_to_eight(1), 7);
        assert_eq!(padding_to_eight(8), 0);
        assert_eq!(align_to_eight(9), 16);

        let mut bytes = vec![1, 2, 3];
        pad_to_eight(&mut bytes);
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[3..], &[0, 0, 0, 0, 0]);

        let mut offset = 3;
        read_alignment_padding(&bytes, &mut offset).unwrap();
        assert_eq!(offset, 8);
    }

    #[test]
    fn primitives_padding_validation_rejects_non_zero_bytes() {
        let mut offset = 0;
        let error = read_zero_padding(&[0, 0, 1], &mut offset, 3).unwrap_err();

        assert!(matches!(error, LumbaError::InvalidReservedFlags(_)));
        assert_eq!(error.code().as_str(), "LB0025");
    }
}
