//! Deserialization entry points and adapter types.

use lyma_syntax::{
    LymaKey, LymaMappingEntry, LymaNumber, LymaSequence, LymaTaggedValue, LymaValue,
};
use serde::Deserializer as _;
use serde::de::{self, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::{Deserialize, forward_to_deserialize_any};

use crate::{Error, Result};

/// Serde deserializer adapter that reads from a borrowed Lyma value.
#[derive(Debug, Clone, Copy)]
pub struct ValueDeserializer<'de> {
    value: &'de LymaValue,
}

impl<'de> ValueDeserializer<'de> {
    /// Creates a new deserializer adapter for a borrowed Lyma value.
    #[must_use]
    pub const fn new(value: &'de LymaValue) -> Self {
        Self { value }
    }

    /// Returns the underlying borrowed Lyma value.
    #[must_use]
    pub const fn value(self) -> &'de LymaValue {
        self.value
    }

    fn runtime_only_error(kind: &'static str, value: &lyma_syntax::LymaHostValue) -> Error {
        Error::custom(value.label.as_ref().map_or_else(
            || {
                format!(
                    "cannot deserialize runtime-only Lyma {kind} value `{}`",
                    value.kind
                )
            },
            |label| {
                format!(
                    "cannot deserialize runtime-only Lyma {kind} value `{}` ({label})",
                    value.kind
                )
            },
        ))
    }

    fn key_error() -> Error {
        Error::custom("cannot deserialize runtime-only Lyma host mapping key")
    }

    fn invalid_type(unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Error {
        de::Error::invalid_type(unexpected, expected)
    }

    fn invalid_value(unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Error {
        de::Error::invalid_value(unexpected, expected)
    }

    fn mismatch_error(value: &LymaValue, expected: &dyn de::Expected) -> Error {
        match value {
            LymaValue::Function(value) => Self::runtime_only_error("function", value),
            LymaValue::UserData(value) => Self::runtime_only_error("userdata", value),
            LymaValue::HostObject(value) => Self::runtime_only_error("host object", value),
            other => Self::invalid_type(unexpected_value(other), expected),
        }
    }

    fn deserialize_scalar<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Null(_) => visitor.visit_unit(),
            LymaValue::Boolean(value) => visitor.visit_bool(*value),
            LymaValue::Number(LymaNumber::Integer(value)) => visitor.visit_i64(*value),
            LymaValue::Number(LymaNumber::Float(value)) => visitor.visit_f64(*value),
            LymaValue::String(value) => visitor.visit_borrowed_str(value),
            LymaValue::Sequence(sequence) => {
                visitor.visit_seq(SequenceAccess::new(&sequence.items))
            }
            LymaValue::Mapping(mapping) => visitor.visit_map(MappingAccess::new(&mapping.entries)),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_any(visitor)
            }
            LymaValue::Function(value) => Err(Self::runtime_only_error("function", value)),
            LymaValue::UserData(value) => Err(Self::runtime_only_error("userdata", value)),
            LymaValue::HostObject(value) => Err(Self::runtime_only_error("host object", value)),
        }
    }

    fn deserialize_integer<V>(
        self,
        visitor: V,
        expected: &'static str,
        visit: impl FnOnce(V, i64) -> Result<V::Value>,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Number(LymaNumber::Integer(value)) => visit(visitor, *value),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_integer(visitor, expected, visit)
            }
            other => Err(Self::mismatch_error(other, &expected)),
        }
    }

    fn deserialize_float<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Number(LymaNumber::Integer(value)) => {
                visitor.visit_f64(integer_to_f64(*value)?)
            }
            LymaValue::Number(LymaNumber::Float(value)) => visitor.visit_f64(*value),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_float(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a number")),
        }
    }
}

/// Converts a borrowed Lyma value into a Serde-deserializable Rust value.
///
/// Tagged values are transparent for non-enum targets: the tag wrapper is
/// ignored and the payload is deserialized directly. For enum targets, tagged
/// values are treated as externally tagged variants where the Lyma tag name is
/// the variant name and the tagged payload is the variant content.
///
/// # Errors
///
/// Returns an error when the Lyma value shape does not match the requested
/// Serde data model or when the value contains runtime-only Lyma values.
pub fn from_value<T>(value: &LymaValue) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    T::deserialize(ValueDeserializer::new(value))
}

