# Getting started

## What Lyma is

Lyma means **LUa Markup Assembly**. The workspace is split so parsing is independent from evaluation:

- parse/format with `lyma-parser`
- bridge Serde data with `lyma-serde`
- evaluate with `lyma-eval`
- plug in a backend through `lyma-runtime`
- use `lyma-engine-omnilua` when you want a ready-made Lua engine

Parser APIs are **engine-agnostic** and **safe by default**.

## Dependency choices

### Parser-only

```toml
[dependencies]
lyma = "0.1"
```

Default features enable `parser` only.

### Parser + evaluation + OmniLua

```toml
[dependencies]
lyma = { version = "0.1", features = ["eval", "engine-omnilua"] }
```

### Parser + Serde bridge

```toml
[dependencies]
lyma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

### Parser + LYBA binary support

```toml
[dependencies]
lyma = { version = "0.1", features = ["lyba"] }
```

The `lyba` feature adds the inert binary container API as `lyma::lyba`. It
does not enable evaluation, Lua, or OmniLua.

## Parse a document

```rust
use lyma::parser::{FileId, parse_str};

let parsed = parse_str(FileId(1), "service.lyma", "name: api\nreplicas: 3\n");
assert!(parsed.diagnostics.is_empty());
```

## Format a document

```rust
let result = lyma::parser::format_str(
    lyma::parser::FileId(1),
    "service.lyma",
    "name:'api'\n",
);

assert!(result.parsed.diagnostics.is_empty());
assert_eq!(result.formatted.text, "name: api\n");
```

## Serialize to canonical Lyma text

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Config<'a> {
    name: &'a str,
    enabled: bool,
}

let text = lyma::serde::to_string(&Config {
    name: "api",
    enabled: true,
})?;

assert_eq!(text, "enabled: true\nname: api\n");
# Ok::<(), lyma::serde::Error>(())
```

The facade helpers are `lyma::serde::to_value`, `to_string`, `to_string_with_options`, and `from_value`. Text serialization is canonical and requires string mapping keys.

## Encode and decode LYBA

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

LYBA loading is parse-only and inert: no Lua execution, no chunk compilation,
no resolver/module activation, and no automatic import handling.

## Evaluate with OmniLua

```rust
use lyma::engine_omnilua::OmniLuaEngine;
use lyma::eval::{AstEvaluator, EvaluationOptions, EvaluationProfile};
use lyma::parser::{FileId, parse_str};
use lyma::runtime::RuntimeLimits;

let parsed = parse_str(FileId(1), "calc.lyma", "value: =21 * 2\n");
let engine = OmniLuaEngine::default();
let profile = EvaluationProfile::permissive(RuntimeLimits::unbounded());
let evaluator = AstEvaluator {
    engine: &engine,
    options: EvaluationOptions {
        profile: &profile,
        ..EvaluationOptions::default()
    },
};

let values = evaluator.evaluate_file(&parsed.file, "calc.lyma", None)?;
# Ok::<(), lyma::eval::EvaluationError>(())
```

`EvaluationOptions::default()` is intentionally minimal and may fail closed on backends that cannot enforce every restricted runtime limit:

- restricted profile
- no resolver
- no module registry
- no tag resolver
- no schema validator
- unknown tags rejected for schema-validated documents

For LYBA, `Limits::default()` equals `Limits::public()`: public trust policy,
8 MiB max input, 16 MiB max decoded logical section bytes, 2 MiB max stored
section payload, 64 KiB max blob display, and 8 MiB max JSON output.

## Add host capabilities explicitly

Use these extension points only when needed:

- `ResourceResolver` for imports/includes/schema loads
- `ModuleRegistry` for `@use`
- `TagResolver` for `!tag`
- `SchemaValidator` for schema validation
- custom `ProfilePolicy` for limits/output policy

See `docs/api.md` and `docs/security.md`.

For CLI workflows see `docs/cli.md`, especially `cargo run -p lyma-cli --features lyba -- lyba ...`.
