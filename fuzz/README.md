# LUMBA fuzzing

Targets:
- `lumba_read`: raw container bytes for header/table/section decoding.
- `lumba_value`: raw `VALS` payload bytes wrapped into a bounded one-section container before decode.

Checked-in corpus seeds cover: bad magic, bad version, bad header size, overlapping sections, overflowing offsets, huge counts, nonminimal varints, invalid refs, recursive values, oversized blobs, and truncated payloads.

Local/security gate when fuzz tooling is available:

```powershell
cargo +nightly fuzz run lumba_read fuzz/corpus/lumba_read -- -max_total_time=30 -rss_limit_mb=256 -timeout=5
cargo +nightly fuzz run lumba_value fuzz/corpus/lumba_value -- -max_total_time=30 -rss_limit_mb=256 -timeout=5
```

Expected gate properties: no panic, bounded memory use via `-rss_limit_mb=256` plus strict in-target `Limits`, no unbounded allocation/OOM within configured limits, and deterministic typed malformed-input errors (replay in each target must return the same error code or success classification twice).

If `cargo-fuzz` is unavailable in normal CI, or the host lacks the sanitizer runtime needed to execute libFuzzer binaries, keep the compile/smoke gate separate from default workspace builds and run the bounded fuzz command on a sanitizer-capable security host:

```powershell
cargo +nightly fuzz check lumba_read
cargo +nightly fuzz check lumba_value
cargo test -p luma-lumba --test properties
```
