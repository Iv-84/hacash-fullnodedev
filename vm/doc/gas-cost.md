Gas Cost
===

## Base cost every opcode

These are fixed compute gas units from `GasTable::new`. Opcode bytes not listed here use the default base cost `1` (including reserved/invalid byte values before validation rejects them).

- 2: BRL, BRS, BRSL, BRSLN, XLG, XOP, AND, OR, EQ, NEQ, LT, GT, LE, GE, NOT, ADD, SUB, MAX, MIN, INC, DEC
- 3: BSHR, BSHL, BXOR, BOR, BAND
- 4: MUL, DIV, MOD, ABSDIFF, INSERT, REMOVE, TAKEFIRST, TAKELAST, APPEND
- 5: MGET, GGET, NEWLIST, NEWMAP, SQRT, SQRTUP
- 6: DIVUP, DIVEXACT, POW, ADDMOD, CLAMP, FIN2, CLEAR, KEYS, VALUES, TUPLE2LIST, UNPACK
- 8: MULADD, MULSUB, CLONE, MERGE, PACKLIST, PACKMAP, PACKTUPLE
- 10: MPUT, GPUT, CALLSELF, CALLSELFVIEW, CALLSELFPURE, MULMOD, FIN3, FINP3, FINP4
- 12: MTAKE, CALLUSEVIEW, CALLUSEPURE, MULDIV, MULDIVUP
- 16: NTENV, NTCTL, NTFUNC, CALLTHIS, CALLSUPER, CODECALL
- 18: FIN4
- 20: LOG1, CALLEXTVIEW
- 24: LOG2, CALLEXT, CALL
- 28: LOG3, ACTENV, SDEL
- 32: LOG4, ACTVIEW, SLOAD, SSTAT, FINPOW3
- 48: ACTION
- 64: SGET, SNEW, SEDIT, SRENT, SRECV
- 128: SPUT

## Gas limits

- max_gas_of_tx: 111911 (= decode_gas_budget(99), from `TX_GAS_BUDGET_CAP_BYTE = 99` in `protocol/src/context/gas.rs:47`). This is the current hard cap for per-tx gas.
- call_base_main: 48. Compute surcharge added on top of measured Main-entry work.
- call_base_p2sh: 64. Compute surcharge added on top of measured P2SH-entry work.
- call_base_abst: 80. Compute surcharge added on top of measured abstract/hook-entry work.

Per-entry billing formula:

- Let `work` be VM-measured buckets accumulated during this entry (`gas_use` delta since entry start).
- This entry charges `work + call_base_*` into the shared gas meter. `call_base` is applied as compute gas at entry return.
- Insufficient host gas follows normal `settle_compute_gas` / out-of-gas behavior.

## tx.gas_max (1-byte) encoding

`TransactionType3.gas_max` is a 1-byte lookup key. Runtime decodes it with `decode_gas_budget()` and applies the chain cap clamp before `gas_init_tx` (see `protocol/src/context/gas.rs`; VM mirror constants are in `vm/src/rt/gas.rs`).

- `gas_max=0`: no VM gas budget. Any contract/p2sh call fails to initialize gas.
- `gas_max>0`: decoded budget is read from `GAS_BUDGET_LOOKUP_1P07_FROM_138` and then clamped by the chain cap byte (`gas_max_byte.min(TX_GAS_BUDGET_CAP_BYTE)`, where `TX_GAS_BUDGET_CAP_BYTE = 99` → max budget `111911`).
- `b=0`: gas is `0`.
- `b in 1..=255`: `gas = GAS_BUDGET_LOOKUP_1P07_FROM_138[b]`.
- Table generation rule: `table[i] = floor(138 * 1.07^i)`, `i in 0..=255`.
- Table max: `table[255] = 4,292,817,207`.

## Tx-level gas settle (non-bytecode)

- Precharge at tx start (only when `gas_max > 0`): deduct max possible charge for `budget` from tx main address before action execution.
- Refund at tx end: refund `max_charge - used_charge`.
- Burn accounting: used charge is added into `ast_vm_gas_burn_238` stats.
- Rounding: burn amount uses ceil division.

Formula:

- `burn_amount(cost) = ceil(cost * gas_price_purity_238 / gas_rate)`
- `max_charge = burn_amount(initial_budget)`
- `used_charge = burn_amount(initial_budget - gas_remaining)`
- `refund = max_charge - used_charge`

## Dynamic gas

Dynamic gas is added on top of opcode base gas. Unless noted as linear multiplication, `byte/N` and `item/N` use runtime ceil division: `0 -> 0`, otherwise `(len - 1) / N + 1`.

For dynamic billing based on a VM value object, byte size is usually `Value::val_size()`.

### Space and movement

- 5: every local slot allocated by ALLOC.
- 20: every new memory key.
- 32: every new global key.
- 32 + byte/64: every cold contract load, where byte is `ContractSto.size()`.
- 1024: every newly created persistent storage key.
- HGROW: first 8 heap segments cost 2, 4, 8, 16, 32, 64, 128, 256; later segments cost 256 each.
- 1 per moved stack item: POPN moves `n`, ROLL0 moves 1, ROLL moves `n + 1`, REV moves `n`, SWAP moves 2.

### Stack payload copy/write/op

- byte/32: DUP, DUPN, GET, GET0, GET1, GET2, GET3, GETX, MGET, GGET, MTAKE, PBUF, PBUFL.
- byte/28: PUT, PUTX, MPUT, GPUT, UNPACK local-slot writes (`stack_write_div`).
- byte/20: CAT, JOIN, BYTE, CUT, LEFT, RIGHT, LDROP, RDROP. The measured byte count is the source/input payload size used by the operation.

### Reference compare

- byte/24: EQ, NEQ, XLG (`==` / `!=` only), using `stack_cmp_div`.
- The compare byte count comes from `value_compare_fee(left, right, container_cmp_header)`.
- Ordered comparisons LT, GT, LE, GE and ordered XLG marks only use base gas.

### Native/action calls

NTFUNC, NTENV, and NTCTL have three components: opcode base, byte-extra, and native fixed gas returned by the native implementation.

- byte/16: NTFUNC input argv bytes and return `val_size()`.
- byte/16: NTENV return `val_size()`.
- byte/16: NTCTL return `val_size()`. Current runtime does not separately byte-meter the NTCTL input value.
- byte/12: ACTION input body bytes.
- byte/12: ACTVIEW input body bytes and return `val_size()`.
- byte/12: ACTENV return `val_size()`.
- ACTION, ACTVIEW, and ACTENV also include host-returned gas (`bgasu`).

### Heap read/write

- byte/16: HREAD, HREADU, HREADUL.
- byte/12: HWRITE, HWRITEX, HWRITEXL.

### Compo/tuple handling

KEYS and VALUES byte counts include output bytes. For maps, byte count includes key bytes plus value bytes.

- item/4: HASKEY, ITEMGET, KEYS, VALUES.
- item/2: PACKLIST, PACKMAP, PACKTUPLE, INSERT, REMOVE, CLEAR, TAKEFIRST, TAKELAST, APPEND.
- item/1: CLONE, MERGE, TUPLE2LIST.
- byte/40: INSERT map key bytes and inserted value bytes; APPEND value bytes; CLEAR total compo `val_size()`; MERGE source bytes; ITEMGET/TAKEFIRST/TAKELAST output value bytes; KEYS/VALUES output bytes; CLONE copied bytes; TUPLE2LIST copied bytes.
- LENGTH has only opcode base gas.

### Log render

- byte/1: LOG1, LOG2, LOG3, LOG4, where byte is the sum of all topic/data `val_size()` values.

### Status and persistent storage

Status (`SGET`/`SPUT`) is priced separately from persistent storage rent.

- SGET: base 64 + `8 * value_byte` for a non-Nil value.
- SPUT: base 128 + `32 * key_byte + 32 * value_byte`.
- SSTAT: base 32 only; it returns a fixed status tuple and does not add SLOAD value-read gas.
- SLOAD: base 32 + `20 + value_byte` for a non-Nil active value.
- SNEW: base 64 + `storage_key_cost` 1024 + `(20 + value_byte) * period`.
- SEDIT: base 64 + `(20 + value_byte) * storage_edit_mul`, where `storage_edit_mul = 4`; it may rebate trimmed live credit.
- SRENT: base 64 + `(20 + current_value_byte) * period`.
- SRECV: base 64 + `((20 + current_value_byte) * period) / 3`.
- SDEL: base 28 only for gas use; it may rebate remaining live credit plus `storage_key_cost`.

