# Fitsh Syntax Guide (vm module)

This guide is based on the lexer, parser, and call resolution in `vm/src/lang`, with emphasis on patterns that are prone to misuse in DeFi/financial scripts.

## 1. Lexical units
- Fitsh uses the `Tokenizer` in `vm/src/lang/tokenizer.rs` to break source code into `Token`s (keywords, operators, punctuation, identifiers, integers, byte/address literals).  
- Integers accept decimal, `0x`-prefixed hexadecimal, and `0b`-prefixed binary byte-aligned notation; literals prefixed with `0x/0b` become `Bytes` tokens, while other numerics become `Integer(u128)`.  
- Addresses are recognized through `FieldAddress::from_readable`; byte strings are written in quotes with escape support (`\n`, `\r`, `\t`, `\\`).  
- Identifiers allow `$` prefixes (slot access) and alphanumeric characters plus `_`/`$`. Symbol sequences are mapped to keywords or operators (e.g., `+=`, `==`, `++`; see `vm/src/rt/lang.rs:30-169`).

## 2. Keywords and operators
- Keywords cover control flow (`if/else/while`), type annotations (`as/is`, `u8`, etc.), declarations/assignments (`bind/var/lib/ext/param`), primitives (`log/print/assert/throw/return/end/abort`), and low-level hooks (`codecall/bytecode`); see `vm/src/rt/lang.rs`. Canonical external call target in source is `ext(i)` (e.g. `call pure ext(1).0x01020304()`); `lib` is for binding only (`lib Name = idx`).  
- Binary operators honor precedence via `parse_next_op` in `syntax.rs`. `+`/`-` have precedence 120, `*`/`/`/`%` 150, while logic short-circuit operators `&&`/`||` are lowest; concatenation is represented by `++`.  
- The `++` operator concatenates bytes/addresses into a new `Bytes`, commonly used in finance contracts to build storage keys (e.g., `"b_" ++ addr`).

## 3. Expressions
- Literals and identifiers support numbers, `nil`, booleans, `bytes`, `address`, and slot references (`$N`) (`syntax.rs:348-520`).  
- Function/method calls:  
  * `foo(...)` first checks IR functions, native calls, and extension hooks before emitting `CALL` et al. (`vm/src/lang/funcs.rs:83-214`).  
  * `this.bar(...)` becomes `CALLTHIS`; `lib_name.func(...)`, `lib_name:func(...)`, and `lib_name::func(...)` generate `CALL`, `CALLEXTVIEW`, and `CALLUSEPURE`, respectively (`syntax.rs`).
  * Parameter-less calls automatically pack a `nil` argument to keep the expectation of arguments in sync (`deal_func_argv`, `pack_func_argvs`).  
- Type checks/conversions: `expr is Type` and `expr as Type` emit `TIS`/`CU*` instructions and are valid only in expression positions (`syntax.rs:216-325`).  
- Control structures double as expressions: `if` and `while` blocks can produce values, mapped to `IRIFR` and `IRWHILE` when used as expressions (`syntax.rs:296-420`).  
- Containers: `list[...]` builds `PACKLIST`, `map{...}` pushes key/value pairs, and `log{...}` requires 2~5 arguments (`syntax.rs:780-851`).

### 3.1 IR container nodes and stack semantics (IRLIST vs IRBLOCK)

Fitsh source compiles into an IR tree which is then code-generated into bytecode. Some IR nodes are *containers* that primarily control evaluation order and stack cleanup.

- `IRLIST` is a **sequence container** that simply concatenates the codegen of its children **without injecting `POP`**.
  - It does **not** require every element to have a return value.
  - Whether `IRLIST` itself “returns a value” depends on its **last child**:
    - if the last child returns a value, the whole `IRLIST` is a value-producing expression (e.g., `a b n PACKLIST`).
    - if the last child returns nothing, the whole `IRLIST` is a statement-like sequence (e.g., `log(...)` ends with `LOG1..LOG4`).
  - Practical implication: if you place multiple value-producing expressions inside an `IRLIST` and never consume them, the values remain on the VM stack unless later instructions pop them.

- `IRBLOCK` is a **statement block** container.
  - It evaluates its children in order and automatically inserts `POP` to discard intermediate values when needed.
  - Use it when you want “evaluate statements, don’t leak stack values”.

- `IRBLOCKR` is a **block expression** container.
  - It behaves like `IRBLOCK`, but requires the **last statement to return a value** and preserves that value as the block’s result.
  - Empty `IRBLOCKR` is invalid.

- `IRIFR` is an **if-expression** container.
  - Both branches must return a value; otherwise it is invalid as an expression.

Rule of thumb:
- Use `IRLIST` when you are building an expression pipeline and you explicitly control stack effects via the final instruction.
- Use `IRBLOCK/IRBLOCKR` when you want block semantics with automatic cleanup (and optional “return last value”).

### 3.2 Operand stack and handwritten IR (see `operand-stack.md`)

IR children codegen **left to right** → operand stack **bottom to top** (`subx`, `suby`, `subz`). This matches Fitsh call syntax (`choose(c,y,n)` → `[c,y,n]`).

Non-obvious cases for bytecode / IR authors:

- **`PACKLIST` / `PACKMAP` / `PACKTUPLE`**: items (and map k/v pairs) first, **literal count last** on stack top.
- **`PUTX` / `local_x_put`**: stack `[idx, val]` — not `(val, idx)`.
- **`XLG` / `XOP`**: only **rhs** on stack; lhs is `locals[idx]` from the instruction mark. Fitsh does not emit `XLG`.
- **`byte` / `BYTE`**, **`buf_cut` / `CUT`**: stack `[buf, …args]` bottom→top — e.g. `buf_cut(buf, start, len)` → `[buf, start, len]`; do **not** reverse to `len, start, buf`.
- **In-place ops** (`GGET`, casts, binary ops): often update the bottom slot instead of push/pop.

## 4. Statement structure
- `var name [$slot]? = expr`: evaluates immediately and stores the result in a local slot; the slot can be explicitly numbered (`syntax.rs:620-654`). Use this for mutable state or expressions with side effects.  
- `let name = expr`: immutable slot binding that runs eagerly and cannot be reassigned; var declarations without further updates are decompiled into `let` statements.  
- `bind name = expr`: lazy/cached macro binding; declarations store the expression template and never emit `PUT`/`GET`. Every reference to `name` clones the template, so it behaves like an inline macro. Because `bind` does not allocate slots, there is no `$slot` form anymore; use `var` whenever you need a reusable slot.
- `lib Foo = idx [: address]?`: binds a short alias to an external library/contract, optionally with an explicit address, for use in `Foo.method(...)` calls.  
- `param { a b c }`: declares function parameters at the top of the body; the implementation uses `ROLL` and `UNPACK` to populate slots (`syntax.rs:412-452`).  
- `codecall Foo.bar` and `bytecode {...}` inject low-level bytecode directly; use them sparingly, as the compiler does not enforce safety and they can bypass language invariants (`syntax.rs:738-780`).  
- `log`, `print`, `assert`, `throw`, `return`, `end`, and `abort` map directly to IR primitives.  
- Avoid naming `list`/`map` blocks the same as `bind` bindings, and do not reuse slots or empty keys to prevent runtime errors.

## 5. Function call rules and arguments
- Function signatures (`FnSign`) are derived solely from `calc_func_sign(name)` (`vm/src/rt/call.rs:18-40`), meaning functions with the same name but differing parameter sets share the same 4-byte selector; this requires careful naming when evolving interfaces.  
- Arguments go through `deal_func_argv`, which either packs or concatenates them:  
  * `pack_func_argvs` imposes a 15-argument limit; exceeding it raises `function argv length cannot more than 15` (`vm/src/lang/funcs.rs:172-188`).  
  * `concat_func_argvs` builds a byte string via repeated `CAT`.  
- If a function call provides no arguments, Fitsh automatically pushes `nil` so the callee still sees at least one value. For interfaces with more than 15 parameters, wrap the payload in `list`/`map` instead.

## 6. High-risk misuse patterns
1. **`bind` is not evaluated immediately**: Bindings with `storage_save`, `print`, or other side effects only execute when the `bind` variable is read; if never read, the effect never happens. Use `var` (or `let` for read-only results) when you rely on immediate execution.  
2. **Slot conflicts/reuse**: Slots can be manually assigned or automatically allocated by `var`. Because `bind` does not consume slots, these collisions now only occur through explicit `$N` or `var` allocations. Avoid reusing the same slot number across multiple `var` declarations or mixing manual numbering with automatic allocations.
3. **Function arguments capped at 15**: The compiler only allows up to 15 arguments in `pack_func_argvs`. For complex DeFi interfaces, wrap inputs into `list` or `map` structures before calling.  
4. **Function signature only hashes the name**: Overloading or adding a same-name function changes the selector globally—choose unique names to avoid accidental dispatch changes.  
5. **`bytecode`/`codecall` bypass type checks**: Use these low-level hooks only when necessary and document the intent for audit trails.  
6. **Implicit `nil` arguments**: `func()` automatically pushes `nil`; functions that inspect arguments via `is nil` should explicitly pass `nil` to avoid ambiguities.  
7. **`param` only parsed at the front**: `param {}` blocks must appear at the beginning of the function body; nested use triggers `bind_local` errors.

## 7. Recommended practices
- For critical settlement state, prefer `var` when you need mutability; reserve `let` or `bind` for pure, side-effect-free computations or caching.  
- Bind external libraries upfront with `lib Alias = idx [: address]` to avoid scattering hard-coded `CALL` indexes.  
- Only use `bytecode`/`codecall` when you actually need low-level bytecode insertion, and document the purpose for audit trails.  
- Append suffixes or namespaces to functions (`transfer_v1`, `withdraw_spot`) to avoid 4-byte selector collisions.  
- Wrap calls that would otherwise need more than 5–6 parameters into `list`/`map`, and let libraries accept `param { argv }` structures for future-proofing.

Pair this guide with the scripts in `vm/tests/lang.rs` to ensure the syntax-to-IR mapping stays aligned. To extend symbols or types, update `KwTy`, `OpTy`, and `Syntax` in `vm/src/lang` and mirror the changes here.
