# Fitsh Smart Contract Language Manual

A practical guide for developers with smart contract experience (e.g., Solidity) to quickly learn and build contracts in Fitsh.

---

## 1. Design Goals, Characteristics, and Comparison with Solidity

### 1.1 Design Goals

- **Stack-based VM**: Fitsh compiles to a stack-based bytecode executed by the Hacash VM
- **Deterministic execution**: Same inputs produce identical outputs for gas and correctness
- **Gas metering**: Each operation consumes gas; used for on-chain resource accounting
- **IR-first compilation**: Source → IR (Intermediate Representation) → Bytecode; IR is the primary compilation target

### 1.2 Key Characteristics

| Feature | Description |
|---------|-------------|
| **IR decompilation** | IR bytecode can be decompiled back to readable Fitsh source (roundtrip stability) |
| **Function selection** | 4-byte name hash; no overloading; same name = same selector |
| **Library binding** | `lib Name = idx [: address]`; index-based with optional deployment address |
| **Inheritance** | Composition-based via `inherit`; no Solidity-style class inheritance |
| **Abstract hooks** | System payment hooks (`PayableHACD`, `PayableAsset`, etc.) via `abstract` |

### 1.3 Comparison with Solidity

| Aspect | Fitsh | Solidity |
|--------|-------|----------|
| Inheritance | `inherit` (composition) | Class inheritance with `is` |
| Modifiers | None; use `assert`/`if` | `modifier` keyword |
| Integer types | `u8`, `u16`, `u32`, `u64`, `u128` | `uint8`..`uint256`, `int` |
| String/Bytes | `bytes` (quoted strings and hex) | `string`, `bytes` |
| Parameters | `param { a b c }` unpacks to slots | Declared in function signature |
| State mutability | `callextview`/`callusepure` for read-only | `view`/`pure` modifiers |
| Low-level call | `codecall` (source-level `end` optional; runs in-place) | `delegatecall` |
| Payment hooks | `abstract PayableHACD` etc. | `receive()`, `fallback()` |

---

## 2. IR Decompilation Output (Core Feature)

### 2.1 What Is IR Decompilation

Fitsh compiles to IR bytecode. This IR can be **decompiled back to Fitsh source** via `format_ircode_to_lang` or `ircode_to_lang`. The decompiled output is human-readable and, with proper options, can be recompiled to byte-identical bytecode.

### 2.2 Why It Matters

- **Auditing**: Inspect compiled contracts without original source
- **Debugging**: Understand what the VM actually executes
- **On-chain verification**: Verify deployed bytecode against source

### 2.3 Decompilation Options

| Option | Effect |
|--------|--------|
| `trim_param_unpack` | Emit `param { $0 $1 ... }` when param names inferred |
| `hide_default_call_argv` | Omit `nil` or `""` placeholder when no args |
| `call_short_syntax` | Controls whether lib index in `call ... ext(i).sig(...)`/`codecall` prints as a named lib when SourceMap is available |
| `flatten_array_list` | Emit `[a, b, c]` instead of `list { a b c }` |
| `flatten_syscall_cat` | Flatten nested `++` in system call args |
| `recover_literals` | Recover and emit numeric/bytes literals |

### 2.4 Output Forms

- **Parameters**: `param { owner amount fee }` or `param { $0 $1 $2 }` when names unavailable
- **Calls**: Canonical output is `call <effect> <target>.<sig>(...)`, e.g. `call view ext(1).0xabcdef01(addr)`
- **Code splice**: Canonical output is `codecall <libidx>.<sig>`, e.g. `codecall 1.0xabcdef01`

---

## 3. Contract Structure (`contract` Keyword)

### 3.1 Top-Level Syntax

```fitsh
pragma fitsh 1.0.0

contract ContractName {

    deploy {
        protocol_cost: amount("1:248"),
        nonce: 1u32,
        construct_argv: 0xaabb2244
    }

    library [
        Lib1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS,
        Lib2: bJKaNA2dLGxJEwp3xSok8g2buv9Bz65H5
    ]

    inherit [
        BaseToken:   emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS,
        TokenHelper: bJKaNA2dLGxJEwp3xSok8g2buv9Bz65H5
    ]

    abstract PayableHACD(from_addr: address, dianum: u32, diamonds: bytes) {
        return 1
    }

    function external transfer_to(addr: address, amt: u64) -> u32 {
        return this.do_transfer(addr, addr, amt)
    }
}
```

### 3.2 Top-Level Elements

| Element | Purpose |
|--------|---------|
| `deploy { ... }` | Deployment config (protocol_cost, nonce, construct_argv) |
| `library [ ... ]` | `Name: Address` pairs for external contracts |
| `inherit [ ... ]` | `Name: Address` pairs for inheritance chain |
| `abstract Name(...) { ... }` | System payment hooks; return 0 = allow, non-zero = reject |

`pragma fitsh x.y.z` is required as the first effective source item. The current compiler version is `1.0.0`:

- `x` is the incompatible major version; mismatched majors are rejected.
- `y` is the compatible feature version; sources requiring a newer minor version are rejected by older compilers.
- `z` is the equivalent-change patch version; patch differences compile with a warning because only optimization/formatting-equivalent changes are expected.

### 3.3 Function Declaration

```fitsh
function [external] [ircode|bytecode] name(param1: type1, param2: type2) -> ret_type { body }
```

- `external`: Marks function as callable by `CALL` (`External`) path
- `ircode`: Compile to IR
- `bytecode`: Compile to raw bytecode
- If omitted, the function body is compiled to bytecode.
- Modifiers have a fixed order: `external` first, then at most one of `ircode` or `bytecode`.
- `virtual`, `inner`, `view`, `pure`, and `struct` are reserved but unsupported in strict Fitsh 1.0.0 source.

