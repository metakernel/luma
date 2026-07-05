//! Deserialization entry points and adapter types.

use luma_syntax::{
    LumaKey, LumaMappingEntry, LumaNumber, LumaSequence, LumaTaggedValue, LumaValue,
};
use serde::Deserializer as _;
use serde::de::{self, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::{Deserialize, forward_to_deserialize_any};

use crate::{Error, Result};

/// Serde deserializer adapter that reads from a borrowed Luma value.
#[derive(Debug, Clone, Copy)]
pub struct ValueDeserializer<'de> {
    value: &'de LumaValue,
}

impl<'de> ValueDeserializer<'de> {
    /// Creates a new deserializer adapter for a borrowed Luma value.
    #[must_use]
    pub const fn new(value: &'de LumaValue) -> Self {
        Self { value }
    }

    /// Returns the underlying borrowed Luma value.
    #[must_use]
    pub const fn value(self) -> &'de LumaValue {
        self.value
    }

    fn runtime_only_error(kind: &'static str, value: &luma_syntax::LumaHostValue) -> Error {
        Error::custom(match &value.label {
            Some(label) => format!(
                "cannot deserialize runtime-only Luma {kind} value `{}` ({label})",
                value.kind
            ),
            None => format!(
                "cannot deserialize runtime-only Luma {kind} value `{}`",
                value.kind
            ),
        })
    }

    fn key_error() -> Error {
        Error::custom("cannot deserialize runtime-only Luma host mapping key")
    }

    fn invalid_type(unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Error {
        de::Error::invalid_type(unexpected, expected)
    }

    fn invalid_value(unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Error {
        de::Error::invalid_value(unexpected, expected)
    }

    fn mismatch_error(value: &LumaValue, expected: &dyn de::Expected) -> Error {
        match value {
            LumaValue::Function(value) => Self::runtime_only_error("function", value),
            LumaValue::UserData(value) => Self::runtime_only_error("userdata", value),
            LumaValue::HostObject(value) => Self::runtime_only_error("host object", value),
            other => Self::invalid_type(unexpected_value(other), expected),
        }
    }

    fn deserialize_scalar<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.value {
            LumaValue::Null(_) => visitor.visit_unit(),
            LumaValue::Boolean(value) => visitor.visit_bool(*value),
            LumaValue::Number(LumaNumber::Integer(value)) => visitor.visit_i64(*value),
            LumaValue::Number(LumaNumber::Float(value)) => visitor.visit_f64(*value),
            LumaValue::String(value) => visitor.visit_borrowed_str(value),
            LumaValue::Sequence(sequence) => {
                visitor.visit_seq(SequenceAccess::new(&sequence.items))
            }
            LumaValue::Mapping(mapping) => visitor.visit_map(MappingAccess::new(&mapping.entries)),
            LumaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_any(visitor)
            }
            LumaValue::Function(value) => Err(Self::runtime_only_error("function", value)),
            LumaValue::UserData(value) => Err(Self::runtime_only_error("userdata", value)),
            LumaValue::HostObject(value) => Err(Self::runtime_only_error("host object", value)),
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
            LumaValue::Number(LumaNumber::Integer(value)) => visit(visitor, *value),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::Number(LumaNumber::Integer(value)) => visitor.visit_f64(*value as f64),
            LumaValue::Number(LumaNumber::Float(value)) => visitor.visit_f64(*value),
            LumaValue::Tagged(tagged) => {
                ValueDeserializer::new(&tagged.value).deserialize_float(visitor)
            }
            other => Err(Self::mismatch_error(other, &"a number")),
        }
    }
}

