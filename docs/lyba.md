# LYBA Binary Format Specification

Status: draft 0.1  
Format name: LYBA  
Related text format: LYMA, Lua YAML-like Markup Assembly  
Recommended file extension: `.lyba`  
Recommended media type: `application/vnd.lyba`  
Default byte order: little-endian  
Execution model: parse-only and inert by default

LYBA is the binary companion format for LYMA. It stores LYMA documents, evaluated LYMA value graphs, optional syntax trees, optional source maps, optional schemas, and optional bundles in a compact, deterministic, random-access container.

LYBA is not a Lua bytecode format. LYBA parsers must not execute Lua. Lua expressions, Lua chunks, host module references, import requests, and other executable LYMA constructs are encoded only as inert data unless a host application explicitly evaluates them under a separate capability policy.

---

## 1. Purpose

LYMA is optimized for people. It is text-first, diffable, editable, and scriptable. LYBA is optimized for machines. It is binary-first, compact, indexed, deterministic, and suitable for fast loading.

A host may use LYBA for:

- cached parse output;
- canonical value serialization;
- offline build artifacts;
- editor indexes;
- package manifests;
- schema-validated data snapshots;
- import bundles;
- conformance fixtures;
- incremental tooling caches;
- fast runtime loading where text parsing is not desired.

The same logical LYMA document may have several valid LYBA representations depending on what is preserved:

| Representation | Preserves | Typical use |
| --- | --- | --- |
| Value image | evaluated or normalized values | runtime loading and data exchange |
| Syntax image | parsed source structure | editors, formatters, diagnostics |
| Bundle image | documents plus imports and metadata | distribution and build caches |
| Hybrid image | values plus selected syntax and spans | runtime data with good diagnostics |

---

## 2. Design goals

LYBA must be:

1. Deterministic
   - same canonical input produces the same canonical bytes;
   - map order is preserved;
   - section ordering is stable;
   - non-deterministic Lua runtime state is not encoded by default.

2. Safe by default
   - opening a LYBA file never executes Lua;
   - bytecode is not part of the core format;
   - host capability requirements are explicit;
   - resource limits are enforceable before allocation.

3. Random-access friendly
   - a reader can locate sections without scanning the entire payload;
   - strings, values, documents, and syntax nodes can be indexed;
   - optional source maps are separated from hot value data.

4. Portable
   - fixed byte order;
   - explicit versioning;
   - UTF-8 strings;
   - well-defined integer and float encodings;
   - extension sections are ignorable or rejectable by policy.

5. LYMA-aware
   - LYMA null is preserved distinctly from absent fields;
   - tags are first-class;
   - document streams are first-class;
   - expression source, Lua chunks, imports, includes, directives, and comments can be preserved when the writer chooses a syntax image.

6. Efficient to implement
   - sectioned container;
   - varint counts and indexes;
   - simple arenas for values and syntax nodes;
   - minimal required feature set;
   - clear conformance levels.

---

## 3. Non-goals

LYBA is not intended to be:

- a general replacement for SQLite, Parquet, MessagePack, FlatBuffers, or Capn Proto;
- a platform-specific memory dump;
- a Lua VM snapshot;
- a Lua bytecode distribution format;
- a compressed archive format by itself;
- a cryptographic trust system by itself;
- a mandatory runtime format for every LYMA implementation.

Compression, signatures, encryption, and native bytecode may be added by extensions, but they are not required for the core profile.

---

## 4. Relationship to LYMA

LYBA is a representation of the LYMA data model, not a different language.

A LYMA text document may be converted to LYBA in one of three major ways:

```text
LYMA text -> parse -> LYBA syntax image
LYMA text -> parse -> evaluate -> LYBA value image
LYMA text plus imports -> assemble -> LYBA bundle image
```

A syntax image can preserve source-level constructs such as:

- comments;
- directives;
- let bindings;
- import and include statements;
- conditionals and loops;
- Lua expression source;
- Lua chunk source;
- tags;
- source spans;
- formatting trivia, optionally.

A value image stores the normalized result after assembly and optional evaluation. It does not need to preserve how the result was produced.

Example LYMA text:

```lyma
@lyma 0.1
@profile safe

let defaults:
  replicas: 3
  timeout_ms: 5000

service:
  name: api
  replicas: =defaults.replicas
  timeout_ms: =defaults.timeout_ms
  tags:
    - public
    - http
```

A syntax image may preserve the `let` binding and expressions. A value image may store only:

```lyma
service:
  name: api
  replicas: 3
  timeout_ms: 5000
  tags:
    - public
    - http
```

Both are valid LYBA use cases.

---

## 5. Conformance levels

A LYBA implementation should declare the highest level it supports.

| Level | Name | Required support |
| --- | --- | --- |
| 0 | Container | header, section table, metadata, uncompressed sections |
| 1 | Core values | null, booleans, numbers, strings, bytes, sequences, maps, document streams |
| 2 | Tagged data | tags, schema references, annotations, diagnostic records |
| 3 | Syntax image | AST nodes, comments, source spans, expression and chunk source |
| 4 | Bundle image | imports, included resources, dependency table, content hashes |
| 5 | Extension runtime | inert Lua chunks, trusted bytecode extension, host capability declarations |

Level 1 is the recommended minimum for data interchange.

Level 3 is the recommended minimum for editor tooling.

Level 4 is the recommended minimum for build artifacts.

Level 5 is optional and must be treated as trusted or capability-gated.

### 5.1 Current `lyma-lyba` draft 0.1 implementation matrix

The current workspace implementation treats every level as **inert data support**, not execution support.

| Level | Status in `lyma-lyba` | Notes |
| --- | --- | --- |
| 0 | implemented | header/section table/footer/metadata verification |
| 1 | implemented | portable values, document stream, canonical value-image helpers |
| 2 | implemented | tags, schemas, diagnostics, policy-gated trusted content |
| 3 | implemented | source, spans, syntax, trivia for editor-oriented images |
| 4 | implemented | dependency and embedded-resource bundle sections, still inert |
| 5 | implemented as inert descriptors | capability/runtime sections are stored/inspected only; no evaluation occurs during load |

Public/default policy remains parse-only: loading, decoding, inspecting, and verifying must not execute Lua.

---

## 6. Terminology

| Term | Meaning |
| --- | --- |
| Container | The full `.lyba` file. |
| Section | A typed binary payload inside the container. |
| Section table | The index that gives section type, offset, size, codec, and checksums. |
| Value arena | Indexed storage for scalar and composite values. |
| ValueRef | Reference to one value in the value arena. |
| StringId | Reference to an interned UTF-8 string. |
| SymbolId | Reference to a string used as a repeated name, tag, directive, or type. |
| Document | One logical LYMA document in a document stream. |
| Syntax node | One node in the parsed LYMA source tree. |
| Source span | File, byte offset, line, column, and length metadata. |
| Extension | A named optional capability outside the core specification. |
| Canonical LYBA | A deterministic byte representation with restricted options. |

---

## 7. High-level file layout

A LYBA file has this structure:

```text
+-------------------------------+
| 64-byte container header       |
+-------------------------------+
| section table                  |
+-------------------------------+
| section payload 0              |
+-------------------------------+
| section payload 1              |
+-------------------------------+
| ...                           |
+-------------------------------+
| optional footer                |
+-------------------------------+
```

All multi-byte fixed-width integers are little-endian.

All section payload offsets are relative to the beginning of the file.

All section payloads must begin at an 8-byte aligned offset. Padding bytes must be zero.

A reader must reject a file when:

- the header magic is invalid;
- the declared header size is smaller than 64 bytes;
- the section table overlaps the header or a section payload;
- two section payloads overlap;
- a section offset or size exceeds the file length;
- a required section is missing;
- an unsupported required extension is declared;
- a compressed section uses an unsupported required codec.

---

## 8. Container header

