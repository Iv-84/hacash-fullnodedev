# VM Contract Call Standard

Version: current implementation
Audience: VM/runtime engineers, contract developers, test authors, auditors
Document role: protocol standard + test acceptance baseline + developer manual

## 1. Purpose

This standard defines the implemented semantics of the VM contract call system, including:

1. call model
2. opcode set and encoding rules
3. target resolution rules
4. permission control system
5. frame and context transition rules
6. `CODECALL` splice semantics
7. upper-layer contract usage rules
8. test acceptance items

This standard does not define gas pricing formulas.

## 2. Core Model

The VM uses two call forms:

1. `Invoke`: standard function call
2. `Splice`: in-place library code splice

Reference model:

```rust
pub enum CallSpec {
    Invoke {
        target: CallTarget,
        effect: EffectMode,
        selector: FnSign,
    },
    Splice {
        lib: u8,
        selector: FnSign,
    },
}

pub enum CallTarget {
    This,
    Self_,
    Upper,
    Super,
    Ext(u8),
    Use(u8),
}
```

Definitions:

1. `Invoke` always creates a new frame.
2. `Splice` does not create a new frame and reuses the current frame.
3. `This` resolves from the current `state_addr`.
4. `Self_`, `Upper`, and `Super` resolve from the current `code_owner`.
5. `Ext(u8)` means library call with context switch.
6. `Use(u8)` means library lookup without context switch.
7. `CODECALL` is the only public opcode form of `Splice`.

## 3. Opcode Set

Current user contract call opcodes:

| Opcode | Byte | Meaning |
|---|---:|---|
| `CODECALL` | `0x0e` | in-place library code splice |
| `CALL` | `0x0f` | generic invoke |
| `CALLEXT` | `0x10` | external library edit call |
| `CALLEXTVIEW` | `0x11` | external library view call |
| `CALLUSEVIEW` | `0x14` | library code-local view call |
| `CALLUSEPURE` | `0x15` | library code-local pure call |
| `CALLTHIS` | `0x18` | `this` edit call |
| `CALLSELF` | `0x19` | `self` edit call |
| `CALLSUPER` | `0x1a` | `super` edit call |
| `CALLSELFVIEW` | `0x1b` | `self` view call |
| `CALLSELFPURE` | `0x1c` | `self` pure call |

Notes:

1. There is no `TAILCALL` opcode in the current implementation.
2. `0x12`, `0x13`, `0x16`, and `0x17` are currently reserved.
3. Reserved slots are not executable call instructions.

## 4. `CALL` Encoding Standard

### 4.1 Width

`CALL` uses a fixed 6-byte body:

| Byte | Content |
|---|---|
| `body[0]` | flags |
| `body[1]` | target argument |
| `body[2..6]` | 4-byte function selector |

### 4.2 Flags Layout

`flags` is defined as:

1. `bits[0..2]`: target kind
2. `bits[3..4]`: effect mode
3. `bits[5..7]`: reserved, must be `0`

### 4.3 Target Encoding

| Value | Target | `body[1]` rule |
|---:|---|---|
| `0` | `This` | must be `0` |
| `1` | `Self_` | must be `0` |
| `2` | `Upper` | must be `0` |
| `3` | `Super` | must be `0` |
| `4` | `Ext(body[1])` | library index |
| `5` | `Use(body[1])` | library index |
| `6` | invalid | invalid |
| `7` | invalid | invalid |

### 4.4 Effect Encoding

| Value | Effect |
|---|---|
| `00` | `Edit` |
| `01` | `View` |
| `10` | `Pure` |
| `11` | invalid |

### 4.5 Valid Combinations

`CALL` has:

- `6` valid targets
- `3` valid effects
- `18` valid target/effect combinations

### 4.6 Mandatory Decode Rejection

The decoder must reject:

1. non-zero reserved bits
2. target values `6` or `7`
3. effect value `11`
4. `This/Self_/Upper/Super` with `body[1] != 0`
5. body length not equal to `6`

## 5. `CODECALL` Encoding Standard

### 5.1 Width

`CODECALL` uses a fixed 5-byte body:

| Byte | Content |
|---|---|
| `body[0]` | `libidx` |
| `body[1..5]` | 4-byte function selector |

### 5.2 Semantics

1. `CODECALL` does not consume a function-argument value.
2. `CODECALL` does not allocate a new frame.
3. `CODECALL` executes target library code in the current frame.
4. `CODECALL` resolves only against the exact library root.

## 6. Target Resolution Rules

Resolution has two stages:

1. choose the anchor address
2. choose the candidate owner set searched for the selector

### 6.1 Anchor Selection

