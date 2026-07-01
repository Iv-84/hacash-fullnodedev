
Call Entry:

    1. Main
    2. Abst

   
Contract Func Type:

    1. Abst
    2. User


Runtime Space:

    1. Oprand stack  (Func)
    2. Local stack   (Func)
    3. Heap          (Func)
    4. Memory        (Contract temp)
    5. Global        (Public temp)
    6. Storage       (Contract state)


Contract Deploy & Store:

    1. contract max size is 65535 byte = 64kb
    2. function max size is 65535 byte = 64kb
    3. deploy or update contract burn 90 fee


Contract Verify:

    1. irnode compile
    2. bytecode finish with END|RET inst
    3. bytecode inst valid
    4. bytecode param check
    5. bytecode jump dest
    6. CODECALL must follow END op


Contract Code Store Fee:

    1. 50x tx fee


Contract KV State Rent:

    1. rent fee = period * (32 + datasize)
    2. one period = 100 blocks (about 8h)
    3. period max = 30000 (about 27.4 years)
    4. retain = clamp(lifetime/3, min=1, max=300 periods = 100 days)
    5. data recover = rent it again before expire
    6. data len max = 1280 bytes = 32 * 40
    7. data type can store = Nil, Bool, Uint, Address, Bytes


Storage Entry Address:

    - Main => 1Mzbf...
    - P2sh => 3DrTG...
    - Abst => vFgHm...



Storage Write Ban:

    - Static Call


VM Logs:

    - Op:  Log1, Log2, Log3, Log4
    - Gas:   20,   24,   28,   32
    - 

Gas Calculation:

    - 1 gas = 1 byte
    - gas price = fee purity = txfee / txsize (base fee per byte on Type3)
    - tx gas budget comes from tx.gas_max (1-byte lookup-table decoded) and is clamped by chain cap TX_GAS_BUDGET_CAP_BYTE = 99 → 111911 gas (max), set in protocol/src/context/gas.rs
    - each VM call consumes at least: main=48, p2sh=64, abst=80 gas (even if bytecode is smaller)
    - see `vm/src/rt/gas.rs` + `vm/doc/gas-cost.md` for the exact encoding/cost table
    - 


Call Kind:

    - Call        <libidx, fnsig>(argv)
    - ThisCall            <fnsig>(argv)
    - SelfCall            <fnsig>(argv)
    - SuperCall           <fnsig>(argv)
    - ViewCall    <libidx, fnsig>(argv)
    - PureCall    <libidx, fnsig>(argv)
    - CodeCall     <libidx, fnsig>         // no user argv; run delegated code in-place and inherit current ExecCtx(domain + frame) privileges


Call Privileges:

    State is Global Value, Memory Value, Storage Data, Log Data.
    CodeCall is NOT a FrameMode: it inherits the upper-level ExecCtx(domain + frame) privileges and does not add an extra nested-call prohibition.
    Ext actions are still gated by (domain, frame, depth) rules.

    - Main          (State Write) => External,        View, Pure, Code
    - Abst          (State Write) =>        Inner, View, Pure, Code
    - P2sh          (State Write) =>               View, Pure, Code
    - View          (State Read ) =>               View, Pure
    - Pure          (           ) =>                    Pure
    - Code          (- inherit -) =>                               -
    - External | Inner (State Write) => External, Inner, View, Pure, Code (All types)


Call Context Change:

    - state_addr (storage/log context) changes only on External
        - code_owner (code owner for self/super resolution) follows resolved owner:
            External => resolved function owner on target contract graph (target or parent)
            Inner => resolved child/parent
            View/Pure => library target
            CodeCall => library target (in-place)


libidx Resolution Split:

    - CALL (External): target library + direct parents only
    - CALLEXTVIEW: target library + direct parents
    - CALLUSEVIEW/CALLUSEPURE/CODECALL: target library exact root only


Inheritance Resolution (CALLTHIS/CALLSELF/CALLSUPER):

    - Search scope is limited to current owner/root + direct parents in inherits order
    - First match wins; inherits list order defines conflict priority
    - Cycle in inherits list is invalid and triggers InheritsError
    - Diamond inheritance is allowed; only true cycles are rejected


Abst Call Param:

    - Construct( bytes[?] )
    - Change( nil )
    - Append( nil )
    - PermitHAC(      to_addr[21], hacash[3~] )
    - PermitSAT(      to_addr[21], satoshi[8] )
    - PermitHACD(     to_addr[21], dianum[1], diamonds[6~] )
    - PermitAsset(    to_addr[21], serial[8], amount[8] )
    - PayableHAC(   from_addr[21], hacash[3~] )
    - PayableSAT(   from_addr[21], satoshi[8] )
    - PayableHACD(  from_addr[21], dianum[1], diamonds[6~] )
    - PayableAsset( from_addr[21], serial[8], amount[8] )


Add Opcode must Modified:

    1. Bytecode define enum     `./rt/bytecode.rs`
    2. Bytecode metadata table  `./rt/bytecode.rs`
    3. Gas table                `./rt/gas.rs`
    4. lang func define         `./rt/lang.rs`
    5. interpreter              `./interpreter/execute.rs`


Comparison Reference:

    1. Move VM
    2. Ethereum VM
    3. Solana VM
    4. Ton VM
    5. CKB VM
    6. EOS VM
    7. NEO VM
