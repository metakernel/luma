//! Serialization entry points and adapter types.

use lyma_syntax::{
    LymaKey, LymaMapping, LymaMappingEntry, LymaNull, LymaNumber, LymaSequence, LymaValue,
    SerializeOptions, serialize_value, serialize_value_with_options,
};
use serde::{
    Serialize,
    ser::{
        self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
        SerializeTupleStruct, SerializeTupleVariant,
    },
};

use crate::{Error, Result};

/// Serde serializer adapter that produces Lyma values.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueSerializer;

impl ValueSerializer {
    /// Creates a new serializer adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    const fn unsupported_named(operation: &'static str) -> Error {
        Error::unsupported(operation)
    }

    fn serialize_signed(value: i128) -> Result<LymaValue> {
        let value = i64::try_from(value)
            .map_err(|_| Self::unsupported_named("integer outside Lyma i64 range"))?;
        Ok(LymaValue::Number(LymaNumber::Integer(value)))
    }

    fn serialize_unsigned(value: u128) -> Result<LymaValue> {
        let value = i64::try_from(value)
            .map_err(|_| Self::unsupported_named("integer outside Lyma i64 range"))?;
        Ok(LymaValue::Number(LymaNumber::Integer(value)))
    }

    const fn serialize_float(value: f64) -> Result<LymaValue> {
        if value.is_finite() {
            Ok(LymaValue::Number(LymaNumber::Float(value)))
        } else {
            Err(Self::unsupported_named("non-finite floating-point value"))
        }
    }
}

#[derive(Debug, Default)]
pub struct SequenceSerializer {
    items: Vec<LymaValue>,
}

impl SequenceSerializer {
    fn new(len: Option<usize>) -> Self {
        Self {
            items: len.map_or_else(Vec::new, Vec::with_capacity),
        }
    }

    fn finish(self) -> LymaValue {
        LymaValue::Sequence(LymaSequence {
            items: self.items,
            span: None,
        })
    }
}

#[derive(Debug, Default)]
pub struct MapSerializer {
    entries: Vec<LymaMappingEntry>,
    next_key: Option<LymaKey>,
}

impl MapSerializer {
    fn new(len: Option<usize>) -> Self {
        Self {
            entries: len.map_or_else(Vec::new, Vec::with_capacity),
            next_key: None,
        }
    }

    fn finish(self) -> Result<LymaValue> {
        if self.next_key.is_some() {
            return Err(Error::custom("serialize_map ended with a key but no value"));
        }

        Ok(LymaValue::Mapping(LymaMapping {
            entries: self.entries,
            duplicate_keys: Vec::new(),
            span: None,
        }))
    }
}

#[derive(Debug, Default)]
pub struct VariantSerializer {
    variant: &'static str,
    inner: Vec<LymaValue>,
}

impl VariantSerializer {
    fn new(variant: &'static str, len: usize) -> Self {
        Self {
            variant,
            inner: Vec::with_capacity(len),
        }
    }

    fn finish(self) -> LymaValue {
        let value = LymaValue::Sequence(LymaSequence {
            items: self.inner,
            span: None,
        });
        wrap_variant(self.variant, value)
    }
}

#[derive(Debug, Default)]
pub struct StructVariantSerializer {
    variant: &'static str,
    entries: Vec<LymaMappingEntry>,
}

impl StructVariantSerializer {
    fn new(variant: &'static str, len: usize) -> Self {
        Self {
            variant,
            entries: Vec::with_capacity(len),
        }
    }

    fn finish(self) -> LymaValue {
        let value = LymaValue::Mapping(LymaMapping {
            entries: self.entries,
            duplicate_keys: Vec::new(),
            span: None,
        });
        wrap_variant(self.variant, value)
    }
}

fn wrap_variant(variant: &'static str, value: LymaValue) -> LymaValue {
    LymaValue::Mapping(LymaMapping {
        entries: vec![LymaMappingEntry {
            key: LymaKey::String(variant.to_owned()),
            value,
            span: None,
        }],
        duplicate_keys: Vec::new(),
        span: None,
    })
}

fn value_to_key(value: LymaValue) -> Result<LymaKey> {
    match value {
        LymaValue::String(value) => Ok(LymaKey::String(value)),
        LymaValue::Number(value) => Ok(LymaKey::Number(value)),
        LymaValue::Boolean(value) => Ok(LymaKey::Boolean(value)),
        _ => Err(Error::unsupported("non-scalar mapping key")),
    }
}