The container header is exactly 64 bytes for version 0.1.

| Offset | Size | Field | Type | Description |
| ---: | ---: | --- | --- | --- |
| 0 | 8 | magic | bytes | `4C 55 4D 42 41 0D 0A 1A` |
| 8 | 2 | major_version | u16 | Major container version. Must be `0` for this draft. |
| 10 | 2 | minor_version | u16 | Minor container version. Must be `1` for this draft. |
| 12 | 2 | header_size | u16 | Must be `64` for this draft. |
| 14 | 2 | endian_marker | u16 | Must be `0x0102`; read as `0x0201` indicates wrong byte order. |
| 16 | 4 | container_flags | u32 | Global flags. |
| 20 | 4 | profile_flags | u32 | Declared profile flags. |
| 24 | 8 | section_table_offset | u64 | Absolute file offset of the section table. |
| 32 | 4 | section_count | u32 | Number of section table entries. |
| 36 | 4 | section_entry_size | u32 | Must be `64` for this draft. |
| 40 | 8 | file_length | u64 | Full file length in bytes, or zero for streaming writers before finalization. |
| 48 | 8 | root_document_count | u64 | Number of root documents, or zero if unknown before reading `DOCS`. |
| 56 | 4 | header_crc32c | u32 | CRC32C of bytes 0 through 55 with this field treated as zero. Zero means absent. |
| 60 | 4 | reserved | u32 | Must be zero. |

The magic bytes are ASCII `LYBA`, followed by carriage return, line feed, and substitute. The substitute byte helps text-mode transfer damage show up early.

### 8.1 Container flags

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | CANONICAL | File claims canonical encoding. |
| 1 | HAS_FOOTER | A footer is present. |
| 2 | HAS_SOURCE | Some source text or source spans are present. |
| 3 | HAS_SYNTAX | Syntax node sections are present. |
| 4 | HAS_VALUES | Value arena sections are present. |
| 5 | HAS_BUNDLE | Dependency or embedded-resource sections are present. |
| 6 | HAS_DIAGNOSTICS | Diagnostic section is present. |
| 7 | HAS_SIGNATURES | Signature or digest sections are present. |
| 8 | REQUIRES_EVAL_CAPABILITIES | Some stored content describes evaluation needs. |
| 9 | CONTAINS_TRUSTED_EXTENSION | At least one required extension is trusted-only. |
| 10..31 | reserved | Must be zero in version 0.1. |

### 8.2 Profile flags

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | DATA_PROFILE | Intended for data-only loading. |
| 1 | SAFE_PROFILE | Intended for safe capability-gated evaluation. |
| 2 | TRUSTED_PROFILE | Requires trusted host policy for evaluation. |
| 3 | VALUE_IMAGE | Contains normalized or evaluated values. |
| 4 | SYNTAX_IMAGE | Contains parsed syntax. |
| 5 | BUNDLE_IMAGE | Contains dependency and bundle metadata. |
| 6 | RUNTIME_VALUES_ALLOWED | Function-like or host runtime descriptors may appear as inert descriptors. |
| 7 | CANONICAL_REQUIRED | Consumers should reject non-canonical encodings. |
| 8..31 | reserved | Must be zero in version 0.1. |

Profile flags describe intent. They do not grant permission. A host loader decides what it allows.

---

## 9. Section table

The section table begins at `section_table_offset` and contains `section_count` entries. Each entry is 64 bytes in version 0.1.

| Offset | Size | Field | Type | Description |
| ---: | ---: | --- | --- | --- |
| 0 | 4 | section_id | FourCC | ASCII section identifier. |
| 4 | 2 | section_version | u16 | Version of this section layout. |
| 6 | 2 | entry_flags | u16 | Flags for this section. |
| 8 | 4 | payload_flags | u32 | Section-specific flags. |
| 12 | 2 | codec_id | u16 | Compression codec. Zero means uncompressed. |
| 14 | 2 | checksum_id | u16 | Checksum or digest algorithm. Zero means none. |
| 16 | 8 | payload_offset | u64 | Absolute file offset of stored payload. |
| 24 | 8 | stored_size | u64 | Number of bytes stored in the file. |
| 32 | 8 | logical_size | u64 | Size after decompression. Same as stored size if uncompressed. |
| 40 | 8 | item_count | u64 | Number of primary records, or zero if not applicable. |
| 48 | 8 | checksum_low | u64 | Low 64 bits of checksum or digest. |
| 56 | 8 | checksum_high | u64 | High 64 bits of checksum or digest. |

### 9.1 Section entry flags

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | REQUIRED | Reader must understand this section to load the file. |
| 1 | UNIQUE | At most one section with this FourCC may appear. |
| 2 | ORDERED | Relative order among sections of this FourCC is meaningful. |
| 3 | CRITICAL_FOR_CANONICAL | Canonical verification depends on this section. |
| 4 | PRIVATE | Section belongs to a private extension namespace. |
| 5 | TRUSTED_ONLY | Section must be ignored or rejected unless trusted policy is active. |
| 6..15 | reserved | Must be zero. |

If a section is marked `REQUIRED` and the reader does not understand its `section_id`, `section_version`, codec, or checksum algorithm, the reader must reject the file.

If a section is not required and unsupported, the reader may ignore it after validating that its offset and size are safe.

---

## 10. Core section identifiers

| FourCC | Name | Required for | Purpose |
| --- | --- | --- | --- |
| `META` | Metadata | Level 0 | Container metadata as LYBA values. |
| `EXTS` | Extension table | Level 0 when extensions exist | Required and optional extension declarations. |
| `STRS` | String table | Level 1 | Interned UTF-8 strings. |
| `SYMS` | Symbol table | Level 1 | Interned names backed by strings. |
| `BLOB` | Blob table | Level 1 when bytes exist | Raw byte arrays and large text payloads. |
| `VALS` | Value arena | Level 1 | Encoded values. |
| `DOCS` | Document table | Level 1 | Root document stream. |
| `TAGS` | Tag registry | Level 2 | Tag metadata and resolver hints. |
| `SCMA` | Schema table | Level 2 | Schema references or embedded schemas. |
| `DIAG` | Diagnostics | Level 2 | Stored diagnostics, warnings, and notes. |
| `SRCF` | Source file table | Level 3 | Source file identities. |
| `SRCS` | Source spans | Level 3 | Span records. |
| `ASTN` | Syntax nodes | Level 3 | Parsed LYMA syntax tree arena. |
| `TRIV` | Trivia | Level 3 | Comments, whitespace, and formatting trivia. |
| `DEPS` | Dependency table | Level 4 | Imports, includes, modules, and external resources. |
| `EMBD` | Embedded resources | Level 4 | Embedded LYMA or LYBA resources. |
| `CAPS` | Capability table | Level 5 | Evaluation capability declarations. |
| `SIGN` | Signatures | Optional | Digests and signatures. |
| `FOOT` | Footer mirror | Optional | Optional end-of-file table mirror. |

A minimal value-only LYBA file needs `STRS`, `VALS`, and `DOCS`. `SYMS` is recommended but may be omitted if no symbol-specific records exist.

---

## 11. Primitive encodings

### 11.1 Fixed-width integers

All fixed-width integers are little-endian.

Supported fixed-width types:

| Type | Size | Range |
| --- | ---: | --- |
| u8 | 1 | 0 to 255 |
| u16 | 2 | 0 to 65535 |
| u32 | 4 | 0 to 4294967295 |
| u64 | 8 | 0 to 18446744073709551615 |
| i8 | 1 | signed two's complement |
| i16 | 2 | signed two's complement |
| i32 | 4 | signed two's complement |
| i64 | 8 | signed two's complement |

### 11.2 Variable-length unsigned integers

`UVar` uses unsigned LEB128.

