# Luma File Format Specification

**Status:** Public draft 0.1  
**Recommended extension:** `.luma`  
**Recommended media type:** `application/vnd.luma`  
**Encoding:** UTF-8  
**Design intent:** YAML-inspired structure with Lua as a first-class expression, composition, and scripting layer.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Design Goals](#2-design-goals)
3. [Non-Goals](#3-non-goals)
4. [Terminology](#4-terminology)
5. [File Identity](#5-file-identity)
6. [Document Shape](#6-document-shape)
7. [Lexical Rules](#7-lexical-rules)
8. [Comments](#8-comments)
9. [Indentation](#9-indentation)
10. [Core Data Model](#10-core-data-model)
11. [Scalars](#11-scalars)
12. [Strings](#12-strings)
13. [Block Strings](#13-block-strings)
14. [Mappings](#14-mappings)
15. [Sequences](#15-sequences)
16. [Inline Lua Values](#16-inline-lua-values)
17. [Lua Expressions](#17-lua-expressions)
18. [Lua Blocks](#18-lua-blocks)
19. [Let Bindings](#19-let-bindings)
20. [Lua Prelude Blocks](#20-lua-prelude-blocks)
21. [Directives](#21-directives)
22. [Imports](#22-imports)
23. [Host Modules](#23-host-modules)
24. [Includes](#24-includes)
25. [Spread Entries](#25-spread-entries)
26. [Conditionals](#26-conditionals)
27. [Loops](#27-loops)
28. [Tags](#28-tags)
29. [Metadata](#29-metadata)
30. [Multiple Documents](#30-multiple-documents)
31. [Evaluation Model](#31-evaluation-model)
32. [Evaluation Environment](#32-evaluation-environment)
33. [Security Profiles](#33-security-profiles)
34. [Determinism](#34-determinism)
35. [Schemas](#35-schemas)
36. [Diagnostics](#36-diagnostics)
37. [Canonical Formatting](#37-canonical-formatting)
38. [Serialization](#38-serialization)
39. [Cycles and Shared References](#39-cycles-and-shared-references)
40. [Grammar Sketch](#40-grammar-sketch)
41. [Generic Examples](#41-generic-examples)
42. [Host API Recommendations](#42-host-api-recommendations)
43. [Conformance Levels](#43-conformance-levels)
44. [Versioning](#44-versioning)
45. [Glossary](#45-glossary)
46. [Final Philosophy](#46-final-philosophy)

---

# 1. Overview

**Luma** is a human-readable structured data format that takes inspiration from YAML's indentation-based readability while making Lua a native part of the format.

A Luma document can be used as plain structured data:

```luma
@luma 0.1
@profile data

id: service.cache
name: "Cache Service"
enabled: true
port: 6379

limits:
  memory_mb: 512
  max_connections: 1000

tags:
  - internal
  - low_latency
```

The same format can also use Lua expressions for derived values:

```luma
@luma 0.1
@profile safe

let base_port = 8000
let instance_index = 3

service:
  id: service.api
  host: localhost
  port: =base_port + instance_index
  url: ="http://" .. _here.host .. ":" .. _here.port
```

And it can embed Lua blocks for advanced behavior, validation, generation, transformation, or host-controlled scripting:

```luma
@luma 0.1
@profile safe

transform: |lua
  return function(record)
    record.slug = string.lower(record.title):gsub("%s+", "-")
    return record
  end
```

Luma is not a YAML superset. It intentionally avoids YAML's implicit type surprises, anchors, and broad compatibility burden. Its core rule is:

> Simple data stays simple. Scripted data is visibly scripted. Unsafe behavior requires explicit trust.

---

# 2. Design Goals

## 2.1 Human-Readable

Luma should be easy to write and review by hand:

- indentation-based structure;
- minimal punctuation for common data;
- clear mappings and sequences;
- stable formatting for version control diffs;
- predictable scalar rules;
- readable long strings;
- source spans suitable for editor diagnostics.

## 2.2 Lua-Native

Lua is not an external templating language bolted onto the format. Lua is a first-class part of Luma:

- Lua expressions may appear as values;
- Lua chunks may define callbacks, validators, generators, and transformations;
- Lua table constructors may be used as inline values;
- Lua-style comments are used;
- Lua-style string escapes are supported;
- host-provided Lua modules can extend the format;
- parsing and evaluation are separate phases.

## 2.3 Safe by Default

A Luma parser must be able to parse a file without executing Lua.

A conforming implementation should support:

- parse-only mode;
- data-only mode;
- safe evaluated mode;
- trusted evaluated mode;
- host-controlled module exposure;
- resource limits for Lua execution;
- import and include resolution policies;
- deterministic evaluation profiles.

## 2.4 Deterministic

Luma should be suitable for repeatable builds, configuration validation, documentation generation, and automated data processing.

The format avoids:

- implicit timestamps;
- environment-dependent values by default;
- unordered iteration in deterministic profiles;
- automatic filesystem or network access;
- YAML-style broad boolean coercion;
- duplicate keys by default.

## 2.5 Good for Public Data Files

Luma should work well for:

- application configuration;
- package manifests;
- content metadata;
- API descriptions;
- data pipelines;
- workflow definitions;
- schemas;
- validation rules;
- build configuration;
- static site metadata;
- template data;
- test fixtures;
- editor-readable structured documents.

---

# 3. Non-Goals

Luma is not intended to be:

1. **A YAML-compatible format**
   - YAML files are not guaranteed to be valid Luma.
   - Luma does not support YAML anchors.
   - Luma uses Lua comments, not `#` comments.

2. **A replacement for Lua source files**
   - Complex libraries should still be written as `.lua` files.
   - Luma is structured data with Lua escape hatches.

3. **A format that executes automatically on parse**
   - Parsing must not execute Lua.
   - Evaluation is explicit.

4. **A binary serialization format**
   - Luma is text-first.
   - Binary data should be referenced externally.

5. **A security sandbox by itself**
   - The format defines security expectations.
   - The host implementation must enforce them.

---

# 4. Terminology

## 4.1 Document

A complete Luma unit inside a file. A file may contain one document or a stream of multiple documents.

## 4.2 Node

A syntactic value in the parsed document tree.

## 4.3 Scalar

A single atomic value such as a string, number, boolean, or null.

## 4.4 Mapping

An ordered set of key-value pairs.

## 4.5 Sequence

An ordered list of values.

## 4.6 Static Model

The parsed representation of a Luma document before any Lua code is evaluated.

## 4.7 Evaluated Model

The runtime representation after expressions, Lua blocks, imports, includes, spreads, loops, conditionals, and tag resolvers have been applied.

## 4.8 Host

The program embedding the Luma parser and evaluator.

## 4.9 Resolver

A host-provided component that resolves imports and includes.

## 4.10 Module Registry

A host-provided component that exposes safe or trusted Lua modules to Luma evaluation.

---

# 5. File Identity

## 5.1 Extension

Recommended extension:

```text
.luma
```

Optional descriptive extensions may be used by projects:

```text
.config.luma
.schema.luma
.catalog.luma
.workflow.luma
.document.luma
.manifest.luma
```

## 5.2 Media Type

Recommended media type:

```text
application/vnd.luma
```

For text processing tools, this may also be treated as:

```text
text/luma
```

## 5.3 Encoding

A Luma file must be UTF-8.

Allowed line endings:

```text
LF
CRLF
```

Loaders must normalize line endings internally to `LF`.

A UTF-8 byte order mark may appear at the beginning of a file, but emitters should not write one.

NUL bytes are invalid.

---

# 6. Document Shape

A Luma file may contain:

```text
optional file directives
optional imports and module declarations
optional let bindings
one root value
optional additional documents
```

The most common root value is a mapping:

```luma
@luma 0.1

id: example.record
name: "Example Record"
enabled: true
```

The root value may also be a sequence:

```luma
@luma 0.1

- alpha
- beta
- gamma
```

Or a scalar:

```luma
@luma 0.1

"single value"
```

For public configuration and data files, a root mapping is recommended because it is easier to extend over time.

---

# 7. Lexical Rules

## 7.1 Character Set

Luma source text is Unicode encoded as UTF-8.

Control characters are invalid outside quoted strings and block content, except for:

```text
tab inside non-indentation content
line feed
carriage return as part of CRLF
```

## 7.2 Significant Characters

The following characters have structural meaning outside strings and Lua blocks:

```text
:
-
@
!
|
>
=
[
]
{
}
,
...
---
```

## 7.3 Whitespace

Spaces are significant for indentation.

Tabs are not allowed for indentation.

Trailing whitespace should be ignored by parsers but should not be emitted by formatters.

## 7.4 Blank Lines

Blank lines are allowed between entries and inside block strings.

Outside block strings and Lua blocks, blank lines do not affect structure.

---

# 8. Comments

Luma uses Lua-style comments.

## 8.1 Line Comments

A line comment starts with `--` outside quoted strings, block strings, and Lua chunks.

```luma
id: service.api -- stable service identifier
port: 8080     -- public HTTP port
```

## 8.2 Full-Line Comments

```luma
-- Basic service configuration.

id: service.api
enabled: true
```

## 8.3 Block Comments

Full implementations should support Lua-style block comments outside strings and Lua blocks:

```luma
--[[
This comment can span multiple lines.
It does not appear in evaluated output.
]]

id: example.document
```

A minimal implementation may support only line comments.

## 8.4 No `#` Comments

The `#` character is not a comment marker in Luma.

```luma
label: Section #1
```

The value is the string:

```text
Section #1
```

This choice keeps the format aligned with Lua, where `#` is the length operator.

---

# 9. Indentation

## 9.1 Spaces Only

Indentation must use spaces.

Tabs used as indentation are invalid.

## 9.2 Indentation Defines Nesting

```luma
server:
  host: localhost
  port: 8080
  tls:
    enabled: true
    cert: ./certs/local.pem
```

Equivalent conceptual structure:

```lua
{
  server = {
    host = "localhost",
    port = 8080,
    tls = {
      enabled = true,
      cert = "./certs/local.pem",
    },
  },
}
```

## 9.3 Indentation Width

Luma does not require a fixed indentation width.

However, canonical formatting uses two spaces.

Valid:

```luma
server:
  host: localhost
  port: 8080
```

Also valid:

```luma
server:
    host: localhost
    port: 8080
```

## 9.4 Sibling Indentation

Sibling entries must use the same indentation level.

Valid:

```luma
limits:
  memory_mb: 512
  workers: 4
```

Invalid:

```luma
limits:
  memory_mb: 512
    workers: 4
```

## 9.5 Indentation Errors

A parser should report indentation errors with precise source spans.

Recommended diagnostic:

```text
E0002 invalid indentation
```

---

# 10. Core Data Model

Luma has two related data models.

## 10.1 Static Data Model

The static model is produced by parsing and does not execute Lua.

It supports:

```text
null
boolean
number
string
block string
sequence
mapping
tagged value
Lua expression
Lua expression block
Lua chunk
let binding
import directive
include directive
use directive
conditional block
loop block
spread entry
metadata block
schema directive
source spans
comments, optionally
```

## 10.2 Evaluated Data Model

The evaluated model is produced after resolution and evaluation.

It supports:

```text
null sentinel
boolean
number
string
table
function, if profile permits
userdata, if host permits
host object, if host permits
tagged value, if unresolved tags are preserved
```

## 10.3 Null

Luma has an explicit null value.

Both `null` and `nil` are null literals:

```luma
missing_value: null
also_missing: nil
```

In evaluated Lua, Luma null should not be represented as raw Lua `nil` inside tables because Lua tables cannot store nil values reliably.

A conforming implementation should use a sentinel such as:

```lua
luma.null
```

## 10.4 Lua Nil Conversion

When a Lua expression or Lua block returns `nil`, the result is converted to Luma null.

```luma
optional_value: =nil
```

This produces a null value.

## 10.5 Numbers

Numbers follow Lua-style numeric syntax.

Examples:

```luma
integer: 42
negative: -12
float: 3.14159
scientific: 1.2e-4
hex_integer: 0xff
hex_float: 0x1.8p1
```

Implementations should preserve integer and float distinctions when the underlying runtime supports them.

NaN and infinity are not valid literal numbers.

## 10.6 Booleans

Only these values are booleans:

```luma
enabled: true
hidden: false
```

These values are strings:

```luma
a: yes
b: no
c: on
d: off
```

This avoids YAML-style implicit coercion surprises.

## 10.7 Strings

Strings may be plain, quoted, or block strings.

```luma
plain: Example text
quoted: "Example text"
single_quoted: 'Example text'
```

## 10.8 Sequences

A sequence is an ordered list:

```luma
items:
  - alpha
  - beta
  - gamma
```

Sequences are conceptually 1-indexed when exposed directly to Lua.

## 10.9 Mappings

A mapping is an ordered collection of key-value pairs:

```luma
settings:
  retries: 3
  timeout_ms: 5000
```

Duplicate keys are errors by default.

---

# 11. Scalars

## 11.1 Scalar Resolution Order

A plain scalar is resolved in this order:

1. empty value -> null;
2. `true` -> boolean true;
3. `false` -> boolean false;
4. `null` -> null;
5. `nil` -> null;
6. Lua-style number -> number;
7. otherwise -> string.

Examples:

```luma
a: true       -- boolean
b: false      -- boolean
c: null       -- null
d: nil        -- null
e: 42         -- number
f: 3.5        -- number
g: yes        -- string
h: off        -- string
i: item_01    -- string
```

## 11.2 Empty Values

An empty mapping value becomes null unless it has a nested block.

```luma
icon:
```

Equivalent:

```luma
icon: null
```

With a nested block:

```luma
metadata:
  created_by: system
  reviewed: false
```

`metadata` is a mapping, not null.

## 11.3 Ambiguous Values

Use quotes when a value should remain a string but looks like another scalar type.

```luma
version: "0.1"
flag_text: "true"
null_text: "null"
number_text: "42"
```

---

# 12. Strings

## 12.1 Plain Strings

Plain strings continue until the end of the line or before a structural `--` comment.

```luma
name: Example Service
```

The value is:

```text
Example Service
```

Leading and trailing whitespace is trimmed.

To preserve leading or trailing spaces, use quoted strings or block strings.

## 12.2 Double-Quoted Strings

Double-quoted strings use Lua-style escapes.

```luma
message: "Line one\nLine two"
unicode: "Symbol: \u{2605}"
path: "C:\\Temp\\example.txt"
```

## 12.3 Single-Quoted Strings

Single-quoted strings are supported and follow Lua string behavior.

```luma
label: 'Example Label'
```

Emitters should prefer double quotes for consistency.

## 12.4 Escape Sequences

Recommended supported escapes:

```text
\a
\b
\f
\n
\r
\t
\v
\\
\"
\'
\z
\ddd
\xXX
\u{XXX}
```

These follow standard Lua string conventions where applicable.

---

# 13. Block Strings

Luma supports YAML-style block strings for readable long text.

## 13.1 Literal Block

```luma
description: |
  This is a literal block.
  Line breaks are preserved.
```

Result:

```text
This is a literal block.
Line breaks are preserved.
```

By default, one trailing newline is preserved.

## 13.2 Strip Final Newline

```luma
description: |-
  No trailing newline is kept.
```

## 13.3 Keep Final Newlines

```luma
description: |+
  Extra trailing blank lines are preserved.


```

## 13.4 Folded Block

```luma
summary: >
  This becomes a paragraph
  where single line breaks fold
  into spaces.

  Blank lines remain paragraph breaks.
```

Result:

```text
This becomes a paragraph where single line breaks fold into spaces.

Blank lines remain paragraph breaks.
```

## 13.5 Block Indentation

The indentation of the first non-empty content line determines the content indentation.

```luma
text: |
  first line
    nested line
  third line
```

Result:

```text
first line
  nested line
third line
```

---

# 14. Mappings

## 14.1 Basic Mapping

```luma
id: package.example
name: "Example Package"
version: "1.0.0"
```

## 14.2 Nested Mapping

```luma
repository:
  type: git
  url: "https://example.invalid/repository.git"
```

## 14.3 Plain Keys

Plain keys are strings.

```luma
id: example.record
display_name: "Example Record"
cache-timeout: 30
api.version: v1
```

The key `api.version` is a single key. It does not create nesting.

## 14.4 Quoted Keys

Use quoted keys for spaces or unusual punctuation.

```luma
"display name": "Example Record"
"x-custom-header": true
"contains:colon": value
```

## 14.5 Expression Keys

Expression keys are Lua expressions inside brackets.

```luma
let field_name = "dynamic_field"

[=field_name]: 123
```

Expression keys are evaluated during the evaluation phase.

Portable public data files should avoid expression keys unless they are necessary.

## 14.6 Key Type Restrictions

For maximum portability, keys should be strings.

A full Lua runtime profile may allow evaluated keys of these types:

```text
string
number
boolean
host-approved userdata
```

These are invalid in portable deterministic mode:

```text
null
nil
NaN
function
thread
table
```

## 14.7 Duplicate Keys

Duplicate explicit keys are errors:

```luma
name: Alpha
name: Beta
```

A spread entry may be overridden by a later explicit key:

```luma
settings:
  ...defaults
  timeout_ms: 3000
```

---

# 15. Sequences

## 15.1 Basic Sequence

```luma
features:
  - search
  - export
  - notifications
```

## 15.2 Sequence of Mappings

```luma
endpoints:
  - method: GET
    path: /health
    cache: false

  - method: POST
    path: /records
    auth_required: true
```

## 15.3 Nested Sequences

```luma
matrix:
  - - 1
    - 0
    - 0
  - - 0
    - 1
    - 0
  - - 0
    - 0
    - 1
```

For compact data, Lua table constructors may be clearer:

```luma
matrix:
  - { 1, 0, 0 }
  - { 0, 1, 0 }
  - { 0, 0, 1 }
```

## 15.4 Empty Sequence

Use a Lua table constructor for an explicit empty sequence:

```luma
items: {}
```

A schema may distinguish an empty sequence from an empty mapping if the host tracks intended table shape.

For unambiguous static data, a future version may add explicit literals such as:

```luma
items: []
```

In version 0.1, `{}` is accepted as a Lua table constructor.

---

# 16. Inline Lua Values

Luma supports inline Lua table constructors.

```luma
point: { x = 12, y = 4 }
color: { r = 0.8, g = 0.7, b = 0.6, a = 1.0 }
labels: { "alpha", "beta", "gamma" }
```

These are Lua table constructors, not JSON objects.

Therefore this is valid:

```luma
range: { min = 1, max = 10 }
```

This is not valid as an inline Lua table constructor:

```luma
range: { min: 1, max: 10 }
```

Use Luma block syntax for that style:

```luma
range:
  min: 1
  max: 10
```

## 16.1 Bare Words Inside Table Constructors

Inside a Lua table constructor, bare words are Lua identifiers, not strings.

This reads variables named `alpha` and `beta`:

```luma
labels: { alpha, beta }
```

Use quoted strings:

```luma
labels: { "alpha", "beta" }
```

## 16.2 Multiline Lua Table Constructors

Inline table constructors should be single-line.

For multiline Lua expressions, use `|expr`:

```luma
settings: |expr
  {
    retries = 3,
    timeout_ms = 5000,
    backoff = "exponential",
  }
```

Or use regular Luma structure:

```luma
settings:
  retries: 3
  timeout_ms: 5000
  backoff: exponential
```

---

# 17. Lua Expressions

Lua expressions are introduced with `=`.

```luma
port: =8000 + 3
slug: =string.lower(title):gsub("%s+", "-")
path: ="/api/" .. version .. "/records"
```

The expression text after `=` is compiled conceptually as:

```lua
return <expression>
```

## 17.1 Expression Scope

Expressions may access:

```text
lexical let bindings
imports
host-approved modules
loop variables
_luma
_here
_parent
_root
_path
_file
```

Example:

```luma
service:
  host: localhost
  port: 8080
  url: ="http://" .. _here.host .. ":" .. _here.port
```

## 17.2 Sibling Access

Sibling keys are not automatically local variables.

This is invalid unless `host` was declared as a let binding:

```luma
service:
  host: localhost
  url: ="http://" .. host
```

Use `_here`:

```luma
service:
  host: localhost
  url: ="http://" .. _here.host
```

Or use `let`:

```luma
let host = "localhost"

service:
  host: =host
  url: ="http://" .. host
```

## 17.3 Expression Result Conversion

Lua result conversion rules:

```text
nil -> Luma null
Lua boolean -> boolean
Lua number -> number
Lua string -> string
Lua table -> sequence, mapping, or table
Lua function -> function if profile permits
userdata -> host value if profile permits
```

If a Lua expression returns multiple values, only the first value is used unless the expression explicitly wraps them in a table.

```luma
first_only: =some_function()
all_values: ={ some_function() }
```

## 17.4 Multiline Expressions

Use `|expr` for multiline expressions.

```luma
record: |expr
  make_record({
    id = "example",
    title = "Example Record",
    enabled = true,
  })
```

The block is compiled conceptually as:

```lua
return make_record({
  id = "example",
  title = "Example Record",
  enabled = true,
})
```

---

# 18. Lua Blocks

Lua chunks are introduced with `|lua`.

```luma
validator: |lua
  return function(value)
    return type(value.email) == "string" and value.email:find("@") ~= nil
  end
```

A `|lua` block is compiled as a Lua chunk and may contain statements.

## 18.1 Return Value

The returned Lua value becomes the Luma value.

```luma
factory: |lua
  local function create_record(id, title)
    return {
      id = id,
      title = title,
      created = true,
    }
  end

  return create_record
```

If a Lua block returns no value, the result is Luma null.

## 18.2 Function Values

Function values are allowed only in profiles that permit runtime values.

```luma
transform: |lua
  return function(record)
    record.normalized = true
    return record
  end
```

A data-only profile must reject this.

## 18.3 Block Chomping

Like text block strings, Lua blocks may use chomping indicators:

```luma
script: |lua-
  return function()
    return true
  end
```

The chomping indicator affects source text passed to Lua only in terms of final trailing newline.

---

# 19. Let Bindings

`let` defines a lexical binding that can be reused later.

A let binding does not appear in the final output.

## 19.1 Expression Let

```luma
let base_timeout = 1000
let multiplier = 3

timeout_ms: =base_timeout * multiplier
```

Output:

```lua
{
  timeout_ms = 3000,
}
```

## 19.2 Structural Let

```luma
let default_headers:
  accept: application/json
  cache-control: no-cache

request:
  headers:
    ...default_headers
    authorization: "Bearer example"
```

## 19.3 Let Scope

A `let` binding is visible to:

```text
following siblings
nested children of following siblings
Lua expressions in the same lexical scope
Lua blocks in the same lexical scope
conditionals and loops in the same lexical scope
```

A binding is not visible before it is declared.

Invalid:

```luma
value: =x
let x = 12
```

Valid:

```luma
let x = 12
value: =x
```

## 19.4 Shadowing

Inner scopes may shadow outer bindings.

```luma
let retries = 3

default_job:
  retries: =retries

critical_job:
  let retries = 8
  retries: =retries
```

## 19.5 Let Names

Let names must be valid Lua identifiers.

Valid:

```text
name
base_url
_private
x1
```

Invalid:

```text
1x
base-url
base.url
```

---

# 20. Lua Prelude Blocks

A Lua prelude block defines multiple helper bindings for the current scope.

```luma
@lua:
  local function kebab_case(text)
    return string.lower(text):gsub("%s+", "-")
  end

  return {
    kebab_case = kebab_case,
  }

title: "Example Document"
slug: =kebab_case(title)
```

The returned table from `@lua` is merged into the current lexical environment.

`@lua` does not emit document output.

## 20.1 Prelude Rules

A prelude block:

```text
may define helper functions
may return a table of bindings
may access earlier let/import/use bindings
must obey the active security profile
must not emit document values directly
```

If the prelude returns null or no value, no bindings are added.

If the prelude returns a non-table value, the loader must raise an error.

## 20.2 Binding Conflicts

If a prelude returns a binding that already exists in the same scope, the loader should reject it by default.

A trusted profile may allow override behavior, but this should be explicit.

---

# 21. Directives

Directives start with `@`.

They are not emitted into the document output unless specifically defined to do so.

## 21.1 Version Directive

```luma
@luma 0.1
```

The version directive should be the first non-comment directive in the file.

If omitted, a loader may assume `0.1`, but emitters should always write it.

## 21.2 Profile Directive

```luma
@profile safe
```

Standard profile declarations:

```text
data
safe
trusted
```

The document declares what it expects, but the host decides what is allowed.

A document declaring `trusted` must not be evaluated in a safe-only environment.

## 21.3 Schema Directive

```luma
@schema "./schemas/service.schema.luma"
```

The schema is loaded and applied after evaluation unless the host validates statically.

## 21.4 Import Directive

```luma
@import "./common.luma" as common
```

## 21.5 Include Directive

```luma
@include "./base_config.luma"
```

## 21.6 Use Directive

```luma
@use std.text as text
```

## 21.7 Lua Prelude Directive

```luma
@lua:
  return {
    double = function(x)
      return x * 2
    end,
  }
```

## 21.8 Metadata Directive

```luma
@meta:
  author: "Example Author"
  license: MIT
```

## 21.9 Reserved Directives

Reserved directive names:

```text
@luma
@profile
@schema
@import
@include
@use
@lua
@if
@elseif
@else
@for
@meta
@doc
@end
```

A literal key may still start with `@` if quoted:

```luma
"@schema": "literal key, not a directive"
```

---

# 22. Imports

Imports load another Luma document or host-approved resource into the current lexical scope.

```luma
@import "./defaults.luma" as defaults

service:
  ...defaults.service
  port: 8080
```

## 22.1 Import Syntax

```luma
@import "<uri>" as name
```

Examples:

```luma
@import "./common/settings.luma" as settings
@import "package://shared/http.luma" as http
@import "config:defaults" as defaults
```

The URI syntax is host-defined.

The alias must be a valid Lua identifier.

## 22.2 Import Result

An import evaluates to the root value of the imported document.

If the imported file contains multiple documents, the import result is a sequence of evaluated documents.

## 22.3 Import Safety

A conforming implementation must not allow arbitrary file system or network access by default.

The host must provide an import resolver.

The resolver should enforce:

```text
allowed roots
allowed URI schemes
maximum import depth
cycle detection
cache policy
profile compatibility
sandbox policy
```

## 22.4 Import Cycles

Import cycles are errors unless the host explicitly supports lazy module semantics.

Example cycle:

```text
a.luma imports b.luma
b.luma imports a.luma
```

The loader must report the full import chain.

---

# 23. Host Modules

Host modules are approved APIs made available to Luma evaluation.

```luma
@use std.text as text
@use std.path as path

file:
  name: "Example Document"
  slug: =text.kebab(file.name)
  output: =path.join("dist", file.slug .. ".html")
```

## 23.1 Use Syntax

```luma
@use module.name as alias
```

The module name is not a file path. It is resolved by the host module registry.

## 23.2 Recommended Safe Modules

A safe profile may expose pure modules such as:

```text
math
string
table
utf8
luma
std.text
std.path, pure path manipulation only
std.date, deterministic parsing only
schema helpers
reference constructors
```

## 23.3 Unsafe Modules

A safe profile should not expose:

```text
io
os
debug
package
require
loadfile
dofile
network APIs
process APIs
filesystem mutation APIs
native module loading
unseeded randomness
system time
```

## 23.4 Module Immutability

Host modules exposed in safe mode should be immutable or copied per evaluation to prevent documents from mutating shared runtime state.

---

# 24. Includes

Includes splice another evaluated document into the current container.

## 24.1 Mapping Include

```luma
@include "./base_service.luma"

id: service.api
port: 8080
```

If the included document evaluates to a mapping, its entries are inserted into the current mapping.

Later explicit entries override included entries.

## 24.2 Sequence Include

```luma
steps:
  @include "./common_steps.luma"
  - id: publish
    run: ./scripts/publish.sh
```

If the included document evaluates to a sequence, its items are appended into the current sequence.

## 24.3 Include Type Errors

Including a mapping into a sequence is an error.

Including a sequence into a mapping is an error.

Including a scalar is an error unless the host defines custom behavior.

## 24.4 Include Versus Import

Use `@import` when you want a named value:

```luma
@import "./defaults.luma" as defaults

settings:
  ...defaults.settings
  timeout_ms: 5000
```

Use `@include` when you want to splice content directly:

```luma
@include "./defaults.luma"

timeout_ms: 5000
```

---

# 25. Spread Entries

Spread entries copy entries from another table into the current mapping or sequence.

## 25.1 Mapping Spread

```luma
let defaults:
  retries: 3
  timeout_ms: 1000
  backoff: fixed

request:
  ...defaults
  timeout_ms: 5000
```

Output:

```lua
{
  request = {
    retries = 3,
    timeout_ms = 5000,
    backoff = "fixed",
  },
}
```

## 25.2 Sequence Spread

```luma
let common_steps:
  - checkout
  - install

pipeline:
  - ...common_steps
  - test
  - package
```

Output:

```lua
{
  pipeline = {
    "checkout",
    "install",
    "test",
    "package",
  },
}
```

## 25.3 Spread Rules

For mappings:

```text
spread value must evaluate to a mapping/table
entries are copied shallowly
multiple spreads are applied in order
later spreads override earlier spread keys
later explicit keys override spread keys
duplicate explicit keys are errors
```

For sequences:

```text
spread value must evaluate to a sequence/table
items are appended in order
```

## 25.4 Deep Merge

Core spread is shallow.

For deep merge, use a host-approved function:

```luma
@use luma.merge as merge

settings: =merge.deep(defaults.settings, {
  http = {
    timeout_ms = 5000,
  },
})
```

---

# 26. Conditionals

Conditional blocks emit content based on Lua expressions.

## 26.1 Mapping Conditional

```luma
let environment = "production"

settings:
  logging: info

  @if environment == "production":
    debug: false
    cache: true

  @else:
    debug: true
    cache: false
```

Only one branch emits entries.

## 26.2 Sequence Conditional

```luma
features:
  - core

  @if include_search:
    - search
    - indexing

  @else:
    - basic_filtering
```

## 26.3 Conditional Chain

Supported forms:

```luma
@if expression:
  ...

@elseif expression:
  ...

@else:
  ...
```

Rules:

```text
@if begins a chain
@elseif must follow @if or @elseif
@else must be last
branches must share indentation
branch content must be indented
```

## 26.4 Truthiness

Luma uses Lua truthiness:

```text
false and nil/null are false
everything else is true
```

Because Luma null is usually represented as a sentinel, the evaluator must treat that sentinel as false for conditionals.

---

# 27. Loops

Loop blocks generate repeated entries.

## 27.1 Sequence Loop

```luma
let names:
  - alpha
  - beta
  - gamma

records:
  @for name in names:
    - id: ="record." .. name
      title: =string.upper(name)
```

Output:

```lua
{
  records = {
    { id = "record.alpha", title = "ALPHA" },
    { id = "record.beta", title = "BETA" },
    { id = "record.gamma", title = "GAMMA" },
  },
}
```

## 27.2 Mapping Loop

```luma
let status_codes:
  ok: 200
  created: 201
  not_found: 404

http_status:
  @for name, code in status_codes:
    [=name]: =code
```

## 27.3 Loop Syntax

```luma
@for value in expression:
  ...

@for key, value in expression:
  ...
```

Single-variable loops iterate over sequence values.

Two-variable loops iterate over mapping entries.

## 27.4 Deterministic Iteration

When iterating over a Luma sequence, order is source order.

When iterating over a Luma mapping, order is source order.

When iterating over a raw Lua table without source order metadata, deterministic profiles must use canonical key order when possible:

```text
numbers ascending
strings bytewise lexicographic
booleans false before true
other keys rejected in deterministic mode
```

Trusted profiles may allow normal Lua table iteration, but this is not deterministic.

---

# 28. Tags

Tags annotate values with semantic meaning.

```luma
position: !Point2D
  x: 12
  y: 4

color: !Color
  r: 0.8
  g: 0.7
  b: 0.6
  a: 1.0
```

## 28.1 Tag Syntax

Inline:

```luma
created_at: !Date "2026-01-01"
```

Block:

```luma
contact: !EmailContact
  name: "Example User"
  email: user@example.invalid
```

## 28.2 Tag Names

Valid tag name styles:

```text
!Point2D
!Color
!Date
!namespace.Type
!package:type
```

## 28.3 Tag Behavior

Tags do not execute code by themselves.

The parser preserves tags as metadata.

During evaluation, the host may register tag resolvers.

Conceptual resolver:

```lua
tags["Point2D"] = function(value)
  return point2d(value.x, value.y)
end
```

## 28.4 Unknown Tags

In parse-only mode, unknown tags are preserved.

In evaluated mode, unknown tags should be handled according to host policy:

```text
preserve as tagged values
reject as errors
ignore with warning
```

The default for schema-validated documents should be to reject unknown tags.

---

# 29. Metadata

Metadata is optional information about the document.

```luma
@meta:
  title: "Example Configuration"
  author: "Example Organization"
  license: MIT
  generated: false
```

Metadata does not appear in the evaluated root value by default.

A loader should expose metadata separately.

Conceptual host representation:

```rust
struct LumaDocument {
    root: LumaValue,
    metadata: LumaMetadata,
    diagnostics: Vec<LumaDiagnostic>,
}
```

Metadata may be used for:

```text
authorship
licenses
generation hints
documentation
indexing
editor tooling
review status
```

---

# 30. Multiple Documents

Luma supports multiple documents in one file using `---`.

```luma
@luma 0.1

---
id: alpha
value: 1

---
id: beta
value: 2
```

A loader may expose this as a document stream or as a sequence of evaluated documents.

## 30.1 Document End Marker

A line containing only `...` may end a document.

```luma
---
id: alpha
...

---
id: beta
...
```

The document end marker is optional.

## 30.2 Directive Scope

File-level directives apply to all documents unless overridden.

Document-level directives apply only to the document that follows.

---

# 31. Evaluation Model

Luma loading has distinct phases.

## 31.1 Phase 1: Decode

```text
read bytes
validate UTF-8
normalize line endings
remove optional BOM
reject NUL bytes
```

## 31.2 Phase 2: Lex

```text
identify indentation
recognize comments
recognize directives
recognize map entries
recognize sequence entries
recognize block headers
preserve Lua expression text
preserve Lua block text
```

## 31.3 Phase 3: Parse

Produce a static AST.

Parsing must not execute Lua.

## 31.4 Phase 4: Resolve

Resolve:

```text
document boundaries
lexical scopes
let bindings
imports
includes
tags
spreads
conditionals
loops
```

Some resolution requires Lua evaluation.

A parse-only loader stops before this phase.

## 31.5 Phase 5: Evaluate Lua

Evaluate:

```text
= expressions
|expr blocks
|lua blocks
@lua preludes
expression keys
conditional expressions
loop expressions
tag constructors, if enabled
```

## 31.6 Phase 6: Normalize

Normalize evaluated values into the host representation:

```text
Lua nil -> Luma null
Lua tables -> sequence/mapping/table
functions -> callable values if profile permits
userdata -> host values if profile permits
```

## 31.7 Phase 7: Validate

Optional schema validation occurs after evaluation.

Some implementations may also support static validation before evaluation.

## 31.8 Phase 8: Freeze

For deterministic data pipelines, evaluated values should be frozen or copied into immutable data structures.

This prevents later callbacks or host code from mutating configuration values unexpectedly.

---

# 32. Evaluation Environment

Each evaluated document receives a controlled Lua environment.

Conceptual safe environment:

```lua
_ENV = {
  luma = luma,
  math = safe_math,
  string = safe_string,
  table = safe_table,
  utf8 = safe_utf8,

  pairs = safe_pairs,
  ipairs = ipairs,
  tonumber = tonumber,
  tostring = tostring,
  type = type,
  assert = assert,
  error = error,
  select = select,
  next = safe_next,

  -- host-approved modules are added here
}
```

## 32.1 Recommended Built-Ins

Recommended safe built-ins:

```text
luma
math
string
table
utf8
pairs
ipairs
tonumber
tostring
type
assert
error
select
next
```

Safe profiles may restrict these further.

## 32.2 Luma Runtime Object

The `luma` object should provide:

```lua
luma.null
luma.is_null(value)
luma.type(value)
luma.clone(value)
luma.freeze(value)
luma.merge_shallow(a, b)
luma.merge_deep(a, b)
luma.source(value)
luma.warn(message)
luma.error(message)
```

## 32.3 Context Variables

Expressions and blocks may access:

```text
_here      current mapping or sequence being built
_parent    parent container
_root      root document currently being built
_path      current path as sequence of keys or indices
_file      current file identity
_luma      loader context
```

Example:

```luma
record:
  title: "Example Document"
  slug: =string.lower(_here.title):gsub("%s+", "-")
```

## 32.4 Scope Isolation

Each document gets its own environment.

Imported documents get their own environment.

Shared host modules may be reused, but document-local let bindings must not leak globally.

---

# 33. Security Profiles

Luma must be safe to parse and inspect without executing code.

Evaluation profiles define what code can do.

## 33.1 Data Profile

The `data` profile allows:

```text
mappings
sequences
plain scalars
quoted strings
block strings
tags as metadata
```

The `data` profile rejects:

```text
= expressions
|expr blocks
|lua blocks
@lua
@for
@if expressions
expression keys
host modules
runtime callbacks
```

This is the safest profile.

## 33.2 Safe Profile

The `safe` profile allows deterministic Lua evaluation.

Allowed:

```text
pure expressions
pure Lua chunks
let bindings
imports through host resolver
includes through host resolver
safe standard libraries
host-approved pure modules
conditionals
loops
spreads
tag constructors registered as safe
```

Forbidden by default:

```text
io
os
debug
package
require
loadfile
dofile
collectgarbage
raw filesystem access
network access
process spawning
native module loading
unbounded loops
unbounded memory allocation
system time
unseeded randomness
```

The host should enforce:

```text
instruction limits
memory limits
recursion limits
import depth limits
execution timeout
maximum output size
maximum document size
```

## 33.3 Trusted Profile

The `trusted` profile allows broader Lua execution.

It may expose:

```text
full Lua libraries
build APIs
editor APIs
filesystem APIs
process APIs
network APIs
debug utilities
```

Trusted profile files must only come from trusted project code.

Untrusted user-provided documents should not be evaluated using the trusted profile.

## 33.4 Runtime Profile Capability

Runtime values such as functions are controlled separately from safety.

A host may support combinations such as:

```text
data only
safe data
safe runtime
trusted data
trusted runtime
```

Example:

```luma
@profile safe

normalizer: |lua
  return function(value)
    return string.lower(value)
  end
```

This is safe only if the host allows function values in output and the function's environment remains sandboxed.

---

# 34. Determinism

A deterministic loader must ensure:

```text
same input files produce same output values
same imports resolve to same resource versions
mapping order is stable
randomness is unavailable unless explicitly seeded
system time is unavailable unless explicitly provided
filesystem queries go through the resolver
network access is unavailable
floating point output is normalized where possible
```

## 34.1 Randomness

Random number generation is not available by default.

A host may provide seeded randomness:

```luma
@use std.random as random

sample:
  seed: 12345
  values: =random.sequence(_here.seed, 5)
```

Unseeded randomness should be forbidden in safe deterministic profiles.

## 34.2 Time

System time is not available by default.

Build timestamps or publication dates should be provided explicitly:

```luma
published_at: "2026-01-01T00:00:00Z"
```

## 34.3 Table Iteration

Raw Lua table iteration is not deterministic in all runtimes.

Deterministic profiles must either preserve insertion/source order or sort keys canonically.

---

# 35. Schemas

Luma schemas are optional but recommended for public data contracts.

A schema may itself be written in Luma.

## 35.1 Schema Directive

```luma
@schema "./schemas/service.schema.luma"
```

## 35.2 Basic Schema Example

```luma
@luma 0.1
@profile data

type: object

required:
  id: string
  name: string
  enabled: boolean

optional:
  description: string
  port:
    type: integer
    min: 1
    max: 65535

  tags:
    type: array
    items: string
```

## 35.3 Supported Schema Types

Recommended schema types:

```text
any
null
boolean
number
integer
string
array
object
table
function
userdata
tagged
```

## 35.4 Object Rules

```luma
type: object

required:
  id: string
  name: string

optional:
  description: string
  enabled:
    type: boolean
    default: true

forbidden:
  - deprecated_field
```

## 35.5 Array Rules

```luma
type: array
items: string
min_items: 1
max_items: 16
unique: true
```

## 35.6 Number Rules

```luma
type: number
min: 0
max: 100
finite: true
```

## 35.7 String Rules

```luma
type: string
min_length: 1
max_length: 64
pattern: "^[a-z0-9_.-]+$"
```

## 35.8 Enum Rules

```luma
type: string
enum:
  - development
  - staging
  - production
```

## 35.9 Default Values

Schemas may provide defaults:

```luma
optional:
  retries:
    type: integer
    default: 3

  timeout_ms:
    type: integer
    default: 5000
```

Defaults should be applied after evaluation but before final validation completes.

## 35.10 Custom Validators

A schema may reference host-approved validators:

```luma
validators:
  - =_root.port >= 1 and _root.port <= 65535
  - =#_root.name > 0
```

Custom validators require the safe or trusted profile.

---

# 36. Diagnostics

A Luma implementation should produce structured diagnostics.

Each diagnostic should include:

```text
severity
error code
message
file
line
column
span
related spans
hint
```

## 36.1 Recommended Error Codes

```text
E0001 invalid UTF-8
E0002 invalid indentation
E0003 tab used for indentation
E0004 unterminated string
E0005 unterminated block comment
E0006 invalid mapping key
E0007 duplicate key
E0008 invalid sequence indentation
E0009 unknown directive
E0010 invalid directive syntax
E0011 unknown tag
E0012 Lua syntax error
E0013 Lua runtime error
E0014 import not found
E0015 import cycle
E0016 include type mismatch
E0017 spread type mismatch
E0018 schema validation error
E0019 unsafe operation
E0020 resource limit exceeded
E0021 unsupported profile
E0022 reserved syntax
E0023 invalid null key
E0024 non-deterministic table iteration
E0025 function value not allowed in this profile
E0026 invalid block scalar
E0027 invalid expression key
E0028 invalid loop target
E0029 invalid tag resolver result
E0030 serialization error
```

## 36.2 Duplicate Key Diagnostic

Input:

```luma
name: Alpha
name: Beta
```

Diagnostic:

```text
E0007 duplicate key
example.luma:2:1

  name: Beta
  ^^^^

key "name" was already defined at example.luma:1:1
```

## 36.3 Lua Runtime Diagnostic

Input:

```luma
timeout_ms: =base_timeout * multiplier
```

Diagnostic:

```text
E0013 Lua runtime error
example.luma:1:13

  timeout_ms: =base_timeout * multiplier
              ^^^^^^^^^^^^^^^^^^^^^^^^^

attempt to perform arithmetic on a nil value: multiplier
```

## 36.4 Unsafe Operation Diagnostic

Input:

```luma
files: =io.popen("ls"):read("*a")
```

Safe profile diagnostic:

```text
E0019 unsafe operation
example.luma:1:8

io is not available in the safe profile
```

---

# 37. Canonical Formatting

A canonical Luma emitter should use:

```text
UTF-8 without BOM
LF line endings
two-space indentation
double quotes for strings needing quotes
plain strings only when unambiguous
one key-value pair per line
blank line between major sections
no trailing whitespace
version directive at top
imports before data
let bindings before first use
```

## 37.1 Recommended Section Order

For public data files:

```luma
@luma 0.1
@profile data
@schema "./schemas/example.schema.luma"

@import "./common/defaults.luma" as defaults
@use std.text as text

@meta:
  title: "Example Document"
  license: MIT

let default_timeout = 5000

id: example.document
name: "Example Document"
```

## 37.2 Quoting Recommendations

Use plain strings for simple identifiers:

```luma
environment: production
mode: batch
```

Use quotes for display text:

```luma
title: "Monthly Report"
```

Use block strings for long text:

```luma
description: |
  This document describes a reusable public data format.
  It is intended to be readable and scriptable.
```

## 37.3 Formatter Behavior

A formatter should preserve:

```text
semantic values
comments, where possible
directive order, where meaningful
blank lines between top-level sections
block string content
Lua block content exactly except indentation normalization
```

A formatter should not reorder mapping keys by default because source order may be meaningful to humans and deterministic loops.

---

# 38. Serialization

A Luma emitter should serialize portable data by default:

```text
null
boolean
number
string
sequence
mapping
tagged values, optionally
```

The emitter should reject by default:

```text
function
thread
userdata
host object
cyclic table
non-string mapping key
NaN
infinity
```

## 38.1 Function Serialization

Raw function bytecode must not be emitted as core Luma.

A trusted application may serialize function references as tagged values:

```luma
normalizer: !FunctionRef "transforms.normalize_email"
```

Resolving such references is host-specific and should be explicit.

## 38.2 Userdata Serialization

Host objects should be serialized through tags or plain data representations.

Example:

```luma
created_at: !Date "2026-01-01T00:00:00Z"
```

## 38.3 Stable Output

A deterministic emitter should:

```text
preserve source order when possible
emit numbers consistently
quote ambiguous strings
normalize line endings
avoid trailing whitespace
reject non-portable values
```

---

# 39. Cycles and Shared References

## 39.1 Static Luma

Static Luma structure cannot contain cycles.

## 39.2 Evaluated Lua Tables

Lua expressions may create cycles:

```luma
bad: |lua
  local t = {}
  t.self = t
  return t
```

Safe profiles should reject cyclic output unless the host explicitly allows it.

## 39.3 Shared Tables

This may produce shared references:

```luma
let shared:
  enabled: true

a: =shared
b: =shared
```

A deterministic data loader should deep-copy or freeze values so that mutating `a` does not unexpectedly mutate `b`.

Runtime profiles may preserve identity if needed.

---

# 40. Grammar Sketch

This grammar is illustrative. It is not a complete parser specification.

```ebnf
file                ::= bom? stream

stream              ::= document (document_separator document)* document_end?

document            ::= header? node?

header              ::= (blank | comment | directive)*

document_separator  ::= "---" line_end
document_end        ::= "..." line_end

node                ::= mapping | sequence | scalar_value

mapping             ::= mapping_entry+
mapping_entry       ::= indent key ":" value_suffix? line_end child_block?
                      | indent let_binding line_end child_block?
                      | indent spread_entry line_end
                      | indent directive_entry line_end child_block?

sequence            ::= sequence_entry+
sequence_entry      ::= indent "-" value_suffix? line_end child_block?
                      | indent "-" spread_expr line_end
                      | indent directive_entry line_end child_block?

key                 ::= plain_key
                      | quoted_string
                      | expression_key

expression_key      ::= "[" "=" lua_expression "]"

value_suffix        ::= scalar_value
                      | expression_value
                      | table_constructor
                      | block_header
                      | tag value_suffix?
                      | empty

scalar_value        ::= null_literal
                      | boolean_literal
                      | number_literal
                      | quoted_string
                      | plain_string

expression_value    ::= "=" lua_expression

block_header        ::= "|"
                      | "|-"
                      | "|+"
                      | ">"
                      | ">-"
                      | ">+"
                      | "|expr"
                      | "|expr-"
                      | "|expr+"
                      | "|lua"
                      | "|lua-"
                      | "|lua+"

tag                 ::= "!" tag_name

let_binding         ::= "let" identifier "=" lua_expression
                      | "let" identifier ":"

spread_entry        ::= "..." lua_expression
spread_expr         ::= "..." lua_expression

directive_entry     ::= version_directive
                      | profile_directive
                      | schema_directive
                      | import_directive
                      | include_directive
                      | use_directive
                      | lua_prelude
                      | if_directive
                      | elseif_directive
                      | else_directive
                      | for_directive
                      | meta_directive

comment             ::= "--" text_until_line_end
blank               ::= whitespace* line_end
```

---

# 41. Generic Examples

## 41.1 Application Configuration

```luma
@luma 0.1
@profile safe
@schema "./schemas/service.schema.luma"

let base_port = 8000
let environment = "production"

id: service.api
name: "Public API Service"
enabled: true

server:
  host: 0.0.0.0
  port: =base_port + 1
  public_url: ="https://api.example.invalid"

limits:
  request_timeout_ms: 5000
  max_body_mb: 16
  max_connections: 1000

logging:
  level: info

  @if environment == "production":
    pretty: false
    sample_rate: 0.1

  @else:
    pretty: true
    sample_rate: 1.0
```

## 41.2 Package Manifest

```luma
@luma 0.1
@profile data

id: package.example_tools
name: "Example Tools"
version: "1.4.2"
license: MIT

repository:
  type: git
  url: "https://example.invalid/example_tools.git"

authors:
  - name: "Example Maintainer"
    email: maintainer@example.invalid

keywords:
  - parser
  - data
  - tooling

dependencies:
  parser_core: "^2.0"
  text_utils: "^1.1"
```

## 41.3 Content Metadata

```luma
@luma 0.1
@profile safe

@use std.text as text

let title = "A Practical Guide to Structured Data"

id: article.structured_data_guide
title: =title
slug: =text.kebab(title)
summary: >
  A short introduction to readable structured data formats,
  validation, and safe embedded scripting.

authors:
  - "Example Author"

published_at: "2026-01-01T00:00:00Z"

tags:
  - data
  - configuration
  - scripting
```

## 41.4 Product Catalog

```luma
@luma 0.1
@profile safe

let tax_rate = 0.07

products:
  - id: product.notebook
    name: "Notebook"
    price: 12.00
    tax: =_here.price * tax_rate
    total: =_here.price + _here.tax

  - id: product.pen_set
    name: "Pen Set"
    price: 8.50
    tax: =_here.price * tax_rate
    total: =_here.price + _here.tax
```

## 41.5 Workflow Definition

```luma
@luma 0.1
@profile safe

let common_steps:
  - id: checkout
    run: ./scripts/checkout.sh

  - id: install
    run: ./scripts/install.sh

workflow:
  id: workflow.build_and_test
  name: "Build and Test"

  triggers:
    - push
    - pull_request

  steps:
    - ...common_steps

    - id: test
      run: ./scripts/test.sh

    - id: package
      run: ./scripts/package.sh
```

## 41.6 Data Transformation

```luma
@luma 0.1
@profile safe

transform: |lua
  return function(record)
    local normalized = {}

    for key, value in pairs(record) do
      normalized[string.lower(key)] = value
    end

    return normalized
  end
```

## 41.7 Schema Example

```luma
@luma 0.1
@profile data

type: object

required:
  id:
    type: string
    pattern: "^[a-z0-9_.-]+$"

  name:
    type: string
    min_length: 1

optional:
  enabled:
    type: boolean
    default: true

  tags:
    type: array
    items: string
    unique: true
```

---

# 42. Host API Recommendations

A robust implementation should separate parsing from evaluation.

## 42.1 Parser API

Conceptual Rust-style API:

```rust
let ast = LumaParser::new()
    .parse_file("config/service.luma", source)?;
```

The parser should not require a Lua VM.

## 42.2 Loader API

```rust
let value = LumaLoader::new()
    .profile(LumaProfile::Safe)
    .resolver(project_resolver)
    .module_registry(module_registry)
    .load(ast)?;
```

## 42.3 Resolver API

```rust
trait LumaResolver {
    fn resolve_import(
        &self,
        from: &LumaFileId,
        uri: &str,
    ) -> Result<LumaResolvedModule>;

    fn resolve_include(
        &self,
        from: &LumaFileId,
        uri: &str,
    ) -> Result<LumaResolvedDocument>;
}
```

## 42.4 Module Registry

```rust
trait LumaModuleRegistry {
    fn get_module(&self, name: &str) -> Result<LuaValue>;
}
```

## 42.5 Tag Resolver

```rust
trait LumaTagResolver {
    fn resolve_tag(
        &self,
        tag: &str,
        value: LumaValue,
        context: LumaContext,
    ) -> Result<LumaValue>;
}
```

## 42.6 Schema Validator

```rust
trait LumaSchemaValidator {
    fn validate(
        &self,
        schema: &LumaValue,
        value: &LumaValue,
    ) -> Vec<LumaDiagnostic>;
}
```

## 42.7 Source Span API

Each parsed node should be able to report:

```text
file
start line
start column
end line
end column
path within document
leading comments
trailing comments
tag metadata
original text, optionally
```

This enables:

```text
editor diagnostics
go-to-definition for imports
hover docs for schema fields
formatting
refactoring
preview tools
live reload
precise error reporting
```

---

# 43. Conformance Levels

## 43.1 Level 0: Static Data Parser

A Level 0 implementation supports:

```text
UTF-8 decoding
line comments
indentation
mappings
sequences
null
booleans
numbers
plain strings
quoted strings
literal block strings
source spans
duplicate key errors
```

It does not evaluate Lua.

## 43.2 Level 1: Complete Static Parser

A Level 1 implementation supports Level 0 plus:

```text
folded block strings
block comments
tags as metadata
directives as AST nodes
multiple documents
Lua expression nodes without evaluation
Lua block nodes without evaluation
```

## 43.3 Level 2: Safe Evaluator

A Level 2 implementation supports Level 1 plus:

```text
= expressions
|expr blocks
let bindings
safe environment
nil-to-null conversion
spreads
conditionals
loops
safe imports
safe includes
resource limits
```

## 43.4 Level 3: Runtime Evaluator

A Level 3 implementation supports Level 2 plus:

```text
|lua blocks
function output values, if enabled
host module registry
tag resolvers
schema validation
metadata extraction
```

## 43.5 Level 4: Full Tooling Implementation

A Level 4 implementation supports Level 3 plus:

```text
canonical formatter
serializer
language server support
incremental parsing
structured diagnostics
schema-aware completion
import graph analysis
deterministic build mode
```

---

# 44. Versioning

## 44.1 Format Version

The version directive declares the Luma syntax version:

```luma
@luma 0.1
```

Version numbers use:

```text
major.minor
```

## 44.2 Compatibility

Patch-level clarifications should not alter syntax.

Minor versions may add syntax that older loaders reject.

Major versions may change semantics.

## 44.3 Loader Behavior

A loader that supports `0.1` should accept:

```luma
@luma 0.1
```

A loader may reject:

```luma
@luma 0.2
@luma 1.0
```

unless it explicitly supports them.

## 44.4 Feature Gates

Future versions may add feature gates:

```luma
@feature inline_arrays
@feature typed_maps
```

Feature gates are not part of version 0.1, but the directive namespace reserves room for them.

---

# 45. Glossary

## Luma

The file format described by this specification.

## Lua Expression

A single Lua expression introduced by `=` and evaluated as `return <expression>`.

## Lua Chunk

A Lua statement block introduced by `|lua`.

## Static AST

The parsed tree before evaluation.

## Evaluated Value

The runtime value after Lua evaluation and structural resolution.

## Safe Profile

A constrained evaluation mode intended for deterministic, sandboxed execution.

## Trusted Profile

A permissive evaluation mode intended only for trusted documents.

## Host

The application embedding Luma.

## Resolver

A host component responsible for resolving imports and includes.

## Tag Resolver

A host component responsible for converting tagged values into host-specific runtime values.

---

# 46. Final Philosophy

Luma is designed to sit between static data and full scripting:

```text
JSON is simple but rigid.
TOML is readable but limited for nested generated data.
YAML is readable but often too implicit and surprising.
Lua is powerful but not always ideal as a structured document format.
Luma is structured data with visible, controlled Lua-native scriptability.
```

The guiding principles are:

```text
Simple values should be obvious.
Computed values should be marked with `=`.
Executable blocks should be marked with `|lua`.
Unsafe capabilities should require explicit trust.
Parsing should never execute code.
Evaluation should be sandboxable and deterministic when needed.
```

