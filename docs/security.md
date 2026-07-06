# Security model

## Defaults

Lyma is safe-by-default at both layers:

- parser APIs are engine-agnostic and never execute Lua
- evaluation capabilities are opt-in

`EvaluationOptions::default()` configures:

- restricted profile
- no imports/includes resolver
- no module registry
- no tag resolver
- no schema validator
- unknown tags rejected for schema-validated documents

`lyma::lyba::Limits::default()` configures the public LYBA policy preset:

- trust policy: public
- max input bytes: 8 MiB
- max decoded logical bytes per section: 16 MiB
- max stored section payload bytes: 2 MiB
- max blob display bytes: 64 KiB
- max JSON/CLI output bytes: 8 MiB
- reserved flags rejected
- extension names warned on by default

## Resolver safety model

`ResolverPolicy` defaults to `deny_all()`:

- no filesystem roots
- no URI schemes
- no network access
- max depth `0`

When enabled, the shared resolver model still:

- rejects `..` traversal
- canonicalizes allowed roots
- rejects symlink escapes outside allowed roots
- tracks depth and cycles with `ResolutionContext`
- blocks networked schemes unless `allow_network` is true

## Evaluation safety model

Restricted evaluation:

- uses `RuntimeLimits::sandboxed()`
- enforces deterministic output rules
- rejects runtime-only outputs like functions/userdata/host objects
- rejects host keys in deterministic mode

The evaluator also rejects obviously unsafe global or module names such as `_G`, `_ENV`, `io`, `os`, `debug`, `require`, `load`, metatable/raw APIs, `coroutine`, `ffi`, `jit`, and known nondeterministic calls like `math.random`.

## LYBA safety model

LYBA is a binary container, not an execution format:

- readers/verifiers never execute Lua
- readers/verifiers never compile stored chunks
- readers never resolve imports/includes/modules automatically
- trusted-only content is rejected under the default public policy with `LB0019`
- `--trusted` / `Limits::trusted()` widen policy gates for inert inspection only
- current draft implementation decodes/writes codec `0` only; unsupported required codecs fail closed

## Extension points are explicit trust boundaries

The host must explicitly provide:

- `ResourceResolver`
- `ModuleRegistry`
- `TagResolver`
- `SchemaValidator`

No capability is silently enabled.

## Recommended host posture

- keep parser-only usage for untrusted input when evaluation is unnecessary
- start from `EvaluationProfile::restricted()`
- use `FilesystemResolver::new(ResolverPolicy::filesystem_only(...))` for local-only imports
- keep `allow_network = false` unless you truly need it
- prefer `InMemory*` helpers for tests and controlled embedding
- only expose deterministic/tagged values across trust boundaries