- Bits 0 through 6 of each byte carry payload.
- Bit 7 indicates continuation.
- Encodings must be minimal in canonical LYBA.
- A reader must reject encodings longer than 10 bytes for u64-range values.

### 11.3 Variable-length signed integers

`SVar` uses zigzag encoding followed by `UVar`.

```text
0  -> 0
-1 -> 1
1  -> 2
-2 -> 3
2  -> 4
```

### 11.4 Floating point

Floating point values are IEEE 754 binary64 encoded as little-endian u64 bits.

Portable canonical LYBA must reject NaN and infinity.

A non-portable trusted profile may encode non-finite floats only when the value record is marked with the `NON_PORTABLE_NUMBER` flag.

### 11.5 UTF-8 strings

All strings are UTF-8. A reader must reject invalid UTF-8 in string records.

Strings are usually interned in the `STRS` section and referenced by `StringId`.

### 11.6 Byte arrays

Raw bytes are stored in `BLOB` or inline when small. Byte arrays have no implied character encoding.

### 11.7 IDs and references

The following reference types are encoded as `UVar`:

| Reference | Meaning |
| --- | --- |
| StringId | zero-based index into `STRS` |
| SymbolId | zero-based index into `SYMS` |
| BlobId | zero-based index into `BLOB` |
| ValueId | zero-based index into `VALS` |
| ValueRef | zero means absent, otherwise `ValueId + 1` |
| NodeId | zero-based index into `ASTN` |
| NodeRef | zero means absent, otherwise `NodeId + 1` |
| SpanId | zero-based index into `SRCS` |
| SpanRef | zero means absent, otherwise `SpanId + 1` |

Absent is not the same as LYMA null. LYMA null is an explicit value record.

---

## 12. Metadata section: `META`

The `META` section stores container-level metadata.

Payload layout:

```text
UVar metadata_value_ref
UVar created_by_string_ref_or_absent
UVar source_profile_symbol_ref_or_absent
UVar lyma_version_string_ref_or_absent
UVar lyba_version_string_ref_or_absent
UVar reserved_count
repeated reserved fields
```

The metadata value should be a map value in the `VALS` section. It may contain fields such as:

```lyma
format: lyba
lyba_version: 0.1
lyma_version: 0.1
image_kind: value
producer: lyma-cli
canonical: true
```

Metadata is descriptive. Loaders must not rely on metadata instead of validating the header and section table.

---

## 13. Extension table section: `EXTS`

The `EXTS` section declares extension features used by the file.

Payload layout:

```text
UVar extension_count
repeat extension_count:
  UVar name_string_id
  UVar version_string_id
  UVar flags
  UVar metadata_value_ref
```

Extension flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | REQUIRED | Reader must understand this extension. |
| 1 | TRUSTED_ONLY | Requires trusted policy. |
| 2 | AFFECTS_CANONICAL | Extension affects canonical byte verification. |
| 3 | MAY_CONTAIN_CODE | Extension may contain executable source or bytecode descriptors. |
| 4 | MAY_RESOLVE_EXTERNAL | Extension may reference external resources. |
| 5..63 | reserved | Must be zero. |

Example extension names:

```text
org.lyma.compression.zstd
org.lyma.signature.ed25519
org.lyma.lua.bytecode.lua54
org.lyma.schema.inline
```

Extension names use reverse-DNS or `org.lyma.*` names. Private extensions should not use the `org.lyma` namespace.

---

## 14. String table section: `STRS`

The `STRS` section stores interned UTF-8 strings.

Payload layout:

```text
UVar string_count
repeat string_count:
  UVar flags
  UVar byte_length
  byte[byte_length] utf8_bytes
```

String flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | NORMALIZED_NFC | Producer claims Unicode NFC normalization. |
| 1 | ASCII_ONLY | String contains only bytes 0 through 127. |
| 2 | PRIVATE | String belongs to a private extension. |
| 3..63 | reserved | Must be zero. |

Canonical LYBA should intern strings by first occurrence in deterministic traversal order.

A reader must not require strings to be unique, but a canonical verifier must reject duplicate strings in `STRS` unless an extension explicitly allows them.

---

## 15. Symbol table section: `SYMS`

The `SYMS` section stores names used frequently as keys, tags, directives, node kinds, profiles, and section annotations.

Payload layout:

```text
UVar symbol_count
repeat symbol_count:
  UVar string_id
  UVar namespace_string_ref
  UVar flags
```

Symbol flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | KEY_SYMBOL | Common map key. |
| 1 | TAG_SYMBOL | Tag name. |
| 2 | DIRECTIVE_SYMBOL | Directive name. |
| 3 | NODE_KIND_SYMBOL | Syntax node kind. |
| 4 | PROFILE_SYMBOL | Profile name. |
| 5 | EXTENSION_SYMBOL | Extension-owned symbol. |
| 6..63 | reserved | Must be zero. |

The namespace field is absent when zero. When present, it references a string such as `lyma`, `schema`, `syntax`, or an extension namespace.

---

## 16. Blob section: `BLOB`

The `BLOB` section stores raw byte arrays, large strings, source text blocks, and extension payloads.

Payload layout:

```text
UVar blob_count
UVar offset_table_byte_length
u64[blob_count] relative_offsets
repeat blob_count:
  UVar flags
  UVar byte_length
  byte[byte_length] bytes
```

Offsets are relative to the start of the blob record area after the offset table.

Blob flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | UTF8_TEXT | Bytes are valid UTF-8 text. |
| 1 | SOURCE_TEXT | Bytes are original or generated source text. |
| 2 | LUA_SOURCE | Bytes are Lua source text. |
| 3 | GENERATED | Blob was generated by a tool. |
| 4 | EXTERNAL_DIGEST_TARGET | Blob participates in external digest verification. |
| 5 | PRIVATE | Blob belongs to a private extension. |
| 6..63 | reserved | Must be zero. |

Small byte arrays may be encoded inline in `VALS`; large byte arrays should use `BLOB`.

---

## 17. Value arena section: `VALS`

The `VALS` section stores all values in an indexed arena.

Payload layout:

```text
UVar value_count
u64[value_count] value_offsets
repeat value_count:
  value_record
```

Each `value_offsets` entry is relative to the start of the value record area.

Each value record starts with:

```text
u8 value_tag
UVar value_flags
UVar span_ref
payload by tag
```

`span_ref` is zero when no source span is associated with the value.

### 17.1 Value tags

| Tag | Name | Payload |
| ---: | --- | --- |
| 0x00 | Null | none |
| 0x01 | BoolFalse | none |
| 0x02 | BoolTrue | none |
| 0x03 | Int | SVar |
| 0x04 | UInt | UVar |
| 0x05 | Float64 | u64 IEEE bits |
| 0x06 | DecimalString | StringId |
| 0x07 | String | StringId |
| 0x08 | BytesInline | UVar length plus bytes |
| 0x09 | BytesBlob | BlobId |
| 0x0A | Sequence | sequence payload |
| 0x0B | Map | map payload |
| 0x0C | Tagged | tag payload |
| 0x0D | ExpressionSource | expression payload |
| 0x0E | LuaChunkSource | Lua chunk payload |
| 0x0F | RuntimeDescriptor | inert runtime descriptor payload |
| 0x10 | ExtensionValue | extension payload |
| 0x11..0x7F | reserved core | reserved for future versions |
| 0x80..0xFF | private extension | extension-defined |

A reader must reject unknown core tags. A reader may ignore unknown extension values only when they appear in unsupported optional sections or metadata; it must reject them when they appear in required document values.