impl<'de> serde::Deserializer<'de> for ValueDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_scalar(visitor)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Boolean(value) => visitor.visit_bool(*value),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_bool(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a boolean")),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an integer", |visitor, value| {
            visitor.visit_i8(
                i8::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an integer", |visitor, value| {
            visitor.visit_i16(
                i16::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an integer", |visitor, value| {
            visitor.visit_i32(
                i32::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an integer", |visitor, value| {
            visitor.visit_i64(value)
        })
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an integer", |visitor, value| {
            visitor.visit_i128(i128::from(value))
        })
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an unsigned integer", |visitor, value| {
            visitor.visit_u8(
                u8::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an unsigned integer", |visitor, value| {
            visitor.visit_u16(
                u16::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an unsigned integer", |visitor, value| {
            visitor.visit_u32(
                u32::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an unsigned integer", |visitor, value| {
            visitor.visit_u64(
                u64::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_integer(visitor, "an unsigned integer", |visitor, value| {
            visitor.visit_u128(
                u128::try_from(value)
                    .map_err(|_| Error::custom("integer out of range for target type"))?,
            )
        })
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_float(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_float(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::custom("expected a single-character string")),
                }
            }
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_char(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a string")),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::String(value) => visitor.visit_borrowed_str(value),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_str(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a string")),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::unsupported("byte slices"))
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::unsupported("byte buffers"))
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Null(_) => visitor.visit_none(),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_option(visitor)
            }
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Null(_) => visitor.visit_unit(),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_unit(visitor)
            }
            other => Err(Self::mismatch_error(other, &"null")),
        }
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Sequence(LymaSequence { items, .. }) => {
                visitor.visit_seq(SequenceAccess::new(items))
            }
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_seq(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a sequence")),
        }
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::Mapping(mapping) => visitor.visit_map(MappingAccess::new(&mapping.entries)),
            LymaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_map(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a mapping")),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LymaValue::String(variant) => {
                visitor.visit_enum(EnumValueAccess::unit(variant.as_str()))
            }
            LymaValue::Mapping(mapping) if mapping.entries.len() == 1 => {
                let entry = &mapping.entries[0];
                let LymaKey::String(variant) = &entry.key else {
                    return Err(Self::invalid_value(
                        unexpected_key(&entry.key),
                        &"a string enum variant name",
                    ));
                };
                visitor.visit_enum(EnumValueAccess::value(variant.as_str(), &entry.value))
            }
            LymaValue::Tagged(LymaTaggedValue { tag, value, .. }) => {
                visitor.visit_enum(EnumValueAccess::value(tag.name.value.as_str(), value))
            }
            other => Err(Self::mismatch_error(
                other,
                &"a string, single-entry mapping, or tagged value representing an enum",
            )),
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }
}

#[derive(Debug, Clone, Copy)]
struct SequenceAccess<'de> {
    items: &'de [LymaValue],
    index: usize,
}

impl<'de> SequenceAccess<'de> {
    const fn new(items: &'de [LymaValue]) -> Self {
        Self { items, index: 0 }
    }
}

impl<'de> SeqAccess<'de> for SequenceAccess<'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(value) = self.items.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        seed.deserialize(ValueDeserializer::new(value)).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len().saturating_sub(self.index))
    }
}

#[derive(Debug, Clone, Copy)]
struct MappingAccess<'de> {
    entries: &'de [LymaMappingEntry],
    index: usize,
    pending_value: Option<&'de LymaValue>,
}

impl<'de> MappingAccess<'de> {
    const fn new(entries: &'de [LymaMappingEntry]) -> Self {
        Self {
            entries,
            index: 0,
            pending_value: None,
        }
    }
}

impl<'de> MapAccess<'de> for MappingAccess<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: de::DeserializeSeed<'de>,
    {
        let Some(entry) = self.entries.get(self.index) else {
            return Ok(None);
        };
        self.index += 1;
        self.pending_value = Some(&entry.value);
        seed.deserialize(KeyDeserializer::new(&entry.key)).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: de::DeserializeSeed<'de>,
    {
        let value = self
            .pending_value
            .take()
            .ok_or_else(|| Error::custom("map value requested before key"))?;
        seed.deserialize(ValueDeserializer::new(value))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len().saturating_sub(self.index))
    }
}

