# Public API

## Facade crate

`lyma` re-exports workspace crates behind features:

- `lyma::syntax`
- `lyma::parser`
- `lyma::runtime`
- `lyma::eval`
- `lyma::serde`
- `lyma::lyba`
- `lyma::engine_omnilua`
- `lyma::tooling`

`lyma::version()` returns the crate version.

Feature highlights:

- default features: `parser`
- `serde = ["syntax", "dep:lyma-serde"]` enables the facade Serde bridge as `lyma::serde`
- `lyba = ["syntax", "dep:lyma-lyba"]` enables the inert binary companion API as `lyma::lyba`
- `eval` and backend features remain opt-in

## Syntax layer

Use `lyma::syntax` for stable data types:

- AST: `LymaFile`, `Document`, `DocumentItem`, `LymaNode`, directives, blocks
- syntax index: `SyntaxIndex`, `SyntaxKind`, `SyntaxNodeId`, `SyntaxNodeInfo`
- values: `LymaValue`, `LymaMapping`, `LymaSequence`, `LymaTaggedValue`
- source model: `FileId`, `LymaSource`, `Span`
- diagnostics: `Diagnostic`, `DiagnosticCode`, `Severity`
- serializer: `serialize_value`, `serialize_value_with_options`

`SyntaxNodeId` values are deterministic preorder IDs for one indexed parse result.
They are stable within that `SyntaxIndex` only and are not persistent across edits
or reparses.

## Parser layer

Use `lyma::parser` when you want engine-neutral parsing, lexing, decoding, or formatting.

Primary entry points:

- `parse_str(FileId, name, text) -> Parsed`
- `parse_source(SourceText) -> Parsed`
- `ParseSession::new(FileId, name)` + `ParseSession::parse(...)` / `ParseSession::apply(...)`
- `Parsed::syntax_index() -> SyntaxIndex`
- `format_str(FileId, name, text) -> ParsedFormatting`
- `format_range_edits(FileId, name, text, range, options) -> Result<(ParsedFormatting, Vec<TextEdit>), FormatRangeError>`
- `format_parsed_range_edits(&Parsed, range, options) -> Result<Vec<TextEdit>, FormatRangeError>`
- `format_file(...) -> ParsedFormatting`
- `lex_str(FileId, name, text) -> Lexed`
- `decode_str(name, text)` / `decode_bytes(name, bytes)`

Lexing also exposes stable lexical editor primitives:

- `Lexed.tokens` preserves public `Token` values with `span`, `leading_trivia`, and `trailing_trivia`
- `TokenKind::LineBreak` and `TokenKind::Comment` let downstream tools keep structural trivia in token streams
- `Lexed.indents: Vec<LineIndent>` records per-line indentation width plus `LineIndent.span`

These are lexical/syntactic primitives only. They describe source layout for
formatting, offset mapping, comment preservation, or lightweight token-aware UX;
semantic token classification still belongs in downstream tools such as
`lymals`.

Incremental parsing is currently an **API shell**:

- public types include `TextChange`, `IncrementalParseInput`, `IncrementalParseResult`, `ParsedDocument`, and `ParseSession`
- update metadata reports `strategy: FullReparse` and `reused: false`
- changes are validated against the previous normalized source text
- invalid/out-of-bounds or non-UTF-8-boundary change ranges return typed `IncrementalParseError` values
- implementation currently applies edits to the previous normalized source and reparses the full document, leaving room for future token/subtree reuse behind the same API

Important guarantee: parser APIs are **engine-agnostic** and do not execute Lua.

Example with public facade APIs only:

```rust
use lyma::Parser;
use lyma::parser::FileId;
use lyma::syntax::SyntaxKind;

let parsed = Parser::new().parse_str(FileId(1), "example.lyma", "root:\n  child: 42\n");
let index = parsed.syntax_index();

let child_offset = parsed.source.as_str().find("child").unwrap();
let child_id = index.smallest_node_at_offset(child_offset).unwrap();
let parent_id = index.parent(child_id).unwrap();

assert_eq!(index.node(child_id).unwrap().kind, SyntaxKind::PlainMappingKey);
assert_eq!(index.node(parent_id).unwrap().kind, SyntaxKind::MappingEntry);
```

This is the upstream primitive for hover-at-offset-style lookups: resolve an
editor byte offset to the smallest indexed syntax node, then inspect that node's
kind/span and slice the source text as needed. Higher-level LSP behavior such as
hover rendering, semantic token classification, references, rename, and
workspace-wide symbol/index queries belongs in downstream consumers such as
`lymals`.

Lexical example with public token/trivia APIs only:

```rust
use lyma::parser::{FileId, TokenKind, lex_str};

let source = "root:\n  child:  next  -- note\n";
let lexed = lex_str(FileId(1), "example.lyma", source);

let next = lexed.tokens.iter().find(|token| token.lexeme == "next").unwrap();
let comment = lexed.tokens.iter().find(|token| token.kind == TokenKind::Comment).unwrap();
let line_break = lexed.tokens.iter().find(|token| token.kind == TokenKind::LineBreak).unwrap();

assert_eq!(&source[next.leading_trivia.byte_range()], "  ");
assert_eq!(&source[next.trailing_trivia.byte_range()], "  ");
assert_eq!(&source[comment.leading_trivia.byte_range()], "  ");
assert_eq!(&source[line_break.span.byte_range()], "\n");
assert_eq!(&source[lexed.indents[1].span.byte_range()], "  ");
```

## Tooling helpers

`lyma::tooling` exposes editor-oriented helpers:

- `format_document_edit`
- `format_document_text_edit`
- `format_document_text_edits`
- `format_document_range_text_edits`
- `FormatRangeOptions`
- `FormatRangeFallback`
- `FormatRangeError`
- `serialize_portable_value`
- `TextRange`
- `TextEdit`

These helpers are upstream editing primitives, not a full language-server API.
They provide canonical formatting edits and stable source-relative edit types for
editor integrations.

Range-formatting is conservative because canonical formatting is still whole-file today:

- invalid or non-UTF-8-boundary ranges return `FormatRangeError`
- valid ranges expand to full intersecting lines and then, when possible, to the smallest containing syntax node span
- if all canonical edits stay inside that expanded range, the API returns minimal source-relative edits for just that range
- if canonical formatting would also change text outside the expanded range, the default fallback is one whole-document replacement edit; set `FormatRangeOptions { fallback: FormatRangeFallback::Reject, .. }` to reject that case instead

Example:

```rust
use lyma::tooling::{
    FormatRangeOptions, TextRange, format_document_range_text_edits,
    format_document_text_edit,
};

let source = "service:\n  name:'api'\n  enabled:true\n";
let (_formatting, whole_edit) = format_document_text_edit("service.lyma", source);
assert_eq!(whole_edit.range, TextRange::new(0, source.len()));

let enabled_start = source.find("enabled").unwrap();
let (_formatting, range_edits) = format_document_range_text_edits(
    "service.lyma",
    source,
    TextRange::new(enabled_start, source.len()),
    FormatRangeOptions::default(),
)?;

assert!(!range_edits.is_empty());
# Ok::<(), lyma::parser::FormatRangeError>(())
```

Downstream `lymals` should build semantic formatting UX, code actions, rename
previews, and other LSP semantics on top of these edit primitives rather than
expecting the core crate to own those policies.

### Incremental parse shell

Use `ParseSession` plus `IncrementalParseInput`/`TextChange` when an editor wants
to feed document updates through a stable incremental API:

```rust
use lyma::parser::{
    FileId, IncrementalParseInput, IncrementalParseStrategy, ParseSession,
    TextChange,
};
use lyma::tooling::TextRange;

let mut session = ParseSession::new(FileId(1), "service.lyma");
let first = session.parse("enabled:true\n");
assert!(first.parsed().diagnostics.is_empty());

let updated = session.apply(IncrementalParseInput::new(vec![TextChange::replace(
    TextRange::new(0, "enabled:true".len()),
    "enabled: false",
)]))?;

assert_eq!(updated.strategy, IncrementalParseStrategy::FullReparse);
assert!(!updated.reused);
assert_eq!(updated.document.source(), "enabled: false\n");
# Ok::<(), lyma::parser::IncrementalParseError>(())
```

Today this remains an API shell around validated full reparses. Downstream tools
can rely on the request/response shape now, while future parser versions may add
token or subtree reuse behind the same API.

## Evaluation layer

Use `lyma::eval::AstEvaluator` to evaluate parsed documents against any backend implementing `LuaRuntimeEngine`.

Primary types:

- `AstEvaluator<'a, E>`
- `EvaluationOptions<'a, E>`
- `EvaluationError`
- `EvaluatedDocument`
- `DocumentMetadata`
- `EvaluationProfile`, `ProfilePolicy`

Main methods:

- `evaluate_file(&LymaFile, source_name, locator) -> Result<Vec<LymaValue>, EvaluationError>`
- `evaluate_file_with_metadata(...) -> Result<Vec<EvaluatedDocument>, EvaluationError>`

## Profiles

Built-in profile policy primitives:

- `EvaluationProfile::restricted()`
- `EvaluationProfile::permissive(RuntimeLimits)`
- static `RESTRICTED_EVALUATION_PROFILE`

Behavior:

- restricted: deterministic, sandboxed, runtime-only outputs denied
- permissive: caller-supplied limits, runtime-only outputs allowed

Document metadata can also request Lyma profiles such as `data`, `safe`, and `trusted`. The evaluator maps those declarations against the host profile policy and will reject `trusted` unless the active host profile is also named `trusted`.

## LYBA binary layer

Enable the facade feature explicitly:

```toml
[dependencies]
lyma = { version = "0.1", features = ["lyba"] }
```

Primary types re-exported from `lyma::lyba`:

- `LybaFile`, `Document`
- `Reader`, `ReadOptions`
- `Writer`, `WriteOptions`
- `Value`
- `Limits`
- `WriterMode`, `CanonicalMode`, `TextReconstructionMode`
- verifier API via `lyma::lyba::verify::Verifier`
- policy enums via `lyma::lyba::policy::{TrustPolicy, ReservedFlagPolicy, ExtensionNamePolicy}`

Minimal round-trip example:

```rust
use lyma::lyba::{Document, Limits, ReadOptions, Reader, Value, WriteOptions, Writer};

let file = lyma::lyba::LybaFile::new().with_document(
    Document::new().with_root_value(Value::String(String::from("hello"))),
);

let bytes = Writer::new(WriteOptions::new().with_limits(Limits::public())).write(&file)?;
let decoded = Reader::new(ReadOptions::new().with_limits(Limits::public())).read(&bytes)?;

assert_eq!(decoded.documents.len(), 1);
# Ok::<(), lyma::lyba::LybaError>(())
```

Portable-value helpers are also available when you already have `lyma::syntax::LymaValue` data:

```rust
use lyma::LymaNull;
use lyma::LymaValue;

let bytes = lyma::lyba::try_to_lyba_value_image(&[LymaValue::Null(LymaNull)])?;
let values = lyma::lyba::try_from_lyba_value_image(&bytes)?;

assert_eq!(values, vec![LymaValue::Null(LymaNull)]);
# Ok::<(), lyma::lyba::LybaError>(())
```

Security/defaults:

- `Reader` and `Verifier` are parse-only; they never execute Lua.
- `Limits::default()` equals `Limits::public()`.
- public/default trust rejects trusted-only content with `LB0019`.
- current draft implementation supports only codec `0` (`none`) for actual payload decoding/writing; other codec IDs remain extension points.

