# Level 1 LUMBA fixtures

Checked-in golden binaries for `luma-lumba` Level 1 minimal value images live here.
Normal tests read these files directly; they do **not** regenerate them.

## Files

- `minimal-values.lumba` / `.json`: scalars and one short string.
- `nested-values.lumba` / `.json`: nested sequence+mapping structure.
- `multiple-documents.lumba` / `.json`: three root documents in one image.
- `tags.lumba` / `.json`: tagged values, including nested tags.
- `duplicate-key-rejection.json`: source-only encode rejection case (`LB0016`); no `.lumba` exists because canonical Level 1 encoding must fail.
- `invalid-refs.lumba` / `.json`: malformed DOCS root reference (`LB0014`).
- `invalid-varints.lumba` / `.json`: malformed varint in DOCS payload (`LB0012`).
- `bad-section-layouts.lumba` / `.json`: section payload declared past EOF (`LB0007`).

## Regeneration

Opt-in only:

`cargo test -p luma-lumba --test level1 regenerate_level1_fixtures -- --ignored`

That ignored test rewrites the checked-in `.lumba` binaries from deterministic helpers. The JSON manifests remain source-of-truth notes and are maintained alongside the binaries.
