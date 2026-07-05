//! Limits and policy knobs for readers and writers.

/// Reserved-bit handling policy for forward compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReservedFlagPolicy {
    /// Reject non-zero reserved bits and fields.
    #[default]
    Reject,
    /// Accept unknown reserved bits for future-tolerant readers.
    AllowFuture,
}

/// Trust policy for reader-side trusted-only content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TrustPolicy {
    /// Reject trusted-only content.
    #[default]
    Public,
    /// Accept trusted-only content.
    Trusted,
}

/// Policy for validating extension names against the reverse-DNS recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ExtensionNamePolicy {
    /// Accept extension names without validation.
    Allow,
    /// Preserve names but mark them as warnings in the decoded model.
    #[default]
    Warn,
    /// Reject names that are not reverse-DNS or `org.luma.*`.
    Reject,
}

/// Resource and structure limits applied during processing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Limits {
    /// Maximum accepted input size in bytes.
    pub max_document_bytes: usize,
    /// Maximum number of sections.
    pub max_sections: usize,
    /// Maximum stored payload size for any one section.
    pub max_section_payload_bytes: usize,
    /// Maximum declared decoded logical payload size for any one section.
    pub max_decoded_logical_bytes: usize,
    /// Maximum bytes a CLI renderer should display from one blob payload.
    pub max_blob_display_bytes: usize,
    /// Maximum nesting depth for composite values.
    pub max_nesting_depth: usize,
    /// Maximum record count for any one table-like section.
    pub max_table_record_count: usize,
    /// Maximum total string count.
    pub max_string_count: usize,
    /// Maximum allowed bytes for a single decoded string.
    pub max_string_bytes: usize,
    /// Maximum total value count.
    pub max_value_count: usize,
    /// Maximum total document count.
    pub max_document_count: usize,
    /// Maximum total syntax node count.
    pub max_syntax_node_count: usize,
    /// Maximum total embedded resource count.
    pub max_resource_count: usize,
    /// Maximum bytes emitted for CLI/JSON output.
    pub max_json_output_bytes: usize,
    /// Reserved flag policy.
    pub reserved_flag_policy: ReservedFlagPolicy,
    /// Trusted-only content policy.
    pub trust_policy: TrustPolicy,
    /// Extension name validation policy.
    pub extension_name_policy: ExtensionNamePolicy,
}

impl Limits {
    /// Creates limits with explicit legacy baseline values and default policy knobs.
    #[must_use]
    pub const fn new(
        max_document_bytes: usize,
        max_sections: usize,
        max_nesting_depth: usize,
    ) -> Self {
        Self {
            max_document_bytes,
            max_sections,
            max_section_payload_bytes: 2 * 1024 * 1024,
            max_decoded_logical_bytes: 16 * 1024 * 1024,
            max_blob_display_bytes: 64 * 1024,
            max_nesting_depth,
            max_table_record_count: 100_000,
            max_string_count: 200_000,
            max_string_bytes: 256 * 1024,
            max_value_count: 1_000_000,
            max_document_count: 10_000,
            max_syntax_node_count: 1_000_000,
            max_resource_count: 10_000,
            max_json_output_bytes: 8 * 1024 * 1024,
            reserved_flag_policy: ReservedFlagPolicy::Reject,
            trust_policy: TrustPolicy::Public,
            extension_name_policy: ExtensionNamePolicy::Warn,
        }
    }

    /// Strict preset for highly defensive readers and CLI use.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            max_document_bytes: 4 * 1024 * 1024,
            max_sections: 1_024,
            max_section_payload_bytes: 1 * 1024 * 1024,
            max_decoded_logical_bytes: 4 * 1024 * 1024,
            max_blob_display_bytes: 16 * 1024,
            max_nesting_depth: 32,
            max_table_record_count: 10_000,
            max_string_count: 50_000,
            max_string_bytes: 64 * 1024,
            max_value_count: 250_000,
            max_document_count: 1_000,
            max_syntax_node_count: 250_000,
            max_resource_count: 1_000,
            max_json_output_bytes: 2 * 1024 * 1024,
            reserved_flag_policy: ReservedFlagPolicy::Reject,
            trust_policy: TrustPolicy::Public,
            extension_name_policy: ExtensionNamePolicy::Warn,
        }
    }

    /// Public/default preset for normal API and CLI usage on untrusted input.
    #[must_use]
    pub const fn public() -> Self {
        Self::new(8 * 1024 * 1024, 4_096, 64)
    }

    /// Trusted preset for privileged tooling and internal pipelines.
    #[must_use]
    pub const fn trusted() -> Self {
        Self {
            max_document_bytes: 64 * 1024 * 1024,
            max_sections: 16_384,
            max_section_payload_bytes: 16 * 1024 * 1024,
            max_decoded_logical_bytes: 128 * 1024 * 1024,
            max_blob_display_bytes: 1 * 1024 * 1024,
            max_nesting_depth: 128,
            max_table_record_count: 1_000_000,
            max_string_count: 2_000_000,
            max_string_bytes: 1 * 1024 * 1024,
            max_value_count: 10_000_000,
            max_document_count: 100_000,
            max_syntax_node_count: 10_000_000,
            max_resource_count: 100_000,
            max_json_output_bytes: 64 * 1024 * 1024,
            reserved_flag_policy: ReservedFlagPolicy::AllowFuture,
            trust_policy: TrustPolicy::Trusted,
            extension_name_policy: ExtensionNamePolicy::Warn,
        }
    }

    /// Returns whether trusted-only content is allowed.
    #[must_use]
    pub const fn allows_trusted_only(&self) -> bool {
        matches!(self.trust_policy, TrustPolicy::Trusted)
    }

    /// Returns whether unknown reserved bits should be tolerated.
    #[must_use]
    pub const fn is_future_tolerant(&self) -> bool {
        matches!(self.reserved_flag_policy, ReservedFlagPolicy::AllowFuture)
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::public()
    }
}

#[cfg(test)]
mod tests {
    use super::{Limits, ReservedFlagPolicy, TrustPolicy};

    #[test]
    fn default_matches_public_preset() {
        assert_eq!(Limits::default(), Limits::public());
    }

    #[test]
    fn policy_presets_set_expected_policy_knobs() {
        let strict = Limits::strict();
        let public = Limits::public();
        let trusted = Limits::trusted();

        assert_eq!(strict.reserved_flag_policy, ReservedFlagPolicy::Reject);
        assert_eq!(public.reserved_flag_policy, ReservedFlagPolicy::Reject);
        assert_eq!(
            trusted.reserved_flag_policy,
            ReservedFlagPolicy::AllowFuture
        );

        assert_eq!(strict.trust_policy, TrustPolicy::Public);
        assert_eq!(public.trust_policy, TrustPolicy::Public);
        assert_eq!(trusted.trust_policy, TrustPolicy::Trusted);
        assert_eq!(
            strict.extension_name_policy,
            super::ExtensionNamePolicy::Warn
        );
        assert_eq!(
            public.extension_name_policy,
            super::ExtensionNamePolicy::Warn
        );
        assert_eq!(
            trusted.extension_name_policy,
            super::ExtensionNamePolicy::Warn
        );
        assert!(strict.max_document_bytes < trusted.max_document_bytes);
        assert!(public.max_json_output_bytes <= trusted.max_json_output_bytes);
    }
}
