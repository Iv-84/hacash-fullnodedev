# AST Action Specification

## 1. Purpose

AST actions add deterministic control-flow to the fixed transaction action format.

- `AstSelect` provides ordered trial-and-select execution.
- `AstIf` provides condition + branch execution on top of `AstSelect`.
- AST composes existing actions; it does not add a separate scripting state machine.
- Transaction-level metadata remains static from serialized transaction bytes, even when runtime execution only takes one branch.

This document describes the current business semantics of AST execution, including rollback, warmup behavior, gas charging, signature collection, and VM/P2SH interaction.

## 2. Availability and Placement

- AST is available only when the `ast` feature is enabled.
- A transaction that contains any AST-level action must be `type3` or above.
- `AstSelect`, `AstIf`, and `ContractMainCall` are AST-level actions.
- AST-level actions are valid at top-level transaction execution and inside AST nesting.
- AST-level actions are not valid inside VM call contexts, because VM call levels are above the AST level range.
- AST nesting depth is limited to `6`.
- `P2SHScriptProve` is top-level only and cannot be introduced from AST or VM runtime calls.

## 3. Node Types

### 3.1 `AstSelect`

`AstSelect(exe_min, exe_max, actions)` tries child actions from left to right.

- Execution stops when either the child list is exhausted or `exe_max` successful children have been committed.
- The node succeeds only if total successful children is at least `exe_min`.
- The node returns the return bytes of the last successful child.
- If there is no successful child and the node still succeeds, it returns empty bytes.
- `exe_min == 0` is valid by design.
- Empty and no-op forms such as `0/0` are valid success paths.

### 3.2 `AstIf`

`AstIf(cond, br_if, br_else)` uses an `AstSelect` as its condition node.

- `cond` success means condition is true.
- `cond` revert means condition is false.
- `cond` return bytes are ignored.
- On true, `br_if` is executed.
- On false, `br_else` is executed.
- The node returns the selected branch return bytes.

## 4. Error Model

AST uses two execution outcomes for child actions.

- `Revert`: recoverable branch failure.
- `Fault`: unrecoverable failure.

The meaning depends on the current AST node.

### 4.1 Inside `AstSelect`

- Child `Revert` means: rollback that child attempt and continue with the next child.
- Child `Fault` means: abort the current `AstSelect`.
- If final success count is below `exe_min`, the whole `AstSelect` returns `Revert`.

### 4.2 Inside `AstIf`

- Condition `Revert` means: condition is false.
- Condition `Fault` means: abort the whole `AstIf`.
- Selected branch `Revert` or `Fault` means: fail the whole `AstIf`.

AST preserves the original failure kind when it rethrows node failure.

## 5. Execution and Rollback Model

AST uses two snapshot layers.

### 5.1 Whole-node snapshot

Each `AstSelect` and `AstIf` starts with a node-level snapshot.

Node rollback makes the AST node atomic from the caller view.

- `AstSelect`: if the node finally fails, all previously successful child effects inside that node are rolled back.
- `AstIf`: if the selected branch fails, both condition side effects and branch side effects are rolled back together.

### 5.2 Per-item snapshot

Each attempted child action inside AST runs under its own item snapshot.

- Child success commits the item snapshot into the parent node state.
- Child failure rolls back only that child attempt.
- The parent node then decides whether to continue, choose another branch, or fail.

### 5.3 What rollback restores

AST rollback restores the transaction runtime state that is meant to be branch-local.

- forked protocol state
- log length
- protocol volatile context state
- VM volatile business state

In current implementation this means rollback restores state, log truncation point, `tex_ledger`, protocol volatile AST context, and VM globals/memory.

### 5.4 What rollback does not restore

Rollback is intentionally non-refunding and non-cooling.

- gas already consumed is not refunded
- VM gas remaining is not restored
- VM contract warmup/cache state is not restored

This makes gas and warmup accounting monotonic for the full transaction, even when AST branches are rolled back.

## 6. Gas Model

### 6.1 Gas channels

AST execution can charge gas through three channels.

1. Fixed AST item try cost: `40` gas per attempted child.
2. Child returned gas: the child action's returned `u32` gas value, charged only on success.
3. Child runtime gas: any direct `ctx.gas_consume(...)` done during child execution, including VM runtime and hook-side runtime gas.

### 6.2 Control-flow node gas

`AstSelect` and `AstIf` always return gas `0`.

This is intentional: the control-flow node itself does not contribute returned gas. Only its child attempts charge gas.

### 6.3 Successful vs failed item attempt

- A successful item attempt charges: `40 + child runtime gas + child returned gas`.
- A failed item attempt charges: `40 + any runtime gas already consumed before failure`.

Rollback never refunds either part.

### 6.4 Top-level transaction loop vs AST

All actions return `(gas, ret_bytes)`, but the top-level transaction action loop intentionally ignores returned gas.

Returned gas is only consumed by composition paths such as:

- AST child execution
- VM `EXTACTION` runtime calls

As a result, wrapping an action inside AST changes how its returned gas participates in charging. This is part of the AST composition model.

