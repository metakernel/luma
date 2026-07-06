# Level 1 LYBA fixtures

Checked-in golden binaries for `lyma-lyba` Level 1 minimal value images live here.
Normal tests read these files directly; they do **not** regenerate them.

## Files

- `minimal-values.lyba` / `.json`: scalars and one short string.
- `nested-values.lyba` / `.json`: nested sequence+mapping structure.
- `multiple-documents.lyba` / `.json`: three root documents in one image.
- `tags.lyba` / `.json`: tagged values, including nested tags.
- `duplicate-key-rejection.json`: source-only encode rejection case (`LB0016`); no `.lyba` exists because canonical Level 1 encoding must fail.
- `invalid-refs.lyba` / `.json`: malformed DOCS root reference (`LB0014`).
- `invalid-varints.lyba` / `.json`: malformed varint in DOCS payload (`LB0012`).
- `bad-section-layouts.lyba` / `.json`: section payload declared past EOF (`LB0007`).

## Regeneration

Opt-in only:

`cargo test -p lyma-lyba --test level1 regenerate_level1_fixtures -- --ignored`

That ignored test rewrites the checked-in `.lyba` binaries from deterministic helpers. The JSON manifests remain source-of-truth notes and are maintained alongside the binaries.