### 17.2 Value flags

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | CANONICAL | Record claims canonical encoding. |
| 1 | FROZEN | Value is intended immutable after load. |
| 2 | GENERATED | Value was generated by evaluation or tooling. |
| 3 | FROM_SOURCE | Value has source provenance. |
| 4 | NON_PORTABLE_NUMBER | Number is not portable, such as NaN or infinity. |
| 5 | REQUIRES_EVALUATION | Value is inert source requiring evaluator capability to produce runtime value. |
| 6 | TRUSTED_ONLY | Value must not be evaluated outside trusted policy. |
| 7 | HAS_ANNOTATIONS | Annotation map is associated through extension or metadata. |
| 8..63 | reserved | Must be zero. |

### 17.3 Null

LYMA null is encoded as tag `0x00`. It is not equivalent to absent references.

Examples:

| Concept | Encoding |
| --- | --- |
| optional field missing in a section record | ValueRef zero |
| map key present with null value | map entry value_ref points to a Null value |
| Lua expression evaluated to nil | Null value, usually with `GENERATED` flag |

### 17.4 Booleans

False and true are encoded as distinct tags so no payload is required.

### 17.5 Numbers

Integers should use `Int` or `UInt` when the exact integer type is known.

Floating values use `Float64`.

`DecimalString` stores arbitrary precision decimal text. Consumers that do not support decimal arithmetic may preserve it as a tagged numeric value or reject it by schema policy.

Canonical rules:

- use `Int` for negative integers;
- use `UInt` for non-negative integers when the source type is unsigned;
- use `Int` for non-negative integers when the source type is signed or unknown;
- use shortest valid `SVar` or `UVar` encoding;
- reject non-finite floats in portable canonical output.

### 17.6 Strings

String values reference `STRS` by `StringId`.

Plain string versus quoted string syntax is not preserved in value images. Syntax images may preserve it through `ASTN` and `TRIV`.

### 17.7 Byte values

Inline bytes are for small byte arrays. Producers should use inline bytes only when length is at most 64 bytes.

Large bytes should use `BytesBlob`.

Canonical LYBA should use `BytesBlob` for byte arrays larger than 64 bytes.

### 17.8 Sequences

Sequence payload:

```text
UVar element_count
repeat element_count:
  UVar element_value_ref
```

Element order is preserved.

Sparse arrays are not part of the core value model. If a host needs sparse arrays, it should encode a map with integer keys or use an extension value.

### 17.9 Maps

Map payload:

```text
UVar entry_count
repeat entry_count:
  UVar key_value_ref
  UVar value_ref
  UVar entry_flags
  UVar entry_span_ref
```

Map order is preserved. Duplicate keys are invalid in canonical LYBA.

Core portable maps should use string keys. LYBA can encode non-string keys because evaluated Lua tables may contain them, but schema-bound and canonical public interchange should reject non-string keys unless the schema explicitly permits them.

Map entry flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | GENERATED_KEY | Key was produced by an expression or tool. |
| 1 | GENERATED_VALUE | Value was produced by an expression or tool. |
| 2 | FROM_SPREAD | Entry came from a spread operation. |
| 3 | FROM_INCLUDE | Entry came from an include. |
| 4 | OVERRIDES_PREVIOUS | Entry overrides an earlier generated entry. |
| 5 | EXPLICIT_SOURCE_ENTRY | Entry was explicit in source. |
| 6..63 | reserved | Must be zero. |

Duplicate key detection uses LYMA equality rules for keys. For canonical portable maps, keys are UTF-8 strings and equality is byte-for-byte equality.

### 17.10 Tagged values

Tagged payload:

```text
UVar tag_symbol_id
UVar inner_value_ref
UVar tag_flags
UVar resolver_hint_value_ref
```

Tag flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | PRESERVE_IF_UNKNOWN | Unknown tags may be preserved. |
| 1 | REJECT_IF_UNKNOWN | Unknown tags must reject. |
| 2 | RESOLVED_BY_HOST | A host resolver already processed this tag. |
| 3 | SCHEMA_TYPE_TAG | Tag identifies a schema or type. |
| 4 | CONSTRUCTOR_TAG | Tag may invoke a constructor during evaluation. |
| 5..63 | reserved | Must be zero. |

Tags do not execute by themselves. A tag resolver is a host capability.

### 17.11 Expression source values

Expression payload:

```text
UVar language_symbol_id
UVar source_string_or_blob_ref
UVar expression_flags
UVar capability_set_ref
UVar result_value_ref
```

`language_symbol_id` is usually `lua.expr`.

`source_string_or_blob_ref` points to a string for short expressions or a blob for long expressions. The high bit of the reference kind is not overloaded; the expression flags declare whether the reference is a string or blob.

Expression flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | SOURCE_IS_BLOB | Source reference is a BlobId. Otherwise it is a StringId. |
| 1 | WAS_EVALUATED | Result value is available. |
| 2 | RESULT_TRUSTED | Result came from trusted evaluation. |
| 3 | PURE_HINT | Producer believes expression is pure. |
| 4 | DETERMINISTIC_HINT | Producer believes expression is deterministic. |
| 5 | REQUIRES_HOST_CONTEXT | Expression references host context. |
| 6 | REQUIRES_IMPORTS | Expression references imports or modules. |
| 7 | TRUSTED_ONLY | Must not evaluate outside trusted policy. |
| 8..63 | reserved | Must be zero. |

If `WAS_EVALUATED` is set, `result_value_ref` points to the result. Otherwise it is zero.

### 17.12 Lua chunk source values

Lua chunk payload:

```text
UVar language_symbol_id
UVar source_blob_id
UVar chunk_flags
UVar capability_set_ref
UVar result_value_ref
```

`language_symbol_id` is usually `lua.chunk`, `lua54`, `lua53`, `luajit`, `luau`, or a host-defined dialect symbol.

Lua chunk source is inert. Loading the value does not compile or run it.

Chunk flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | RETURNS_VALUE | Chunk is expected to return a value. |
| 1 | RETURNS_FUNCTION | Chunk is expected to return a callable. |
| 2 | WAS_EVALUATED | Result value is available. |
| 3 | RESULT_TRUSTED | Result came from trusted evaluation. |
| 4 | REQUIRES_HOST_CONTEXT | Chunk uses host context. |
| 5 | REQUIRES_MODULES | Chunk uses host modules. |
| 6 | TRUSTED_ONLY | Must not evaluate outside trusted policy. |
| 7 | BYTECODE_AVAILABLE_EXTENSION | A bytecode extension section may contain trusted bytecode for this chunk. |
| 8..63 | reserved | Must be zero. |

Core LYBA stores Lua source, not Lua bytecode.

### 17.13 Runtime descriptors

Runtime descriptors are inert descriptions of values that cannot be serialized portably.

Payload:

```text
UVar descriptor_kind_symbol_id
UVar descriptor_flags
UVar descriptor_value_ref
```

Examples of descriptor kinds:

```text
function.ref
host.object.ref
module.symbol
external.resource
```

A runtime descriptor is not the runtime value. The host may resolve it after validating policy.

### 17.14 Extension values

Extension payload:

```text
UVar extension_name_string_id
UVar extension_type_symbol_id
UVar extension_flags
UVar payload_blob_id
UVar fallback_value_ref
```

If the extension is unsupported and a fallback value exists, a reader may use the fallback only when the extension is not required.

---

## 18. Document table section: `DOCS`

The `DOCS` section stores the logical LYMA document stream.

Payload layout:

```text
UVar document_count
repeat document_count:
  UVar document_flags
  UVar root_value_ref
  UVar root_node_ref
  UVar metadata_value_ref
  UVar schema_ref
  UVar profile_symbol_ref
  UVar source_file_ref
  UVar dependency_set_ref
```

Document flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | HAS_VALUE_ROOT | `root_value_ref` is present. |
| 1 | HAS_SYNTAX_ROOT | `root_node_ref` is present. |
| 2 | WAS_EVALUATED | Document value root was evaluated. |
| 3 | DATA_ONLY | Document contains no executable constructs in the stored image. |
| 4 | HAS_METADATA | Metadata value is present. |
| 5 | HAS_SCHEMA | Schema reference is present. |
| 6 | HAS_DEPENDENCIES | Dependency set is present. |
| 7 | TRUSTED_REQUIRED | Evaluation requires trusted policy. |
| 8 | GENERATED | Document was generated by tooling. |
| 9..63 | reserved | Must be zero. |

