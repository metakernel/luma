# LYMA

**LYMA** stands for **Lua YAML-like Markup Assembly**: a Lua-adjacent modelisation language for
structured documents that can stay pure data, or be assembled and evaluated with
explicit host capabilities.

LYMA is built as a Rust workspace with a hard boundary between parsing and
execution. You can parse, format, inspect, and serialize Lyma without linking a
Lua runtime. Evaluation is opt-in, backend-neutral, and capability-based.

## Why LYMA

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
@lyma 0.1
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

Lyma's assembly layer is intentionally host-mediated. Lyma files can reference
other Lyma files through a `ResourceResolver`; Lua helpers are exposed through a
host-approved `ModuleRegistry` and then bound with `@use`.

```text
app.lyma
common/defaults.lyma
fragments/service-base.lyma
fragments/pipeline.lyma
lua/service_helpers.lua
```

```yaml
-- app.lyma
@lyma 0.1
@profile safe
@import "./common/defaults.lyma" as defaults
@include "./fragments/service-base.lyma"
@use service.helpers as helpers

service:
  name: api
  replicas: =defaults.replicas
  release: =helpers.release_name(defaults.channel)
  banner: |lua-
    return helpers.banner("api", defaults.channel)

pipeline:
  - build
  @include "./fragments/pipeline.lyma"
```

```yaml
-- common/defaults.lyma
replicas: 3
channel: stable
regions:
  - us-east
  - eu-west
```

```yaml
-- fragments/service-base.lyma
owner: platform
tier: backend
```

