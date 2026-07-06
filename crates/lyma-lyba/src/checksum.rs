//! Checksum helpers for container headers, footers, and section payloads.

use crate::error::{ErrorContext, LybaError, Result};
use crate::section::{CHECKSUM_CRC32C, CHECKSUM_NONE, SectionEntry};

/// Computes CRC32C over the provided bytes.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    crc32c::crc32c(bytes)
}

/// Computes the canonical header CRC32C with the stored CRC field zeroed.
#[must_use]
pub fn crc32c_header(input: &[u8]) -> u32 {
    let mut header = [0_u8; crate::container::HEADER_LEN];
    header.copy_from_slice(&input[..crate::container::HEADER_LEN]);
    header[56..60].fill(0);
    crc32c(&header[..56])
}

/// Computes the canonical footer CRC32C with the stored CRC field zeroed.
#[must_use]
pub fn crc32c_footer(input: &[u8]) -> u32 {
    let mut footer = [0_u8; crate::container::FOOTER_LEN];
    footer.copy_from_slice(&input[..crate::container::FOOTER_LEN]);
    footer[56..60].fill(0);
    crc32c(&footer[..56])
}

/// Encodes checksum metadata for one payload.
pub fn encode_section_checksum(entry: &mut SectionEntry, payload: &[u8]) -> Result<()> {
    match entry.checksum_id {
        CHECKSUM_NONE => {
            entry.checksum_low = 0;
            entry.checksum_high = 0;
            Ok(())
        }
        CHECKSUM_CRC32C => {
            entry.checksum_low = u64::from(crc32c(payload));
            entry.checksum_high = 0;
            Ok(())
        }
        checksum_id => Err(LybaError::UnsupportedRequiredSection(ErrorContext::new(
            format!("writer does not support checksum algorithm {checksum_id}"),
        ))),
    }
}

/// Validates checksum metadata for one stored payload.
pub fn validate_section_checksum(entry: SectionEntry, payload: &[u8]) -> Result<()> {
    match entry.checksum_id {
        CHECKSUM_NONE => {
            if entry.checksum_low == 0 && entry.checksum_high == 0 {
                Ok(())
            } else {
                Err(LybaError::ChecksumMismatch(
                    ErrorContext::new(format!(
                        "section {} declared no checksum but stored checksum fields were non-zero",
                        entry.section_id.as_str()
                    ))
                    .with_byte_offset(48),
                ))
            }
        }
        CHECKSUM_CRC32C => {
            if entry.checksum_high != 0 {
                return Err(LybaError::ChecksumMismatch(
                    ErrorContext::new(format!(
                        "section {} CRC32C checksum used non-zero high bits",
                        entry.section_id.as_str()
                    ))
                    .with_byte_offset(56),
                ));
            }

            let expected = u64::from(crc32c(payload));
            if entry.checksum_low == expected {
                Ok(())
            } else {
                Err(LybaError::ChecksumMismatch(
                    ErrorContext::new(format!(
                        "section {} CRC32C mismatch: stored 0x{:08X}, computed 0x{:08X}",
                        entry.section_id.as_str(),
                        entry.checksum_low as u32,
                        expected as u32
                    ))
                    .with_byte_offset(48),
                ))
            }
        }
        _ => Ok(()),
    }
}