Visibility note:
- `external` is the runtime visibility marker used by external call resolution.
- If naming is confusing in practice, a future revision may introduce clearer aliases.

### 3.4 Deploy Config

`deploy { ... }` is a strict deployment configuration block, not a runtime `map`.

```fitsh
deploy {
    protocol_cost: amount("1:248"),
    nonce: 1u32,
    construct_argv: 0xaabb2244
}
```

- `protocol_cost` must use `amount("...")`.
- `nonce` must fit in `u32`; a `u32` suffix is recommended.
- `construct_argv` must be a bytes literal (`0x...`, `0b...`, or quoted ASCII bytes).
- Field names cannot be repeated.
- A colon after each field name is required.

### 3.5 Constants

Top-level and function-body constants share one literal grammar:

```fitsh
const NAME [: type] = literal
const LIMIT: u64 = 100
const ENABLED: bool = true
const OWNER: address = emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
const TAG: bytes = 0xaabb
```

Constants are compile-time literals only. They may be integer, bool, bytes, char, or address literals; arbitrary expressions are intentionally not accepted.

---

## 4. Keyword List and Descriptions

### 4.1 Declaration and Assignment

| Keyword | Purpose |
|---------|---------|
| `var` | Mutable local variable; allocates slot |
| `let` | Immutable local variable |
| `bind` | Macro binding; no slot; inline expansion |
| `const` | Compile-time constant |
| `param` | Parameter unpacking into slots |
| `lib` | Library binding |

### 4.2 Control Flow

| Keyword | Purpose |
|---------|---------|
| `if` / `else` | Conditional |
| `while` | Loop |
| `return` | Return value |
| `end` | Terminate execution |
| `abort` | Abort |
| `throw` | Throw error |
| `assert` | Assertion |

### 4.3 Debug and Logging

| Keyword | Purpose |
|---------|---------|
| `print` | Debug print |
| `log` | Log event (2..5 args) |

### 4.4 Call Instructions

| Keyword | Purpose |
|---------|---------|
| `call` | Call external contract |
| `callthis` / `callself` / `callsuper` | Internal calls |
| `callextview` / `callusepure` | Read-only calls |
| `codecall` | CodeCall（in-place splice；source-level `end` optional） |
| `bytecode` | Raw bytecode injection |

### 4.5 Type and Literal Keywords

| Keyword | Purpose |
|---------|---------|
| `as` | Type cast |
| `is` | Type check |
| `nil` | Nil literal |
| `list` | List literal |
| `map` | Map literal |
| `true` / `false` | Boolean literals |
| `u8` .. `u128` | Integer types |
| `bytes` | Bytes type |
| `address` | Address type |

---

## 5. Syntax and Examples

### 5.1 Literals

```fitsh
123                    // Integer
0xABC123               // Hex bytes
0b11110000             // Binary bytes (8*n bits)
"hello \"world\" \n"   // String (bytes) with escapes
'A'                    // Char literal (byte)
nil                    // Nil
true                   // Boolean
false                  // Boolean
emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS  // Address
```

### 5.2 Arrays and Lists

```fitsh
[1, 2, 3]              // Array literal
[]                     // Empty list
list { 1 2 3 }         // List keyword (space-separated)
```

### 5.3 Maps

```fitsh
map { "key": "value", 1: addr }
map { }                // Empty map
```

### 5.4 Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+`, `-`, `*`, `/`, `%`, `**` |
| Bitwise | `<<`, `>>`, `&`, `|`, `^` |
| Comparison | `==`, `!=`, `<`, `<=`, `>`, `>=` |
| Logic | `&&`, `||`, `!` |
| Concatenation | `++` |

Precedence (high to low): `!` → `**` → `*`/`/`/`%` → `+`/`-` → `<<`/`>>` → `>=`/`<=`/`>`/`<` → `==`/`!=` → `&` → `^` → `|` → `&&` → `||` → `++`

### 5.5 Compound Assignment

```fitsh
x += 1
x -= 1
x *= 2
x /= 2
```

### 5.6 Control Flow

```fitsh
if x > 0 {
    print "positive"
} else if x < 0 {
    print "negative"
} else {
    print "zero"
}

while cnt > 0 {
    cnt -= 1
}
```

### 5.7 Block Expressions

A block `{ stmt; stmt; value }` evaluates statements and returns the last expression:

```fitsh
var result = {
    var inner = 10
    inner + 1
}
// result == 11
```

---

## 6. Special Language Structures

### 6.1 `param { ... }`

Unpacks list arguments into local slots 0, 1, 2, ...

```fitsh
param { owner amount fee }
// owner -> slot 0, amount -> slot 1, fee -> slot 2
```

- Must appear at the top of the function body
- Canonical IR: `UNPACK(ROLL0, P0)`

### 6.2 `codecall lib_idx.func_sig`

- Optional argument list is supported: `codecall C.f`, `codecall C.f()`, `codecall C.f(nil)`, `codecall C.f(a)`, `codecall C.f(a, b)`
- `codecall C.f`, `codecall C.f()`, and `codecall C.f(nil)` are equivalent
- Source-level trailing `end` is optional; `codecall C.f` and `codecall C.f` + `end` are both valid
- Used for low-level delegation

```fitsh
codecall 0.0xabcdef01
end
```

### 6.3 `bytecode { ... }`

Injects raw bytecode opcodes by name or number:

```fitsh
bytecode { POP DUP SWAP }
```