fn ensure_portable_mapping_keys(value: &LymaValue) -> Result<()> {
    match value {
        LymaValue::Mapping(mapping) => {
            for entry in &mapping.entries {
                match &entry.key {
                    LymaKey::String(_) => {}
                    LymaKey::Number(_) | LymaKey::Boolean(_) => {
                        return Err(Error::custom(
                            "to_string requires map keys that serialize as strings; use to_value for numeric or boolean keys",
                        ));
                    }
                    LymaKey::Host(host) => {
                        return Err(Error::custom(format!(
                            "to_string requires portable string map keys; host key `{}` is not supported",
                            host.kind
                        )));
                    }
                }

                ensure_portable_mapping_keys(&entry.value)?;
            }
            Ok(())
        }
        LymaValue::Sequence(sequence) => {
            for item in &sequence.items {
                ensure_portable_mapping_keys(item)?;
            }
            Ok(())
        }
        LymaValue::Tagged(tagged) => ensure_portable_mapping_keys(&tagged.value),
        LymaValue::Null(_)
        | LymaValue::Boolean(_)
        | LymaValue::Number(_)
        | LymaValue::String(_)
        | LymaValue::Function(_)
        | LymaValue::UserData(_)
        | LymaValue::HostObject(_) => Ok(()),
    }
}

/// Converts a Serde-serializable Rust value into a Lyma value.
///
/// # Errors
///
/// Returns an error when the input contains unsupported Serde data, including
/// map keys that do not serialize to scalar Lyma keys.
pub fn to_value<T>(value: T) -> Result<LymaValue>
where
    T: Serialize,
{
    value.serialize(ValueSerializer::new())
}

/// Converts a Serde-serializable Rust value into canonical Lyma text.
///
/// # Errors
///
/// Returns an error if conversion to a Lyma value fails or if the resulting value
/// cannot be rendered as portable Lyma text. Unlike [`to_value`], this requires
/// every mapping key in the serialized value to be a string key.
pub fn to_string<T>(value: T) -> Result<String>
where
    T: Serialize,
{
    let value = to_value(value)?;
    ensure_portable_mapping_keys(&value)?;
    serialize_value(&value).map_err(Error::from_diagnostic)
}

/// Converts a Serde-serializable Rust value into canonical Lyma text with explicit options.
///
/// # Errors
///
/// Returns an error if conversion to a Lyma value fails or if the resulting value
/// cannot be rendered as portable Lyma text. Unlike [`to_value`], this requires
/// every mapping key in the serialized value to be a string key.
pub fn to_string_with_options<T>(value: T, options: SerializeOptions) -> Result<String>
where
    T: Serialize,
{
    let value = to_value(value)?;
    ensure_portable_mapping_keys(&value)?;
    serialize_value_with_options(&value, options).map_err(Error::from_diagnostic)
}

