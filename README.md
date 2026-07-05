# luma

**Luma** stands for **LUa Markup Assembly**: a Lua-adjacent markup language for
structured documents that can stay pure data, or be assembled and evaluated with
explicit host capabilities.

Luma is built as a Rust workspace with a hard boundary between parsing and
execution. You can parse, format, inspect, and serialize Luma without linking a
Lua runtime. Evaluation is opt-in, backend-neutral, and capability-based.

## Why Luma

- Data-first syntax for ordered maps, lists, scalars, comments, and multiline
  text.
- Lua-adjacent evaluation for explicit values: `=expr`, `|lua` chunks, `@if`,
  `@for`, and expression keys.
- Assembly primitives for larger systems: `@import`, `@include`, `@use`, tags,
  schema hooks, and host-provided modules.
- Safe defaults: parser-only by default, deny-all evaluation options, restricted
  profiles, and resolver policies that reject traversal unless the host opts in.
- Rust integrations for parsing, formatting, Serde conversion, tooling edits,
  conformance tests, and optional OmniLua-backed evaluation.

## Language snapshot

Core data syntax parses without Lua. Anything that evaluates code or crosses a
host boundary is explicit in the document and inert until the embedding host
enables the corresponding evaluator capability.

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
let defaults:
  replicas: 3
  port: 8080
let regions:
  - us-east
  - eu-west

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

This example shows comments, metadata, local bindings, ordered maps and lists,
tags, multiline text, explicit expressions, Lua chunks, and evaluator-controlled
branches and loops.

## Assembly model

Luma's assembly layer is intentionally host-mediated. Luma files can reference
other Luma files through a `ResourceResolver`; Lua helpers are exposed through a
host-approved `ModuleRegistry` and then bound with `@use`.

```text
app.luma
common/defaults.luma
fragments/service-base.luma
fragments/pipeline.luma
lua/service_helpers.lua
```

```luma
-- app.luma
@luma 0.1
@profile safe
@import "./common/defaults.luma" as defaults
@include "./fragments/service-base.luma"
@use service.helpers as helpers

service:
  name: api
  replicas: =defaults.replicas
  release: =helpers.release_name(defaults.channel)
  banner: |lua-
    return helpers.banner("api", defaults.channel)

pipeline:
  - build
  @include "./fragments/pipeline.luma"
```

```luma
-- common/defaults.luma
replicas: 3
channel: stable
regions:
  - us-east
  - eu-west
```

```luma
-- fragments/service-base.luma
owner: platform
tier: backend
```

```luma
-- fragments/pipeline.luma
- test
- deploy
```

```lua
-- lua/service_helpers.lua
return {
  release_name = function(channel)
    return "api-" .. channel
  end,
  banner = function(name, channel)
    return string.upper(name) .. " / " .. channel
  end,
}
```

The host decides how `./common/*.luma` and `./fragments/*.luma` resolve, and how
`lua/service_helpers.lua` becomes the `service.helpers` module. No filesystem,
network, module, tag, or schema capability is enabled implicitly.

## Install

Parser-only usage is the default:

```toml
[dependencies]
luma = "0.1"
```

Enable only the layers you need:

```toml
[dependencies]
luma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```toml
[dependencies]
luma = { version = "0.1", features = ["omnilua"] }
```

Feature guide:

| Feature | Enables |
| --- | --- |
| `parser` | default parser, formatter, syntax types |
| `serde` | `luma::serde::{to_value, to_string, from_value}` |
| `eval` | backend-neutral evaluator and host extension traits |
| `engine-omnilua` | optional OmniLua backend |
| `omnilua` | ergonomic evaluation facade plus OmniLua backend |

`features = ["eval", "engine-omnilua"]` remains available when you want the
lower-level evaluator and backend features explicitly.

## CLI

From the workspace:

```powershell
cargo run -p luma-cli -- parse examples/app.luma --emit ast
cargo run -p luma-cli -- fmt examples/app.luma
cargo run -p luma-cli -- check examples/app.luma
cargo run -p luma-cli -- conformance --all-features
```

Evaluation is intentionally restricted by default:

```powershell
cargo run -p luma-cli --features engine-omnilua -- eval examples/app.luma --emit value --engine omnilua
```

The CLI uses `EvaluationOptions::default()`. Backends that cannot enforce the
restricted sandbox fail closed instead of silently weakening the policy, so host
applications should use the library API when they need a deliberate execution
profile, resolver, module registry, tag resolver, or schema validator.

## Rust API

### Parse and format without Lua

```rust
use luma::parser::{FileId, format_str, parse_str};

