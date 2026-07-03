# Value Conversion and Normalization Spec

This document is the single source of truth for `Value` conversion, normalization, and comparison behavior.

## 1. Scope

`Value` variants:

- Scalar/value domain: `Nil`, `Bool`, `U8`, `U16`, `U32`, `U64`, `U128`, `Bytes`, `Address`
- Special domain: `Compo`, `Tuple`, `Handle`

Unless explicitly stated, all conversion and comparison rules only apply to the scalar/value domain.

## 2. API Orthogonality

- `canbe_*`: convert to Rust/native type, does not mutate `self`
- `to_*`: convert to normalized `Value`, does not mutate `self`
- `cast_*`: explicit cast in place, mutates `self`

This separation is strict and must remain stable.

## 3. Canonical Normalization Primitives

## 3.1 Byte normalization (`extract_bytes_ec`)

Output type: `Vec<u8>`.

Implementation: `Value::extract_bytes_with_error_code` in `vm/src/value/canbe.rs`. Do not use `Value::scalar_bytes` for this path.

- `Bool` -> `[0x01]` or `[0x00]`
- `U8/U16/U32/U64/U128` -> fixed-width big-endian bytes (leading zero bytes preserved)
- `Bytes` -> raw bytes (including `Bytes([])`; empty bytes is a valid value, not absence)
- `Address` -> raw address bytes
- `Nil` -> error (typed absence; not equivalent to `Bytes([])`)
- `Compo`, `Tuple`, `Handle` -> error

Field serialization (`Value::serialize` / `scalar_bytes`) is separate: `Nil` encodes as type tag with zero payload and round-trips as `Nil`, not `Bytes([])`.

## 3.2 Bool normalization (`extract_bool`)

Output type: `bool`.

- `Bool(b)` -> `b`
- `Nil` -> `false`
- `U*` -> `n != 0`
- `Bytes` -> any non-zero byte => `true`, otherwise `false`
- `Address` -> any non-zero byte => `true`, otherwise `false`
- `Compo`, `Tuple`, `Handle` -> error

## 3.3 Stack arithmetic uint gate (`to_uint`)

Output type: `Value` (`U8/U16/U32/U64/U128`).

Implementation: `Value::to_uint` in `vm/src/value/convert.rs`.

Runtime **arithmetic / bit-op** paths (`ADD`, `BSHL`, `INC`, etc.) call this gate before width promotion. It is **strict**:

- `U8/U16/U32/U64/U128` -> keep current uint value/type
- All other types (`Nil`, `Bool`, `Bytes`, `Address`, `Compo`, `Tuple`, `Handle`, ...) -> error

Operands must already be uint variants or the instruction fails immediately. Use explicit `CU*` / `CTO U*` on the stack when a wider scalar domain is needed.

## 3.4 Explicit uint cast normalization (`to_u128`)

Output type: `u128`.

Used by explicit uint casts (`CU*`, `CTO U*`) when the source is not already `Bytes` with width-aware rules. This is **broader** than stack arithmetic `to_uint` (section 3.3):

- `U*` -> value widening to `u128`
- `Nil` -> `0`
- `Bool` -> `0|1`
- `Bytes` / `Address`:
  - drop leading zero bytes
  - remaining length `<=16` => parse as big-endian `u128`
  - remaining length `>16` => error
- `Compo`, `Tuple`, `Handle` -> error

## 4. Explicit Cast Rules (`cast_*` / `CU*` / `CTO`)

## 4.1 `cast_bool` / `CTO Bool`

- Uses `extract_bool`
- Same acceptance/failure set as section 3.2

## 4.2 `cast_u8/u16/u32/u64/u128` / `CU8..CU128`

For target width `W` bits (`N = W/8` bytes):

- If source is `Bytes`:
  - if `len <= N`: left-pad zeros to `N` and parse
  - if `len > N`: only allowed when truncated prefix bytes are all zero; otherwise error
- If source is not `Bytes`:
  - normalize by `to_u128`
  - check target range (`u8/u16/u32/u64/u128`)
  - overflow => error

Explicit uint cast uses section 3.4 (`to_u128`) plus width/range checks. Stack arithmetic uses section 3.3 (`to_uint`, uint-only). `Bytes` on explicit cast keeps width-aware cast semantics; `Bytes` does not implicitly enter stack arithmetic.

## 4.3 `cast_buf` / `CBYTES`

- Uses byte normalization from section 3.1 (`extract_bytes_ec`)
- `Nil`, `Compo`, `Tuple`, `Handle` are rejected
- `Bytes([])` is accepted (empty bytes is a value, not absence)

## 4.4 `cast_addr`

- First `cast_buf` / `cast_bytes()` (section 3.1 / 4.3)
- Then parse exact `Address` byte length via `Address::from_bytes`
- Length mismatch or invalid address bytes => error
- Implementation: `Value::cast_addr` in `vm/src/value/cast.rs`

## 4.5 `cast_to` / `CTO <type>`