```yaml
-- fragments/pipeline.lyma
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

The host decides how `./common/*.lyma` and `./fragments/*.lyma` resolve, and how
`lua/service_helpers.lua` becomes the `service.helpers` module. No filesystem,
network, module, tag, or schema capability is enabled implicitly.

## Install

Parser-only usage is the default:

```toml
[dependencies]
lyma = "0.1"
```

Enable only the layers you need:

```toml
[dependencies]
lyma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

```toml
[dependencies]
lyma = { version = "0.1", features = ["omnilua"] }
```

```toml
[dependencies]
lyma = { version = "0.1", features = ["lyba"] }
```

Feature guide:

| Feature | Enables |
| --- | --- |
| `parser` | default parser, formatter, syntax types |
| `serde` | `lyma::serde::{to_value, to_string, from_value}` |
| `lyba` | `lyma::lyba` binary container reader/writer/verifier |
| `eval` | backend-neutral evaluator and host extension traits |
| `engine-omnilua` | optional OmniLua backend |
| `omnilua` | ergonomic evaluation facade plus OmniLua backend |

`features = ["eval", "engine-omnilua"]` remains available when you want the
lower-level evaluator and backend features explicitly.

## CLI

From the workspace:

```powershell
cargo run -p lyma-cli -- parse examples/app.lyma --emit ast
cargo run -p lyma-cli -- fmt examples/app.lyma
cargo run -p lyma-cli -- check examples/app.lyma
cargo run -p lyma-cli -- conformance --all-features
cargo run -p lyma-cli --features lyba -- lyba inspect values.lyba --emit header
cargo run -p lyma-cli --features lyba -- lyba verify values.lyba
```

Evaluation is intentionally restricted by default:

```powershell
cargo run -p lyma-cli --features engine-omnilua -- eval examples/app.lyma --emit value --engine omnilua
```

The CLI uses `EvaluationOptions::default()`. Backends that cannot enforce the
restricted sandbox fail closed instead of silently weakening the policy, so host
applications should use the library API when they need a deliberate execution
profile, resolver, module registry, tag resolver, or schema validator.

LYBA commands are inert by default: `decode`, `inspect`, and `verify` never
execute Lua, compile chunks, resolve imports, or activate host modules. `encode`
accepts static portable values only and rejects runtime/eval constructs.

## Rust API

### Parse and format without Lua

```rust
use lyma::parser::{FileId, format_str, parse_str};

let source = "name:'api'\nenabled:true\n";
let parsed = parse_str(FileId(1), "service.lyma", source);
assert!(parsed.diagnostics.is_empty());
assert_eq!(parsed.file.documents.len(), 1);

let formatted = format_str(FileId(1), "service.lyma", source);
assert_eq!(formatted.formatted.text, "name: api\nenabled: true\n");
```

Parser APIs are engine-agnostic. They never create a Lua runtime and never
execute expressions.

### Editor primitives without LSP policy

```rust
use lyma::parser::{FileId, IncrementalParseInput, ParseSession, TextChange, parse_str};
use lyma::syntax::SyntaxKind;
use lyma::tooling::{TextRange, format_document_range_text_edits, format_document_text_edit};

let source = "service:\n  name:'api'\n  enabled:true\n";
let parsed = parse_str(FileId(1), "service.lyma", source);
let index = parsed.syntax_index();
let name_offset = source.find("name").unwrap();
let key_id = index.smallest_node_at_offset(name_offset).unwrap();
assert_eq!(index.node(key_id).unwrap().kind, SyntaxKind::PlainMappingKey);

let (_formatting, whole_edit) = format_document_text_edit("service.lyma", source);
assert_eq!(whole_edit.range, TextRange::new(0, source.len()));

let (_formatting, range_edits) = format_document_range_text_edits(
    "service.lyma",
    source,
    TextRange::new(name_offset, source.len()),
    Default::default(),
)?;
assert!(!range_edits.is_empty());

let mut session = ParseSession::new(FileId(1), "service.lyma");
session.parse(source);
let updated = session.apply(IncrementalParseInput::new(vec![TextChange::replace(
    TextRange::new(source.find("enabled:true").unwrap(), source.find("enabled:true").unwrap() + "enabled:true".len()),
    "enabled: false",
)]))?;
assert_eq!(updated.document.source(), "service:\n  name:'api'\n  enabled: false\n");
# Ok::<(), Box<dyn std::error::Error>>(())
```

These are upstream primitives for editor integrations: syntax lookup by offset,
canonical formatting edits, conservative range formatting, and an incremental
parse shell. Full LSP semantics such as semantic tokens, find references,
rename, and workspace indexing are downstream responsibilities for `lymals` or
other language servers.

### Serialize Rust data

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Service<'a> {
    name: &'a str,
    replicas: i32,
}

let text = lyma::serde::to_string(&Service {
    name: "api",
    replicas: 3,
})?;

assert_eq!(text, "name: api\nreplicas: 3\n");
# Ok::<(), lyma::serde::Error>(())
```

`to_string` emits canonical Lyma text and requires string mapping keys. Use
`to_value` when you want the structured `LymaValue` representation.

### Evaluate with OmniLua

```rust
use lyma::parser::FileId;
use lyma::runtime::RuntimeLimits;
use lyma::{Loader, OmniLuaEngine, Parser, Profile};

let parsed = Parser::new().parse_str(FileId(1), "calc.lyma", "answer: =40 + 2\n");
assert!(parsed.diagnostics.is_empty());

let engine = OmniLuaEngine::default();
let profile = Profile::permissive(RuntimeLimits::unbounded());
let documents = Loader::new(&engine)
    .profile(&profile)
    .load_file(&parsed.file, "calc.lyma", None)?;

assert_eq!(documents.len(), 1);
# Ok::<(), lyma::eval::EvaluationError>(())
```

Use `Loader` for the ergonomic facade, or `lyma::eval::AstEvaluator` when you
need the lower-level evaluator directly.

### Read and write LYBA without execution

```toml
[dependencies]
lyma = { version = "0.1", features = ["lyba"] }
```

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

`Reader` and `Writer` are binary container APIs only. They do not execute Lua.

## Safety model

- Parser and syntax crates are engine-neutral and never execute Lua.
- Evaluation is capability-based and deny-by-default.
- `EvaluationOptions::default()` uses a restricted profile with no resolver, no
  module registry, no tag resolver, and no schema validator.
- `lyma::lyba::Limits::default()` is the public/untrusted-input preset:
  8 MiB max input, 16 MiB max decoded logical section bytes, 2 MiB max stored
  section payload, 64 KiB max blob display, 8 MiB max JSON output, and
  `TrustPolicy::Public`.
- Imports, includes, modules, tags, and schema validation require explicit host
  wiring.
- Resolver policies reject parent traversal and network access unless the host
  deliberately allows them.
- Restricted evaluation rejects obvious unsafe global or module names such as `_G`, `_ENV`,
  `io`, `os`, `debug`, `require`, `load`, metatable/raw APIs, coroutine APIs,
  FFI/JIT hooks, and nondeterministic calls such as `math.random`.
- LYBA readers reject trusted-only sections under the public policy with
  `LB0019`; `--trusted` / `Limits::trusted()` widen inspection limits but still
  do not execute stored source.

See `docs/security.md` for the detailed trust model.

## LYBA draft 0.1 status

- opt-in only: `lyma = { features = ["lyba"] }`
- supports draft Level 0-5 section families as inert data/model constructs
- CLI writer modes: `value`, `runtime-data`, `editor-cache`, `bundle`, `fixture`,
  plus relaxed/strict canonical output
- current caveats: compression support is currently codec `0` only; trusted-only
  inspection is policy-gated; fuzz runtime execution depends on host/toolchain
  sanitizer support

## Workspace layout

- `lyma`: public facade crate
- `lyma-syntax`: stable AST, value, span, diagnostic, and serialization types
- `lyma-parser`: engine-agnostic parser, lexer, decoder, and formatter
- `lyma-serde`: Serde adapter for `LymaValue` and canonical text output
- `lyma-runtime`: backend-neutral runtime traits
- `lyma-eval`: AST evaluator and host extension points
- `lyma-engine-omnilua`: optional OmniLua backend
- `lyma-cli`: command-line parser, checker, formatter, evaluator, and
  conformance runner

## Examples

```powershell
cargo run --example parse_and_format
cargo run --example tooling
cargo run --example loader_omnilua --features omnilua
```

`examples/tooling.rs` shows lexical token/trivia lookup via `lex_str`,
hover-at-offset-style syntax lookup, formatting edits, range formatting, and
incremental parse updates using only default features. `examples/app.lyma` is a
small CLI sample input.

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