let source = "name:'api'\nenabled:true\n";
let parsed = parse_str(FileId(1), "service.luma", source);
assert!(parsed.diagnostics.is_empty());
assert_eq!(parsed.file.documents.len(), 1);

let formatted = format_str(FileId(1), "service.luma", source);
assert_eq!(formatted.formatted.text, "name: api\nenabled: true\n");
```

Parser APIs are engine-agnostic. They never create a Lua runtime and never
execute expressions.

### Serialize Rust data

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

`to_string` emits canonical Luma text and requires string mapping keys. Use
`to_value` when you want the structured `LumaValue` representation.

### Evaluate with OmniLua

```rust
use luma::parser::FileId;
use luma::runtime::RuntimeLimits;
use luma::{Loader, OmniLuaEngine, Parser, Profile};

let parsed = Parser::new().parse_str(FileId(1), "calc.luma", "answer: =40 + 2\n");
assert!(parsed.diagnostics.is_empty());

let engine = OmniLuaEngine::default();
let profile = Profile::permissive(RuntimeLimits::unbounded());
let documents = Loader::new(&engine)
    .profile(&profile)
    .load_file(&parsed.file, "calc.luma", None)?;

assert_eq!(documents.len(), 1);
# Ok::<(), luma::eval::EvaluationError>(())
```

Use `Loader` for the ergonomic facade, or `luma::eval::AstEvaluator` when you
need the lower-level evaluator directly.

## Safety model

- Parser and syntax crates are engine-neutral and never execute Lua.
- Evaluation is capability-based and deny-by-default.
- `EvaluationOptions::default()` uses a restricted profile with no resolver, no
  module registry, no tag resolver, and no schema validator.
- Imports, includes, modules, tags, and schema validation require explicit host
  wiring.
- Resolver policies reject parent traversal and network access unless the host
  deliberately allows them.
- Restricted evaluation rejects obvious unsafe references such as `_G`, `_ENV`,
  `io`, `os`, `debug`, `require`, `load`, metatable/raw APIs, coroutine APIs,
  FFI/JIT hooks, and nondeterministic calls such as `math.random`.

See `docs/security.md` for the detailed trust model.

## Workspace layout

- `luma`: public facade crate
- `luma-syntax`: stable AST, value, span, diagnostic, and serialization types
- `luma-parser`: engine-agnostic parser, lexer, decoder, and formatter
- `luma-serde`: Serde adapter for `LumaValue` and canonical text output
- `luma-runtime`: backend-neutral runtime traits
- `luma-eval`: AST evaluator and host extension points
- `luma-engine-omnilua`: optional OmniLua backend
- `luma-cli`: command-line parser, checker, formatter, evaluator, and
  conformance runner

## Examples

```powershell
cargo run --example parse_and_format
cargo run --example tooling
cargo run --example loader_omnilua --features omnilua
```

`examples/app.luma` is a small CLI sample input.

## Docs

- `docs/getting-started.md`
- `docs/api.md`
- `docs/engine-backends.md`
- `docs/security.md`
- `docs/conformance.md`
- `docs/cli.md`
- `docs/spec-notes.md`

## Conformance

```powershell
cargo test --test conformance --all-features
```

The conformance matrix covers parser behavior, directives, blocks, comments,
safe evaluation, imports/includes/modules, metadata, schema hooks, tags,
formatting, serialization, stable diagnostics, and backend-neutral evaluator
semantics.