| Target | Anchor source |
|---|---|
| `This` | current `state_addr` |
| `Self_` | current `code_owner` |
| `Upper` | current `code_owner` |
| `Super` | current `code_owner` |
| `Ext(lib)` | current `lib_table[lib]` |
| `Use(lib)` | current `lib_table[lib]` |
| `CODECALL lib` | current `lib_table[lib]` |

### 6.2 Candidate Search Set

| Target / form | Search set |
|---|---|
| `This` | anchor + direct parents |
| `Self_` | anchor only |
| `Upper` | anchor + direct parents |
| `Super` | direct parents only |
| `Ext(lib)` | anchor + direct parents |
| `Use(lib)` | anchor only |
| `CODECALL lib` | anchor only |

Constraints:

1. The current implementation searches only the anchor and its direct parents.
2. The current implementation does not recursively search grandparents or deeper ancestors.
3. Direct parent order follows the contract `inherit` list order.

## 7. Visibility Rules

Only the following class requires external visibility:

- `Invoke { target: Ext(_), effect: Edit, .. }`

Therefore:

1. `CALLEXT` requires `external`
2. generic `CALL` requires `external` only when encoded as `Ext(lib) + Edit`
3. `CALLEXTVIEW` does not require `external`
4. `CALLUSEVIEW/CALLUSEPURE` do not require `external`
5. `CALLTHIS/CALLSELF/CALLSUPER/CALLSELFVIEW/CALLSELFPURE` do not require `external`
6. `CODECALL` does not require `external`

## 8. Context Transition Rules

Each frame carries:

1. `context_addr`
2. `state_addr`
3. `code_owner`
4. `lib_table`

### 8.1 Context-Switching Calls

Only `Ext(lib)` switches context.

Transition:

1. `context_addr := anchor`
2. `state_addr := Some(anchor)`
3. `code_owner := resolved owner`
4. `lib_table := resolved owner contract.library`

Applies to:

1. `CALLEXT`
2. `CALLEXTVIEW`
3. generic `CALL` with target `Ext(lib)`

### 8.2 Non-Context-Switching Calls

The following do not switch `context_addr/state_addr`:

1. `This`
2. `Self_`
3. `Upper`
4. `Super`
5. `Use(lib)`
6. `CODECALL`

They still update:

1. `code_owner := resolved owner`
2. `lib_table := resolved owner contract.library`

## 9. Permission Control System

Permissions are determined by two dimensions:

1. `entry`: `Main / P2sh / Abst`
2. `effect`: `Edit / View / Pure`

### 9.1 Effect Propagation

| Current effect | Next `Invoke(Edit)` | Next `Invoke(View)` | Next `Invoke(Pure)` | `CODECALL` |
|---|---|---|---|---|
| `Edit` | allowed | allowed | allowed | inherits `Edit` |
| `View` | rejected | allowed | allowed | inherits `View` |
| `Pure` | rejected | rejected | allowed | inherits `Pure` |

Rules:

1. `Invoke` uses explicit effect.
2. `CODECALL` inherits current effect.
3. `Pure` cannot escalate to `View/Edit`.
4. `View` cannot escalate to `Edit`.

### 9.2 Entry-Level Restrictions

The interpreter enforces:

1. outer `Main` forbids internal `Edit` calls
2. outer `P2sh` forbids any `Invoke(Edit)`
3. `Abst` forbids external edit library calls, i.e. `Ext(lib) + Edit`

Notes:

1. edit calls on `this/self/upper/super` are internal edit calls
2. `CALLEXT` is an external edit library call
3. `CODECALL` is not `Invoke`; it only inherits current entry/effect context

### 9.3 Action Permissions

Action permission is independent from selector resolution but depends on current `entry/effect/call_depth`:

1. `ACTION` is allowed only in `Main + Edit + outer entry`
2. `ACTENV` and `ACTVIEW` are forbidden in `Pure`
3. `CODECALL` cannot re-enable top-level `ACTION` because it is not outer entry

### 9.4 State Write Permissions

Write-like operations are forbidden in:

1. `View`
2. `Pure`

Therefore:

1. `CALLEXTVIEW` and `CALLUSEVIEW/CALLUSEPURE` target code cannot write state
2. `CODECALL` also cannot write state if it inherits `View/Pure`

## 10. Frame Standard

### 10.1 `Invoke`

Execution steps:

1. validate the current top operand as function argv
2. resolve the target function
3. pop the argv value
4. allocate a new frame
5. clear operand stack, local stack, and heap in the callee frame
6. re-check argv against callee signature
7. execute callee code

Result:

1. isolated parameters
2. isolated locals
3. isolated heap
4. return value checked against callee return contract

### 10.2 `CODECALL`

Execution steps:

1. do not pop an argv value
2. do not allocate a new frame
3. replace only `bindings`, `pc`, `exec`, and `codes`
4. keep `operands`, `locals`, `heap`, `call_argv`, and `types`