A document may have both a value root and a syntax root.

A value-only reader ignores syntax roots.

A syntax-aware reader may reconstruct source-level structure when `ASTN`, `SRCS`, and optionally `TRIV` are present.

---

## 19. Tag registry section: `TAGS`

The `TAGS` section stores known tag declarations.

Payload layout:

```text
UVar tag_count
repeat tag_count:
  UVar tag_symbol_id
  UVar tag_uri_string_ref
  UVar tag_flags
  UVar schema_ref
  UVar metadata_value_ref
```

Tag flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | KNOWN_TO_PRODUCER | Producer knew this tag. |
| 1 | HAS_SCHEMA | Schema reference present. |
| 2 | REQUIRES_RESOLVER | Host resolver needed for evaluated value. |
| 3 | PORTABLE | Tag has portable semantics. |
| 4 | TRUSTED_ONLY | Resolver requires trusted policy. |
| 5..63 | reserved | Must be zero. |

Example tag registry as LYMA metadata:

```lyma
tags:
  - name: Service
    uri: urn:lyma:example:service
    portable: true
  - name: Duration
    uri: urn:lyma:example:duration
    portable: true
```

---

## 20. Schema table section: `SCMA`

The `SCMA` section stores schema references or embedded schemas.

Payload layout:

```text
UVar schema_count
repeat schema_count:
  UVar schema_flags
  UVar schema_uri_string_ref
  UVar schema_value_ref
  UVar schema_digest_blob_ref
  UVar metadata_value_ref
```

Schema flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | URI_PRESENT | Schema URI is present. |
| 1 | VALUE_PRESENT | Embedded schema value is present. |
| 2 | DIGEST_PRESENT | Digest is present. |
| 3 | VALIDATED_BY_PRODUCER | Producer validated documents against this schema. |
| 4 | REQUIRED_BY_DOCUMENT | At least one document requires this schema. |
| 5 | TRUSTED_VALIDATOR_REQUIRED | Schema uses custom validators. |
| 6..63 | reserved | Must be zero. |

Schemas are data until a host schema validator runs. Custom validators must be treated as evaluation capabilities.

---

## 21. Diagnostics section: `DIAG`

The `DIAG` section stores diagnostics produced while parsing, assembling, evaluating, validating, or writing.

Payload layout:

```text
UVar diagnostic_count
repeat diagnostic_count:
  UVar severity
  UVar code_symbol_id
  UVar message_string_id
  UVar primary_span_ref
  UVar related_count
  repeat related_count:
    UVar related_span_ref
    UVar related_message_string_ref
  UVar diagnostic_flags
```

Severity values:

| Value | Name |
| ---: | --- |
| 0 | note |
| 1 | help |
| 2 | warning |
| 3 | error |
| 4 | fatal |

A LYBA file may contain diagnostics and still be loadable. A host policy decides whether warning-bearing or error-bearing files are accepted.

Canonical release artifacts should not contain parse or validation errors.

---

## 22. Source file table section: `SRCF`

The `SRCF` section identifies source files or virtual source buffers.

Payload layout:

```text
UVar source_file_count
repeat source_file_count:
  UVar uri_string_id
  UVar display_name_string_ref
  UVar source_flags
  UVar source_blob_ref
  UVar digest_blob_ref
```

Source flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | URI_PRESENT | URI string is meaningful. |
| 1 | SOURCE_EMBEDDED | Source text is embedded through `source_blob_ref`. |
| 2 | DIGEST_PRESENT | Digest is present. |
| 3 | GENERATED_SOURCE | Source was generated. |
| 4 | VIRTUAL_SOURCE | Source does not correspond to a filesystem path. |
| 5 | PRIVATE_SOURCE | Source path should not be displayed without policy approval. |
| 6..63 | reserved | Must be zero. |

Source URIs are identifiers, not permissions. A loader must not read external paths merely because a source URI appears.

---

## 23. Source span section: `SRCS`

The `SRCS` section stores source spans.

Payload layout:

```text
UVar span_count
repeat span_count:
  UVar source_file_id
  UVar byte_offset
  UVar byte_length
  UVar start_line
  UVar start_column
  UVar end_line
  UVar end_column
  UVar span_flags
```

Lines and columns are one-based. Byte offsets are zero-based and refer to UTF-8 bytes in the associated source.

Span flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | GENERATED | Span was generated by tooling. |
| 1 | SYNTHETIC | Span does not map directly to source bytes. |
| 2 | MACRO_OR_INCLUDE_EXPANSION | Span came from an include or assembly operation. |
| 3 | EXPRESSION_RESULT | Span points to source that produced an evaluated result. |
| 4..63 | reserved | Must be zero. |

---

## 24. Syntax node section: `ASTN`

The `ASTN` section stores parsed LYMA syntax nodes.

Payload layout:

```text
UVar node_count
u64[node_count] node_offsets
repeat node_count:
  syntax_node_record
```

Node record header:

```text
UVar node_kind_symbol_id
UVar node_flags
UVar primary_span_ref
UVar leading_trivia_ref
UVar trailing_trivia_ref
UVar field_count
repeat field_count:
  UVar field_name_symbol_id
  UVar field_kind
  field_payload
```

Field kinds:

| Value | Name | Payload |
| ---: | --- | --- |
| 0 | absent | none |
| 1 | bool | u8 |
| 2 | uvar | UVar |
| 3 | svar | SVar |
| 4 | string | StringId |
| 5 | symbol | SymbolId |
| 6 | value_ref | ValueRef |
| 7 | node_ref | NodeRef |
| 8 | node_list | count plus NodeRef values |
| 9 | span_ref | SpanRef |
| 10 | blob_ref | BlobId |
| 11 | token_text | StringId |
| 12 | extension | extension payload |

Core syntax node kinds should include:

```text
document
mapping
map_entry
sequence
sequence_entry
plain_scalar
quoted_scalar
block_string
lua_expression
lua_chunk
tagged_value
directive
let_binding
import_directive
include_directive
use_directive
if_block
elseif_block
else_block
for_block
spread_entry
expression_key
comment
error_node
```

The syntax node model is intentionally flexible so the text grammar can evolve without rewriting the entire binary format.

A syntax image should preserve enough information to support diagnostics, semantic navigation, and lossless or mostly lossless reformatting depending on the trivia level chosen by the producer.

---

## 25. Trivia section: `TRIV`

The `TRIV` section stores comments and formatting trivia.

Payload layout:

```text
UVar trivia_count
repeat trivia_count:
  UVar trivia_kind
  UVar span_ref
  UVar text_string_or_blob_ref
  UVar trivia_flags
```

Trivia kinds:

| Value | Name |
| ---: | --- |
| 0 | whitespace |
| 1 | newline |
| 2 | line_comment |
| 3 | block_comment |
| 4 | blank_line |
| 5 | indentation |
| 6 | punctuation |
| 7 | malformed |
| 8 | extension |

A canonical value image should omit `TRIV`. A syntax image intended for editor round-tripping should include it.

---

## 26. Dependency section: `DEPS`

The `DEPS` section records imports, includes, host modules, schemas, external resources, and build-time dependencies.

Payload layout:

```text
UVar dependency_count
repeat dependency_count:
  UVar dependency_kind
  UVar uri_string_id
  UVar alias_symbol_ref
  UVar flags
  UVar source_span_ref
  UVar resolved_digest_blob_ref
  UVar metadata_value_ref
```

Dependency kinds:

| Value | Name | Meaning |
| ---: | --- | --- |
| 0 | import | LYMA `@import` |
| 1 | include | LYMA `@include` |
| 2 | module | LYMA `@use` host module |
| 3 | schema | schema reference |
| 4 | source | original source file |
| 5 | generated | generated intermediate |
| 6 | external_resource | non-LYMA external resource |
| 7 | extension | extension-defined dependency |

Dependency flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | REQUIRED | Dependency is required to reproduce the image. |
| 1 | EMBEDDED | Dependency payload is embedded in `EMBD`. |
| 2 | RESOLVED | Producer resolved the dependency. |
| 3 | DIGEST_PRESENT | Digest is present. |
| 4 | NETWORK_URI | URI may require network resolution. |
| 5 | FILE_URI | URI may refer to a filesystem path. |
| 6 | HOST_MODULE | Dependency is a host module. |
| 7 | TRUSTED_ONLY | Requires trusted policy. |
| 8..63 | reserved | Must be zero. |

Loaders must not resolve dependencies automatically. Resolution is a host capability.

---

## 27. Embedded resource section: `EMBD`

The `EMBD` section stores embedded resources used by bundles.

Payload layout:

```text
UVar embedded_count
repeat embedded_count:
  UVar dependency_ref
  UVar resource_kind
  UVar flags
  UVar blob_ref
  UVar metadata_value_ref
```

Resource kinds:

| Value | Name |
| ---: | --- |
| 0 | lyma_text |
| 1 | lyba_container |
| 2 | schema_lyma |
| 3 | lua_source |
| 4 | generic_bytes |
| 5 | extension |

Embedded resources are inert. A bundle loader may expose them to a resolver only after policy checks.

---

## 28. Capability table section: `CAPS`

The `CAPS` section describes evaluation capabilities needed by stored expression and chunk source.

Payload layout:

```text
UVar capability_set_count
repeat capability_set_count:
  UVar flags
  UVar capability_count
  repeat capability_count:
    UVar capability_symbol_id
    UVar requirement_flags
    UVar metadata_value_ref
```

Capability examples:

```text
lua.eval.expr
lua.eval.chunk
module.resolve
resource.import
resource.include
tag.resolve
schema.validate
host.context.read
host.context.write
runtime.function.create
```

Requirement flags:

| Bit | Name | Meaning |
| ---: | --- | --- |
| 0 | REQUIRED_FOR_EVALUATION | Required to evaluate. |
| 1 | REQUIRED_FOR_REPRODUCTION | Required to reproduce same value image. |
| 2 | PURE_EXPECTED | Producer expects pure behavior. |
| 3 | DETERMINISTIC_EXPECTED | Producer expects deterministic behavior. |
| 4 | TRUSTED_ONLY | Requires trusted policy. |
| 5 | MAY_READ_EXTERNAL | Capability may read external data. |
| 6 | MAY_WRITE_EXTERNAL | Capability may mutate external state. |
| 7..63 | reserved | Must be zero. |

The capability table is descriptive and enforceable. A host must deny anything it has not explicitly allowed.

---

## 29. Signature and digest section: `SIGN`

The `SIGN` section stores digests, signatures, and integrity metadata.

Payload layout:

```text
UVar record_count
repeat record_count:
  UVar record_kind
  UVar algorithm_symbol_id
  UVar covered_range_kind
  UVar covered_section_count
  repeat covered_section_count:
    UVar section_table_index
  UVar payload_blob_id
  UVar metadata_value_ref
```

Record kinds:

| Value | Name |
| ---: | --- |
| 0 | digest |
| 1 | signature |
| 2 | certificate_chain |
| 3 | transparency_record |
| 4 | extension |

Core LYBA does not mandate any signature algorithm. Signature verification is a host policy.

A digest or signature does not make evaluation safe. It only helps establish integrity and provenance.

---

## 30. Optional footer

If `HAS_FOOTER` is set, the file ends with a footer.

Footer layout:

```text
u64 section_table_offset
u32 section_count
u32 section_entry_size
u64 file_length
u32 footer_crc32c
u32 magic_footer
```

`magic_footer` is the ASCII bytes `LMBF` interpreted as a little-endian u32.

The footer enables readers to discover the section table from the end of a file. This is useful for append-oriented writers.

When both header and footer are present, they must agree.

---

## 31. Compression codecs

Core LYBA requires support only for uncompressed sections.

Codec IDs:

| ID | Name | Requirement |
| ---: | --- | --- |
| 0 | none | required |
| 1 | zstd | optional extension |
| 2 | deflate | optional extension |
| 3 | lz4 | optional extension |
| 4..32767 | reserved | future standard codecs |
| 32768..65535 | private | private codecs |

A section using a nonzero codec must declare the corresponding extension in `EXTS` when the section is required.

Canonical LYBA should use no compression. Distribution LYBA may use compression.

Implementation note: the current `lyma-lyba` reader/writer supports codec ID `0` (`none`) only. Other codec IDs remain draft extension points and are rejected when required.

---

## 32. Checksums

Checksum IDs:

| ID | Name | Size |
| ---: | --- | ---: |
| 0 | none | 0 |
| 1 | crc32c | 32 bits |
| 2 | xxh3_64 | 64 bits |
| 3 | blake3_128 | 128 bits |
| 4..32767 | reserved | varies |
| 32768..65535 | private | varies |

Checksums are computed over the stored payload bytes, not the decompressed bytes, unless an extension says otherwise.

For 32-bit checksums, store the value in `checksum_low` and set `checksum_high` to zero.

For 64-bit checksums, store the value in `checksum_low` and set `checksum_high` to zero.

For 128-bit digests, store low 64 bits in `checksum_low` and high 64 bits in `checksum_high`, using little-endian interpretation.

Checksums detect corruption. They are not signatures.

---

## 33. Canonical LYBA

Canonical LYBA is a restricted encoding intended for reproducible builds, content hashing, cache keys, and conformance tests.

A canonical file must satisfy all of these rules:

1. Header
   - `CANONICAL` flag is set.
   - Version is encoded exactly as supported by the writer.
   - Reserved fields are zero.
   - Header CRC is either zero or correct.

2. Sections
   - No compression.
   - Payloads are 8-byte aligned.
   - Padding bytes are zero.
   - Section table is sorted by canonical section order.
   - No duplicate unique sections.
   - Unsupported optional sections are omitted.

3. Strings
   - UTF-8 is valid.
   - Duplicate interned strings are rejected.
   - Strings are interned by first deterministic traversal occurrence.

4. Values
   - `UVar` and `SVar` encodings are minimal.
   - Maps have no duplicate keys.
   - Map order is source order or declared canonical order.
   - Non-finite floats are rejected.
   - Runtime descriptors are omitted unless explicitly allowed by canonical profile.

5. Syntax
   - Syntax nodes are ordered by pre-order traversal unless an extension declares another order.
   - Trivia records are ordered by source order.

6. Metadata
   - Timestamps are omitted unless explicitly part of source data.
   - Producer-specific non-deterministic fields are omitted.
   - Absolute local paths are omitted or normalized to declared virtual URIs.

7. Extensions
   - Required extensions must affect canonical bytes deterministically.
   - Private extensions are forbidden in public canonical interchange unless a namespace policy allows them.

A canonical reader should be able to recompute a stable content hash over the complete file after normalizing fields that are explicitly excluded from hashing by the profile.

---

## 34. Value equality and ordering

LYBA preserves LYMA value equality rules.

Recommended equality for portable values:

| Type | Equality |
| --- | --- |
| null | all null values equal |
| boolean | same boolean value |
| integer | same mathematical integer and compatible numeric mode |
| float | same IEEE value, excluding NaN in portable mode |
| decimal | same canonical decimal string |
| string | same UTF-8 byte sequence |
| bytes | same byte sequence |
| sequence | same length and pairwise equal elements |
| map | same ordered entries for strict equality, or same key-value set for semantic equality |
| tagged | same tag and equal inner value |