#[derive(Debug, Clone, Copy)]
struct EnumValueAccess<'de> {
    variant: &'de str,
    value: Option<&'de LymaValue>,
}

impl<'de> EnumValueAccess<'de> {
    const fn unit(variant: &'de str) -> Self {
        Self {
            variant,
            value: None,
        }
    }

    const fn value(variant: &'de str, value: &'de LymaValue) -> Self {
        Self {
            variant,
            value: Some(value),
        }
    }
}

impl<'de> EnumAccess<'de> for EnumValueAccess<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize(serde::de::value::StrDeserializer::<Error>::new(
            self.variant,
        ))?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for EnumValueAccess<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        match self.value {
            Some(LymaValue::Null(_)) | None => Ok(()),
            Some(other) => Err(Error::custom(format!(
                "expected unit variant payload to be null, got {}",
                value_kind(other)
            ))),
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: de::DeserializeSeed<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::custom("expected enum variant payload"))?;
        seed.deserialize(ValueDeserializer::new(value))
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::custom("expected tuple variant payload"))?;
        ValueDeserializer::new(value).deserialize_seq(visitor)
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let value = self
            .value
            .ok_or_else(|| Error::custom("expected struct variant payload"))?;
        ValueDeserializer::new(value).deserialize_map(visitor)
    }
}

#[derive(Debug, Clone, Copy)]
struct KeyDeserializer<'de> {
    key: &'de LymaKey,
}

impl<'de> KeyDeserializer<'de> {
    const fn new(key: &'de LymaKey) -> Self {
        Self { key }
    }
}

impl<'de> serde::Deserializer<'de> for KeyDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::String(value) => visitor.visit_borrowed_str(value),
            LymaKey::Number(LymaNumber::Integer(value)) => visitor.visit_i64(*value),
            LymaKey::Number(LymaNumber::Float(value)) => visitor.visit_f64(*value),
            LymaKey::Boolean(value) => visitor.visit_bool(*value),
            LymaKey::Host(_) => Err(ValueDeserializer::key_error()),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::Boolean(value) => visitor.visit_bool(*value),
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"a boolean key",
            )),
        }
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::Number(LymaNumber::Integer(value)) => visitor.visit_i64(*value),
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"an integer key",
            )),
        }
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::Number(LymaNumber::Integer(value)) => {
                let value = u64::try_from(*value)
                    .map_err(|_| Error::custom("integer key out of range for target type"))?;
                visitor.visit_u64(value)
            }
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"an unsigned integer key",
            )),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::Number(LymaNumber::Integer(value)) => {
                visitor.visit_f64(integer_to_f64(*value)?)
            }
            LymaKey::Number(LymaNumber::Float(value)) => visitor.visit_f64(*value),
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"a numeric key",
            )),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::custom("expected a single-character string key")),
                }
            }
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"a string key",
            )),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::String(value) => visitor.visit_borrowed_str(value),
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"a string key",
            )),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LymaKey::String(value) => visitor.visit_enum(EnumValueAccess::unit(value)),
            other => Err(ValueDeserializer::invalid_type(
                unexpected_key(other),
                &"a string key",
            )),
        }
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        i8 i16 i32 i128 u8 u16 u32 u128 f32 bytes byte_buf option unit unit_struct
        newtype_struct seq tuple tuple_struct map struct
    }
}

fn integer_to_f64(value: i64) -> Result<f64> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|error| Error::custom(format!("failed to convert integer to float: {error}")))
}

const fn value_kind(value: &LymaValue) -> &'static str {
    match value {
        LymaValue::Null(_) => "null",
        LymaValue::Boolean(_) => "boolean",
        LymaValue::Number(LymaNumber::Integer(_)) => "integer",
        LymaValue::Number(LymaNumber::Float(_)) => "float",
        LymaValue::String(_) => "string",
        LymaValue::Sequence(_) => "sequence",
        LymaValue::Mapping(_) => "mapping",
        LymaValue::Tagged(_) => "tagged value",
        LymaValue::Function(_) => "function",
        LymaValue::UserData(_) => "userdata",
        LymaValue::HostObject(_) => "host object",
    }
}