### 6.4 `list { ... }`

Alternative form for lists (space-separated):

```fitsh
list { 1 2 3 }
```

### 6.5 `map { ... }`

Key-value pairs with `:` separator:

```fitsh
map { "k": "v", 1: addr }
```

### 6.6 `log { ... }`

Log event with 2..5 arguments. Supports `()`, `{}`, `[]` delimiters:

```fitsh
log(1, 2)
log[1, 2, 3, 4, 5]
```

---

## 7. Implicit and Explicit Type Conversions

### 7.1 Implicit Conversions

| Context | Allowed | Notable Rejects |
|---------|--------|------------------|
| Arithmetic | Integer widening; Bytes→Uint (1..16 bytes after trim) | Empty bytes; bytes >16; Bool/Address/Nil |
| Byte ops (concat, slice) | Bool/Uint/Address→Bytes | Nil |
| Branching | Truthiness (any value) | — |
| Call args | Uint widening; Bytes↔Address | Bytes→Uint, Bool→anything |

### 7.2 Explicit Casts

```fitsh
x as u8
x as u16
x as u32
x as u64
x as u128
x as bytes
x as address
```

### 7.3 Type Checks

```fitsh
x is nil
x is not nil
x is list
x is map
x is u64
x is bytes
x is address
```

### 7.4 Pitfalls

1. **Stack arithmetic is uint-only**: `ADD`/`SUB`/`MUL` require operands that are already `u8`..`u128`; `Nil`/`Bool`/`Bytes` are not silently coerced—cast with `as u64` first.
2. **Cross-type compare is rejected**: `Bytes([0x01]) == U8(1)` fails; cast explicitly before comparing.
3. **Explicit stack casts are wider than function boundaries**: `nil as u64`, `1 as bool`, etc. work on the stack; function param checks allow uint-family casts only (`value-cast.md` §6.2).
4. **Bytes↔Uint asymmetry**: Uint→Bytes is fixed-width; Bytes→Uint uses trim + minimal width (explicit `CU*`/`CTO U*` only).
5. **Empty bytes**: `Bytes([])` is not a numeric zero; use `0 as u64` when you need zero.
6. **Map duplicate keys**: literal `map { ... }` / `PACKMAP` and `MERGE` reject duplicate keys (unrecoverable **Fault**); `INSERT` overwrites an existing key (`value-cast.md` §8).

---

## 8. Variables and Local Stack Slots

### 8.1 Variable Types

| Declaration | Slot | Mutability | Evaluation |
|-------------|------|------------|------------|
| `var` | Allocates | Mutable | Immediate |
| `let` | Allocates | Immutable | Immediate |
| `bind` | No slot | — | Lazy (when referenced) |

### 8.2 Slot Addressing

- `$0`, `$1`, ... refer to slots directly
- `var x $5 = 10` assigns to slot 5 explicitly
- `param { a b c }` binds a→0, b→1, c→2

### 8.3 Direct Slot References (`$0`, `$1`, ...)

Identifiers starting with `$` followed by a decimal number (0–255) are **direct slot references**. They bypass the symbol table and bind to the local slot at the given index.

#### Syntax

| Form | Meaning |
|------|---------|
| `$N` | Read slot N (0 ≤ N ≤ 255) |
| `$N = expr` | Write to slot N directly |
| `var name $N = expr` | Bind name to slot N and assign |

#### Relationship with `param`

`param { a b c }` unpacks arguments into slots 0, 1, 2. After that:

- `$0` is the same as the first param
- `$1` is the same as the second param
- `$2` is the same as the third param

You can read or write via `$N` without using the param name.

#### Read and write

```fitsh
param { owner amount }
$0 = "new owner"       // write to slot 0 (overwrites owner)
let first = $0         // read from slot 0
$4 = 999               // write to slot 4 (if allocated)
```

#### Explicit slot in `var` / `let`

To bind a slot to a name:

```fitsh
var opt $10 = 123      // bind "opt" to slot 10, assign 123
let first_arg = $0     // read slot 0 into a new binding
```

#### Slot conflicts

- A slot can only be bound once by `var` or `let` with explicit `$N`
- Avoid mixing `$N` writes with other bindings to the same slot
- Manual slots are reserved via `reserve_slot`; duplicate use causes `slot N already bound`

#### Use cases

| Scenario | Example |
|----------|---------|
| Overwrite param | `$0 = "new owner"` |
| Low-level slot access | `$N` for tight control |
| Interop with `unpack` | `unpack(self.total(), 2)` then `$2`, `$3` |
| Debug / inspection | Inspect slots by index |

#### CAUTION

- Direct writes bypass usual checks (e.g. immutability of `let`).
- `$N` can overwrite params or other locals; use with care.
- Slots must be allocated (e.g. `param` or `var`/`let` with explicit `$N` or earlier auto-allocation) before use.

### 8.4 Example

```fitsh
param { owner amount }
var total = 200        // auto-allocated slot
var opt $10 = 123      // explicit slot 10
$0 = "new owner"       // write to slot 0 (owner)
let first = $0         // read from slot 0
```

---

## 9. Contract Resource Spaces

Contracts can use six resource spaces. Understanding their scope, lifetime, and use cases helps you choose the right one.

### 9.1 Overview