Result:

1. library code shares the current runtime frame state
2. library code directly observes current locals/stack/heap
3. return validation continues to use the caller frame's current return contract
4. logical `call_depth` is still increased

## 11. Parameters and Return Values

### 11.1 `Invoke`

`Invoke` must consume one argv value from the operand stack.

Compiler convention:

1. single argument: raw value
2. multiple arguments: packed argv container
3. zero arguments: `nil`

### 11.2 `CODECALL`

`CODECALL` does not rebuild a new parameter context.

Therefore:

1. `CODECALL` consumes one explicit argv expression value from the operand stack
2. `CODECALL` does not rebuild locals
3. normal callee `param { ... }` prologue still executes if present
4. that prologue operates on the inherited current operand-stack state

Upper-layer development requirement:

1. a `CODECALL` target should be designed specifically for splice execution
2. an arbitrary normal business function should not be reused as a `CODECALL` target without an explicit frame-layout contract
3. any required locals/stack layout must be treated as part of the library interface

### 11.3 Native Calls (`NTFUNC`, `NTENV`, `NTCTL`)

Native calls use **two stack models**. Opcode metadata, the compiler, and hand-written bytecode must follow the model for each opcode family.

#### Env-read model (`NTENV`, `ACTENV`)

Opcode metadata: **`0 → 1`** (no stack operand consumed).

1. Only the opcode byte and native index byte(s) appear in bytecode.
2. Source calls with zero arguments (e.g. `context_address()`, `block_height()`) compile directly — no `nil` push.
3. This differs from `NTCTL` / `NTFUNC` even when the source call also has zero arguments.

#### Value-call model (`NTFUNC`, `NTCTL`)

Opcode metadata: **`1 → 1`** (always consumes exactly one argv value from the operand stack).

Compiler convention (same rule as `Invoke` §11.1):

1. single argument: raw value on stack
2. multiple arguments: packed argv container on stack
3. zero source arguments: push **`nil`** (`PNIL`) before the native opcode

For **`NTCTL`** natives registered with **`argv_len = 0`** (e.g. `intent_pop()`, `intent_kind()`):

1. Fitsh source remains zero-arg: `intent_pop()`
2. Bytecode is `PNIL; NTCTL <idx>` (plus terminal)
3. Runtime validates argv is `nil`; any other value is `IntentError`
4. Hand-written bytecode **must** include the `nil` placeholder; omitting it causes stack underflow

For **`NTFUNC`**, every current native requires at least one source argument; the same `1 → 1` rule applies.

**Decompilation note**: formatters may omit the default `nil` placeholder when `hide_default_call_argv` is enabled. That is display-only; on-chain bytecode still contains `PNIL`.

**Why metadata differs from native `argv_len`**: `argv_len` describes source-level arity; opcode `input` describes the uniform stack contract for the whole opcode family. Zero-arg `NTCTL` natives bridge the two via the `nil` sentinel, matching the Invoke zero-arg convention in §11.1.

**Operand stack on other opcodes** (`PACK*`, `PUTX`, `XLG`, in-place storage gets, etc.): see `operand-stack.md`. Operand letters in `bytecode.rs` are **bottom → top**, matching IR `subx`, `suby`, …

## 12. Source-Language Manual

Canonical source form for `Invoke` is:

- `call <effect> <target>.<sig>(...)`

Where:

1. `<effect>` is one of `edit / view / pure`
2. `<target>` is one of `this / self / upper / super / ext(i) / use(i)`
3. `<sig>` is a 4-byte selector (hex or named function mapped to selector)

All 18 valid `Invoke` combinations are:

- `call edit this.0x01020304()`
- `call view this.0x01020304()`
- `call pure this.0x01020304()`
- `call edit self.0x01020304()`
- `call view self.0x01020304()`
- `call pure self.0x01020304()`
- `call edit upper.0x01020304()`
- `call view upper.0x01020304()`
- `call pure upper.0x01020304()`
- `call edit super.0x01020304()`
- `call view super.0x01020304()`
- `call pure super.0x01020304()`
- `call edit ext(1).0x01020304()`
- `call view ext(1).0x01020304()`
- `call pure ext(1).0x01020304()`
- `call edit use(1).0x01020304()`
- `call view use(1).0x01020304()`
- `call pure use(1).0x01020304()`

Canonical source form for `Splice` is:

- `codecall <libidx>.<sig>(...)`

Source sugar accepted as equivalent empty-argv forms:

- `codecall C.f`
- `codecall C.f()`
- `codecall C.f(nil)`

`codecall` accepts only `.` as separator.

### 12.1 Inheritance-Line Shortcuts

