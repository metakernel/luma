# luma

Luma stands for **LUa Markup Assembly**: a Lua-adjacent markup language with an engine-agnostic parser, optional evaluation, and safe-by-default host integration.

## Crate layout

- `luma`: public facade crate
- `luma-syntax`: stable AST, value, span, and diagnostic types
- `luma-parser`: engine-agnostic parser/formatter
- `luma-runtime`: backend-neutral runtime traits
- `luma-eval`: engine-agnostic evaluator and host extension points
- `luma-engine-omnilua`: optional OmniLua backend
- `luma-cli`: CLI

## Quickstart

### Parse without Lua

```rust
use luma::parser::{FileId, parse_str};

let parsed = parse_str(FileId(1), "example.luma", "name: Example\nenabled: true\n");
assert!(parsed.diagnostics.is_empty());
assert_eq!(parsed.file.documents.len(), 1);
```

Parser APIs are **engine-agnostic** and safe by default: parsing and formatting do not require a Lua engine or runtime access.

### Evaluate with OmniLua

```rust
use luma::engine_omnilua::OmniLuaEngine;
use luma::eval::{AstEvaluator, EvaluationOptions};
use luma::parser::{FileId, parse_str};

let parsed = parse_str(FileId(1), "example.luma", "answer: =40 + 2\n");
assert!(parsed.diagnostics.is_empty());

let engine = OmniLuaEngine::default();
let evaluator = AstEvaluator {
    engine: &engine,
    options: EvaluationOptions::default(),
};

let documents = evaluator
    .evaluate_file(&parsed.file, "example.luma", None)
    .unwrap();

assert_eq!(documents.len(), 1);
```

Enable the backend with:

```toml
[dependencies]
luma = { version = "0.1", features = ["eval", "engine-omnilua"] }
```

## CLI

```powershell
cargo run -p luma-cli -- parse examples/app.luma --emit ast
cargo run -p luma-cli -- fmt examples/app.luma
cargo run -p luma-cli -- eval examples/app.luma --emit value
cargo run -p luma-cli -- conformance --all-features
```

## Safety model

- Parser and syntax crates are engine-neutral.
- Evaluation is capability-based and deny-by-default.
- Imports/includes/modules/tags/schema validation require explicit host wiring.
- Resolver policies reject traversal and network access unless allowed.
- Default evaluation profile is restricted and deterministic.

## Docs

- `docs/getting-started.md`
- `docs/api.md`
- `docs/engine-backends.md`
- `docs/security.md`
- `docs/conformance.md`
- `docs/cli.md`
- `docs/spec-notes.md`
