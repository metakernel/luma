//! Compression codec identifiers and reader policy helpers.

use crate::error::{ErrorContext, LumbaError, Result};
use crate::section::SectionEntry;

/// Compression codec ID for uncompressed sections.
pub const CODEC_NONE: u16 = 0;
/// Reserved compression codec ID for zstd-compressed sections.
pub const CODEC_ZSTD: u16 = 1;
/// Reserved compression codec ID for deflate-compressed sections.
pub const CODEC_DEFLATE: u16 = 2;
/// Reserved compression codec ID for lz4-compressed sections.
pub const CODEC_LZ4: u16 = 3;

/// Reader action for a validated section entry's codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecReadStrategy {
    /// Decode the stored payload bytes directly.
    ReadStoredPayload,
    /// Ignore the section because it is optional and uses an unsupported codec.
    SkipOptionalSection,
}

/// Returns whether the codec is supported by this implementation.
#[must_use]
pub const fn is_supported_codec(codec_id: u16) -> bool {
    codec_id == CODEC_NONE
}

/// Returns a stable human-readable codec name.
#[must_use]
pub const fn codec_name(codec_id: u16) -> &'static str {
    match codec_id {
        CODEC_NONE => "none",
        CODEC_ZSTD => "zstd",
        CODEC_DEFLATE => "deflate",
        CODEC_LZ4 => "lz4",
        _ => "unknown",
    }
}

/// Resolves how the reader should handle a section payload.
pub fn read_section_codec_strategy(entry: SectionEntry) -> Result<CodecReadStrategy> {
    if entry.codec_id == CODEC_NONE {
        if entry.logical_size != entry.stored_size {
            return Err(LumbaError::InvalidSectionTable(ErrorContext::new(format!(
                "section {} declared codec {} ({}) but logical size {} differed from stored size {}",
                entry.section_id.as_str(),
                entry.codec_id,
                codec_name(entry.codec_id),
                entry.logical_size,
                entry.stored_size
            ))));
        }

        return Ok(CodecReadStrategy::ReadStoredPayload);
    }

    if entry.is_required() {
        return Err(LumbaError::UnsupportedCodec(ErrorContext::new(format!(
            "required section {} uses unsupported codec {} ({})",
            entry.section_id.as_str(),
            entry.codec_id,
            codec_name(entry.codec_id)
        ))));
    }

    Ok(CodecReadStrategy::SkipOptionalSection)
}

#[cfg(test)]
mod tests {
    use super::{
        CODEC_NONE, CODEC_ZSTD, CodecReadStrategy, codec_name, read_section_codec_strategy,
    };
    use crate::section::{CHECKSUM_NONE, SECTION_FLAG_REQUIRED, SectionEntry, SectionId};

    fn entry(codec_id: u16, entry_flags: u16) -> SectionEntry {
        SectionEntry {
            section_id: SectionId::DIAG,
            section_version: 1,
            entry_flags,
            payload_flags: 0,
            codec_id,
            checksum_id: CHECKSUM_NONE,
            payload_offset: 0,
            stored_size: 8,
            logical_size: 8,
            item_count: 0,
            checksum_low: 0,
            checksum_high: 0,
        }
    }

    #[test]
    fn codec_names_cover_known_ids() {
        assert_eq!(codec_name(CODEC_NONE), "none");
        assert_eq!(codec_name(CODEC_ZSTD), "zstd");
        assert_eq!(codec_name(99), "unknown");
    }

    #[test]
    fn optional_unsupported_codecs_are_skippable() {
        let strategy = read_section_codec_strategy(entry(CODEC_ZSTD, 0))
            .expect("optional unsupported codec should be skippable");

        assert_eq!(strategy, CodecReadStrategy::SkipOptionalSection);
    }

    #[test]
    fn codec_none_requires_matching_sizes() {
        let mut invalid = entry(CODEC_NONE, SECTION_FLAG_REQUIRED);
        invalid.logical_size = 16;

        let error = read_section_codec_strategy(invalid)
            .expect_err("uncompressed sections must keep stored and logical sizes equal");

        assert_eq!(error.code().as_str(), "LB0005");
    }
}