| Source form | Runtime meaning |
|---|---|
| `this.f(args)` | `Invoke(This, Edit, f)` |
| `self.f(args)` | `Invoke(Self_, Edit, f)` |
| `super.f(args)` | `Invoke(Super, Edit, f)` |
| `self:f(args)` | `Invoke(Self_, View, f)` |
| `self::f(args)` | `Invoke(Self_, Pure, f)` |

### 12.2 Library-Line Shortcuts

| Source form | Runtime meaning |
|---|---|
| `C.f(args)` | `Invoke(Ext(C), Edit, f)` |
| `C:f(args)` | `Invoke(Ext(C), View, f)` |
| `C::f(args)` | `Invoke(Use(C), Pure, f)` |
| `codecall C.f` | `Splice(ext(C), f)` |

Notes:

1. Shortcut forms are input sugar.
2. Decompilation output should normalize to canonical `call` / `codecall` form.

### 12.3 Generic `call`

Generic `call` may explicitly specify:

1. effect: `edit / view / pure`
2. target: `this / self / upper / super / ext(idx) / use(idx) / bound library name`

Examples:

- `call edit upper.f(args)`
- `call pure use(1).f(args)`
- `call view ext(0).f(args)`

## 13. Upper-Layer Contract Guidance

### 13.1 When to Use `CALLEXT/CALLEXTVIEW/CALLUSEVIEW/CALLUSEPURE`

Use them when:

1. calling through the library public interface
2. isolation of locals/stack/heap is required
3. parameter and return contracts should follow normal function-call rules
4. current frame internals should remain encapsulated

### 13.2 When to Use `CODECALL`

Use it when:

1. template-like code injection is intended
2. the current function tail should transfer directly into library logic
3. caller and library share an explicit locals/stack/heap agreement
4. new-frame allocation and parameter movement should be avoided

### 13.3 Disallowed Assumptions

1. do not assume `CODECALL` returns to the remaining source code after the splice point
2. do not assume recursive inheritance search beyond direct parents
3. do not assume `CALLEXTVIEW/CALLUSEVIEW/CALLUSEPURE` require `external`
4. do not treat arbitrary normal functions as safe `CODECALL` splice targets

## 14. Test Acceptance Checklist

The following must be treated as acceptance items:

1. `CALL` rejects non-zero reserved bits, invalid target tags, invalid effect bits, and invalid arg usage
2. `CALLEXT` enforces external visibility
3. `CALLEXTVIEW` resolves the library root and its direct parents; `CALLUSEVIEW/CALLUSEPURE` stay on the exact library root
4. `CODECALL` resolves only the library root
5. `This` uses state-chain semantics, `Self_` exact code-root semantics, `Upper` code-chain semantics, `Super` direct-parent semantics
6. `Ext(lib)` switches `context_addr/state_addr`
7. `Use(lib)` and `CODECALL` do not switch `context_addr/state_addr`
8. `CODECALL` reuses current `operands / locals / heap / call_argv / types`
9. `CODECALL` inherits current effect
10. `CODECALL` may continue into nested calls, but must not re-enable top-level `ACTION`
11. `Main/P2sh/Abst` restrictions must match the runtime gates
12. current search scope must stay limited to anchor plus direct parents only

## 15. Summary

| Form | Search set | Context switch | New frame | Effect source |
|---|---|---:|---:|---|
| `Invoke(This, *)` | state root + direct parents | no | yes | explicit |
| `Invoke(Self_, *)` | code root only | no | yes | explicit |
| `Invoke(Upper, *)` | code root + direct parents | no | yes | explicit |
| `Invoke(Super, *)` | direct parents only | no | yes | explicit |
| `Invoke(Ext(lib), *)` | library root + direct parents | yes | yes | explicit |
| `Invoke(Use(lib), *)` | library root only | no | yes | explicit |
| `CODECALL` | library root only | no | no | inherited |

## 16. Value Type Boundaries at Function Calls

Function entry/exit uses `check_param_type` / `cast_param` (`vm/src/value/cast.rs`), which is **stricter** than stack-level `CTO` / `CU*` / `CBYTES`.

| Layer | Typical use | Allowed coercion |
|-------|-------------|------------------|
| Stack explicit cast | `x as u64`, `CBYTES`, `CTO Address` | Sections 4.1–4.4 in `value-cast.md` |
| Function param / return check | Typed signature at `Invoke` boundary | Identical type; uint-family casts only |

Implications for contract authors:

1. A value that passes `as u64` on the stack may still fail when passed into a typed `u32` param if it does not fit.
2. `Bytes`, `Bool`, `Address`, and `Nil` cannot cross into a different param type via boundary cast—even when stack `CTO` would succeed.
3. Large heap payloads must be read with `heap_read` into `Bytes` before passing to typed params, ACTION, or NTFUNC.
