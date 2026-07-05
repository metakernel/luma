//! Metadata section model and helpers.

use crate::error::{LumbaError, Result};
use crate::policy::Limits;
use crate::primitives::UVar;
use crate::value::{MapEntry, Value, ValueDecodeMode, decode_value_table, encode_value_table};
use crate::write::{CanonicalMode, WriterMode};
use std::collections::BTreeMap;

/// `META`
pub const META_SECTION_NAME: &str = "META";

/// Deterministic metadata map stored in the `META` section.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metadata {
    /// String-keyed metadata entries.
    pub entries: BTreeMap<String, Value>,
}

impl Metadata {
    /// Creates an empty metadata map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces a metadata entry.
    #[must_use]
    pub fn with_entry(mut self, key: impl Into<String>, value: Value) -> Self {
        self.entries.insert(key.into(), value);
        self
    }

    /// Returns a metadata entry by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    /// Converts metadata into its canonical map-value representation.
    #[must_use]
    pub fn as_map_value(&self) -> Value {
        Value::Map(
            self.entries
                .iter()
                .map(|(key, value)| MapEntry {
                    key: Value::String(key.clone()),
                    value: value.clone(),
                })
                .collect(),
        )
    }

    /// Builds deterministic runtime-data metadata for a value image.
    #[must_use]
    pub fn runtime_data_value_image() -> Self {
        Self::new()
            .with_entry("canonical", Value::Bool(true))
            .with_entry("format", Value::String(String::from("lumba")))
            .with_entry("image_kind", Value::String(String::from("value")))
            .with_entry("luma_version", Value::String(String::from("0.1")))
            .with_entry("lumba_version", Value::String(String::from("0.1")))
    }

    pub(crate) fn from_map_value(value: Value) -> Result<Self> {
        let Value::Map(entries) = value else {
            return Err(LumbaError::invalid_section_table(
                "META payload must contain exactly one map value",
            ));
        };

        let mut metadata = Self::new();
        for entry in entries {
            let Value::String(key) = entry.key else {
                return Err(LumbaError::invalid_section_table(
                    "META map keys must be strings",
                ));
            };
            metadata.entries.insert(key, entry.value);
        }
        Ok(metadata)
    }
}

pub(crate) fn encode_metadata(metadata: &Metadata, limits: &Limits) -> Result<Vec<u8>> {
    encode_value_table(
        &[metadata.as_map_value()],
        limits,
        WriterMode::Canonical(CanonicalMode::Strict),
    )
}

pub(crate) fn metadata_item_count(payload: &[u8]) -> Result<u64> {
    let mut offset = 0;
    Ok(UVar::decode(payload, &mut offset)
        .map_err(|_| {
            LumbaError::invalid_section_table("META payload started with an invalid count")
        })?
        .0)
}

pub(crate) fn decode_metadata(payload: &[u8], limits: &Limits) -> Result<Metadata> {
    let mut values = decode_value_table(payload, limits, ValueDecodeMode::Portable, 0)?;
    if values.len() != 1 {
        return Err(LumbaError::invalid_section_table(
            "META payload must contain exactly one metadata map",
        ));
    }
    Metadata::from_map_value(values.pop().expect("length checked"))
}

#[cfg(test)]
mod tests {
    use super::{Metadata, decode_metadata, encode_metadata};
    use crate::{policy::Limits, value::Value};

    #[test]
    fn metadata_round_trips_as_single_map_payload() {
        let metadata = Metadata::new()
            .with_entry("format", Value::String(String::from("lumba")))
            .with_entry("canonical", Value::Bool(true));

        let payload = encode_metadata(&metadata, &Limits::public()).expect("META should encode");
        let decoded = decode_metadata(&payload, &Limits::public()).expect("META should decode");

        assert_eq!(decoded, metadata);
    }
}
