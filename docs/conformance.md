# Conformance

## Test harness

Conformance is driven by `tests/conformance.rs` plus the env-filtered harness in `tests/harness/mod.rs`.

Primary command:

```powershell
cargo test --test conformance --all-features
```

Supported filters:

- `LUMA_CONFORMANCE_LEVEL`
- `LUMA_CONFORMANCE_SECTION`
- `LUMA_CONFORMANCE_PROFILE`
- `LUMA_CONFORMANCE_BACKEND`

Example:

```powershell
$env:LUMA_CONFORMANCE_LEVEL='level3'; $env:LUMA_CONFORMANCE_BACKEND='omnilua'; cargo test --test conformance --all-features
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
- Level 4 verifies canonical formatting and portable serialization.

## What conformance means here

Luma uses conformance tests to lock down:

- stable diagnostics
- stable AST/value behavior
- backend-neutral evaluator semantics
- security defaults
- public tooling outputs