Canonical duplicate-key detection for portable maps uses semantic key equality.

When a map contains non-string keys, canonical ordering is not required by core LYBA because source order is preserved. If a host chooses to sort non-string keys, it must document the policy.

---

## 35. Lua and executable content

LYBA treats all executable material as inert data.

Core rules:

- LYBA loading must not execute Lua.
- LYBA loading must not compile Lua.
- Lua expressions are stored as source text or as syntax nodes.
- Lua chunks are stored as source text or blobs.
- Evaluation results may be stored as normal LYBA values.
- Lua bytecode is not part of the core format.
- Host object references are descriptors, not live objects.

A host that wants to evaluate stored source must:

1. inspect profile flags;
2. inspect `CAPS`;
3. verify source provenance if needed;
4. apply resource limits;
5. provide explicit module and resolver capabilities;
6. evaluate with a sandbox appropriate to the profile;
7. treat evaluation failure as a diagnostic, not a parse failure.

### 35.1 Optional bytecode extension

A future or private extension may store Lua bytecode. Such an extension must be:

- declared in `EXTS`;
- marked `TRUSTED_ONLY`;
- marked `MAY_CONTAIN_CODE`;
- rejected by default readers;
- bound to a specific Lua dialect and VM version;
- integrity-checked before use;
- never used as a substitute for source-level safety checks.

Public LYBA interchange should not rely on bytecode.

---

## 36. Security model

A LYBA reader must defend against malicious binary input.

Required reader checks:

- validate magic and version before reading dynamic offsets;
- validate `file_length` when present;
- validate all section offsets and sizes;
- reject overlapping sections;
- enforce maximum section count;
- enforce maximum string length;
- enforce maximum value count;
- enforce maximum nesting depth while materializing values;
- reject invalid UTF-8;
- reject invalid varints;
- reject reserved flags unless policy allows future-tolerant reading;
- reject unsupported required extensions;
- reject unsafe runtime descriptors unless policy allows them;
- never resolve external URIs without host approval;
- never evaluate Lua during parsing.

Recommended default limits:

| Limit | Suggested default |
| --- | ---: |
| file size | host-defined |
| section count | 1024 |
| string count | 10 million |
| single string length | 64 MiB |
| value count | 50 million |
| nesting depth | 512 |
| document count | 1 million |
| syntax node count | 100 million |
| embedded resource count | 1 million |

Hosts should lower these limits for untrusted input.

The current public API/CLI preset is intentionally much lower than the broad draft suggestions above:

- max input bytes: 8 MiB
- max decoded logical bytes per section: 16 MiB
- max stored section payload bytes: 2 MiB
- max blob display bytes: 64 KiB
- max JSON output bytes: 8 MiB
- trust policy: public by default

---

## 37. Versioning

The container version is stored in the header as major and minor.

Version rules:

- A reader may load a file with the same major version and a minor version less than or equal to what it supports.
- A reader must reject a file with a greater major version unless an explicit compatibility mode is enabled.
- A minor version may add new optional sections, new optional flags, and new value tags only when older readers can reject or ignore them safely.
- A major version may change incompatible layouts.

Section versions are independent from the container version. A section version change is allowed when the container version supports that section layout.

Reserved bits and fields are for forward compatibility. Writers for version 0.1 must set them to zero.

---

## 38. Error handling

LYBA readers should report structured diagnostics with:

```text
severity
error code
message
file offset
section id, if known
section index, if known
record index, if known
related source span, if available
```

Recommended error codes:

| Code | Meaning |
| --- | --- |
| LB0001 | invalid magic |
| LB0002 | unsupported version |
| LB0003 | invalid endian marker |
| LB0004 | invalid header size |
| LB0005 | invalid section table |
| LB0006 | overlapping sections |
| LB0007 | offset outside file |
| LB0008 | unsupported required section |
| LB0009 | unsupported required extension |
| LB0010 | unsupported codec |
| LB0011 | checksum mismatch |
| LB0012 | invalid varint |
| LB0013 | invalid UTF-8 |
| LB0014 | invalid value reference |
| LB0015 | invalid syntax node reference |
| LB0016 | duplicate key in canonical map |
| LB0017 | non-canonical encoding |
| LB0018 | resource limit exceeded |
| LB0019 | trusted-only content rejected |
| LB0020 | unsafe evaluation request |
| LB0021 | malformed extension payload |
| LB0022 | invalid source span |
| LB0023 | invalid document table |
| LB0024 | unsupported numeric value |
| LB0025 | invalid reserved flags |

A reader should distinguish format errors from policy errors.

Example:

- invalid varint is a format error;
- trusted-only bytecode rejected by a safe host is a policy error.

Known draft 0.1 implementation caveats:

- fuzzing support currently has compile-checked scaffolding in the workspace; runtime `cargo fuzz` still depends on host/toolchain sanitizer support
- no-execution guarantees apply even when Level 5 capability/runtime sections are present; those sections are descriptive only until a separate host policy chooses to evaluate related source

Open questions for future drafts:

- which compressed codecs, if any, should become part of the interoperable core profile beyond codec `0`
- whether additional syntax-heavy image profiles should define stricter canonical interoperability requirements
- how trusted bytecode or signed-extension stories should be standardized without weakening the no-execution-by-default contract

---

## 39. Generic examples

### 39.1 Value image example

Source LYMA:

```lyma
@lyma 0.1
@profile data

service:
  name: api
  replicas: 3
  enabled: true
  endpoints:
    - path: /health
      method: GET
    - path: /metrics
      method: GET
```

A value-only LYBA file would contain:

- `STRS` entries for keys and strings such as `service`, `name`, `api`, `replicas`, `enabled`, `endpoints`, `path`, `/health`, `method`, `GET`, `/metrics`;
- `VALS` records for booleans, integers, strings, endpoint maps, the endpoint sequence, the service map, and the root map;
- `DOCS` with one document pointing to the root map.

No syntax section is required.

### 39.2 Syntax image example

Source LYMA:

```lyma
-- deployment settings
let defaults:
  replicas: 3

service:
  replicas: =defaults.replicas
```

A syntax image may contain:

- a `TRIV` record for the comment;
- an `ASTN` node for the `let` binding;
- an `ASTN` node for the expression;
- `SRCS` spans for each value and node;
- optional `VALS` records for the parsed static scalar values;
- optional evaluated result value `3` if the writer performed evaluation.

### 39.3 Bundle example

A bundle may contain:

```text
main.lyma
common/defaults.lyma
schemas/service.schema.lyma
```

The LYBA bundle would contain:

- one `DOCS` entry for the assembled main document;
- `DEPS` records for imports and schema references;
- `EMBD` records containing the imported text or binary resources;
- digests in `SIGN` or dependency metadata;
- value output in `VALS`.

The bundle still does not resolve anything automatically when loaded. It only makes resources available to a host resolver if policy permits.

---

## 40. Recommended writer modes

### 40.1 Runtime data mode

Purpose: fastest loading of trusted build output.

Recommended sections:

```text
META
STRS
SYMS
VALS
DOCS
SCMA, optional
SIGN, optional
```

Recommended flags:

```text
HAS_VALUES
DATA_PROFILE
VALUE_IMAGE
CANONICAL
```

Omit:

```text
ASTN
TRIV
SRCF
SRCS
CAPS
Lua source blobs
```

### 40.2 Editor cache mode

Purpose: preserve parse structure and diagnostics.

Recommended sections:

```text
META
STRS
SYMS
BLOB
VALS
DOCS
SRCF
SRCS
ASTN
TRIV
DIAG
DEPS
```

Recommended header flags:

```text
HAS_VALUES
HAS_SOURCE
HAS_SYNTAX
HAS_DIAGNOSTICS
VALUE_IMAGE
SYNTAX_IMAGE
```