### 6.5 Transaction gas budget

- Gas budget is initialized only when a `type3+` transaction has non-zero `gas_max`.
- Gas budget is decoded from the transaction gas byte and capped by the chain cap.
- Current transaction gas cap is `111911` (= decode_gas_budget(99), from chain constant `TX_GAS_BUDGET_CAP_BYTE`).
- Max gas charge is pre-collected before execution and unused gas is refunded after transaction settlement.

AST item attempts always consume the fixed try cost, so any AST path that actually attempts children requires initialized transaction gas.

The only AST paths that can succeed without child gas usage are true no-op paths that never attempt a child, such as an empty `0/0` select.

### 6.6 `extra9` × AST / VM / P2SH charging matrix

The following combinations are intentional and should be understood as different charging channels, not as inconsistent execution.

- For legacy Type1/Type2 transactions, `extra9` is a fee-settlement property: if any top-level serialized action has `extra9=true`, `fee_got()` is reduced by one unit level and the burned delta is recorded in legacy accounting.
- For Type3 transactions, `extra9` is not a transaction-wide fee classification. It is a delta-only returned-gas surcharge used only at charge sites that actually consume returned gas.
- `AstSelect` / `AstIf` child attempt in Type3: the fixed AST try cost `40` is charged as shared context gas; on child success, returned gas is additionally charged through the AST returned-gas path using `extra9_surcharge(child.extra9(), child_gas)`.
- `AstSelect` / `AstIf` child = `ContractMainCall` in Type3: total charge is `40` try cost + VM shared runtime gas/min-call gas + any AST returned-gas surcharge from the child action result.
- top-level `ContractMainCall` in Type3: the top-level Type3 loop charges `extra9_surcharge(action.extra9(), ret_gas)`, while VM shared runtime gas is consumed directly through the context gas ledger.
- runtime-created actions (`EXTACTION` / host ACTION path) in Type3 use the same delta-only `extra9` surcharge rule at their returned-gas charge site; they do not retroactively rewrite transaction fee settlement.
- transfer hooks that enter P2SH or contract abstract calls consume shared VM/runtime gas directly; they are not recharged again through an AST returned-gas path unless they themselves create a nested AST child action that succeeds.
- P2SH and abstract hook execution may be rolled back by AST item rollback at the state/log/volatile layer, but any runtime gas spent in those hook calls remains consumed.

## 7. `extra9` semantics by transaction type

### 7.1 Legacy Type1/Type2

Legacy Type1/Type2 transactions treat `extra9` as a top-level fee-settlement property.

- If any serialized top-level action reports `extra9() == true`, `tx.fee_got()` is reduced by one unit level when possible.
- The block fee receiver gets `fee_got()`, not the full `fee()`.
- The burned difference `fee - fee_got` is recorded in `tx_fee_burn90_238` legacy accounting.
- Legacy top-level execution still ignores action returned gas.

These are transaction-level properties and are evaluated from the serialized top-level action list.

### 7.2 Type3

Type3 does not reuse the legacy `burn90` fee-split model.

- `tx.fee_got()` stays equal to the full transaction fee.
- Miner fee receipt is therefore unchanged by `extra9` on Type3.
- Gas purity pricing for Type3 uses the normal full-fee view.
- `extra9` only affects returned-gas charge sites that explicitly call `extra9_surcharge(...)`.
- The current surcharge is delta-only: `extra9_surcharge(true, gas) == gas * 9`, while plain actions add `0` returned-gas charge at that site.

### 7.3 AST / runtime returned-gas charging

For current Type3 composition layers, returned-gas charging happens only where returned gas is explicitly consumed:

- Type3 top-level transaction loop
- AST child success path
- runtime-created action (`EXTACTION`) path

At those sites, `extra9` means “apply the +9x delta surcharge for this returned-gas item”. It does not create a transaction-wide Type3 fee classification.

### 7.4 VM boundary

`ContractMainCall` itself reports `extra9 == false`.

If its bytecode later performs runtime `EXTACTION`, the called runtime action still applies its own returned-gas surcharge rule at that call site, but transaction-level fee settlement remains whatever the transaction type already defines.

Dynamic runtime calls do not rewrite transaction fee settlement after the transaction has been formed.

## 8. Signature Semantics

### 8.1 Static transaction signatures

Transaction signature precheck is static.

- `tx.req_sign()` is collected from the serialized action tree.
- `AstSelect` collects all descendant action signature requirements.
- `AstIf` collects `cond`, `br_if`, and `br_else` signature requirements together.
- Non-privkey addresses are filtered out from transaction signature verification.

This means transaction signature requirements are based on serialized AST structure, not on the branch actually taken at runtime.

This is intentional because transaction signature validity must be deterministic from transaction bytes.

### 8.2 Runtime-created actions

Runtime-created `EXTACTION` payloads are not part of `tx.actions()`.

Because of that, their signature requirements are enforced at runtime by `ctx_action_call(...)`.

- runtime action bytes are parsed into an action object
- runtime `req_sign()` is checked at the call site
- duplicate addresses are deduplicated per call