Writer modes exposed by the API:

| Mode | Purpose |
| --- | --- |
| `WriterMode::Pretty` | value-focused human/default output |
| `WriterMode::RuntimeData` | deterministic runtime data image |
| `WriterMode::EditorCache` | source + syntax + trivia + diagnostics cache |
| `WriterMode::BuildBundle` | inert bundle/dependency image |
| `WriterMode::ConformanceFixture` | fixture/value+syntax expectations |
| `WriterMode::Canonical(CanonicalMode::{Relaxed,Strict})` | canonicalized output policies |

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

`lyma::runtime` defines backend-neutral contracts:

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

## Serde adapter policy (`lyma-serde`)

Use `lyma_serde::{to_value, to_string, to_string_with_options, from_value}` to bridge Rust `serde` data with `lyma::syntax::LymaValue`.

Through the facade crate, enable `features = ["serde"]` and call the same helpers from `lyma::serde`:

```toml
[dependencies]
lyma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Example<'a> {
    name: &'a str,
    enabled: bool,
}

let text = lyma::serde::to_string(&Example {
    name: "demo",
    enabled: true,
})?;

assert_eq!(text, "enabled: true\nname: demo\n");
# Ok::<(), lyma::serde::Error>(())
```

`to_string` uses `lyma_syntax` serialization rules, so output is canonical Lyma text rather than preserving Rust field order quirks from custom emitters.

### Serialization support

`to_value` supports these Serde shapes:

- scalars: `bool`, strings, `char`, finite floats, and integers within Lyma's `i64` range
- `Option::None`, unit, and unit structs as Lyma `null`
- `Option::Some` and newtype structs as their inner value
- sequences, tuples, and tuple structs as Lyma sequences
- maps and structs as Lyma mappings
- enums in externally tagged form:
  - unit variant -> string variant name
  - newtype variant -> `{ variant: payload }`
  - tuple variant -> `{ variant: [..] }`
  - struct variant -> `{ variant: { .. } }`

Unsupported or ambiguous shapes fail during serialization:

- bytes / `serde_bytes` are rejected (`unsupported: byte slices`)
- non-finite floats (`NaN`, `+/-inf`) are rejected (`unsupported: non-finite floating-point value`)
- `u64`, `u128`, or `i128` values outside signed 64-bit range are rejected (`unsupported: integer outside Lyma i64 range`)
- map keys for `to_value` must serialize to scalar Lyma keys only: string, number, or boolean
- map keys that serialize to `null`, sequences, mappings, bytes, or other non-scalars are rejected (`unsupported: non-scalar mapping key`)

`to_string` and `to_string_with_options` are stricter than `to_value`:

- they first serialize to `LymaValue`, then emit canonical Lyma text
- all mapping keys in the resulting value must be strings
- numeric and boolean keys are allowed by `to_value` but rejected by text serialization with an error directing callers to use `to_value`

### Deserialization support

`from_value` supports:

- matching scalar Lyma null/bool/number/string values
- sequences for sequences, tuples, and tuple structs
- mappings for maps and structs
- `null` for unit, unit structs, and `Option::None`
- any non-null value for `Option::Some`
- enums from either:
  - a string variant name
  - a single-entry mapping whose key is the string variant name
  - a tagged Lyma value whose tag name is treated as the variant name

Tagged Lyma values are otherwise transparent: when the Rust target is not an enum, `from_value` ignores the tag wrapper and deserializes the tagged payload directly.

Unsupported or mismatched deserialization fails with explicit shape errors:

- bytes / byte buffers are rejected as unsupported
- `char` requires a one-character string
- enum maps must have exactly one entry and that entry's key must be a string variant name
- runtime-only Lyma values (`function`, `userdata`, `host object`) and runtime-only host mapping keys cannot be deserialized and report explicit runtime-only errors
