# Public API

## Facade crate

`luma` re-exports workspace crates behind features:

- `luma::syntax`
- `luma::parser`
- `luma::runtime`
- `luma::eval`
- `luma::engine_omnilua`
- `luma::tooling`

`luma::version()` returns the crate version.

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