fn unexpected_value(value: &LymaValue) -> de::Unexpected<'_> {
    match value {
        LymaValue::Null(_) => de::Unexpected::Unit,
        LymaValue::Boolean(value) => de::Unexpected::Bool(*value),
        LymaValue::Number(LymaNumber::Integer(value)) => de::Unexpected::Signed(*value),
        LymaValue::Number(LymaNumber::Float(value)) => de::Unexpected::Float(*value),
        LymaValue::String(value) => de::Unexpected::Str(value),
        LymaValue::Sequence(_) => de::Unexpected::Seq,
        LymaValue::Mapping(_) => de::Unexpected::Map,
        LymaValue::Tagged(_) => de::Unexpected::Other("tagged value"),
        LymaValue::Function(_) => de::Unexpected::Other("runtime-only function"),
        LymaValue::UserData(_) => de::Unexpected::Other("runtime-only userdata"),
        LymaValue::HostObject(_) => de::Unexpected::Other("runtime-only host object"),
    }
}

fn unexpected_key(key: &LymaKey) -> de::Unexpected<'_> {
    match key {
        LymaKey::String(value) => de::Unexpected::Str(value),
        LymaKey::Number(LymaNumber::Integer(value)) => de::Unexpected::Signed(*value),
        LymaKey::Number(LymaNumber::Float(value)) => de::Unexpected::Float(*value),
        LymaKey::Boolean(value) => de::Unexpected::Bool(*value),
        LymaKey::Host(_) => de::Unexpected::Other("runtime-only host key"),
    }
}

#[cfg(test)]
mod tests {
    use lyma_syntax::{
        FileId, LymaHostValue, LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber,
        LymaSequence, LymaTag, LymaTagName, LymaTaggedValue, LymaValue, Span,
    };
    use serde::Deserialize;

