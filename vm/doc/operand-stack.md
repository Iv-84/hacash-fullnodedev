# Operand Stack and Handwritten IR Parameter Order

Version: current implementation  
Audience: VM engineers, bytecode authors, Fitsh/IR tooling authors, auditors

## 1. Purpose

This document records how **operand-stack layout** relates to **handwritten IR child order** (`subx`, `suby`, `subz`, …). It clarifies opcodes whose stack shape is easy to misread even when the implementation is consistent.

Opcode gas and full semantics live elsewhere (`gas-cost.md`, `value-cast.md`). This file only covers **parameter / stack ordering**.

## 2. Core rule (matches Fitsh source)

For IR nodes with multiple children, codegen evaluates children **left to right**:

1. `subx` is pushed first → **bottom** of the operand stack  
2. `suby`, `subz`, … follow → toward the **top**  
3. The instruction then consumes from the **top** downward

Therefore, for a call-shaped IR node `f(a, b, c)`:

- Stack before the opcode (bottom → top): `[a, b, c]`  
- This matches source argument order and matches opcodes such as `choose(cond, yes, no)` → `[cond, yes, no]`

**Notation in `bytecode.rs` comments:** comma-separated letters are **bottom → top**, same as IR children.

## 3. Opcodes with non-obvious stack shapes

### 3.1 `PACKLIST` / `PACKMAP` / `PACKTUPLE` — trailing count

**IRLIST tail shape:** `[item₁, …, itemₙ, count_literal, PACK*]`

| Stack (bottom → top) | Role |
|----------------------|------|
| `v₁ … vₙ` | Elements (map: `k₁,v₁,k₂,v₂,…`) |
| `count` | Total item count (`n` or `2×pairs` for maps) |

The count is the **last** child, on stack **top** — not `pack(n, items…)` with `n` first.

Fitsh literals (`list { … }`, `map { … }`) emit this shape automatically.

### 3.2 `PUTX` / `GETX` — dynamic local index

| Opcode | Stack (bottom → top) | IR / source call |
|--------|----------------------|------------------|
| `GETX` | `[idx]` | `local_x(idx)` |
| `PUTX` | `[idx, val]` | `local_x_put(idx, val)` |

**Common mistake:** treating `PUTX` as `(value, index)` because of an old `v,i` comment. The correct order is **index first, value on top**.

Fitsh normal assignments use `PUT` with a fixed index in the instruction immediate; the compiler does **not** emit `PUTX` for ordinary `var`/`let`.

### 3.3 `XLG` / `XOP` — rhs on stack only

| Opcode | Stack | Semantics |
|--------|-------|-----------|
| `XLG` | `[rhs]` + mark | `locals[idx] op rhs` → result replaces `rhs` on stack |
| `XOP` | `[rhs]` + mark | `locals[idx] op= rhs`; result stays in local — `byte/28` stack_write on result `val_size` |

The **left** operand is always the local slot encoded in the instruction mark, not an IR sibling child.

- `XLG` is suitable for `local == expr`, not for arbitrary `expr op local` when `op` is non-commutative (use `GET` + `GT` / etc.).  
- Fitsh does **not** emit `XLG`; it uses `GET` + binary compare for slot comparisons.

`XOP` is used by Fitsh for `local += rhs` when `idx < 64`.

### 3.4 `PUT` / `GET` — index in immediate

Stack holds only the **value** (`PUT`) or nothing extra (`GET` pushes). Slot index is in the bytecode parameter, not a stack operand.

### 3.5 In-place peek ops (not push/pop)

These take one logical argument but **replace** the top slot instead of `pop; push`:

| Examples | Behavior |
|----------|----------|
| `GGET`, `MGET`, `SLOAD`, `SSTAT` | `[key]` → same slot becomes value |
| `CU*`, unary casts | `[v]` → `v` updated in place |
| Binary `ADD`, `CAT`, … | `[x, y]` → `[x op y]` (depth −1) |

Handwritten IR still lists children in normal call order; only the stack **effect** differs from a pure `push` API.

### 3.6 Receiver-on-bottom mutators

`INSERT`, `MERGE`, `APPEND`, `CAT`, etc.: first IR child is the **container / destination** at stack bottom; operands popped from the top.

Example: `insert(container, key, value)` → `[container, key, value]`.

### 3.7 `UNPACK` + `param` prelude

Function entry may place argv on the stack; `param { a b … }` lowers to `UNPACK` / `ROLL0` / `PUT` sequences (see `fitsh-note.md` §4). Stack order: `[container, start_idx]` bottom→top (`start_idx` popped first). `UNPACK` also charges `item/4` on the source container length (`gas-cost.md`).

### 3.8 `BYTE` / `CUT` — buf on bottom

Same rule as `buf_cut(buf, start, len)` / `byte(buf, idx)`: **buffer is `subx` (stack bottom)**; numeric args are toward the top.

| Opcode | Stack (bottom → top) | IR / source call | Consumption |
|--------|----------------------|------------------|-------------|
| `BYTE` | `buf, idx` | `byte(buf, idx)` | pop `idx`; peek `buf` → u8 at index |
| `CUT` | `buf, ost, len` | `buf_cut(buf, start, len)` | pop `len`, `ost`; peek `buf` → slice in place |

**Common mistake:** annotating or hand-writing `CUT` as `len, ost, buf` (reversed). `_codes9` in `vm/src/tests/stack.rs` is the canonical handwritten example: `PBUF … P1 PU8 3 CUT` → `[buf, ost=1, len=3]`.

`LEFT` / `RIGHT` / `LDROP` / `RDROP` differ: length `n` is in the **instruction immediate**, with `buf` on the stack top slot (`buf_left(n, buf)` codegen).

## 4. Quick reference table

| Opcode / pattern | Stack bottom → top | Notes |
|------------------|-------------------|--------|
| `CHOOSE` | `cond, yes, no` | Same as `choose(c,y,n)`; +movement gas 2 |
| `PACKLIST` | `v…, count` | Count on top |
| `PACKMAP` | `k,v,…, count` | Even count; pairs in order |
| `PUTX` | `idx, val` | **Not** `val, idx` |
| `GETX` | `idx` | |
| `BYTE` | `buf, idx` | Same as `byte(buf, idx)` |
| `CUT` | `buf, ost, len` | Same as `buf_cut(buf, start, len)`; **not** `len, ost, buf` |
| `HREAD` | `start, len` | |
| `HWRITE` | `offset, val` | |
| `GPUT` / `MPUT` | `key, val` | |
| `SNEW` | `key, val, type` | |
| `XLG` | `rhs` | `local[idx] op rhs` |
| `XOP` | `rhs` | `local[idx] op= rhs`; +`byte/28` stack_write on result |

## 5. Related documents

- `fitsh-note.md` §3.1 — `IRLIST` vs `IRBLOCK` stack cleanup  
- `call-standard.md` §11.3 — native call stack models (`NTENV` vs `NTFUNC`/`NTCTL`)  
- `value-cast.md` — cast and compare rules on stack operands  
- `vm/src/rt/bytecode.rs` — per-opcode stack comment on the enum variant
