# Spec notes

## Naming

Lyma is documented as **LUa Markup Assembly**.

## Architecture notes

- parsing/formatting are separate from evaluation
- evaluator semantics are intended to be backend-neutral
- OmniLua is optional, not part of the parser contract
- the root `lyma` crate is the public facade

## Profile notes

- `@profile data` implies data-only output expectations
- schema-validated documents also run in data-only mode
- `@profile trusted` is rejected unless the host activates a trusted profile policy

## Unknown tags

`UnknownTagPolicy` controls fallback behavior:

- `Preserve`
- `Reject`
- `RejectForSchemaValidatedDocuments` (default)

Default behavior preserves extensibility for non-schema documents while keeping schema-bound documents strict.

## Serialization notes

Portable serialization is intentionally narrow:

- portable scalars/sequences/mappings/tagged values serialize
- runtime-only values do not
- non-string mapping keys do not
- non-finite floats do not

## Backend-neutral contract

Parser APIs are engine-agnostic. Evaluation APIs depend on runtime traits rather than on OmniLua directly, so hosts can integrate alternate Lua engines while keeping the same AST/evaluator surface.