| Resource | Scope | Lifetime | Key/Index | Max size | Fitsh API |
|----------|-------|----------|-----------|----------|-----------|
| **Locals** | Per function call | Call duration | Slot index (0–255) | 256 slots | `var`, `let`, `param`, `$N` |
| **Heap** | Per function call | Call duration | Byte offset | 64 segments × 256 B | `heap_grow`, `heap_write`, `heap_read` |
| **Memory** | Per contract | Transaction | Key (bytes) | ~16 keys | `memory_put`, `memory_get` |
| **Global** | Transaction-wide | Transaction | Key (bytes) | ~20 keys | `global_put`, `global_get` |
| **Storage** | Per contract | Persistent (rent) | Key (bytes) | Rent-based | `storage_load`, `storage_save`, `storage_del`, `storage_rest`, `storage_rent` |
| **Log** | Per contract | Persistent (on chain) | — | 2–5 args per event | `log(...)` |

### 9.2 Locals (Local Stack Slots)

**Scope**: Current function call (frame).  
**Lifetime**: Until the function returns; then reclaimed.

**Use when**:
- Local variables (`var`, `let`, `param`)
- Intermediate values
- Function parameters and return-value staging

**Characteristics**:
- Indexed by slot (0–255)
- Fast; no key hashing
- Automatically reclaimed on return

**Example**: `param { a b }` → a in slot 0, b in slot 1; `var x = 1` → auto-allocated slot.

### 9.3 Heap

**Scope**: Current function call (frame).  
**Lifetime**: Until the function returns.

**Use when**:
- Large buffers or binary data in a single call
- Offset-based access (like C arrays)
- Passing raw bytes to callees via `heap_read` into `Bytes`

**Characteristics**:
- Byte array; grow by segments (256 bytes each)
- `heap_grow(n)` allocates; `heap_write(offset, data)` / `heap_read(offset, len)` access
- `heap_write(offset, data)` accepts a runtime `u32` offset; `heap_write_x` / `heap_write_xl` are `u8` / `u16` immediate-offset helpers
- `heap_read_uint`, `heap_read_uint_long` read fixed-width integers
- Max 64 segments (~16 KB)

**Example**: Parsing or building binary structures within one call.

### 9.4 Memory (Contract Temp)

**Scope**: Per contract (`state_addr`).  
**Lifetime**: Current transaction only; cleared after tx.

**Use when**:
- Passing data between different calls to the same contract in one tx
- Multi-step flows (e.g. deposit → swap → withdraw)
- Cross-call state within a tx

**Characteristics**:
- Key-value (key is bytes)
- Each contract has its own memory
- Not persisted; tx-scoped

**Example** (AMM): `prepare` saves `in_sat`, `in_zhu`; `PayableSAT` / `PayableHAC` read them to complete the flow.

```fitsh
// Step 1: prepare
memory_put("in_sat", sat)
memory_put("in_zhu", zhu)

// Step 2: PayableHAC (later in same tx)
var in_zhu = memory_get("in_zhu")
memory_put("in_sat", nil)  // clear after use
```

### 9.5 Global (Transaction-Wide Temp)

**Scope**: Entire transaction.  
**Lifetime**: Current transaction only.

**Use when**:
- Sharing data across different contracts in one tx
- Transaction-level flags or counters
- Cross-contract coordination

**Characteristics**:
- Single key-value map for the whole tx
- All contracts see the same map
- Not persisted

**Example**: Shared tx-id or step counter used by multiple contracts.

```fitsh
global_put("tx_step", 1)
// ... later, in another contract
var step = global_get("tx_step")
```

### 9.6 Storage (Contract State)

**Scope**: Per contract (`state_addr`).  
**Lifetime**: Persistent; requires rent; survives blocks.

**Use when**:
- Persistent state (balances, config, totals)
- Data that must survive across tx and blocks

**Characteristics**:
- Key-value; key is bytes (e.g. `"b_" ++ addr`)
- Rent-based; `storage_rent(key, amount)` to pay
- `storage_rest(key)` for expiry
- Value types: Nil, Bool, Uint, Address, Bytes
- Max value size ~1280 bytes
- 1 period = 100 blocks; max rent periods per entry = 30000
- Boundary: at exact due height, data is still valid and `storage_rest(key)` returns `0`; expiration starts at next block

**Example**: Token balances, AMM reserves, config.

```fitsh
bind bk = "b_" ++ addr
var balance = storage_load(bk)
if balance is nil {
    balance = 0 as u64
}
storage_save(bk, balance + amount)
```

### 9.7 Log (Events)

**Scope**: Per contract (`state_addr`).  
**Lifetime**: Persistent (on-chain events).

**Use when**:
- Emitting events for indexers or UIs
- Audit trail of actions

**Characteristics**:
- 2–5 arguments per log
- Stored on chain; not queryable from contract

**Example**: `log("Transfer", from, to, amount)`

### 9.8 Decision Guide

| Need | Resource |
|------|----------|
| Local variable / parameter | Locals |
| Large binary buffer in one call | Heap |
| Pass data between calls to same contract in one tx | Memory |
| Share data across contracts in one tx | Global |
| Persistent contract state | Storage |
| Emit events | Log |

### 9.9 Summary

- **Locals / Heap**: Call-scoped; for in-call computation.
- **Memory**: Contract-scoped, tx only; for multi-step flows in one tx.
- **Global**: Tx-scoped; for cross-contract coordination.
- **Storage**: Contract-scoped, persistent; for long-term state.
- **Log**: Contract-scoped, persistent; for events.

---

## 10. `bind` Macro Binding

### 10.1 Behavior

- **Lazy evaluation**: Expression is not evaluated at declaration; evaluated when referenced
- **No slot**: Does not allocate; no `PUT`/`GET`
- **Inline expansion**: Each reference clones the expression template

### 10.2 Use Case

```fitsh
bind bk = "b_" ++ addr
var balance = storage_load(bk)
storage_save(bk, balance + 100)
```

### 10.3 Caution