impl serde::Serializer for ValueSerializer {
    type Ok = LymaValue;
    type Error = Error;
    type SerializeSeq = SequenceSerializer;
    type SerializeTuple = SequenceSerializer;
    type SerializeTupleStruct = SequenceSerializer;
    type SerializeTupleVariant = VariantSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        Ok(LymaValue::Boolean(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok> {
        Self::serialize_signed(i128::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok> {
        Self::serialize_signed(i128::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok> {
        Self::serialize_signed(i128::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok> {
        Ok(LymaValue::Number(LymaNumber::Integer(value)))
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok> {
        Self::serialize_signed(value)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok> {
        Self::serialize_unsigned(u128::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok> {
        Self::serialize_unsigned(u128::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok> {
        Self::serialize_unsigned(u128::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok> {
        Self::serialize_unsigned(u128::from(value))
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok> {
        Self::serialize_unsigned(value)
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok> {
        Self::serialize_float(f64::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok> {
        Self::serialize_float(value)
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        Ok(LymaValue::String(value.to_string()))
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        Ok(LymaValue::String(value.to_owned()))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok> {
        Err(Self::unsupported_named("byte slices"))
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        Ok(LymaValue::Null(LymaNull))
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        Ok(LymaValue::Null(LymaNull))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok> {
        Ok(LymaValue::String(variant.to_owned()))
    }

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        Ok(wrap_variant(variant, value.serialize(self)?))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(SequenceSerializer::new(len))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        Ok(SequenceSerializer::new(Some(len)))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(SequenceSerializer::new(Some(len)))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Ok(VariantSerializer::new(variant, len))
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(MapSerializer::new(len))
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        Ok(MapSerializer::new(Some(len)))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Ok(StructVariantSerializer::new(variant, len))
    }
}

impl SerializeSeq for SequenceSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.items.push(value.serialize(ValueSerializer::new())?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTuple for SequenceSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTupleStruct for SequenceSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeTupleVariant for VariantSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.inner.push(value.serialize(ValueSerializer::new())?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(self.finish())
    }
}

impl SerializeMap for MapSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        if self.next_key.is_some() {
            return Err(Error::custom("serialize_key called before serialize_value"));
        }

        self.next_key = Some(value_to_key(key.serialize(ValueSerializer::new())?)?);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        let key = self
            .next_key
            .take()
            .ok_or_else(|| Error::custom("serialize_value called before serialize_key"))?;

        self.entries.push(LymaMappingEntry {
            key,
            value: value.serialize(ValueSerializer::new())?,
            span: None,
        });
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

impl SerializeStruct for MapSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.entries.push(LymaMappingEntry {
            key: LymaKey::String(key.to_owned()),
            value: value.serialize(ValueSerializer::new())?,
            span: None,
        });
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        self.finish()
    }
}

impl SerializeStructVariant for StructVariantSerializer {
    type Ok = LymaValue;
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        self.entries.push(LymaMappingEntry {
            key: LymaKey::String(key.to_owned()),
            value: value.serialize(ValueSerializer::new())?,
            span: None,
        });
        Ok(())
    }

    fn end(self) -> Result<Self::Ok> {
        Ok(self.finish())
    }
}

impl ser::Serializer for &mut ValueSerializer {
    type Ok = LymaValue;
    type Error = Error;
    type SerializeSeq = SequenceSerializer;
    type SerializeTuple = SequenceSerializer;
    type SerializeTupleStruct = SequenceSerializer;
    type SerializeTupleVariant = VariantSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok> {
        ValueSerializer.serialize_bool(value)
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok> {
        ValueSerializer.serialize_i8(value)
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok> {
        ValueSerializer.serialize_i16(value)
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok> {
        ValueSerializer.serialize_i32(value)
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok> {
        ValueSerializer.serialize_i64(value)
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok> {
        ValueSerializer.serialize_i128(value)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok> {
        ValueSerializer.serialize_u8(value)
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok> {
        ValueSerializer.serialize_u16(value)
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok> {
        ValueSerializer.serialize_u32(value)
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok> {
        ValueSerializer.serialize_u64(value)
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok> {
        ValueSerializer.serialize_u128(value)
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok> {
        ValueSerializer.serialize_f32(value)
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok> {
        ValueSerializer.serialize_f64(value)
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok> {
        ValueSerializer.serialize_char(value)
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok> {
        ValueSerializer.serialize_str(value)
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok> {
        ValueSerializer.serialize_bytes(value)
    }

    fn serialize_none(self) -> Result<Self::Ok> {
        ValueSerializer.serialize_none()
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        ValueSerializer.serialize_some(value)
    }

    fn serialize_unit(self) -> Result<Self::Ok> {
        ValueSerializer.serialize_unit()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok> {
        ValueSerializer.serialize_unit_struct(name)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok> {
        ValueSerializer.serialize_unit_variant(name, variant_index, variant)
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        ValueSerializer.serialize_newtype_struct(name, value)
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok>
    where
        T: ?Sized + Serialize,
    {
        ValueSerializer.serialize_newtype_variant(name, variant_index, variant, value)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        ValueSerializer.serialize_seq(len)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        ValueSerializer.serialize_tuple(len)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        ValueSerializer.serialize_tuple_struct(name, len)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        ValueSerializer.serialize_tuple_variant(name, variant_index, variant, len)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        ValueSerializer.serialize_map(len)
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        ValueSerializer.serialize_struct(name, len)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        ValueSerializer.serialize_struct_variant(name, variant_index, variant, len)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lyma_syntax::{
        LymaKey, LymaMapping, LymaNull, LymaNumber, SerializeOptions, serialize_value,
        serialize_value_with_options,
    };
    use serde::Serialize;

    use super::{to_string, to_string_with_options, to_value};
    use crate::Error;

    #[derive(Serialize)]
    struct ExampleStruct {
        name: &'static str,
        enabled: bool,
    }

    #[derive(Serialize)]
    struct NestedStruct {
        title: &'static str,
        values: Vec<i32>,
    }

    #[derive(Serialize)]
    struct Newtype(i32);

    #[derive(Serialize)]
    enum ExampleEnum {
        Unit,
        Tuple(i32, &'static str),
        Struct { count: u8 },
        Newtype(bool),
    }

    struct BytesValue<'a>(&'a [u8]);

    impl Serialize for BytesValue<'_> {
        fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            serializer.serialize_bytes(self.0)
        }
    }

    #[derive(Serialize)]
    struct CanonicalStruct {
        name: &'static str,
        enabled: bool,
        maybe: Option<&'static str>,
        values: Vec<i32>,
    }

    #[test]
    fn serializes_primitives_and_structures_in_order() {
        let mut map = BTreeMap::new();
        map.insert("numbers", vec![1_i32, 2, 3]);
        map.insert("other", vec![4_i32]);

        let value = to_value(("hello", Some(5_u32), None::<u8>, (), map)).unwrap();

        assert!(matches!(
            value,
            lyma_syntax::LymaValue::Sequence(lyma_syntax::LymaSequence { items, .. })
            if matches!(items[0], lyma_syntax::LymaValue::String(ref s) if s == "hello")
                && matches!(items[1], lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Integer(5)))
                && matches!(items[2], lyma_syntax::LymaValue::Null(_))
                && matches!(items[3], lyma_syntax::LymaValue::Null(_))
                && matches!(items[4], lyma_syntax::LymaValue::Mapping(_))
        ));
    }

    #[test]
    fn serializes_primitive_conversions_exactly() {
        assert_eq!(
            to_value(false).unwrap(),
            lyma_syntax::LymaValue::Boolean(false)
        );
        assert_eq!(
            to_value('λ').unwrap(),
            lyma_syntax::LymaValue::String("λ".to_owned())
        );
        assert_eq!(
            to_value(i128::from(i64::MIN)).unwrap(),
            lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Integer(i64::MIN))
        );
        assert_eq!(
            to_value(u64::try_from(i64::MAX).unwrap()).unwrap(),
            lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Integer(i64::MAX))
        );
        assert_eq!(
            to_value(1.25_f32).unwrap(),
            lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Float(1.25))
        );
        assert_eq!(
            to_value(-2.5_f64).unwrap(),
            lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Float(-2.5))
        );
    }

    #[test]
    fn serializes_structs_newtypes_and_enum_variants() {
        assert!(matches!(
            to_value(ExampleStruct {
                name: "demo",
                enabled: true,
            })
            .unwrap(),
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 2
                && matches!(&entries[0].key, lyma_syntax::LymaKey::String(key) if key == "name")
                && matches!(&entries[1].key, lyma_syntax::LymaKey::String(key) if key == "enabled")
        ));

        assert_eq!(
            to_value(Newtype(7)).unwrap(),
            lyma_syntax::LymaValue::Number(lyma_syntax::LymaNumber::Integer(7))
        );

        assert_eq!(
            to_value(ExampleEnum::Unit).unwrap(),
            lyma_syntax::LymaValue::String(String::from("Unit"))
        );

        assert!(matches!(
            to_value(ExampleEnum::Tuple(1, "two")).unwrap(),
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 1
                && matches!(&entries[0].key, lyma_syntax::LymaKey::String(key) if key == "Tuple")
                && matches!(entries[0].value, lyma_syntax::LymaValue::Sequence(_))
        ));

        assert!(matches!(
            to_value(ExampleEnum::Struct { count: 2 }).unwrap(),
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 1
                && matches!(entries[0].value, lyma_syntax::LymaValue::Mapping(_))
        ));

        assert!(matches!(
            to_value(ExampleEnum::Newtype(true)).unwrap(),
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 1
                && matches!(entries[0].value, lyma_syntax::LymaValue::Boolean(true))
        ));
    }

    #[test]
    fn maps_none_and_unit_to_null_and_rejects_unsupported_values() {
        assert_eq!(
            to_value(None::<u8>).unwrap(),
            lyma_syntax::LymaValue::Null(LymaNull)
        );
        assert_eq!(
            to_value(()).unwrap(),
            lyma_syntax::LymaValue::Null(LymaNull)
        );

        let bytes_error = to_value(BytesValue(b"abc")).unwrap_err();
        assert_eq!(
            bytes_error,
            Error::Unsupported {
                operation: "byte slices"
            }
        );

        let int_error = to_value(u128::MAX).unwrap_err();
        assert_eq!(
            int_error,
            Error::Unsupported {
                operation: "integer outside Lyma i64 range"
            }
        );

        let float_error = to_value(f64::NAN).unwrap_err();
        assert_eq!(
            float_error,
            Error::Unsupported {
                operation: "non-finite floating-point value"
            }
        );
    }

    #[test]
    fn string_key_maps_serialize_to_value_and_text() {
        let mut map = BTreeMap::new();
        map.insert("alpha", 1_i32);
        map.insert("beta", 2_i32);

        let value = to_value(&map).unwrap();
        assert!(matches!(
            value,
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 2
                && matches!(&entries[0].key, LymaKey::String(key) if key == "alpha")
                && matches!(&entries[1].key, LymaKey::String(key) if key == "beta")
        ));

        assert_eq!(to_string(&map).unwrap(), "alpha: 1\nbeta: 2\n");
    }

    #[test]
    fn to_string_matches_lyma_syntax_canonical_output_for_equivalent_value() {
        let value = ExampleStruct {
            name: "demo",
            enabled: true,
        };

        let expected = serialize_value(&to_value(&value).unwrap()).unwrap();

        assert_eq!(to_string(&value).unwrap(), expected);
    }

    #[test]
    fn to_string_emits_canonical_text_for_struct_with_option_and_sequence() {
        let value = CanonicalStruct {
            name: "demo",
            enabled: true,
            maybe: None,
            values: vec![1, 2],
        };

        assert_eq!(
            to_string(&value).unwrap(),
            "name: demo\nenabled: true\nmaybe: null\nvalues:\n  - 1\n  - 2\n"
        );
    }

    #[test]
    fn to_string_with_options_matches_lyma_syntax_output_for_equivalent_value() {
        let value = NestedStruct {
            title: "demo",
            values: vec![1, 2],
        };
        let options = SerializeOptions { indent_width: 4 };

        let expected = serialize_value_with_options(&to_value(&value).unwrap(), options).unwrap();

        assert_eq!(to_string_with_options(&value, options).unwrap(), expected);
    }

    #[test]
    fn to_value_accepts_numeric_and_boolean_map_keys() {
        let mut numeric = BTreeMap::new();
        numeric.insert(7_i32, "seven");

        let numeric_value = to_value(numeric).unwrap();
        assert!(matches!(
            numeric_value,
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 1
                && matches!(&entries[0].key, LymaKey::Number(LymaNumber::Integer(7)))
                && matches!(&entries[0].value, lyma_syntax::LymaValue::String(value) if value == "seven")
        ));

        let mut boolean = BTreeMap::new();
        boolean.insert(true, 1_i32);

        let boolean_value = to_value(boolean).unwrap();
        assert!(matches!(
            boolean_value,
            lyma_syntax::LymaValue::Mapping(LymaMapping { entries, .. })
            if entries.len() == 1
                && matches!(&entries[0].key, LymaKey::Boolean(true))
                && matches!(entries[0].value, lyma_syntax::LymaValue::Number(LymaNumber::Integer(1)))
        ));
    }

    #[test]
    fn to_string_rejects_non_string_map_keys_before_syntax_serialization() {
        let mut numeric = BTreeMap::new();
        numeric.insert(7_i32, "seven");

        assert_eq!(
            to_string(numeric).unwrap_err(),
            Error::Custom(
                "to_string requires map keys that serialize as strings; use to_value for numeric or boolean keys"
                    .to_owned()
            )
        );

        let mut nested = BTreeMap::new();
        let mut inner = BTreeMap::new();
        inner.insert(false, 1_i32);
        nested.insert("outer", inner);

        assert_eq!(
            to_string(nested).unwrap_err(),
            Error::Custom(
                "to_string requires map keys that serialize as strings; use to_value for numeric or boolean keys"
                    .to_owned()
            )
        );
    }

    #[test]
    fn to_value_rejects_complex_map_keys() {
        let mut map = BTreeMap::new();
        map.insert(("left", "right"), 1_i32);

        assert_eq!(
            to_value(map).unwrap_err(),
            Error::Unsupported {
                operation: "non-scalar mapping key"
            }
        );
    }
}
