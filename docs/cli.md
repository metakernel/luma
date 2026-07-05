# CLI

## Overview

`luma-cli` provides parse, check, eval, format, and conformance commands.

```sh
cargo run -p luma-cli -- <command>
```

Global flag:

- `--output human|json`

## Commands

### `parse`

Parse only.

```sh
cargo run -p luma-cli -- parse app.luma --emit ast
```

`--emit` values:

- `none`
- `ast`
- `source`

### `check`

Parse by default; evaluate when `--evaluate` is set.

```sh
cargo run -p luma-cli -- check app.luma
cargo run -p luma-cli -- check app.luma --evaluate --emit value --engine omnilua
```

### `eval`

Parse and evaluate.

```powershell
cargo run -p luma-cli --features engine-omnilua -- eval app.luma --emit value --engine omnilua
```

If the CLI is built without an evaluation backend, eval returns a stable diagnostic explaining that an engine feature is required.

`luma-cli` uses `EvaluationOptions::default()`, which means restricted evaluation. The current OmniLua backend fails closed when restricted sandbox limits cannot be enforced, so this command is useful as a backend/diagnostic smoke path rather than a turnkey execution profile.

`--emit` values:

- `none`
- `ast`
- `value`
- `source`

### `fmt`

Format a document.

```sh
cargo run -p luma-cli -- fmt app.luma
```

Default emit mode is `source`.

### `conformance`

Run the conformance harness.

```sh
cargo run -p luma-cli -- conformance --all-features
```

## No-subcommand mode

Running `luma INPUT` behaves like `check INPUT` and supports:

- `--emit`
- `--evaluate`
- `--engine`

## Output model

- human mode prints diagnostics and payload text/json
- json mode returns `command`, `ok`, `diagnostics`, and any emitted payload