/// Converts a borrowed Luma value into a Serde-deserializable Rust value.
///
/// Tagged values are transparent for non-enum targets: the tag wrapper is
/// ignored and the payload is deserialized directly. For enum targets, tagged
/// values are treated as externally tagged variants where the Luma tag name is
/// the variant name and the tagged payload is the variant content.
///
/// # Errors
///
/// Returns an error when the Luma value shape does not match the requested
/// Serde data model or when the value contains runtime-only Luma values.
pub fn from_value<T>(value: &LumaValue) -> Result<T>
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
            LumaValue::Boolean(value) => visitor.visit_bool(*value),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::String(value) => {
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => visitor.visit_char(ch),
                    _ => Err(Error::custom("expected a single-character string")),
                }
            }
            LumaValue::Tagged(tagged) => {
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
            LumaValue::String(value) => visitor.visit_borrowed_str(value),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::Null(_) => visitor.visit_none(),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::Null(_) => visitor.visit_unit(),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::Sequence(LumaSequence { items, .. }) => {
                visitor.visit_seq(SequenceAccess::new(items))
            }
            LumaValue::Tagged(tagged) => {
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
            LumaValue::Mapping(mapping) => visitor.visit_map(MappingAccess::new(&mapping.entries)),
            LumaValue::Tagged(tagged) => {
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
            LumaValue::String(variant) => {
                visitor.visit_enum(EnumValueAccess::unit(variant.as_str()))
            }
            LumaValue::Mapping(mapping) if mapping.entries.len() == 1 => {
                let entry = &mapping.entries[0];
                let LumaKey::String(variant) = &entry.key else {
                    return Err(Self::invalid_value(
                        unexpected_key(&entry.key),
                        &"a string enum variant name",
                    ));
                };
                visitor.visit_enum(EnumValueAccess::value(variant.as_str(), &entry.value))
            }
            LumaValue::Tagged(LumaTaggedValue { tag, value, .. }) => {
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
    items: &'de [LumaValue],
    index: usize,
}

impl<'de> SequenceAccess<'de> {
    const fn new(items: &'de [LumaValue]) -> Self {
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
    entries: &'de [LumaMappingEntry],
    index: usize,
    pending_value: Option<&'de LumaValue>,
}

impl<'de> MappingAccess<'de> {
    const fn new(entries: &'de [LumaMappingEntry]) -> Self {
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
    value: Option<&'de LumaValue>,
}

impl<'de> EnumValueAccess<'de> {
    const fn unit(variant: &'de str) -> Self {
        Self {
            variant,
            value: None,
        }
    }

    const fn value(variant: &'de str, value: &'de LumaValue) -> Self {
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
            None => Ok(()),
            Some(LumaValue::Null(_)) => Ok(()),
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
    key: &'de LumaKey,
}

impl<'de> KeyDeserializer<'de> {
    const fn new(key: &'de LumaKey) -> Self {
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
            LumaKey::String(value) => visitor.visit_borrowed_str(value),
            LumaKey::Number(LumaNumber::Integer(value)) => visitor.visit_i64(*value),
            LumaKey::Number(LumaNumber::Float(value)) => visitor.visit_f64(*value),
            LumaKey::Boolean(value) => visitor.visit_bool(*value),
            LumaKey::Host(_) => Err(ValueDeserializer::key_error()),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.key {
            LumaKey::Boolean(value) => visitor.visit_bool(*value),
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
            LumaKey::Number(LumaNumber::Integer(value)) => visitor.visit_i64(*value),
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
            LumaKey::Number(LumaNumber::Integer(value)) => {
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
            LumaKey::Number(LumaNumber::Integer(value)) => visitor.visit_f64(*value as f64),
            LumaKey::Number(LumaNumber::Float(value)) => visitor.visit_f64(*value),
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
            LumaKey::String(value) => {
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
            LumaKey::String(value) => visitor.visit_borrowed_str(value),
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
            LumaKey::String(value) => visitor.visit_enum(EnumValueAccess::unit(value)),
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

fn value_kind(value: &LumaValue) -> &'static str {
    match value {
        LumaValue::Null(_) => "null",
        LumaValue::Boolean(_) => "boolean",
        LumaValue::Number(LumaNumber::Integer(_)) => "integer",
        LumaValue::Number(LumaNumber::Float(_)) => "float",
        LumaValue::String(_) => "string",
        LumaValue::Sequence(_) => "sequence",
        LumaValue::Mapping(_) => "mapping",
        LumaValue::Tagged(_) => "tagged value",
        LumaValue::Function(_) => "function",
        LumaValue::UserData(_) => "userdata",
        LumaValue::HostObject(_) => "host object",
    }
}

fn unexpected_value(value: &LumaValue) -> de::Unexpected<'_> {
    match value {
        LumaValue::Null(_) => de::Unexpected::Unit,
        LumaValue::Boolean(value) => de::Unexpected::Bool(*value),
        LumaValue::Number(LumaNumber::Integer(value)) => de::Unexpected::Signed(*value),
        LumaValue::Number(LumaNumber::Float(value)) => de::Unexpected::Float(*value),
        LumaValue::String(value) => de::Unexpected::Str(value),
        LumaValue::Sequence(_) => de::Unexpected::Seq,
        LumaValue::Mapping(_) => de::Unexpected::Map,
        LumaValue::Tagged(_) => de::Unexpected::Other("tagged value"),
        LumaValue::Function(_) => de::Unexpected::Other("runtime-only function"),
        LumaValue::UserData(_) => de::Unexpected::Other("runtime-only userdata"),
        LumaValue::HostObject(_) => de::Unexpected::Other("runtime-only host object"),
    }
}

fn unexpected_key(key: &LumaKey) -> de::Unexpected<'_> {
    match key {
        LumaKey::String(value) => de::Unexpected::Str(value),
        LumaKey::Number(LumaNumber::Integer(value)) => de::Unexpected::Signed(*value),
        LumaKey::Number(LumaNumber::Float(value)) => de::Unexpected::Float(*value),
        LumaKey::Boolean(value) => de::Unexpected::Bool(*value),
        LumaKey::Host(_) => de::Unexpected::Other("runtime-only host key"),
    }
}

#[cfg(test)]
mod tests {
    use luma_syntax::{
        LumaHostValue, LumaKey, LumaMapping, LumaMappingEntry, LumaNull, LumaNumber, LumaSequence,
        LumaTag, LumaTagName, LumaTaggedValue, LumaValue, Span,
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
        let value = LumaValue::Mapping(LumaMapping {
            entries: vec![
                LumaMappingEntry {
                    key: LumaKey::String("name".to_owned()),
                    value: LumaValue::String("demo".to_owned()),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("enabled".to_owned()),
                    value: LumaValue::Boolean(true),
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
        let value = LumaValue::Mapping(LumaMapping {
            entries: vec![
                LumaMappingEntry {
                    key: LumaKey::String("flag".to_owned()),
                    value: LumaValue::Boolean(true),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("count".to_owned()),
                    value: LumaValue::Number(LumaNumber::Integer(7)),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("letter".to_owned()),
                    value: LumaValue::String("Z".to_owned()),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("ratio".to_owned()),
                    value: LumaValue::Number(LumaNumber::Float(1.5)),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("maybe".to_owned()),
                    value: LumaValue::Number(LumaNumber::Integer(9)),
                    span: None,
                },
                LumaMappingEntry {
                    key: LumaKey::String("values".to_owned()),
                    value: LumaValue::Sequence(LumaSequence {
                        items: vec![
                            LumaValue::Number(LumaNumber::Integer(1)),
                            LumaValue::Number(LumaNumber::Integer(2)),
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
        let value = LumaValue::Sequence(LumaSequence {
            items: vec![
                LumaValue::Number(LumaNumber::Integer(1)),
                LumaValue::Number(LumaNumber::Integer(2)),
                LumaValue::Number(LumaNumber::Integer(3)),
            ],
            span: None,
        });

        assert_eq!(from_value::<Vec<i32>>(&value).unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn deserializes_null_into_option_and_unit() {
        let value = LumaValue::Null(LumaNull);

        assert_eq!(from_value::<Option<i32>>(&value).unwrap(), None);
        assert_eq!(from_value::<()>(&value).unwrap(), ());
    }

    #[test]
    fn deserializes_enum_from_string_mapping_and_tagged_values() {
        assert_eq!(
            from_value::<ExampleEnum>(&LumaValue::String("Unit".to_owned())).unwrap(),
            ExampleEnum::Unit
        );

        let tuple = LumaValue::Mapping(LumaMapping {
            entries: vec![LumaMappingEntry {
                key: LumaKey::String("Tuple".to_owned()),
                value: LumaValue::Sequence(LumaSequence {
                    items: vec![
                        LumaValue::Number(LumaNumber::Integer(1)),
                        LumaValue::String("two".to_owned()),
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

        let tagged = LumaValue::Tagged(LumaTaggedValue {
            tag: LumaTag {
                name: LumaTagName {
                    value: "Struct".to_owned(),
                },
                span: Span::new(Default::default(), 0, 0),
            },
            value: Box::new(LumaValue::Mapping(LumaMapping {
                entries: vec![LumaMappingEntry {
                    key: LumaKey::String("count".to_owned()),
                    value: LumaValue::Number(LumaNumber::Integer(2)),
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

        let transparent = LumaValue::Tagged(LumaTaggedValue {
            tag: LumaTag {
                name: LumaTagName {
                    value: "Ignored".to_owned(),
                },
                span: Span::new(Default::default(), 0, 0),
            },
            value: Box::new(LumaValue::Boolean(true)),
            span: None,
        });
        assert_eq!(from_value::<bool>(&transparent).unwrap(), true);
    }

    #[test]
    fn runtime_only_values_return_clear_errors() {
        let value = LumaValue::Function(LumaHostValue {
            kind: "lua.function".to_owned(),
            label: Some("handler".to_owned()),
        });

        let error = from_value::<bool>(&value).unwrap_err();
        assert_eq!(
            error.to_string(),
            "cannot deserialize runtime-only Luma function value `lua.function` (handler)"
        );
    }

    #[test]
    fn reports_expected_type_and_range_errors() {
        let bool_error =
            from_value::<bool>(&LumaValue::Number(LumaNumber::Integer(1))).unwrap_err();
        assert_eq!(
            bool_error.to_string(),
            "invalid type: integer `1`, expected a boolean"
        );

        let range_error =
            from_value::<u8>(&LumaValue::Number(LumaNumber::Integer(-1))).unwrap_err();
        assert_eq!(
            range_error,
            Error::Custom("integer out of range for target type".to_owned())
        );

        let char_error = from_value::<char>(&LumaValue::String("no".to_owned())).unwrap_err();
        assert_eq!(
            char_error,
            Error::Custom("expected a single-character string".to_owned())
        );
    }

    #[test]
    fn rejects_non_string_enum_variant_keys() {
        let value = LumaValue::Mapping(LumaMapping {
            entries: vec![LumaMappingEntry {
                key: LumaKey::Boolean(true),
                value: LumaValue::Null(LumaNull),
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