One storage period is 100 blocks.

### Manual burn

- BURN: default base gas 1 + immediate `u16` gas.

### IR format fee

IR code is converted to runtime bytecode before execution. The executable stream is the compiled code only; it is not prefixed with a synthetic BURN instruction.

- byte/16: raw serialized IR byte length (`FnObj.codes.len()`).
- Charging point: frame entry, immediately before the frame's first instruction executes.
- Gas bucket: resource gas.
- The IR format fee is charged on every execution of that IR function body.

## Examples

1. SLOAD with `value_byte = 40`:

```text
base = 32
dynamic = 20 + 40 = 60
total = 92 gas
```

2. SEDIT with `value_byte = 80`, before any rebate:

```text
base = 64
dynamic = (20 + 80) * 4 = 400
total = 464 gas
```

3. LOG2 with total log bytes 100:

```text
base = 24
dynamic = 100
total = 124 gas
```

4. SWAP:

```text
base = 1
dynamic = 2 moved items
total = 3 gas
```

## Expected Usage (DeFi Scenarios)

These are rough estimates. Real costs depend on opcode mix, contract sizes, host-returned gas from ACTION/ACTVIEW/ACTENV, and storage state.

- Contract load cost: `32 * new_loads + sum(ceil(contract_bytes_i / 64))`.
- Common U64-like balances use `value_byte = 8`.
- Existing SEDIT of a U64-like value costs `64 + (20 + 8) * 4 = 176`.
- SNEW of a U64-like value for 1 period costs `64 + 1024 + (20 + 8) = 1116`.

| Scenario | Contract Loads | ACTION | NTFUNC | Other Compute Opcodes | Heap | Storage | Log | Estimated Total |
|---|---|---|---|---|---|---|---|---|
| Light DeFi op | 1 cold load, ~8KB | 0 | 0 | ~300 opcodes, avg 2 gas | none | `SLOAD*2` + existing `SEDIT*2`, U64 values | `LOG2*1`, ~24B | ~1250-1400 |
| Typical DeFi op | 3 cold loads, each ~8KB | 1 body ~96B, host gas ~200 | 3 hash calls, ~32B input | ~1200 opcodes | HGROW 2 seg + HWRITE/HREAD 512B | `SLOAD*10` + existing `SEDIT*6`, U64 values | `LOG3*2`, ~48B each | ~5000-5600 |
| Heavy DeFi op | 6 cold loads, each ~12KB | 2 bodies ~256B, host gas ~400 each | 6 hash calls, ~32B input | ~1800 opcodes | HGROW 4 seg + HWRITE/HREAD 1024B | `SLOAD*20` + existing `SEDIT*12`, U64 values | `LOG4*2`, ~64B each | ~9500-10500 |

Suggested workflow:

1. List contracts/libraries touched, estimate cold contract loads and contract sizes.
2. List counts of `SLOAD`, `SNEW`, `SEDIT`, `SRENT`, `SRECV`, `SGET`, and `SPUT` with value/key byte sizes.
3. If large parameters are assembled in heap, list HGROW segments and HREAD/HWRITE byte sizes.
4. Treat ACTION/ACTVIEW/ACTENV body bytes and host-returned gas as variables.

## Coverage Notes

The following are intentionally not separately metered in the current version beyond base or existing aggregate gas:

- Storage key length/hash CPU cost in `skey()`; key bytes are validated and long keys may be hashed, but no extra hash-work gas is charged.
- Value key encoding/decoding and type-cast CPU overhead are not itemized as separate dynamic gas.
- SWAP/ROLL/REV/POPN use item movement gas, not payload byte-copy gas.
- NTCTL input value size is not separately byte-metered; only return bytes and native fixed gas are charged.

Potential future refinement:

- Add explicit gas for heavy key-hash paths if storage key size or hash cost becomes a bottleneck.
- Add NTCTL input-size metering if intent/control payload size becomes a practical bottleneck.