Side effects (e.g. `storage_save`, `print`) in a `bind` expression only run when the binding is **read**. If never read, the effect never happens. Use `var` for immediate execution.

---

## 11. Other Distinctive Features

### 11.1 Function Call Syntax

| Syntax | Opcode | Use |
|--------|--------|-----|
| `lib.func(...)` | CALL | State-changing call |
| `lib:func(...)` | CALLEXTVIEW | View call |
| `lib::func(...)` | CALLUSEPURE | Pure code-local call |
| `calluseview 1::sig(...)` | CALLUSEVIEW | View code-local call |
| `this.func(...)` | CALLTHIS | Current contract |
| `self.func(...)` | CALLSELF | Current contract |
| `super.func(...)` | CALLSUPER | Parent in inherit chain |

Library resolution note:
- `CALL` (`lib.func(...)`) resolves the library address first, then searches the target and its direct parents only.
- `CALLEXTVIEW` resolves against target + direct parents, while `CALLUSEVIEW/CALLUSEPURE/CODECALL` stay on the exact target library root.

### 11.2 Call Permission and State Access Control

The VM now models runtime permission as **ExecCtx = (ExecDomain, FrameMode)**.

- `ExecDomain` describes the current dispatch policy source: `TopMain`, `TopP2sh`, `TopAbst`, or `Contract`.
- `FrameMode` describes current state-access strength: `External`, `Inner`, `View`, or `Pure`.
- Fixed call instructions (`CALL`, `CALLEXTVIEW`, `CALLUSEVIEW`, `CALLUSEPURE`, `CALLTHIS`, `CALLSELF`, `CALLSUPER`) switch to `ExecDomain::Contract` and set a fixed `FrameMode`.
- `CODECALL` inherits the caller's full `ExecCtx` and runs in-place on the current frame.

#### Selected Call Instructions: Dispatch Matrix

| Call | Syntax | Callee `ExecCtx` | Lookup root | Inheritance search | Frame behavior | State read | State write |
|------|--------|------------------|-------------|--------------------|----------------|------------|-------------|
| `call` | `lib.func(...)` | `Contract + External` | library target | Yes (target + direct parents only) | New frame | Yes | Yes |
| `callextview` | `lib:func(...)` | `Contract + View` | library target | Yes (target + direct parents only) | New frame | Yes | No |
| `calluseview` | `calluseview lib::sig(...)` | `Contract + View` | library target | No (exact root only) | New frame | Yes | No |
| `callusepure` | `lib::func(...)` | `Contract + Pure` | library target | No (exact root only) | New frame | No | No |
| `codecall` | `codecall lib::sig` | inherits caller `ExecCtx` | library target | No (exact root only) | In-place (no new frame) | Inherits | Inherits |
| `callthis` | `this.func(...)` | `Contract + Inner` | `state_addr` | Yes | New frame | Yes | Yes |
| `callself` | `self.func(...)` | `Contract + Inner` | `code_owner` | Yes | New frame | Yes | Yes |
| `callsuper` | `super.func(...)` | `Contract + Inner` | direct parents of `code_owner` | No extra DFS beyond direct-parent entry set | New frame | Yes | Yes |

#### Address Transition Rules (`state_addr` / `code_owner`)

| Call | `state_addr` | `code_owner` |
|------|--------------|--------------|
| `call` | switch to library target | switch to resolved function owner (target or direct parent) |
| `callextview` | unchanged | switch to resolved function owner (target or direct parent) |
| `calluseview` | unchanged | switch to resolved function owner on exact library root |
| `callusepure` | unchanged | switch to resolved function owner on exact library root |
| `codecall` | unchanged | switch current frame to library target |
| `callthis` | unchanged | resolved owner from `state_addr` chain |
| `callself` | unchanged | resolved owner from `code_owner` chain |
| `callsuper` | unchanged | resolved owner from direct-parent entry set |

**State** = Storage, Global, Memory, Log.

**Important**: `codecall` runs in the **current frame** and **fully inherits the caller's ExecCtx** — it has **no independent state access control logic**. All state operations (storage read/write, EXTACTION/EXTENV/EXTVIEW, NTFUNC/NTENV/NTCTL) in the codecall body are governed by the inherited domain/frame permissions. Unlike an isolated call frame, `codecall` does **not** forbid subsequent nested calls; nested `CALL*` instructions continue to follow the normal `(domain, frame, depth)` runtime gates.

#### Orthogonal Permission Matrix

Domain restrictions:

| `ExecDomain` | Allowed calls |
|---|---|
| `TopMain` | `CALL`, `CALLEXTVIEW`, `CALLUSEVIEW`, `CALLUSEPURE`, `CODECALL` |
| `TopP2sh` | `CALLEXTVIEW`, `CALLUSEVIEW`, `CALLUSEPURE`, `CODECALL` |
| `TopAbst` | `CALLTHIS`, `CALLSELF`, `CALLSUPER`, `CALLEXTVIEW`, `CALLUSEVIEW`, `CALLUSEPURE`, `CODECALL` |
| `Contract` | no extra domain restriction |

Frame restrictions:

| `FrameMode` | Allowed calls | State read | State write |
|---|---|---|---|
| `External` | unrestricted by frame mode | Yes | Yes |
| `Inner` | unrestricted by frame mode | Yes | Yes |
| `View` | `CALLEXTVIEW`, `CALLUSEVIEW`, `CALLUSEPURE` | Yes | No |
| `Pure` | `CALLUSEPURE` only | No | No |