    use super::from_value;
    use crate::Error;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct ExampleStruct {
        name: String,
        enabled: bool,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct PrimitiveStruct {
        flag: bool,
        count: u8,
        letter: char,
        ratio: f64,
        maybe: Option<i32>,
        values: Vec<i32>,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    enum ExampleEnum {
        Unit,
        Tuple(i32, String),
        Struct { count: u8 },
        Newtype(bool),
    }

    #[test]
    fn deserializes_mapping_into_struct() {
        let value = LymaValue::Mapping(LymaMapping {
            entries: vec![
                LymaMappingEntry {
                    key: LymaKey::String("name".to_owned()),
                    value: LymaValue::String("demo".to_owned()),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("enabled".to_owned()),
                    value: LymaValue::Boolean(true),
                    span: None,
                },
            ],
            duplicate_keys: Vec::new(),
            span: None,
        });

        assert_eq!(
            from_value::<ExampleStruct>(&value).unwrap(),
            ExampleStruct {
                name: "demo".to_owned(),
                enabled: true,
            }
        );
    }

    #[test]
    fn deserializes_primitive_fields_options_and_sequences_into_struct() {
        let value = LymaValue::Mapping(LymaMapping {
            entries: vec![
                LymaMappingEntry {
                    key: LymaKey::String("flag".to_owned()),
                    value: LymaValue::Boolean(true),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("count".to_owned()),
                    value: LymaValue::Number(LymaNumber::Integer(7)),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("letter".to_owned()),
                    value: LymaValue::String("Z".to_owned()),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("ratio".to_owned()),
                    value: LymaValue::Number(LymaNumber::Float(1.5)),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("maybe".to_owned()),
                    value: LymaValue::Number(LymaNumber::Integer(9)),
                    span: None,
                },
                LymaMappingEntry {
                    key: LymaKey::String("values".to_owned()),
                    value: LymaValue::Sequence(LymaSequence {
                        items: vec![
                            LymaValue::Number(LymaNumber::Integer(1)),
                            LymaValue::Number(LymaNumber::Integer(2)),
                        ],
                        span: None,
                    }),
                    span: None,
                },
            ],
            duplicate_keys: Vec::new(),
            span: None,
        });

        assert_eq!(
            from_value::<PrimitiveStruct>(&value).unwrap(),
            PrimitiveStruct {
                flag: true,
                count: 7,
                letter: 'Z',
                ratio: 1.5,
                maybe: Some(9),
                values: vec![1, 2],
            }
        );
    }

    #[test]
    fn deserializes_sequence_into_vec() {
        let value = LymaValue::Sequence(LymaSequence {
            items: vec![
                LymaValue::Number(LymaNumber::Integer(1)),
                LymaValue::Number(LymaNumber::Integer(2)),
                LymaValue::Number(LymaNumber::Integer(3)),
            ],
            span: None,
        });

        assert_eq!(from_value::<Vec<i32>>(&value).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn deserializes_null_into_option_and_unit() {
        let value = LymaValue::Null(LymaNull);

        assert_eq!(from_value::<Option<i32>>(&value).unwrap(), None);
        from_value::<()>(&value).unwrap();
    }

    #[test]
    fn deserializes_enum_from_string_mapping_and_tagged_values() {
        assert_eq!(
            from_value::<ExampleEnum>(&LymaValue::String("Unit".to_owned())).unwrap(),
            ExampleEnum::Unit
        );

        let tuple = LymaValue::Mapping(LymaMapping {
            entries: vec![LymaMappingEntry {
                key: LymaKey::String("Tuple".to_owned()),
                value: LymaValue::Sequence(LymaSequence {
                    items: vec![
                        LymaValue::Number(LymaNumber::Integer(1)),
                        LymaValue::String("two".to_owned()),
                    ],
                    span: None,
                }),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        });
        assert_eq!(
            from_value::<ExampleEnum>(&tuple).unwrap(),
            ExampleEnum::Tuple(1, "two".to_owned())
        );

        let tagged = LymaValue::Tagged(LymaTaggedValue {
            tag: LymaTag {
                name: LymaTagName {
                    value: "Struct".to_owned(),
                    span: Span::new(FileId::default(), 1, 7),
                },
                span: Span::new(FileId::default(), 0, 0),
            },
            value: Box::new(LymaValue::Mapping(LymaMapping {
                entries: vec![LymaMappingEntry {
                    key: LymaKey::String("count".to_owned()),
                    value: LymaValue::Number(LymaNumber::Integer(2)),
                    span: None,
                }],
                duplicate_keys: Vec::new(),
                span: None,
            })),
            span: None,
        });
        assert_eq!(
            from_value::<ExampleEnum>(&tagged).unwrap(),
            ExampleEnum::Struct { count: 2 }
        );

        let transparent = LymaValue::Tagged(LymaTaggedValue {
            tag: LymaTag {
                name: LymaTagName {
                    value: "Ignored".to_owned(),
                    span: Span::new(FileId::default(), 1, 8),
                },
                span: Span::new(FileId::default(), 0, 0),
            },
            value: Box::new(LymaValue::Boolean(true)),
            span: None,
        });
        assert!(from_value::<bool>(&transparent).unwrap());
    }

    #[test]
    fn runtime_only_values_return_clear_errors() {
        let value = LymaValue::Function(LymaHostValue {
            kind: "lua.function".to_owned(),
            label: Some("handler".to_owned()),
        });

        let error = from_value::<bool>(&value).unwrap_err();
        assert_eq!(
            error.to_string(),
            "cannot deserialize runtime-only Lyma function value `lua.function` (handler)"
        );
    }

    #[test]
    fn reports_expected_type_and_range_errors() {
        let bool_error =
            from_value::<bool>(&LymaValue::Number(LymaNumber::Integer(1))).unwrap_err();
        assert_eq!(
            bool_error.to_string(),
            "invalid type: integer `1`, expected a boolean"
        );

        let range_error =
            from_value::<u8>(&LymaValue::Number(LymaNumber::Integer(-1))).unwrap_err();
        assert_eq!(
            range_error,
            Error::Custom("integer out of range for target type".to_owned())
        );

        let char_error = from_value::<char>(&LymaValue::String("no".to_owned())).unwrap_err();
        assert_eq!(
            char_error,
            Error::Custom("expected a single-character string".to_owned())
        );
    }

    #[test]
    fn rejects_non_string_enum_variant_keys() {
        let value = LymaValue::Mapping(LymaMapping {
            entries: vec![LymaMappingEntry {
                key: LymaKey::Boolean(true),
                value: LymaValue::Null(LymaNull),
                span: None,
            }],
            duplicate_keys: Vec::new(),
            span: None,
        });

        let error = from_value::<ExampleEnum>(&value).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid value: boolean `true`, expected a string enum variant name"
        );
    }
}
