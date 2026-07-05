# luma

Luma stands for **LUa Markup Assembly**: a Lua-adjacent markup language with an engine-agnostic parser, optional evaluation, and safe-by-default host integration.

## Crate layout

- `luma`: public facade crate
- `luma-syntax`: stable AST, value, span, and diagnostic types
- `luma-serde`: Serde adapter for `LumaValue` and canonical text output
- `luma-parser`: engine-agnostic parser/formatter
- `luma-runtime`: backend-neutral runtime traits
- `luma-eval`: engine-agnostic evaluator and host extension points
- `luma-engine-omnilua`: optional OmniLua backend
- `luma-cli`: CLI

## Quickstart

### Luma at a glance

Core data syntax parses without Lua. Evaluation features such as `=`
expressions, `|lua` blocks, `@if`, `@for`, tags, imports, and host-provided
names only run when you explicitly enable an evaluator and backend.

```yaml
-- Version, profile, and metadata directives are explicit document controls.
@luma 0.1
@profile safe
@meta:
  title: Example Service
  owner: platform

--[[
Evaluation features below are inert until the host enables an evaluator.
]]
-- Local bindings keep repeated values close to the document.
let defaults:
  replicas: 3
  port: 8080
let regions:
  - us-east
  - eu-west

-- Tags let a host attach domain-specific behavior to a value.
service: !Service
  name: api
  enabled: true
  replicas: =defaults.replicas
  ports:
    - =defaults.port
    - 9090
  labels:
    tier: backend
    critical: true
  description: |
    Public HTTP API.
    Evaluated values stay explicit.
  release_note: |lua-
    local version = "2026.07"
    return "deployed from Lua block " .. version
  healthcheck:
    path: /health
    timeout_ms: =5 * 1000
  -- Control flow is evaluator-owned, not parser-owned.
  @if environment == "prod":
    debug: false
  @else:
    debug: true

pipeline:
  - build
  - test
  @for _, region in regions:
    - =region
```

### Parse without Lua

```rust
use luma::parser::{FileId, parse_str};

let parsed = parse_str(FileId(1), "example.luma", "name: Example\nenabled: true\n");
assert!(parsed.diagnostics.is_empty());
assert_eq!(parsed.file.documents.len(), 1);
```

Parser APIs are **engine-agnostic** and safe by default: parsing and formatting do not require a Lua engine or runtime access.

### Serialize Rust data to canonical Luma text

Enable the facade Serde bridge with:

```toml
[dependencies]
luma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Service<'a> {
    name: &'a str,
    replicas: i32,
}

let text = luma::serde::to_string(&Service {
    name: "api",
    replicas: 3,
})?;

assert_eq!(text, "name: api\nreplicas: 3\n");
# Ok::<(), luma::serde::Error>(())
```

`luma::serde` re-exports the facade helpers `to_value`, `to_string`, `to_string_with_options`, and `from_value`. `to_string` emits canonical Luma text and requires string mapping keys.

### Evaluate with OmniLua

```rust
use luma::engine_omnilua::OmniLuaEngine;
use luma::eval::{AstEvaluator, EvaluationOptions, EvaluationProfile};
use luma::parser::{FileId, parse_str};
use luma::runtime::RuntimeLimits;

let parsed = parse_str(FileId(1), "example.luma", "answer: =40 + 2\n");
assert!(parsed.diagnostics.is_empty());

let engine = OmniLuaEngine::default();
let profile = EvaluationProfile::permissive(RuntimeLimits::unbounded());
let evaluator = AstEvaluator {
    engine: &engine,
    options: EvaluationOptions {
        profile: &profile,
        ..EvaluationOptions::default()
    },
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
cargo run -p luma-cli -- conformance --all-features
```

`luma-cli eval` uses restricted evaluation defaults. The current OmniLua backend fails closed when asked to enforce unsupported sandbox limits, so hosts that want executable evaluation should use the library API and choose an explicit profile.

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