`TopAbst` disallows `CALL` (External) to prevent reentrancy from payment hooks into external contracts. `CODECALL` itself does not add an extra nested-call ban; nested calls inside splice code still follow the ordinary entry/effect/depth checks.

#### State Access Control Matrix by Mode

**Storage/Global/Memory/Log Access**:

| Mode | Storage read | Storage write | Global/Memory read | Global/Memory write | Log |
|------|--------------|---------------|-------------------|---------------------|
| Main, P2sh, Abst | Yes | Yes | Yes | Yes | Yes |
| External, Inner | Yes | Yes | Yes | Yes | Yes |
| View | Yes | No | Yes | No | No |
| Pure | No | No | No | No | No |

**Extended Calls (EXTACTION / EXTENV / EXTVIEW)**:

| Mode | EXTACTION | EXTENV | EXTVIEW | Notes |
|------|-----------|--------|---------|-------|
| **Main** (depth==0) | ✅ Yes | ✅ Yes | ✅ Yes | Full access |
| **Main** (depth>0) | ❌ No | ✅ Yes | ✅ Yes | EXTACTION blocked in nested calls, including nested codecall paths |
| **P2sh, Abst** | ❌ No | ✅ Yes | ✅ Yes | EXTACTION entry-only |
| **External, Inner** | ❌ No | ✅ Yes | ✅ Yes | EXTACTION entry-only |
| **View** | ❌ No | ✅ Yes | ✅ Yes | Read-only environment access |
| **Pure** | ❌ No | ❌ No | ❌ No | No external state access |

**Native Calls (NTFUNC / NTENV / NTCTL)**:

| Opcode | Native Call | Source args | Pure Mode | View Mode | Edit (Main/External/Inner) | Function |
|--------|-------------|-------------|-----------|-----------|----------------------------|----------|
| NTENV | `context_address` | 0 | ❌ Forbidden (`InstDisabled`) | ✅ Allowed | ✅ Allowed | Read VM execution context address |
| NTFUNC | `sha2/sha3/ripemd160` | 1 | ✅ Allowed | ✅ | ✅ | Pure hash functions |
| NTFUNC | `hac_to_mei/zhu(amount)`, `mei/zhu_to_hac(count)`, `fold64_to_u64(data)`, `u64_to_fold64(n)` | 1 | ✅ Allowed | ✅ | ✅ | Pure amount/encoding conversion (decode/encode arg types differ; see §11.4) |
| NTFUNC | `pack_asset(serial, amount)` | 2 | ✅ Allowed | ✅ | ✅ | Build AssetAmt bytes from two u64 |
| NTFUNC | `address_ptr` | 1 | ✅ Allowed | ✅ | ✅ | Pure address pointer extraction |
| NTCTL | `defer`, `intent_*` writes (e.g. `intent_put`, `intent_pop`) | 0–2 | ❌ Forbidden (`IntentError`) | ❌ Forbidden (`IntentError`) | ✅ Allowed (Edit only) | Modify tx-local VM state (defer registry, intent scope) |
| NTCTL | `intent_*` reads (e.g. `intent_get`, `intent_kind`, `intent_len`) | 0–2 | ❌ Forbidden (`IntentError`) | ✅ Allowed | ✅ Allowed | Read tx-local intent state |

**Native stack conventions** (opcode-level vs source-level arity):

The VM uses two native call stack models. This is intentional; do not assume all natives share the same stack shape.

| Model | Opcodes | Opcode stack | Source zero-arg form |
|-------|---------|--------------|----------------------|
| Env read | `NTENV`, `ACTENV` | `0 → 1` (no operand consumed) | `context_address()`, `block_height()` |
| Value call | `NTFUNC`, `NTCTL` | `1 → 1` (always pops one argv value) | compiler pushes `nil` when source args are empty |

For **`NTCTL`** / **`NTFUNC`**, opcode metadata always requires one stack input. Natives registered with `argv_len = 0` (e.g. `intent_pop()`, `intent_kind()`) still use the value-call model: the compiler emits `PNIL` then `NTCTL idx`, and the runtime accepts only `nil` as argv. Hand-written bytecode must include the same `nil` placeholder. Decompilers may hide this placeholder when `hide_default_call_argv` is enabled (display only). See also `call-standard.md` §11.3.

For **`NTENV`**, opcode metadata is true zero-input (`0 → 1`); no `nil` placeholder is used.

**Effect gate error codes** (why forbidden calls do not all share one code):

| Layer | Examples | Pure forbidden | Typical error |
|-------|----------|----------------|---------------|
| VM opcode read gate | `SGET`, `NTENV` | yes | `InstDisabled` — instruction disabled in current effect |
| Host action gate | `ACTENV`, `ACTVIEW`, `EXTENV`, `EXTVIEW` | yes | `ActDisabled` — action/env/view call blocked in current effect |
| Native ctl gate | `NTCTL` `intent_*` | per native (`ctl_require_edit` / `ctl_require_non_pure`) | `IntentError` — includes native name; View may allow reads while blocking writes |

`NTENV` uses `InstDisabled` (not `ActDisabled`) because it reads VM-local execution state at the opcode layer, same category as storage read instructions (`nsr!` → `InstDisabled`). `ACTENV` goes through the host action layer and therefore reports `ActDisabled` in `Pure`.

**Summary**:
- **EXTACTION** (asset transfers): Only `Main` mode at `depth == 0`
- **EXTENV** (`block_height`, `tx_main_address`): Forbidden in `Pure`, allowed elsewhere (`ActDisabled`)
- **EXTVIEW** (`check_signature`, `balance`): Forbidden in `Pure`, allowed elsewhere — read-only chain state queries (`ActDisabled`)
- **NTFUNC** (pure computation): Always allowed in all modes including `Pure`
- **NTENV** (`context_address`): Forbidden in `Pure` (reads VM state, `InstDisabled`), allowed elsewhere
- **NTCTL** (`defer`, `intent_*`): Edit-only for writes; reads allowed in View but not Pure; zero source args require a `nil` stack placeholder at bytecode level

