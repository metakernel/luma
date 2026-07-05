# Luma examples

This directory contains small, runnable examples for the public `luma` facade.

## Parser-only examples

These use Luma's default `parser` feature and do not create a Lua runtime:

```powershell
cargo run --example parse_and_format
cargo run --example tooling
```

`tooling` demonstrates the current upstream editor primitives:

- syntax lookup from a byte offset via `Parsed::syntax_index()`
- whole-document formatting as canonical text edits
- conservative range formatting edits
- incremental parse session updates through the current full-reparse shell

Those primitives are intentionally lower-level than full LSP features. Downstream
servers such as `lumals` should layer semantic tokens, find-references, rename,
and workspace indexing on top of them.

## Evaluation example

Evaluation is optional. Enable the `omnilua` feature to use the ergonomic
`Loader` facade with the OmniLua backend:

```powershell
cargo run --example loader_omnilua --features omnilua
```

Running the same example without `--features omnilua` prints a short message
instead of evaluating source.

## CLI sample input

`app.luma` is a small source file you can use with the CLI:

```powershell
cargo run -p luma-cli -- parse examples/app.luma --emit ast
cargo run -p luma-cli -- fmt examples/app.luma
```