### 40.3 Build bundle mode

Purpose: reproducible assembled package.

Recommended sections:

```text
META
EXTS
STRS
SYMS
BLOB
VALS
DOCS
SCMA
DEPS
EMBD
SIGN
```

Recommended flags:

```text
HAS_VALUES
HAS_BUNDLE
VALUE_IMAGE
BUNDLE_IMAGE
CANONICAL
```

### 40.4 Conformance fixture mode

Purpose: testing parsers, evaluators, and serializers.

Recommended sections:

```text
META
STRS
SYMS
VALS
DOCS
SRCF
SRCS
ASTN
TRIV
DIAG
```

Fixture metadata should declare expected diagnostics, expected canonical text output, and expected value output.

Recommended header flags:

```text
HAS_VALUES
HAS_SOURCE
HAS_SYNTAX
HAS_DIAGNOSTICS
VALUE_IMAGE
SYNTAX_IMAGE
```

---

## 41. Recommended reader algorithm

A robust reader should follow this order:

1. Read first 64 bytes.
2. Validate magic, version, header size, endian marker, and reserved fields.
3. Read section table.
4. Validate section count, entry size, offsets, sizes, and overlap.
5. Validate required sections and extensions.
6. Validate codecs and checksums according to policy.
7. Load `EXTS` first when present.
8. Load `STRS` and validate UTF-8.
9. Load `SYMS` and validate references.
10. Load `BLOB` metadata without allocating large blobs unless needed.
11. Load `VALS` indexes and validate record boundaries.
12. Load `DOCS` and validate root references.
13. Load optional schema, source, syntax, trivia, dependency, and diagnostic sections as requested.
14. Materialize values lazily when possible.
15. Apply host policy before resolving dependencies or evaluating source.

Readers should prefer lazy materialization for large files.

---

## 42. Recommended writer algorithm

A writer should follow this order:

1. Choose writer mode: value, syntax, hybrid, bundle, or fixture.
2. Build logical document stream.
3. Intern strings by deterministic traversal.
4. Intern symbols.
5. Build blob table.
6. Build value arena.
7. Build document table.
8. Build optional syntax, source, trivia, schema, dependency, diagnostic, and signature sections.
9. Encode each section to an in-memory payload or temporary stream.
10. Apply optional compression according to policy.
11. Compute checksums if enabled.
12. Assign aligned offsets.
13. Write header with placeholder checksum.
14. Write section table.
15. Write payloads and zero padding.
16. Write optional footer.
17. Fill final file length and header checksum.
18. Verify the file by reading it back in strict mode.

Canonical writers should fail rather than emit non-canonical bytes when canonical mode is requested.

---

## 43. Text round-tripping

LYBA to LYMA text conversion depends on what was preserved.

| LYBA contents | Possible text output |
| --- | --- |
| value only | canonical LYMA text with no comments or original expressions |
| value plus spans | canonical LYMA text with diagnostics mapped to source |
| syntax plus trivia | mostly lossless source reconstruction |
| syntax without trivia | structured pretty-printed LYMA |
| bundle | one or more reconstructed source files if embedded source exists |

A value image cannot reconstruct original comments, quote choices, `let` declarations, imports, includes, or expressions unless syntax information is also present.

---

## 44. Interoperability with LYMA constructs

| LYMA construct | Value image | Syntax image |
| --- | --- | --- |
| plain scalar | normalized value | scalar node plus token text |
| quoted string | string value | quote style and token text optional |
| block string | string value | block style and chomping preserved |
| sequence | sequence value | sequence node and item nodes |
| mapping | map value | mapping node and entry nodes |
| tag | tagged value or resolved value | tag node preserved |
| comment | omitted | trivia record |
| `let` | omitted after evaluation | let node preserved |
| `=expr` | result value if evaluated, or expression source value | expression node preserved |
| `|lua` | result value if evaluated, or Lua source value | chunk node preserved |
| `@import` | assembled value or dependency record | directive node preserved |
| `@include` | assembled entries or dependency record | directive node preserved |
| `@use` | capability or dependency record | directive node preserved |
| `@if` | selected branch result | conditional nodes preserved |
| `@for` | generated entries | loop node preserved |
| spread | merged entries | spread node preserved |
| schema directive | schema metadata | directive node preserved |
| document separator | document table entry | document node boundary |

---

## 45. Public interchange profile

The public interchange profile is the safest recommended subset.

Allowed:

- uncompressed sections;
- metadata;
- strings and symbols;
- null, booleans, integers, finite floats, strings, bytes, sequences, maps, tagged values;
- string map keys;
- document streams;
- optional schemas;
- optional diagnostics;
- optional source spans;
- optional syntax nodes and comments.

Rejected:

- trusted-only sections;
- Lua bytecode extensions;
- external resource resolution during load;
- runtime descriptors without fallback values;
- non-finite floats;
- private required extensions;
- unsupported required codecs;
- duplicate map keys;
- non-minimal varints in canonical files.

This profile should be the default for libraries and command-line tools that accept untrusted files.

---

## 46. Private and experimental extensions

Private extensions are allowed but must be explicit.

Rules:

- private section IDs should use `PRIVATE` section flag;
- private value tags should use the range `0x80` through `0xFF`;
- private extensions must declare a stable extension name in `EXTS`;
- private required extensions make the file non-portable;
- private extensions should include fallback values where possible;
- public tools should preserve unknown optional private sections when rewriting only if they can do so without invalidating offsets, checksums, or signatures.

Experimental extensions should use names such as:

```text
com.example.lyba.experimental.feature_name
```

They should not use `org.lyma` unless accepted into the standard namespace.

---

## 47. Minimal implementation checklist

A minimal Level 1 reader must implement:

- header validation;
- section table validation;
- uncompressed section reading;
- `STRS` parsing;
- `VALS` parsing for null, booleans, integers, finite floats, strings, sequences, maps, and tagged values;
- `DOCS` parsing;
- reference validation;
- resource limits;
- no execution.

A minimal Level 1 writer must implement:

- deterministic string interning;
- value arena construction;
- document table construction;
- header and section table writing;
- 8-byte alignment;
- zero padding;
- canonical varints when canonical mode is requested.

A Level 3 implementation additionally needs:

- source file table;
- source spans;
- syntax nodes;
- comments and trivia;
- text reconstruction policy.

A Level 4 implementation additionally needs:

- dependency table;
- embedded resources;
- digest handling;
- resolver integration policy.

---

## 48. Open design questions for draft 0.1

The following items are intentionally left open for implementation feedback:

1. Whether canonical public interchange should require string-only map keys or merely recommend them.
2. Whether decimal numbers should remain `DecimalString` or become a structured decimal record.
3. Whether `SYMS` should be mandatory for all files or optional for small files.
4. Whether compression should remain fully extension-based or whether one codec should become required for bundle readers.
5. Whether source reconstruction should target exact lossless round-trip or formatter-stable round-trip.
6. Whether checksum algorithms should be mandatory in canonical files.
7. Whether a deterministic map-key ordering profile should exist in addition to source-order preservation.
8. Whether an official schema section layout should be split from generic value storage.

These questions do not block Level 1 value-image implementations.

---

## 49. Summary

LYBA is the binary representation of LYMA, Lua YAML-like Markup Assembly. It provides a compact, deterministic, sectioned container for LYMA values, document streams, syntax trees, source maps, schemas, dependencies, diagnostics, and optional bundle metadata.

The core principles are:

```text
load bytes safely
preserve LYMA semantics
never execute during parsing
make capabilities explicit
keep public interchange portable
allow richer tooling through optional sections
```

A simple LYBA file can be just strings, values, and documents. A rich LYBA file can preserve the full parsed source structure and dependency graph. Both remain part of the same format because both represent the same LYMA model at different stages of the parse, assembly, and evaluation pipeline.
