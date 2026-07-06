# Engine backends

## Overview

Lyma evaluation is backend-neutral. `lyma-eval` depends on traits from `lyma-runtime`, not on a concrete Lua implementation.

## OmniLua backend

The built-in optional backend is `lyma-engine-omnilua`.

Exports:

- `OmniLuaEngine`
- `OmniLuaChunk`
- `OmniLuaValue`
- `OmniLuaEnvironment`
- `OmniLuaModule`
- `engine_name() -> "omnilua"`

### Setup

```toml
[dependencies]
lyma = { version = "0.1", features = ["eval", "engine-omnilua"] }
```

```rust
use lyma::engine_omnilua::OmniLuaEngine;
use lyma::eval::{AstEvaluator, EvaluationOptions, EvaluationProfile};
use lyma::runtime::RuntimeLimits;

let engine = OmniLuaEngine::default();
let profile = EvaluationProfile::permissive(RuntimeLimits::unbounded());
let evaluator = AstEvaluator {
    engine: &engine,
    options: EvaluationOptions {
        profile: &profile,
        ..EvaluationOptions::default()
    },
};
```

If you keep `EvaluationOptions::default()`, evaluation stays fail-closed under the restricted profile. Backends that cannot enforce every sandbox limit must reject execution instead of silently weakening the policy.

## Backend implementor guide

To add a new engine, implement:

1. `Engine` for a stable backend name
2. `RuntimeEnvironmentFactory` for fresh isolated environments
3. `RuntimeModuleFactory` for host module creation
4. `RuntimeValueCodec` for Lyma/runtime value conversion and freezing
5. `LuaRuntimeEngine` for compile/evaluate of expressions and chunks

Your environment type must implement `RuntimeEnvironment` so Lyma can:

- fork isolated evaluation scopes
- inject builtins/context
- inject modules

Your module type must implement `RuntimeModule` so `@use` exports can be materialized.

## Backend expectations

Backends should preserve Lyma's safety model:

- return stable `LuaRuntimeError` diagnostics
- honor `RuntimeLimits`
- support isolated environments
- avoid ambient filesystem/network/process access unless the host deliberately adds it
- make value conversion deterministic where possible

## Raw engine usage

For hosts that want direct expression or chunk execution without AST evaluation, use `lyma::eval::EvaluationPlan` with any `LuaRuntimeEngine`.
