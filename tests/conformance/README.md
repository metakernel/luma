# Conformance corpus

Layout:
- `tests/conformance/levelN/**.luma`: source fixtures
- `tests/conformance/levelN/**.meta`: fixture metadata
- `tests/snapshots/levelN/**.{ast,json,diag,fmt}`: expected outputs
- `tests/harness/mod.rs`: corpus loader, filters, snapshot runner

Metadata keys:
- `title`
- `mode`: `parse`, `eval`, `format`, `serialize`
- `section`: comma-separated spec sections
- `profile`: `any`, `safe`, `data`, `tooling`
- `backend`: `parse`, `mock`, `omnilua`, `tooling`
- `features`: optional cargo features, comma-separated
- `relaxed_limits`, `max_instructions`, `max_table_entries`: optional eval controls

Filters are env-driven:
- `LUMA_CONFORMANCE_LEVEL=level2`
- `LUMA_CONFORMANCE_SECTION=17.1,27`
- `LUMA_CONFORMANCE_PROFILE=safe,data`
- `LUMA_CONFORMANCE_BACKEND=mock,omnilua`

Primary command:

```powershell
cargo test --test conformance --all-features
```

Example filtered run:

```powershell
$env:LUMA_CONFORMANCE_LEVEL='level3'; $env:LUMA_CONFORMANCE_BACKEND='omnilua'; cargo test --test conformance --all-features
```
