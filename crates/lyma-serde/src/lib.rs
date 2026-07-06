//! Serde adapters for converting between Rust data structures and Lyma values.
//!
//! ## Supported serializer shapes
//!
//! `to_value` accepts the Serde data model shapes that map directly onto portable
//! Lyma data:
//!
//! - booleans, strings, and `char` as scalar Lyma booleans/strings (`char`
//!   serializes as a single-character string)
//! - signed and unsigned integers that fit in Lyma's `i64` range
//! - finite `f32`/`f64` values
//! - `Option::None`, unit, and unit structs as Lyma `null`
//! - `Option::Some`, newtype structs, and newtype variants by serializing only
//!   their inner value
//! - sequences, tuples, and tuple structs as Lyma sequences
//! - maps and structs as Lyma mappings
//! - enum variants using Serde's externally tagged shape:
//!   - unit variants => string variant name
//!   - newtype variants => single-entry mapping `{ variant: payload }`
//!   - tuple variants => single-entry mapping `{ variant: [..] }`
//!   - struct variants => single-entry mapping `{ variant: { .. } }`
//!
//! ## Unsupported serializer shapes
//!
//! Unsupported input fails with `Error::unsupported(...)` or a custom shape
//! error:
//!
//! - `serialize_bytes` / `serde_bytes` => rejected as unsupported `"byte slices"`
//! - non-finite floats (`NaN`, `+inf`, `-inf`) => rejected as unsupported
//!   `"non-finite floating-point value"`
//! - `u64`/`u128` and `i128` values outside the Lyma `i64` range => rejected as
//!   unsupported `"integer outside Lyma i64 range"`
//! - map keys must serialize to scalar Lyma keys only: strings, numbers, or
//!   booleans. Keys that serialize to `null`, sequences, mappings, bytes, or any
//!   other non-scalar shape fail as unsupported `"non-scalar mapping key"`
//!
//! `to_string` and `to_string_with_options` first call `to_value`, then pass the
//! result through `lyma_syntax`'s canonical text serializer. They are stricter
//! than `to_value`: every mapping key in the final value must be a string key so
//! the emitted text stays portable. Numeric and boolean keys therefore fail with
//! `"to_string requires map keys that serialize as strings; use to_value for
//! numeric or boolean keys"`.
//!
//! ## Supported deserializer shapes
//!
//! `from_value` accepts:
//!
//! - scalar Lyma null/bool/number/string values for matching scalar Serde targets
//! - sequences for `SeqAccess`, tuples, and tuple structs
//! - mappings for maps and structs
//! - `null` for unit, unit structs, and `Option::None`
//! - non-null values for `Option::Some`
//! - strings, single-entry mappings, or tagged Lyma values for enums
//!
//! Tagged Lyma values are transparent for non-enum targets: the tag wrapper is
//! ignored and the payload is deserialized directly. For enum targets, the tag
//! name is treated as the externally tagged variant name and the tagged payload
//! becomes the variant content.
//!
//! ## Unsupported deserializer shapes
//!
//! Deserialization fails when the requested Serde shape does not match the Lyma
//! value shape, and also for these notable cases:
//!
//! - byte slices / byte buffers => unsupported `"byte slices"` or
//!   `"byte buffers"`
//! - `char` expects a one-character string; longer or empty strings fail with
//!   `"expected a single-character string"`
//! - enum deserialization requires a string, single-entry mapping with a string
//!   key, or tagged value; other shapes fail with a type mismatch
//! - runtime-only Lyma values or host mapping keys fail with explicit errors such
//!   as `"cannot deserialize runtime-only Lyma function value ..."`

#![forbid(unsafe_code)]

mod de;
mod error;
mod ser;

pub use de::{ValueDeserializer, from_value};
pub use error::{Error, Result};
pub use ser::{ValueSerializer, to_string, to_string_with_options, to_value};
