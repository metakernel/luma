# Conformance

## Test harness

Conformance is driven by `tests/conformance.rs` plus the env-filtered harness in `tests/harness/mod.rs`.

Primary command:

```powershell
cargo test --test conformance --all-features
```

Supported filters:

- `LYMA_CONFORMANCE_LEVEL`
- `LYMA_CONFORMANCE_SECTION`
- `LYMA_CONFORMANCE_PROFILE`
- `LYMA_CONFORMANCE_BACKEND`

Example:

```powershell
$env:LYMA_CONFORMANCE_LEVEL='level3'; $env:LYMA_CONFORMANCE_BACKEND='omnilua'; cargo test --test conformance --all-features
```

## Matrix

| Level | Scope | Backend/profile coverage | Status |
| --- | --- | --- | --- |
| 0 | core parsing, diagnostics, ordering | parser | implemented |
| 1 | directives, blocks, comments, multi-doc AST | parser | implemented |
| 2 | safe evaluation, imports/includes/modules, runtime limits | mock engine, OmniLua, safe profile | implemented |
| 3 | metadata, schema behavior, tags, runtime-only output rejection | mock engine, OmniLua, data/safe behavior | implemented |
| 4 | formatting, serialization, tooling facade | parser/tooling | implemented |

## Current status notes

- Level 0/1 verify parser conformance only.
- Level 2/3 require the `eval` feature; OmniLua-specific coverage additionally requires `engine-omnilua`.
- Level 4 verifies canonical formatting and portable-value serialization through
  the tooling facade.
- `lyma::tooling::serialize_portable_value` intentionally remains the
  `lyma_syntax`-level wrapper for existing `LymaValue` users; typed Rust `serde`
  entry points live in `lyma-serde` / `lyma::serde` and feed the same portable
  value model rather than replacing this conformance surface.

## What conformance means here

Lyma uses conformance tests to lock down:

- stable diagnostics
- stable AST/value behavior
- backend-neutral evaluator semantics
- security defaults
- public tooling outputs
