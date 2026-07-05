# Public API

## Facade crate

`luma` re-exports workspace crates behind features:

- `luma::syntax`
- `luma::parser`
- `luma::runtime`
- `luma::eval`
- `luma::serde`
- `luma::engine_omnilua`
- `luma::tooling`

`luma::version()` returns the crate version.

Feature highlights:

- default features: `parser`
- `serde = ["syntax", "dep:luma-serde"]` enables the facade Serde bridge as `luma::serde`
- `eval` and backend features remain opt-in

## Syntax layer

Use `luma::syntax` for stable data types:

- AST: `LumaFile`, `Document`, `DocumentItem`, `LumaNode`, directives, blocks
- values: `LumaValue`, `LumaMapping`, `LumaSequence`, `LumaTaggedValue`
- source model: `FileId`, `LumaSource`, `Span`
- diagnostics: `Diagnostic`, `DiagnosticCode`, `Severity`
- serializer: `serialize_value`, `serialize_value_with_options`

## Parser layer

Use `luma::parser` when you want engine-neutral parsing, lexing, decoding, or formatting.

Primary entry points:

- `parse_str(FileId, name, text) -> Parsed`
- `parse_source(SourceText) -> Parsed`
- `format_str(FileId, name, text) -> ParsedFormatting`
- `format_file(...) -> ParsedFormatting`
- `lex_str(FileId, name, text) -> Lexed`
- `decode_str(name, text)` / `decode_bytes(name, bytes)`

Important guarantee: parser APIs are **engine-agnostic** and do not execute Lua.

## Tooling helpers

`luma::tooling` exposes editor-oriented helpers:

- `format_document_edit`
- `format_document_text_edit`
- `serialize_portable_value`
- `TextRange`
- `TextEdit`

## Evaluation layer

Use `luma::eval::AstEvaluator` to evaluate parsed documents against any backend implementing `LuaRuntimeEngine`.

Primary types:

- `AstEvaluator<'a, E>`
- `EvaluationOptions<'a, E>`
- `EvaluationError`
- `EvaluatedDocument`
- `DocumentMetadata`
- `EvaluationProfile`, `ProfilePolicy`

Main methods:

- `evaluate_file(&LumaFile, source_name, locator) -> Result<Vec<LumaValue>, EvaluationError>`
- `evaluate_file_with_metadata(...) -> Result<Vec<EvaluatedDocument>, EvaluationError>`

## Profiles

Built-in profile policy primitives:

- `EvaluationProfile::restricted()`
- `EvaluationProfile::permissive(RuntimeLimits)`
- static `RESTRICTED_EVALUATION_PROFILE`

Behavior:

- restricted: deterministic, sandboxed, runtime-only outputs denied
- permissive: caller-supplied limits, runtime-only outputs allowed

Document metadata can also request Luma profiles such as `data`, `safe`, and `trusted`. The evaluator maps those declarations against the host profile policy and will reject `trusted` unless the active host profile is also named `trusted`.

## Extension points

### Resolver

- trait: `ResourceResolver`
- defaults: `DenyAllResolver`
- ready-made impls: `FilesystemResolver`, `InMemoryResolver`
- shared types: `ResolutionRequest`, `ResolutionContext`, `ResolverPolicy`, `ResourceLocator`

### Modules

- trait: `ModuleRegistry<E>`
- defaults: `DenyAllModuleRegistry`
- ready-made impl: `InMemoryModuleRegistry`
- shared types: `ModuleLookupRequest`, `ModuleLookupError`

### Tags

- trait: `TagResolver`
- defaults: `DenyAllTagResolver`
- ready-made impl: `InMemoryTagResolver`
- shared types: `TagResolutionRequest`, `TagResolutionError`, `UnknownTagPolicy`

### Schema

- trait: `SchemaValidator`
- defaults: `DenyAllSchemaValidator`
- ready-made impl: `InMemorySchemaValidator`
- shared types: `SchemaValidationRequest`, `SchemaValidationError`

## Runtime/backend APIs

`luma::runtime` defines backend-neutral contracts:

- `Engine`
- `LuaRuntimeEngine`
- `RuntimeEnvironmentFactory`
- `RuntimeEnvironment`
- `RuntimeModuleFactory`
- `RuntimeModule`
- `RuntimeValueCodec`
- `RuntimeLimits`
- `LuaSourceText`
- `LuaRuntimeError`

Use `EvaluationPlan<E>` for direct expression/chunk execution when you need raw engine access outside AST evaluation.

## Serde adapter policy (`luma-serde`)

Use `luma_serde::{to_value, to_string, to_string_with_options, from_value}` to bridge Rust `serde` data with `luma::syntax::LumaValue`.

Through the facade crate, enable `features = ["serde"]` and call the same helpers from `luma::serde`:

```toml
[dependencies]
luma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Example<'a> {
    name: &'a str,
    enabled: bool,
}

let text = luma::serde::to_string(&Example {
    name: "demo",
    enabled: true,
})?;

assert_eq!(text, "enabled: true\nname: demo\n");
# Ok::<(), luma::serde::Error>(())
```

`to_string` uses `luma_syntax` serialization rules, so output is canonical Luma text rather than preserving Rust field order quirks from custom emitters.

### Serialization support

`to_value` supports these Serde shapes:

- scalars: `bool`, strings, `char`, finite floats, and integers within Luma's `i64` range
- `Option::None`, unit, and unit structs as Luma `null`
- `Option::Some` and newtype structs as their inner value
- sequences, tuples, and tuple structs as Luma sequences
- maps and structs as Luma mappings
- enums in externally tagged form:
  - unit variant -> string variant name
  - newtype variant -> `{ variant: payload }`
  - tuple variant -> `{ variant: [..] }`
  - struct variant -> `{ variant: { .. } }`

Unsupported or ambiguous shapes fail during serialization:

- bytes / `serde_bytes` are rejected (`unsupported: byte slices`)
- non-finite floats (`NaN`, `+/-inf`) are rejected (`unsupported: non-finite floating-point value`)
- `u64`, `u128`, or `i128` values outside signed 64-bit range are rejected (`unsupported: integer outside Luma i64 range`)
- map keys for `to_value` must serialize to scalar Luma keys only: string, number, or boolean
- map keys that serialize to `null`, sequences, mappings, bytes, or other non-scalars are rejected (`unsupported: non-scalar mapping key`)

`to_string` and `to_string_with_options` are stricter than `to_value`:

- they first serialize to `LumaValue`, then emit canonical Luma text
- all mapping keys in the resulting value must be strings
- numeric and boolean keys are allowed by `to_value` but rejected by text serialization with an error directing callers to use `to_value`

### Deserialization support

`from_value` supports:

- matching scalar Luma null/bool/number/string values
- sequences for sequences, tuples, and tuple structs
- mappings for maps and structs
- `null` for unit, unit structs, and `Option::None`
- any non-null value for `Option::Some`
- enums from either:
  - a string variant name
  - a single-entry mapping whose key is the string variant name
  - a tagged Luma value whose tag name is treated as the variant name

Tagged Luma values are otherwise transparent: when the Rust target is not an enum, `from_value` ignores the tag wrapper and deserializes the tagged payload directly.

Unsupported or mismatched deserialization fails with explicit shape errors:

- bytes / byte buffers are rejected as unsupported
- `char` requires a one-character string
- enum maps must have exactly one entry and that entry's key must be a string variant name
- runtime-only Luma values (`function`, `userdata`, `host object`) and runtime-only host mapping keys cannot be deserialized and report explicit runtime-only errors
