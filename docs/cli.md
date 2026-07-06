# CLI

## Overview

`lyma-cli` provides parse, check, eval, format, conformance, and optional LYBA commands.

```sh
cargo run -p lyma-cli -- <command>
```

Global flag:

- `--output human|json`

## Commands

### `parse`

Parse only.

```sh
cargo run -p lyma-cli -- parse app.lyma --emit ast
```

`--emit` values:

- `none`
- `ast`
- `source`

### `check`

Parse by default; evaluate when `--evaluate` is set.

```sh
cargo run -p lyma-cli -- check app.lyma
cargo run -p lyma-cli -- check app.lyma --evaluate --emit value --engine omnilua
```

### `eval`

Parse and evaluate.

```powershell
cargo run -p lyma-cli --features engine-omnilua -- eval app.lyma --emit value --engine omnilua
```

If the CLI is built without an evaluation backend, eval returns a stable diagnostic explaining that an engine feature is required.

`lyma-cli` uses `EvaluationOptions::default()`, which means restricted evaluation. The current OmniLua backend fails closed when restricted sandbox limits cannot be enforced, so this command is useful as a backend/diagnostic smoke path rather than a turnkey execution profile.

`--emit` values:

- `none`
- `ast`
- `value`
- `source`

### `fmt`

Format a document.

```sh
cargo run -p lyma-cli -- fmt app.lyma
```

Default emit mode is `source`.

### `conformance`

Run the conformance harness.

```sh
cargo run -p lyma-cli -- conformance --all-features
```

### `lyba` (`--features lyba`)

Opt-in LYBA encode/decode/inspect/verify support.

The feature gate is explicit at both layers:

- library: `lyma = { version = "0.1", features = ["lyba"] }`
- CLI: `cargo run -p lyma-cli --features lyba -- lyba ...`

```sh
cargo run -p lyma-cli --features lyba -- lyba encode values.lyma values.lyba --mode runtime-data --footer --checksum crc32c
cargo run -p lyma-cli --features lyba -- lyba decode values.lyba --trusted
cargo run -p lyma-cli --features lyba -- lyba inspect values.lyba --emit capabilities --trusted
cargo run -p lyma-cli --features lyba -- lyba verify values.lyba
```

Subcommands:

- `encode <input.lyma> <output.lyba>` parses static LYMA values and writes a LYBA file without evaluating Lua/imports/includes
- `decode <input.lyba>` reads inert root values from any supported LYBA mode
- `inspect <input.lyba>` emits `header`, `sections`, `values`, `resources`, or `capabilities`
- `verify <input.lyba>` validates the file and reports verifier diagnostics

Writer mode mapping:

| CLI flag | Writer mode | Intended output |
| --- | --- | --- |
| `--mode value` | `WriterMode::Pretty` | value-only/default image |
| `--mode runtime-data` | `WriterMode::RuntimeData` | deterministic runtime-data artifact |
| `--mode editor-cache` | `WriterMode::EditorCache` | source/syntax/trivia/diagnostics cache |
| `--mode bundle` | `WriterMode::BuildBundle` | inert dependency/resource bundle |
| `--mode fixture` | `WriterMode::ConformanceFixture` | conformance fixture image |
| `--canonical` | `WriterMode::Canonical(Relaxed)` | relaxed canonical output |
| `--strict` | `WriterMode::Canonical(Strict)` | strict canonical output |

Common LYBA flags:

- `--mode value|runtime-data|editor-cache|bundle|fixture`
- `--canonical` for relaxed canonical writer mode
- `--strict` for strict canonical writer mode
- `--public` / `--trusted` to override trust policy without changing numeric limits
- `--limits public|strict|trusted` to select the concrete API limit preset

Encode-only flags:

- `--include-source` embeds source text as inert source metadata
- `--footer` emits the fixed footer
- `--checksum none|crc32c` sets section-entry checksum metadata

Default LYBA CLI limits match `lyma-lyba`'s public API preset:

- max input bytes: 8 MiB
- max decoded logical bytes per section: 16 MiB
- max section payload bytes: 2 MiB
- max blob display bytes: 64 KiB
- max JSON output bytes: 8 MiB

Trust defaults:

- `--limits public` is the default
- `--public` keeps public trust even if you choose a larger numeric preset
- `--trusted` only changes trust policy; it does **not** execute stored code
- trusted-only sections or values are rejected under the public policy with `LB0019`

`decode` and `inspect` never execute Lua or resolve runtime descriptors. Trusted-only inputs fail under the default/public policy with `LB0019`; `--trusted` allows inert inspection only.

Oversized decode/inspect output is bounded by the active limits preset. The CLI returns summarized JSON payloads or deterministic limit failures instead of rendering unbounded data.

`encode` does **not** evaluate Lua, imports, includes, loops, spreads, conditionals, or `let` bindings. It rejects unsupported syntax with diagnostics instead of executing or dropping it.

Supported draft coverage in the current implementation is Level 0-5 section families, but with draft 0.1 caveats:

- actual compression support is currently codec `0` only (`none`)
- oversized `values`/`resources`/`capabilities` output is summarized or rejected by the active limits preset rather than streamed unbounded

## No-subcommand mode

Running `lyma INPUT` behaves like `check INPUT` and supports:

- `--emit`
- `--evaluate`
- `--engine`

## Output model

- human mode prints diagnostics and payload text/json
- json mode returns `command`, `ok`, `diagnostics`, and any emitted payload