#### EXTACTION Restriction

| Condition | EXTACTION allowed |
|-----------|-------------------|
| mode == Main AND depth == 0 | Yes |
| mode != Main OR depth > 0 | No |

`transfer_hac_to`, `transfer_sat_to`, etc. can only run at the top-level main call. They are disabled in `codecall`, in abstract/payment hooks, and in nested calls.

#### Summary

- **call** → External: full state access; callee must be marked `external`
- **callextview** → View: read-only; no storage/global/memory/log writes
- **callusepure** → Pure: no state access; only pure computation and nested CALLUSEPURE
- **codecall** → inherits mode; runs in-place; nested calls remain allowed subject to normal runtime gates; EXTACTION still depends on inherited mode/depth
- **callthis/callself/callsuper** → Inner: full state access; internal-only

### 11.3 Function Lookup: `this`, `self`, and `super`

The VM maintains two key addresses during execution:

- **state_addr**: The storage/log owner — the contract initially called (entry point). Stays unchanged through nested inner/view/pure/codecall dispatch.
- **code_owner**: The code owner — the contract whose code is currently executing. Changes when resolution chooses another owner.

| Call | Resolves in | Search order |
|------|-------------|---------------|
| `this.func(...)` | state_addr | current contract → direct parents (in order) |
| `self.func(...)` | code_owner | current contract → direct parents (in order) |
| `super.func(...)` | code_owner's parents only | direct parents only (skip current owner) |

**When do they differ?** When `super` or `self` moves execution into a parent's code: `code_owner` becomes the parent, but `state_addr` stays the child. Then `this` still resolves in the child (storage context), while `self` resolves in the parent (current code owner).

#### Example 1: Direct call (no inheritance)

```fitsh
contract Token {
    function external balance_of(addr: address) -> u64 {
        bind bk = "b_" ++ addr
        var balance = storage_load(bk)
        if balance is nil {
            balance = 0 as u64
        }
        return balance
    }
    function external transfer_to(addr: address, amt: u64) -> u32 {
        return this.do_transfer(addr, addr, amt)
    }
    function do_transfer(from: address, to: address, amt: u64) -> u32 {
        // ...
    }
}
```

Here `this`, `self`, and `super` all resolve in the same contract. `this.do_transfer(...)` and `self.do_transfer(...)` behave identically.

#### Example 2: Inheritance — `inherit` chain

```fitsh
contract Base {
    function get_value() -> u64 { return 3 }
}
contract Parent {
    inherit [Base: 0x...]
    function get_value() -> u64 { return 2 }
    function compute() -> u64 {
        return this.get_value() * 10000 + self.get_value() * 100 + super.get_value()
    }
}
contract Child {
    inherit [Parent: 0x...]
    function get_value() -> u64 { return 1 }
    function external run() -> u32 {
        let v = super.compute()
        assert v == 10203
        return 0
    }
}
```

- `Child.run()` calls `super.compute()` → resolves in Parent (skip Child). We execute Parent's `compute()`.
- **state_addr** = Child (unchanged)
- **code_owner** = Parent (current code owner)

Inside Parent's `compute()`:

- `this.get_value()` → resolves in **state_addr** (Child) → Child's `get_value()` → **1**
- `self.get_value()` → resolves in **code_owner** (Parent) → Parent's `get_value()` → **2**
- `super.get_value()` → skip Parent, search Parent's inherits → Base's `get_value()` → **3**

Result: `1*10000 + 2*100 + 3 = 10203`

#### Example 3: Inheritance order (first match wins)

```fitsh
contract A { function f() -> u64 { return 10 } }
contract B { function f() -> u64 { return 20 } }
contract C {
    inherit [A: 0x..., B: 0x...]
    function external run() -> u64 {
        return self.f()
    }
}
```

`self.f()` searches C → A → B. A defines `f` first, so the result is **10**. Inherit order defines priority.

#### Example 4: `super` skips current contract

```fitsh
contract Base {
    function helper() -> u64 { return 100 }
}
contract Child {
    inherit [Base: 0x...]
    function helper() -> u64 { return 1 }
    function external run() -> u64 {
        return super.helper()
    }
}
```