If a runtime-created action is itself an AST node, its nested AST signature collection still follows the same static-within-that-payload rule.

## 9. VM Interaction

### 9.1 `ContractMainCall`

`ContractMainCall` runs VM code in `ExecMode::Main`.

- VM is initialized once per transaction and reused for nested VM calls.
- The context level is switched to main-call level during VM execution and restored after return.
- VM code is allowed to use `EXTACTION`, `EXTENV`, and `EXTVIEW` under the VM runtime rules.

### 9.2 AST rollback and VM rollback

AST snapshot/rollback only restores VM volatile business state.

- restored: VM globals and VM memory
- not restored: shared gas counter and warmed contract cache

This means a failed AST branch can still leave warmed contracts available for later branches in the same transaction.

That warmup persistence is intentional because contract warmup/load accounting is tied to already-paid transaction execution cost.

### 9.3 Runtime EXTACTION boundary

Runtime `EXTACTION` is only allowed in `ExecMode::Main`.

It is not available in:

- P2SH execution mode
- pure/view-like restricted modes
- contract abstract call paths that are not main mode

This is an important boundary when designing AST + VM combinations.

### 9.4 Non-bug patterns around `extra9` and VM

The following outcomes are intentional protocol semantics and should not be treated as bugs by reviewers.

- unreachable AST branches and later `AstSelect` children still contribute to static `req_sign` aggregation because signature requirements are defined from serialized transaction structure, not from the eventual runtime path.
- Legacy Type1/Type2 `extra9` fee semantics are decided from serialized top-level actions; current Type3 returned-gas surcharge semantics are local to the charge site that actually consumes returned gas.
- AST rollback restores branch-local state but does not refund gas and does not cool VM warmup/cache state, so failed VM/P2SH branches can still make later branches cheaper to load.
- top-level legacy execution and nested AST / VM `EXTACTION` execution intentionally differ in returned-gas handling because the legacy top-level transaction loop discards returned gas while Type3 composition layers may consume it.
- dynamic runtime VM behavior cannot retroactively rewrite legacy fee settlement, current Type3 fee settlement, or transaction pre-signature requirements after transaction bytes have been signed and broadcast.

## 10. P2SH and Action Hook Interaction

Transfer actions may trigger VM hooks after their state change.

- HAC, SAT, HACD, and Asset transfer actions run post-action hooks
- hooks may invoke P2SH code or contract abstract code, depending on `from` and `to`

This produces the following semantics inside AST.

- The transfer action body and its hook execution are part of the same AST item attempt.
- If the hook fails, the AST item rollback reverts both the transfer side effects and hook side effects.
- Any runtime gas spent by the hook remains consumed.

### 10.1 P2SH proof lifetime

- `P2SHScriptProve` stores a P2SH proof in transaction runtime context.
- A scriptmh address can only be proved once per transaction runtime.
- P2SH proof is a top-level preparation action; it is not an AST child action.

In practice, a transaction that wants AST-controlled transfer branches from a scriptmh address must prepare the required `P2SHScriptProve` earlier in the same top-level transaction flow.
A proof prepared before entering an AST node remains available after later AST branch rollback because it belongs to the pre-snapshot transaction runtime context.

### 10.2 Hook mode boundary

P2SH hooks run in `ExecMode::P2sh` and contract hooks run in abstract-call mode.

These hook paths are not equivalent to `ContractMainCall` main-mode execution:

- they can consume runtime gas
- they can mutate branch-local state that AST may later roll back
- they cannot use main-mode-only runtime `EXTACTION`

## 11. Design Rules for Authors

When authoring AST transactions, the following rules are part of the protocol semantics.

- `AstSelect` is an ordered trial list, not a parallel chooser.
- `exe_max` stops later children from executing, but later serialized children still contribute to static metadata such as transaction signatures; legacy Type1/Type2 fee semantics also continue to depend on serialized top-level actions.
- `AstIf.cond` is success-driven, not value-driven; branch choice depends on success vs revert, not on returned bytes.
- `AstSelect` success is provisional until the whole node succeeds; failing `exe_min` rolls back earlier successful children in that node.
- AST rollback is state rollback only; it is not gas refund and not warmup refund.
- Moving an action under AST or invoking it through runtime `EXTACTION` changes how returned gas is consumed; this is expected composition behavior.
- P2SH proof actions must remain top-level, so AST cannot lazily create a new proof inside a branch.
- VM main calls and hook-triggered VM calls are different execution modes and should not be treated as interchangeable.

## 12. Summary

AST in this protocol is a deterministic transaction-composition layer with the following core properties.

- Runtime path selection is reversible for state, logs, and VM volatile business memory.
- Gas and VM warmup are monotonic across the full transaction.
- Transaction signatures are static from serialized transaction bytes, while fee semantics depend on transaction family: legacy Type1/Type2 use top-level serialized extra9 actions and Type3 uses explicit returned-gas charge sites.
- VM main calls, P2SH hooks, and abstract hooks each keep their own execution-mode boundaries inside AST.

These rules define the intended AST business semantics and must be used as the standard reference when reasoning about AST behavior.