- Dispatches to `cast_bool`, `cast_u*`, `cast_buf`, `cast_addr`
- `CTO` target type param must be in the explicit-cast set: `Bool/U8/U16/U32/U64/U128/Bytes/Address`
- `CTO` target outside this set (`Nil`, `Compo`, `Tuple`, `Handle`) => `InstParamsErr`
- `TIS` type param accepts all declared `ValueTy` (including `Nil` / `Compo` / `Tuple` / `Handle`) for type checking
- `TIS` unknown or reserved type id => `InstParamsErr`
- `CTO` unknown or reserved type id => `InstParamsErr`

## 5. Implicit Conversion Trigger Map

## 5.1 Bool context (implicit bool normalization)

Uses section 3.2 (`extract_bool`):

- Logic ops: `AND`, `OR`, `NOT`
- Control-flow condition: `CHOOSE`, `BRL`, `BRS`, `BRSL`, `BRSLN`, `AST`
- `CTO Bool`

## 5.2 Numeric arithmetic context (uint-only gate)

Uses section 3.3 (`to_uint`): **both operands must already be uint variants** (`U8`..`U128`), then width promotion (`cast_arithmetic`) to the wider uint type.

Applied by:

- Arithmetic: `ADD`, `SUB`, `MUL`, `DIV`, `MOD`, `POW`, `MAX`, `MIN`
- Bit ops: `BSHL`, `BSHR`, `BAND`, `BOR`, `BXOR`
- Unary numeric: `INC`, `DEC` (operand must already be uint; no implicit `to_uint` widening from `Nil`/`Bytes`)

`Nil`, `Bool`, `Bytes`, and `Address` do **not** silently enter this path. Cast with `CU*` / `CTO U*` first when needed.

Failure in either operand gate or width/range checks => immediate error.

## 5.3 Equality context (`EQ`, `NEQ`)

- If both operands are uint types (`U8/U16/U32/U64/U128`):
  - convert both to `u128`
  - compare numeric value
- Otherwise, operands must have the same runtime type; cross-type compare fails.
- Same-type compare rules:
  - `Nil == Nil`
  - `Bool` compares bool value
  - `Bytes` compares raw bytes
  - `Address` compares raw address bytes
  - `Compo` compares pointer identity (`ptr_eq`)

## 5.4 Ordered compare context (`LT`, `LE`, `GT`, `GE`)

- Ordered comparison is `uint`-only.
- Both operands must already be uint types: `U8/U16/U32/U64/U128`.
- No implicit `to_u128` conversion is applied for `Nil/Bool/Bytes/Address`.
- Operands are compared as numeric uint values.
- Any non-uint operand => immediate type error.

## 5.5 Byte-handle context

Uses section 3.1 (`extract_bytes_ec`):

- `CAT`, `JOIN`, `BYTE`, `CUT`, `LEFT`, `RIGHT`, `LDROP`, `RDROP`

`Nil`, `Compo`, `Tuple`, `Handle` are rejected in this path.

## 5.6 External/native call data context

Uses `extract_call_data`:

- `Nil` => `[]`
- Otherwise => `extract_bytes_ec` (scalars and `Bytes` only)

## 6. Function Param/Return Type Check Rules

## 6.1 Signature-level constraints (`ValueTy`)

- Param type cannot be `Nil`, `Tuple`
- Return type cannot be `Nil`

## 6.2 Runtime checked cast (`check_param_type` / `cast_param`)

Function boundaries are **stricter than stack-level explicit casts** (`CTO` / `CU*` / `CBYTES`):

| Context | Allowed conversions |
|---------|---------------------|
| Stack `CTO` / `CU*` / `CBYTES` | Full explicit-cast family (sections 4.1–4.4) |
| Function param / return boundary | Identical type; uint-family narrowing/widening only |

Rules:

- If source and target are identical: pass
- Allowed uint casts: `U8/U16/U32/U64/U128` ↔ same family with fit check
- All other source/target combinations: `CallArgvTypeFail`

Note: `check_param_type(v, ty)` is the non-mutating wrapper. See `vm/doc/call-standard.md` §16.

Heap payload for callees or host calls: materialize with `heap_read` (or keep data as `Bytes`) before crossing a function or `extract_call_data` boundary.

## 7. Deterministic Edge Cases (Normative)

- `U16(1) == U8(1)` is `true` (both uint; compare as `u128`)
- `U32(5) != U8(4)` is `true` (both uint; compare as `u128`)
- `Nil == Bool(false)` fails (cross-type compare is not allowed)
- `Bytes([0,1]) == U8(1)` fails (cross-type compare is not allowed)
- `Bytes([0,1]) == Bytes([1])` is `false` (same-type raw-bytes compare)
- Ordered compare with non-uint values (`Nil`, `Bool`, `Bytes`, `Address`, `Compo`, `Tuple`, `Handle`) always fails
- `Nil` and `Bool(false)` are both falsy in `extract_bool` / `CHOOSE`, but `Nil == Bool(false)` fails (cross-type `EQ`)

## 8. Compo Map Semantics (Normative)

Map duplicate-key policy is classified by operation intent:

| Intent | Operation | Duplicate key behavior |
|--------|-----------|------------------------|
| **Materialize** (multi-key commit) | `PACKMAP` / `pack_map`, `map { ... }` literal | **Reject** with `CompoPackError` |
| **Materialize** (multi-key commit) | `MERGE` / `merge` (map + map) | **Reject** with `CompoPackError` if source key already exists in destination |
| **Upsert** (single-key write) | `INSERT` / `insert` on map | **Overwrite** value for existing key |

Multi-key commits (`PACKMAP`, `MERGE`) require disjoint key sets: duplicate keys are contract errors and must fail as an unrecoverable **Fault** (`CompoPackError` → `XError::Fault`), not a recoverable Revert. Single-key upserts (`INSERT`) model ledger field updates and intentionally overwrite.

List `MERGE` appends elements; it has no key domain and does not deduplicate elements.

## 9. Map Key Semantics

Map keys are flat bytes derived from scalar values. Uint keys use **value-defined minimal big-endian** encoding (`uint_key_bytes` in `vm/src/value/util.rs`, applied by `extract_key_bytes` in `vm/src/value/canbe.rs`). Field serialization (`scalar_bytes`) is separate and keeps fixed-width uint bytes.

### 9.1 Uint key normalization (normative)

For any uint scalar (`U8`..`U128`, and future wider uints when enabled):

1. Interpret the value as an unsigned integer.
2. Encode as big-endian with leading zero bytes trimmed.
3. If the result is empty (numeric zero), use `[0x00]` (empty keys are rejected).

Examples:

- `U8(1)`, `U64(1)`, `U128(1)` → `[0x01]`
- `U8(0)`, `U64(0)` → `[0x00]`
- `U16(0x0102)` → `[0x01, 0x02]`

**Consequence**: numerically equal uints share one map slot regardless of variant width. This matches `value_content_eq` for uint operands. Future `U256` (or wider) keys extend only the maximum value range; existing small-value encodings stay unchanged.

`KEYS` returns stored key bytes as `Bytes(...)`; uint keys round-trip as bytes, not as typed uint variants.

### 9.2 Address / Bytes key collision

`extract_key_bytes(Address(X))` and `extract_key_bytes(Bytes(X.serialize()))` produce identical bytes. They share the same map slot even though `value_content_eq` rejects cross-type comparison between them.

**Consequence**: do not mix `Address` and `Bytes` keys with the same byte payload in one map. A uint minimal-BE key can also collide with a `Bytes` key when the byte sequences match (e.g. `U64(1)` and `Bytes([0x01])`).

**Mitigation**: treat each map as single key-family (uint-only, bytes-only, or address-only) by contract convention.

### 9.3 Dual byte→uint conversion paths

Raw bytes can become uint via:

- `buf_to_uint` — minimal active width (`vm/src/value/convert.rs`)
- `cast_to_uint_width` / `bytes_to_uint_width` — fixed target width (`vm/src/value/cast.rs`)

Both paths may yield different `Value` variants for the same source bytes, but **map keys depend only on the numeric value** (§9.1), not on the variant width.

### 9.4 Bool / Nil / empty bytes

- **Bool**: `extract_key_bytes` rejects `Bool` (map keys cannot be bool).
- **Nil**: rejected as a map key.
- **Bytes([])**: rejected (empty key).
- Canonical bool byte decoding vs runtime `extract_bool` divergence is intentional; see section 3.2. It does not affect map storage.

### 9.5 Scalar bytes vs map keys

| Path | Uint encoding | Used for |
|------|---------------|----------|
| `scalar_bytes` | fixed-width big-endian | `Value::serialize`, field wire format |
| `extract_key_bytes` / `uint_key_bytes` | minimal big-endian (+ zero → `[0x00]`) | `Compo` map, `GKVMap`, `MKVMap`, intent KV, contract storage keys |

Do not use `scalar_bytes` for map key derivation.

---

## 10. U256 Reservation Contract (Forward Compatibility)

This section defines mandatory constraints before enabling `U256`, to prevent compatibility breaks and normalization drift.

- Type id `7` and keyword `u256` are reserved and currently disabled.
- Type id `11` (formerly `HeapSlice`) and bytecode `0x88` (formerly `HSLICE`) are reserved and disabled.
- Any decode/build path receiving type id `7` must fail explicitly (not silently downgraded).
- Any type parser receiving `u256` must fail explicitly with a reserved/not-enabled error.
- Activation must be version-gated (e.g. codeconf/height fork switch), not implicit by parser/runtime fallback.
- `U256` activation is all-or-nothing for conversion paths:
  - update scalar domain (`Value`, `ValueTy`, serialization/parse)
  - update explicit cast family (`cast_u256`, `CU256`, `CTO U256`)
  - update implicit numeric normalization (`to_uint`, arithmetic width promotion, ordered compare)
  - update equality uint numeric-compare path to include `U256` as uint operand
- While `U256` is disabled, numeric scalar normalization remains bounded by current active max width (`u128` / 16 bytes).
- No mixed mode is allowed: enabling parser keyword without runtime normalization support is forbidden.