`super.helper()` skips Child and searches only in Base. Result: **100** (Base's implementation).

#### Example 5: When to use each

| Use case | Prefer |
|----------|--------|
| Call own or inherited function, resolved from storage context | `this` |
| Call from current code owner (e.g. after `super` into parent) | `self` |
| Call parent's implementation, bypassing override | `super` |

**Summary**: `this` = storage context; `self` = current code owner; `super` = parent chain only.

### 11.4 Native Calls

Native builtins fall into three opcode families. Stack shape and effect gates differ — see §11.2 permission matrix and `call-standard.md` §11.3.

| Family | Opcode | Examples | Stack model |
|--------|--------|----------|-------------|
| Pure func | `NTFUNC` | `sha2`, `hac_to_mei` | `1 → 1`; always one argv on stack |
| VM env read | `NTENV` | `context_address` | `0 → 1`; true zero-arg at bytecode level |
| Runtime ctl | `NTCTL` | `defer`, `intent_*` | `1 → 1`; zero source args use `nil` placeholder |

| Function | Description |
|----------|-------------|
| `context_address()` | Current execution context address (`NTENV`; forbidden in `Pure`, error `InstDisabled`) |
| `block_height()` | Current block height |
| `sha2(data)` | SHA-256 hash |
| `sha3(data)` | SHA3 hash |
| `ripemd160(data)` | RIPEMD-160 hash |
| `hac_to_mei(amount)` | Decode HAC **Amount serialized bytes** to mei count (`u64`); buffer must be fully consumed (trailing bytes rejected) |
| `hac_to_zhu(amount)` | Decode HAC **Amount serialized bytes** to zhu count (`u128`); same rules as `hac_to_mei` |
| `mei_to_hac(mei)` | Encode **mei unit count** (integer scalar, not Amount bytes) to HAC Amount bytes |
| `zhu_to_hac(zhu)` | Encode **zhu unit count** (integer scalar, not Amount bytes) to HAC Amount bytes |
| `u64_to_fold64(n)` | Encode u64 integer scalar to Fold64 bytes |
| `fold64_to_u64(data)` | Decode Fold64 serialized bytes to u64; buffer must be fully consumed |

`hac_to_*` and `mei/zhu_to_hac` intentionally use different argument types: the former expects on-wire Amount bytes (often from `transfer_*` `amount` or a `buf_*` slice), the latter expects unit-count integers. Typical round-trip: `mei_to_hac(n)` → Amount bytes → `hac_to_mei(...)` → `n`. Do not pass integers to `hac_to_*` or Amount bytes to `mei/zhu_to_hac`.
| `pack_asset(serial, amount)` | Encode `(u64,u64)` into AssetAmt bytes |

### 11.5 Extension Actions (EXTACTION)

| Function | Description |
|----------|-------------|
| `transfer_hac_to(addr, amount)` | Transfer HAC |
| `transfer_hac_from(addr, amount)` | Transfer HAC from |
| `transfer_hac_from_to(from, to, amount)` | Transfer HAC between addresses |
| `transfer_sat_to`, `transfer_sat_from`, `transfer_sat_from_to` | SAT transfers |
| `transfer_hacd_single_to`, `transfer_hacd_to`, etc. | HACD transfers |
| `transfer_asset_to`, `transfer_asset_from`, `transfer_asset_from_to` | Asset transfers |

**Note**: EXTACTION is disabled in `codecall` context.

### 11.6 Storage Functions

| Function | Description |
|----------|-------------|
| `storage_load(key)` | Load value |
| `storage_save(key, value)` | Save value |
| `storage_del(key)` | Delete key |
| `storage_rest(key)` | Get rent expiry |
| `storage_rent(key, amount)` | Pay rent |

### 11.7 Memory and Heap

| Function | Description |
|----------|-------------|
| `memory_put(key, value)` | Put into memory; `memory_put(key, nil)` clears the key. Statement-only, no return value. |
| `memory_get(key)` | Get from memory |
| `global_put(key, value)` | Global temp storage; `global_put(key, nil)` clears the key. Statement-only, no return value. |
| `global_get(key)` | Global get |
| `heap_grow(n)` | Grow heap |

TEX note:
- `TexCellAct` is top-level only; it must not execute from runtime `CALL` context.
- TEX conditions observe the current in-tx state at the action point, before final TEX settlement.
- TEX diamond `get` claims quantity only; final names are assigned later at settlement in FIFO order across the whole transaction.
| `heap_write(offset, data)` | Write to heap at a runtime `u32` offset |
| `heap_read(offset, len)` | Read from heap |

### 11.8 Data Structure Functions

| Function | Description |
|----------|-------------|
| `length(list)` | List length |
| `keys(map)` | Map keys |
| `values(map)` | Map values |
| `has_key(map, key)` | Check key |
| `take_first(list)` | Take first element; consumes the list and discards other elements |
| `take_last(list)` | Take last element; consumes the list and discards other elements |
| `append(list, item)` | Append |
| `insert(list, index, item)` | Insert |
| `remove(list, index)` | Remove |
| `clone(val)` | Clone |
| `clear(collection)` | Clear |

### 11.9 Buffer Functions

| Function | Description |
|----------|-------------|
| `buf_cut(buf, start, len)` | Slice |
| `buf_left(n, buf)` | Left n bytes |
| `buf_right(n, buf)` | Right n bytes |
| `buf_left_drop(n, buf)` | Drop left n |
| `buf_right_drop(n, buf)` | Drop right n |
| `byte(buf, index)` | Byte at index |
| `size(buf)` | Size |

### 11.10 Other Ext Functions

| Function | Description |
|----------|-------------|
| `check_signature(addr)` | Verify signature |
| `balance(addr)` | Balance bytes |

### 11.11 Notable: Optional Trailing Comma/Semicolon

Fitsh allows **omitting commas or semicolons at the end of statements and expressions**. This is a distinctive syntax feature compared to C-like languages.

- **Statements**: `var x = 1` and `var x = 1;` are equivalent
- **Expression sequences**: In `list { 1 2 3 }`, elements are space-separated; trailing commas are unnecessary
- **Top-level elements**: Commas between `library`, `inherit` array items are optional

Example:

```fitsh
var a = 1
var b = 2
list { 1 2 3 }
map { "k": "v" }
```

Developers coming from Solidity or similar languages should note: Fitsh does not require statement-ending semicolons.

---

## Quick Reference

- **Function args limit**: 15 (pack list); wrap in `list`/`map` for more
- **Function signature**: 4-byte hash of name only; no overloading
- **`param`**: Must be at top of body
- **`codecall`**: Source-level trailing `end` is optional; runs as in-place splice
- **`bind`**: Lazy; use `var` for side effects
