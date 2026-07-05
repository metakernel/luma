# Getting started

## What Luma is

Luma means **LUa Markup Assembly**. The workspace is split so parsing is independent from evaluation:

- parse/format with `luma-parser`
- bridge Serde data with `luma-serde`
- evaluate with `luma-eval`
- plug in a backend through `luma-runtime`
- use `luma-engine-omnilua` when you want a ready-made Lua engine

Parser APIs are **engine-agnostic** and **safe by default**.

## Dependency choices

### Parser-only

```toml
[dependencies]
luma = "0.1"
```

Default features enable `parser` only.

### Parser + evaluation + OmniLua

```toml
[dependencies]
luma = { version = "0.1", features = ["eval", "engine-omnilua"] }
```

### Parser + Serde bridge

```toml
[dependencies]
luma = { version = "0.1", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
```

## Parse a document

```rust
use luma::parser::{FileId, parse_str};

let parsed = parse_str(FileId(1), "service.luma", "name: api\nreplicas: 3\n");
assert!(parsed.diagnostics.is_empty());
```

## Format a document

```rust
let result = luma::parser::format_str(
    luma::parser::FileId(1),
    "service.luma",
    "name:'api'\n",
);

assert!(result.parsed.diagnostics.is_empty());
assert_eq!(result.formatted.text, "name: api\n");
```

## Serialize to canonical Luma text

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Config<'a> {
    name: &'a str,
    enabled: bool,
}

let text = luma::serde::to_string(&Config {
    name: "api",
    enabled: true,
})?;

assert_eq!(text, "enabled: true\nname: api\n");
# Ok::<(), luma::serde::Error>(())
```

The facade helpers are `luma::serde::to_value`, `to_string`, `to_string_with_options`, and `from_value`. Text serialization is canonical and requires string mapping keys.

## Evaluate with OmniLua

```rust
use luma::engine_omnilua::OmniLuaEngine;
use luma::eval::{AstEvaluator, EvaluationOptions, EvaluationProfile};
use luma::parser::{FileId, parse_str};
use luma::runtime::RuntimeLimits;

let parsed = parse_str(FileId(1), "calc.luma", "value: =21 * 2\n");
let engine = OmniLuaEngine::default();
let profile = EvaluationProfile::permissive(RuntimeLimits::unbounded());
let evaluator = AstEvaluator {
    engine: &engine,
    options: EvaluationOptions {
        profile: &profile,
        ..EvaluationOptions::default()
    },
};

let values = evaluator.evaluate_file(&parsed.file, "calc.luma", None)?;
# Ok::<(), luma::eval::EvaluationError>(())
```

`EvaluationOptions::default()` is intentionally minimal and may fail closed on backends that cannot enforce every restricted runtime limit:

- restricted profile
- no resolver
- no module registry
- no tag resolver
- no schema validator
- unknown tags rejected for schema-validated documents

## Add host capabilities explicitly

Use these extension points only when needed:

- `ResourceResolver` for imports/includes/schema loads
- `ModuleRegistry` for `@use`
- `TagResolver` for `!tag`
- `SchemaValidator` for schema validation
- custom `ProfilePolicy` for limits/output policy

See `docs/api.md` and `docs/security.md`.
