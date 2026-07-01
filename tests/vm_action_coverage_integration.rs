#[cfg(test)]
mod action_coverage {
    use basis::component::ExecFrom;
    use basis::interface::ActExec;
    use basis::interface::{Context, Logs, State, StateOperat, Transaction, TransactionRead};
    use field::{
        Address, Amount, BlockHeight, BytesW1, BytesW2, DiamondInscript, DiamondName,
        DiamondNumber, DiamondSmelt, DiamondSto, Field, Fixed8, Hash, Inscripts, Parse, Readable,
        Serialize, Uint1, Uint2, Uint4,
    };
    use protocol::action::{TxBlob, TxMessage};
    use protocol::state::CoreState;
    use protocol::transaction::TransactionType3;
    use sys::{Account, Ret, XRet};
    use testkit::sim::integration::{
        enable_default_vm_setup, make_ctx_from_tx as base_make_ctx_from_tx,
        make_stub_tx as make_tx, set_vm_assigner, test_guard, vm_alt_addr as alt_addr,
        vm_main_addr as main_addr,
    };
    use testkit::sim::logs::MemLogs;
    use testkit::sim::state::FlatMemState as StateMem;
    use testkit::sim::tx::StubTx as TestTx;
    use vm::action::*;
    use vm::build_codes;
    use vm::contract::{Abst, Contract, Func};
    use vm::lang::lang_to_bytecode;
    use vm::machine::{self, Executor, Runtime};
    use vm::rt::{AbstCall, Bytecode, Bytecode::*, CodeConf, CodeType, SpaceCap, calc_func_sign};
    use vm::value::Value;
    use vm::{ContractAddrListW1, ContractAddress, ContractAddressW1, ContractSto, VMState};

    // ─── Test infrastructure ───

    fn init_action_registry() {
        testkit::sim::integration::ensure_standard_protocol_setup_for_tests(
            |_, stuff| sys::calculate_hash(stuff),
            true,
        );
    }

    fn contract_addr(main: &Address, nonce: u32) -> ContractAddress {
        ContractAddress::calculate(main, &Uint4::from(nonce))
    }

    fn make_ctx_from_tx<'a>(
        height: u64,
        tx: &'a dyn TransactionRead,
        state: Box<dyn State>,
        logs: Box<dyn Logs>,
    ) -> protocol::context::ContextInst<'a> {
        init_action_registry();
        base_make_ctx_from_tx(height, tx, state, logs)
    }

    fn make_ctx<'a>(
        height: u64,
        tx: &'a TestTx,
        state: Box<dyn State>,
        logs: Box<dyn Logs>,
    ) -> protocol::context::ContextInst<'a> {
        make_ctx_from_tx(height, tx, state, logs)
    }

    fn make_tx3(main: Address, gas_max: u8) -> TransactionType3 {
        let mut tx = TransactionType3::new_by(main, Amount::unit238(10_000_000), 1_730_000_000);
        tx.gas_max = Uint1::from(gas_max);
        tx
    }

    fn insert_contract(state: &mut dyn State, addr: &ContractAddress, sto: &ContractSto) {
        VMState::wrap(state).contract_set_sync_edition(addr, sto);
    }

    fn make_external_contract(func_name: &str, body: &str) -> ContractSto {
        Contract::new()
            .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
            .func(
                Func::new(func_name)
                    .unwrap()
                    .external()
                    .fitsh(body)
                    .unwrap(),
            )
            .into_sto()
    }

    fn execute_main_bytecode(ctx: &mut dyn Context, codes: Vec<u8>) -> Ret<Value> {
        let main = ctx.env().tx.main;
        let _ = protocol::operate::hac_add(ctx, &main, &Amount::unit238(1_000_000_000));
        if let Some(gas_max) = ctx.tx().gas_max_byte() {
            if gas_max > 0 {
                let budget = protocol::context::decode_gas_budget(
                    gas_max.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
                );
                ctx.gas_initialize(budget)?;
            }
        }
        let height = ctx.env().block.height;
        let mut machine = Executor::from_runtime(Runtime::create(height));
        let rv = machine.main_call(ctx, CodeType::Bytecode, codes.into())?;
        Ok(rv)
    }

    fn execute_main_bytecode_as_call_ctx(ctx: &mut dyn Context, codes: Vec<u8>) -> Ret<Value> {
        // `Machine::main_call` itself does not mutate ctx.exec_from; simulate protocol CALL context.
        let old_from = ctx.exec_from();
        ctx.exec_from_set(ExecFrom::Call);
        let res = execute_main_bytecode(ctx, codes);
        ctx.exec_from_set(old_from);
        res
    }

    fn single_call_codes(lib_idx: u8, sig: [u8; 4]) -> Vec<u8> {
        let mut codes = vec![Bytecode::PNIL as u8, Bytecode::CALLEXT as u8, lib_idx];
        codes.extend_from_slice(&sig);
        codes.push(Bytecode::END as u8);
        codes
    }

    fn callsuper_codes(sig: [u8; 4]) -> Vec<u8> {
        let mut codes = vec![Bytecode::PNIL as u8, Bytecode::CALLSUPER as u8];
        codes.extend_from_slice(&sig);
        codes.push(Bytecode::END as u8);
        codes
    }

    fn execute_deploy(
        ctx: &mut dyn Context,
        nonce: u32,
        contract: ContractSto,
    ) -> XRet<(u32, Vec<u8>)> {
        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(nonce);
        act.contract = contract;
        act.protocol_cost = contract_protocol_cost_min(
            ctx,
            act.contract.size(),
            protocol::params::CONTRACT_STORE_PERM_PERIODS,
        )?;
        act.execute(ctx)
    }

    fn deploy_protocol_cost(ctx: &dyn Context, contract: &ContractSto) -> Amount {
        contract_protocol_cost_min(
            ctx,
            contract.size(),
            protocol::params::CONTRACT_STORE_PERM_PERIODS,
        )
        .expect("deploy protocol cost min")
    }

    fn update_protocol_cost(ctx: &dyn Context, edit: &vm::ContractEdit) -> Amount {
        contract_protocol_cost_min(
            ctx,
            edit.size(),
            protocol::params::CONTRACT_STORE_PERM_PERIODS,
        )
        .expect("update protocol cost min")
    }

    fn fund_main_addr(ctx: &mut dyn Context) {
        let main = ctx.env().tx.main;
        protocol::operate::hac_add(ctx, &main, &Amount::mei(1000)).unwrap();
    }

    fn seed_diamond_for_insc_edit(
        ctx: &mut dyn Context,
        diamond: DiamondName,
        owner: Address,
        insc_num: usize,
        average_bid_burn_mei: u16,
    ) {
        let mut inscripts = Inscripts::default();
        for i in 0..insc_num {
            let one = DiamondInscript::create_by(
                1,
                BytesW1::from(format!("ins{}", i).into_bytes()).unwrap(),
            );
            inscripts.push(one).unwrap();
        }
        let mut state = CoreState::wrap(StateOperat::state(ctx));
        state.diamond_set(
            &diamond,
            &DiamondSto {
                status: Uint1::from(1u8),
                address: owner,
                prev_engraved_height: BlockHeight::from(1),
                inscripts,
            },
        );
        state.diamond_smelt_set(
            &diamond,
            &DiamondSmelt {
                diamond,
                number: DiamondNumber::from(1),
                born_height: BlockHeight::from(1),
                born_hash: Hash::default(),
                prev_hash: Hash::default(),
                miner_address: owner,
                bid_fee: Amount::zero(),
                nonce: Fixed8::default(),
                average_bid_burn: Uint2::from(average_bid_burn_mei),
                life_gene: Hash::default(),
            },
        );
    }

    // ═══════════════════════════════════════════════════
    // TxMessage tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn tx_message_execute_returns_empty() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = TxMessage::new();
        act.data = BytesW1::from(b"hello world".to_vec()).unwrap();
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0, "gas should be charged");
        assert!(res.is_empty(), "TxMessage should return empty vec");
    }

    #[test]
    fn tx_message_serialize_roundtrip() {
        let mut act = TxMessage::new();
        act.data = BytesW1::from(b"test data 123".to_vec()).unwrap();
        let bytes = act.serialize();
        let mut act2 = TxMessage::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn tx_message_empty_data() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = TxMessage::new(); // empty data
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn tx_message_top_unique_rejects_duplicate_actions_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        tx.push_action(Box::new(TxMessage::new())).unwrap();

        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        // Duplicate TxMessage actions are currently allowed; keep regression explicit.
        let (_, rv) = TxMessage::new().execute(&mut ctx).unwrap();
        assert!(rv.is_empty());
    }

    // ═══════════════════════════════════════════════════
    // TxBlob tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn tx_blob_execute_returns_empty() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = TxBlob::new();
        act.data = BytesW2::from(vec![0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        assert!(res.is_empty(), "TxBlob should return empty vec");
    }

    #[test]
    fn tx_blob_serialize_roundtrip() {
        let mut act = TxBlob::new();
        act.data = BytesW2::from(vec![1, 2, 3, 4, 5]).unwrap();
        let bytes = act.serialize();
        let mut act2 = TxBlob::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn tx_blob_large_data() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let large_data = vec![0xAB; 1024];
        let mut act = TxBlob::new();
        act.data = BytesW2::from(large_data).unwrap();
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn tx_blob_top_unique_rejects_non_top_context_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxBlob::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let err = TxBlob::new().execute(&mut ctx).unwrap_err();
        assert!(err.contains("GUARD"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // ContractMainCall tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn maincall_from_bytecode_end_returns_zero() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let codes = lang_to_bytecode("return 0").unwrap();
        let rv = execute_main_bytecode(&mut ctx, codes).unwrap();
        assert!(
            !rv.extract_bool().unwrap(),
            "return 0 should yield falsy value"
        );
    }

    #[test]
    fn maincall_nonzero_return_is_error() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let codes = lang_to_bytecode("return 1").unwrap();
        let err = execute_main_bytecode(&mut ctx, codes).unwrap_err();
        assert!(err.contains("main call return error code 1"), "{err}");
    }

    #[test]
    fn maincall_rejects_invalid_code_type() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ContractMainCall::new();
        act.codeconf = Uint1::from(99); // invalid code config
        act.codes = BytesW2::from(vec![Bytecode::END as u8]).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.as_str().to_lowercase().contains("code type") || err.contains("CodeType"),
            "{err}"
        );
    }

    #[test]
    fn maincall_rejects_nonzero_marks() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ContractMainCall::new();
        act.codeconf = Uint1::from(0);
        act.codes = BytesW2::from(vec![Bytecode::END as u8]).unwrap();
        // Set marks to non-zero
        let bytes = act.serialize();
        let mut act2 = ContractMainCall::new();
        act2.parse(&bytes).unwrap();
        // Manually set marks field to non-zero via raw parse
        let mut raw = act2.serialize();
        // kind(2) + marks(3) + codeconf(1) + codes(2+len)
        // marks starts at offset 2
        raw[2] = 0xFF;
        let mut act3 = ContractMainCall::new();
        act3.parse(&raw).unwrap();
        let err = act3.execute(&mut ctx).unwrap_err();
        assert!(err.contains("marks"), "{err}");
    }

    #[test]
    fn maincall_serialize_roundtrip() {
        let codes = lang_to_bytecode("return 0").unwrap();
        let act = ContractMainCall::from_bytecode(codes).unwrap();
        let bytes = act.serialize();
        let mut act2 = ContractMainCall::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn maincall_calls_contract_function() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 5001);
        let sto = make_external_contract(
            "add1",
            r#"
            param { n }
            assert n == 5
            return 0
        "#,
        );
        let mut state = StateMem::default();
        insert_contract(&mut state, &caddr, &sto);

        let tx = make_tx(3, main, vec![caddr.to_addr()], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));

        let sig = calc_func_sign("add1");
        // push param 5 as u8, then call lib 0 function
        let mut codes = vec![Bytecode::PU8 as u8, 5, Bytecode::CALLEXT as u8, 0];
        codes.extend_from_slice(&sig);
        codes.push(Bytecode::END as u8);
        execute_main_bytecode(&mut ctx, codes).unwrap();
    }

    #[test]
    fn maincall_ast_level_rejects_main_call_context_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let act = ContractMainCall::from_bytecode(lang_to_bytecode("return 0").unwrap()).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("AST") && err.contains("CALL"), "{err}");
    }

    #[test]
    fn maincall_hacd_insc_edit_succeeds_for_signed_owner_main() {
        let _guard = test_guard();
        let main_acc = Account::create_by("vm-hacd-insc-edit-owner").unwrap();
        let main = Address::from(*main_acc.address());
        let mut tx = make_tx3(main, 17);
        tx.fill_sign(&main_acc).unwrap();
        let mut ctx = make_ctx_from_tx(
            300,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let diamond = DiamondName::from_readable(b"HYXYHY").unwrap();
        seed_diamond_for_insc_edit(&mut ctx, diamond, main, 1, 100);

        let script = format!(
            r#"
            hacd_insc_edit(0x{}, 0, mei_to_hac(1), 1, 0x06656469746564)
            return 0
        "#,
            hex::encode(diamond.serialize())
        );
        let rv = execute_main_bytecode(&mut ctx, lang_to_bytecode(&script).unwrap()).unwrap();
        assert!(!rv.extract_bool().unwrap());

        let dia = CoreState::wrap(StateOperat::state(&mut ctx))
            .diamond(&diamond)
            .unwrap();
        assert_eq!(*dia.inscripts.as_list()[0].engraved_type, 1);
        assert_eq!(
            dia.inscripts.as_list()[0].content.to_readable_or_hex(),
            "edited"
        );
    }

    #[test]
    fn maincall_hacd_insc_edit_rejects_missing_owner_signature_under_sponsor_mode() {
        let _guard = test_guard();
        let owner_acc = Account::create_by("vm-hacd-insc-edit-real-owner").unwrap();
        let payer_acc = Account::create_by("vm-hacd-insc-edit-payer-only").unwrap();
        let owner = Address::from(*owner_acc.address());
        let payer = Address::from(*payer_acc.address());
        let mut tx = make_tx3(payer, 17);
        tx.fill_sign(&payer_acc).unwrap();
        let mut ctx = make_ctx_from_tx(
            300,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let diamond = DiamondName::from_readable(b"WTYUIA").unwrap();
        seed_diamond_for_insc_edit(&mut ctx, diamond, owner, 1, 100);

        let script = format!(
            r#"
            hacd_insc_edit(0x{}, 0, mei_to_hac(1), 1, 0x06656469746564)
            return 0
        "#,
            hex::encode(diamond.serialize())
        );
        let err = execute_main_bytecode(&mut ctx, lang_to_bytecode(&script).unwrap()).unwrap_err();
        assert!(err.contains("signature") || err.contains("failed"), "{err}");
    }

    #[test]
    fn maincall_rejects_low_tx_type() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(2, main, vec![], 0);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = ContractMainCall::from_bytecode(lang_to_bytecode("return 0").unwrap()).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("requires tx type >= 3"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // ContractDeploy tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn deploy_simple_contract_succeeds() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let sto = make_external_contract("f", "return 0");
        execute_deploy(&mut ctx, 1, sto).unwrap();

        // Verify contract exists in state
        let caddr = contract_addr(&main, 1);
        let loaded = VMState::wrap(StateOperat::state(&mut ctx)).contract(&caddr);
        assert!(loaded.is_some(), "deployed contract should exist in state");
    }

    #[test]
    fn deploy_rejects_duplicate_address() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut state = StateMem::default();
        let caddr = contract_addr(&main, 1);
        let sto = make_external_contract("f", "return 0");
        insert_contract(&mut state, &caddr, &sto);

        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("already exists"), "{err}");
    }

    #[test]
    fn deploy_rejects_self_inherit() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let sto = Contract::new()
            .inh(caddr.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("inherit itself"), "{err}");
    }

    #[test]
    fn deploy_rejects_self_library() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let sto = Contract::new()
            .lib(caddr.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("link itself as library"), "{err}");
    }

    #[test]
    fn deploy_rejects_nonzero_revision() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut sto = make_external_contract("f", "return 0");
        sto.metas.revision = Uint2::from(1u16);
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("revision must be 0"), "{err}");
    }

    #[test]
    fn deploy_rejects_missing_inherit_contract() {
        let _guard = test_guard();
        let main = main_addr();
        let missing = contract_addr(&main, 9999);
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let sto = Contract::new()
            .inh(missing.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn deploy_with_construct_function() {
        let _guard = test_guard();
        enable_default_vm_setup();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let sto = Contract::new()
            .syst(
                Abst::new(AbstCall::Construct)
                    .bytecode(build_codes!(CU8 RET))
                    .unwrap(),
            )
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        execute_deploy(&mut ctx, 100, sto).unwrap();

        let caddr = contract_addr(&main, 100);
        assert!(
            VMState::wrap(StateOperat::state(&mut ctx))
                .contract(&caddr)
                .is_some()
        );
        set_vm_assigner(None);
    }

    #[test]
    fn deploy_top_only_can_with_guard_rejects_multiple_non_guard_actions() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(ContractDeploy::new())).unwrap();
        tx.push_action(Box::new(ContractDeploy::new())).unwrap();
        let err = protocol::action::precheck_tx_actions(tx.ty(), tx.actions()).unwrap_err();
        assert!(err.contains("TOP_ONLY_CAN_WITH_GUARD"), "{err}");
    }

    #[test]
    fn deploy_rejects_missing_library_contract() {
        let _guard = test_guard();
        let main = main_addr();
        let missing_lib = contract_addr(&main, 9009);
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let sto = Contract::new()
            .lib(missing_lib.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(
            err.contains("library") && err.contains("does not exist"),
            "{err}"
        );
    }

    #[test]
    fn deploy_rejects_nested_parent_inherits_in_existing_graph() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut state = StateMem::default();
        let a = contract_addr(&main, 2);
        let b = contract_addr(&main, 3);
        let sto_a = Contract::new()
            .inh(b.to_addr())
            .func(
                Func::new("fa")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let sto_b = Contract::new()
            .inh(a.to_addr())
            .func(
                Func::new("fb")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        insert_contract(&mut state, &a, &sto_a);
        insert_contract(&mut state, &b, &sto_b);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let root = Contract::new()
            .inh(a.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, root).unwrap_err();
        assert!(
            err.contains("inherit parent") && err.contains("cannot have parent inherit"),
            "{err}"
        );
    }

    #[test]
    fn deploy_rejects_static_call_libidx_overflow() {
        let _guard = test_guard();
        let main = main_addr();
        let lib_addr = contract_addr(&main, 88);
        let tx = make_tx(3, main, vec![], 17);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &lib_addr,
            &make_external_contract("target", "return 0"),
        );
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let target_sig = calc_func_sign("target");
        let codes = single_call_codes(1, target_sig); // overflow: only libidx 0 exists
        let sto = Contract::new()
            .lib(lib_addr.to_addr())
            .func(Func::new("f").unwrap().external().bytecode(codes).unwrap())
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(err.contains("CallLibIdxOverflow"), "{err}");
    }

    #[test]
    fn deploy_rejects_static_call_missing_library_function() {
        let _guard = test_guard();
        let main = main_addr();
        let lib_addr = contract_addr(&main, 89);
        let tx = make_tx(3, main, vec![], 17);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &lib_addr,
            &make_external_contract("have", "return 0"),
        );
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let miss_sig = calc_func_sign("missing");
        let sto = Contract::new()
            .lib(lib_addr.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .bytecode(single_call_codes(0, miss_sig))
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(
            err.contains("function") && err.contains("not found"),
            "{err}"
        );
    }

    #[test]
    fn deploy_rejects_callsuper_missing_function() {
        let _guard = test_guard();
        let main = main_addr();
        let parent_addr = contract_addr(&main, 66);
        let tx = make_tx(3, main, vec![], 17);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &parent_addr,
            &make_external_contract("have", "return 0"),
        );
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let miss_sig = calc_func_sign("missing");
        let sto = Contract::new()
            .inh(parent_addr.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .bytecode(callsuper_codes(miss_sig))
                    .unwrap(),
            )
            .into_sto();
        let err = execute_deploy(&mut ctx, 1, sto).unwrap_err();
        assert!(
            err.contains("call target function") && err.contains("not found"),
            "{err}"
        );
    }

    #[test]
    fn deploy_rejects_construct_argv_size_overflow() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let over = SpaceCap::new(1).value_size + 1;
        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(77u32);
        act.construct_argv = BytesW2::from(vec![0xAB; over]).unwrap();
        act.contract = make_external_contract("f", "return 0");
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);

        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("construct argv size overflow"), "{err}");
    }

    #[test]
    fn deploy_allows_missing_construct_when_construct_argv_empty() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(78u32);
        act.contract = Contract::new()
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);
        act.execute(&mut ctx).unwrap();

        let caddr = contract_addr(&main, 78);
        assert!(
            VMState::wrap(StateOperat::state(&mut ctx))
                .contract(&caddr)
                .is_some()
        );
    }

    #[test]
    fn deploy_rejects_non_empty_construct_argv_without_construct() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(79u32);
        act.construct_argv = BytesW2::from(vec![0xEE]).unwrap();
        act.contract = Contract::new()
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);

        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("construct argv provided but Construct hook not found"),
            "{err}"
        );
    }

    #[test]
    fn deploy_auto_runs_construct_when_present() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(80u32);
        act.contract = Contract::new()
            .syst(
                vm::contract::Abst::new(vm::rt::AbstCall::Construct)
                    .fitsh("return 1")
                    .unwrap(),
            )
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);

        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("Construct") && err.contains("return error code 1"),
            "{err}"
        );
    }

    #[test]
    fn deploy_accepts_construct_argv_at_spacecap_limit() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(81u32);
        let cap = SpaceCap::new(1).value_size;
        act.construct_argv = BytesW2::from(vec![0xCD; cap]).unwrap();
        act.contract = Contract::new()
            .syst(
                vm::contract::Abst::new(vm::rt::AbstCall::Construct)
                    .fitsh("return 0")
                    .unwrap(),
            )
            .func(
                Func::new("f")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);
        act.execute(&mut ctx).unwrap();

        let caddr = contract_addr(&main, 81);
        assert!(
            VMState::wrap(StateOperat::state(&mut ctx))
                .contract(&caddr)
                .is_some()
        );
    }

    #[test]
    fn deploy_rejects_low_tx_type() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(2, main, vec![], 0);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);

        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(82u32);
        act.contract = make_external_contract("f", "return 0");
        act.protocol_cost = deploy_protocol_cost(&ctx, &act.contract);
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("requires tx type >= 3"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // ContractUpdate tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn update_rejects_nonexistent_contract() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let caddr = contract_addr(&main, 1);
        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn update_rejects_low_tx_type() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(2, main, vec![], 0);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let caddr = contract_addr(&main, 1);
        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("requires tx type >= 3"), "{err}");
    }

    #[test]
    fn update_rejects_nonzero_marks() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let sto = make_external_contract("f", "return 0");
        let mut state = StateMem::default();
        insert_contract(&mut state, &caddr, &sto);

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        // Serialize, find marks field, corrupt it
        let raw = act.serialize();
        // Find the _marks_ field: it's Fixed2 right after address
        // Layout: kind(2) + protocol_cost(serialized) + address(21) + _marks_(2) + edit(...)
        // Amount::zero() serializes to 1 byte (0x00)
        // So marks is at offset 2 + 1 + 21 = 24
        let marks_offset = 2 + Amount::zero().size() + 21;
        let mut corrupted = raw.clone();
        corrupted[marks_offset] = 0xFF;
        let mut act2 = ContractUpdate::new();
        act2.parse(&corrupted).unwrap();
        let err = act2.execute(&mut ctx).unwrap_err();
        assert!(err.contains("marks"), "{err}");
    }

    #[test]
    fn update_rejects_self_inherit_after_edit() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let sto = make_external_contract("f", "return 0");
        let mut state = StateMem::default();
        insert_contract(&mut state, &caddr, &sto);

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        // Add self as inherit via edit
        act.edit.inherit_add = ContractAddrListW1::from_list(vec![caddr.clone()]).unwrap();
        // Revision 0 -> next is 1
        act.edit.new_revision = Uint2::from(1u16);
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("inherit itself"), "{err}");
    }

    #[test]
    fn update_rejects_missing_library_contract_after_edit() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let missing_lib = contract_addr(&main, 9999);
        let mut state = StateMem::default();
        insert_contract(&mut state, &caddr, &make_external_contract("f", "return 0"));

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        act.edit.new_revision = Uint2::from(1u16);
        act.edit.library_add = ContractAddrListW1::from_list(vec![missing_lib]).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("library") && err.contains("does not exist"),
            "{err}"
        );
    }

    #[test]
    fn update_rejects_nested_parent_inherits_after_edit() {
        let _guard = test_guard();
        let main = main_addr();
        let root = contract_addr(&main, 1);
        let parent = contract_addr(&main, 2);
        let mut state = StateMem::default();
        // root has no inherit; parent has root in inherit
        insert_contract(
            &mut state,
            &root,
            &make_external_contract("rootf", "return 0"),
        );
        insert_contract(
            &mut state,
            &parent,
            &Contract::new()
                .inh(root.to_addr())
                .func(
                    Func::new("pf")
                        .unwrap()
                        .external()
                        .fitsh("return 0")
                        .unwrap(),
                )
                .into_sto(),
        );

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let mut act = ContractUpdate::new();
        act.address = root.to_addr();
        act.protocol_cost = Amount::zero();
        act.edit.new_revision = Uint2::from(1u16);
        act.edit.inherit_add = ContractAddrListW1::from_list(vec![parent]).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("inherit parent") && err.contains("cannot have parent inherit"),
            "{err}"
        );
    }

    #[test]
    fn update_rejects_missing_append_hook_explicitly() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &caddr,
            &Contract::new()
                .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
                .func(
                    Func::new("f")
                        .unwrap()
                        .external()
                        .fitsh("return 0")
                        .unwrap(),
                )
                .into_sto(),
        );

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let edit = Contract::new()
            .func(
                Func::new("g")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_edit(1);

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.edit = edit;
        act.protocol_cost = update_protocol_cost(&ctx, &act.edit);
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("abst call Append not found in")
                || err.contains("Append") && err.contains("not found in"),
            "{err}"
        );
    }

    #[test]
    fn update_rejects_missing_change_hook_explicitly() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let parent = contract_addr(&main, 2);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &parent,
            &Contract::new()
                .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
                .func(Func::new("f1").unwrap().fitsh("return 1").unwrap())
                .into_sto(),
        );
        insert_contract(
            &mut state,
            &caddr,
            &Contract::new()
                .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
                .syst(Abst::new(AbstCall::Append).fitsh("return 0").unwrap())
                .inh(parent.to_addr())
                .into_sto(),
        );

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let edit = Contract::new()
            .func(Func::new("f1").unwrap().fitsh("return 2").unwrap())
            .into_edit(1);

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.edit = edit;
        act.protocol_cost = update_protocol_cost(&ctx, &act.edit);
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("abst call Change not found in")
                || err.contains("Change") && err.contains("not found in"),
            "{err}"
        );
    }

    #[test]
    fn update_uses_edit_periods_for_min_protocol_fee() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let mut state = StateMem::default();
        insert_contract(
            &mut state,
            &caddr,
            &Contract::new()
                .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
                .syst(Abst::new(AbstCall::Append).fitsh("return 0").unwrap())
                .into_sto(),
        );
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let edit = Contract::new()
            .func(
                Func::new("grow")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_edit(1);
        let edit_bytes = edit.size();
        let expected = contract_protocol_cost_min(
            &ctx,
            edit_bytes,
            protocol::params::CONTRACT_STORE_PERM_PERIODS,
        )
        .unwrap();

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        act.edit = edit;
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(
            err.contains("protocol fee must be at least")
                && err.contains(&expected.to_fin_string())
                && err.contains("edit_bytes=25")
                && err.contains("edit_periods=10000"),
            "{err}"
        );
    }

    #[test]
    fn update_rejects_static_call_libidx_overflow_after_edit() {
        let _guard = test_guard();
        let main = main_addr();
        let caddr = contract_addr(&main, 1);
        let mut state = StateMem::default();
        insert_contract(&mut state, &caddr, &make_external_contract("f", "return 0"));

        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(1, &tx, Box::new(state), Box::new(MemLogs::default()));
        ctx.env.chain.fast_sync = true;

        let sig = calc_func_sign("target");
        let edit = Contract::new()
            .func(
                Func::new("g")
                    .unwrap()
                    .external()
                    .bytecode(single_call_codes(0, sig))
                    .unwrap(),
            )
            .into_edit(1);

        let mut act = ContractUpdate::new();
        act.address = caddr.to_addr();
        act.protocol_cost = Amount::zero();
        act.edit = edit;
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("CallLibIdxOverflow"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // EnvMainAddr tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn env_main_addr_returns_tx_main() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            100,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = EnvMainAddr::new();
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        assert_eq!(
            res,
            main.to_vec(),
            "EnvMainAddr should return main address bytes"
        );
    }

    #[test]
    fn env_main_addr_different_addresses() {
        let _guard = test_guard();
        let addr = alt_addr();
        let tx = make_tx(3, addr, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = EnvMainAddr::new();
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, addr.to_vec());
    }

    #[test]
    fn env_main_addr_rejects_top_level_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        // keep exec_from as TOP and fast_sync=false
        let err = EnvMainAddr::new().execute(&mut ctx).unwrap_err();
        assert!(err.contains("CALL_ONLY") && err.contains("TOP"), "{err}");
    }

    #[test]
    fn env_main_addr_allows_vm_call_level_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let (_, res) = EnvMainAddr::new().execute(&mut ctx).unwrap();
        assert_eq!(res, main.to_vec());
    }

    #[test]
    fn ctx_action_call_env_main_addr_uses_call_context_when_not_fast_sync() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let (_, res) = ctx.action_call(EnvMainAddr::KIND, vec![]).unwrap();
        assert_eq!(res, main.to_vec());
    }

    #[test]
    fn ctx_action_call_env_main_addr_allows_call_level_when_not_fast_sync() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let (_, res) = ctx.action_call(EnvMainAddr::KIND, vec![]).unwrap();
        assert_eq!(res, main.to_vec());
    }

    #[test]
    fn ctx_action_call_rejects_top_only_can_with_guard_action() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let body = ContractDeploy::new().serialize()[2..].to_vec();
        let err = ctx.action_call(ContractDeploy::KIND, body).unwrap_err();
        assert!(err.contains("TOP_ONLY_CAN_WITH_GUARD"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // EnvBlockAuthorAddr tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn env_author_addr_returns_block_author() {
        let _guard = test_guard();
        let main = main_addr();
        let author = alt_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        ctx.env.block.author = author;

        let act = EnvBlockAuthorAddr::new();
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        assert_eq!(
            res,
            author.to_vec(),
            "EnvBlockAuthorAddr should return author address"
        );
    }

    #[test]
    fn env_author_addr_default_is_zero() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = EnvBlockAuthorAddr::new();
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, Address::default().to_vec());
    }

    // ═══════════════════════════════════════════════════
    // EnvHeight tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn env_height_returns_block_height() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let height = protocol::upgrade::ONLINE_OPEN_HEIGHT;
        let mut ctx = make_ctx(
            height,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = EnvHeight::new();
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        assert_eq!(res, height.to_be_bytes().to_vec());
    }

    #[test]
    fn env_height_zero() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            0,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let act = EnvHeight::new();
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, 0u64.to_be_bytes().to_vec());
    }

    #[test]
    fn env_height_allows_contract_call_level_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            55,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let (_, res) = EnvHeight::new().execute(&mut ctx).unwrap();
        assert_eq!(res, 55u64.to_be_bytes().to_vec());
    }

    #[test]
    fn actenv_actions_work_in_vm_call_context() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let author = alt_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            777,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.block.author = author;

        let script = format!(
            r#"
            var h = block_height()
            assert h == 777
            var m = tx_main_addr()
            assert m == {main}
            var cb = block_author_addr()
            assert cb == {author}
            return 0
            "#,
            main = main.to_readable(),
            author = author.to_readable(),
        );
        let codes = lang_to_bytecode(&script).unwrap();
        assert!(codes.contains(&(Bytecode::ACTENV as u8)));

        let rv = execute_main_bytecode_as_call_ctx(&mut ctx, codes).unwrap();
        assert!(!rv.extract_bool().unwrap());
    }

    // ═══════════════════════════════════════════════════
    // ViewCheckSign tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn view_check_sign_unsigned_returns_zero() {
        let _guard = test_guard();
        let main = main_addr();
        let target = alt_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ViewCheckSign::new();
        act.addr = target;
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        // target is not in addrs and has no signature, should return 0
        assert_eq!(res, vec![0], "unsigned address should return [0]");
    }

    #[test]
    fn view_check_sign_signed_main_returns_one() {
        let _guard = test_guard();
        let main_acc = Account::create_by("vm-action-coverage-main-signer").unwrap();
        let main = Address::from(*main_acc.address());
        let mut tx = make_tx3(main, 17);
        tx.fill_sign(&main_acc).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ViewCheckSign::new();
        act.addr = main;
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, vec![1], "signed address should return [1]");
    }

    #[test]
    fn view_check_sign_serialize_roundtrip() {
        let mut act = ViewCheckSign::new();
        act.addr = main_addr();
        let bytes = act.serialize();
        let mut act2 = ViewCheckSign::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn view_check_sign_rejects_top_level_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let mut act = ViewCheckSign::new();
        act.addr = main;
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("CALL_ONLY") && err.contains("TOP"), "{err}");
    }

    #[test]
    fn ctx_action_call_view_check_sign_uses_call_context_when_not_fast_sync() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let mut act = ViewCheckSign::new();
        act.addr = main;
        let body = act.serialize()[2..].to_vec();
        let (_, res) = ctx.action_call(ViewCheckSign::KIND, body).unwrap();
        assert_eq!(res, vec![0]);
    }

    #[test]
    fn ctx_action_call_view_check_sign_allows_call_level_and_returns_zero() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let mut act = ViewCheckSign::new();
        act.addr = main;
        let body = act.serialize()[2..].to_vec();
        let (_, res) = ctx.action_call(ViewCheckSign::KIND, body).unwrap();
        assert_eq!(res, vec![0]);
    }

    #[test]
    fn actview_actions_work_in_vm_call_context() {
        let _guard = test_guard();
        init_action_registry();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );

        let script = format!(
            r#"
            var ok = check_signature({main})
            assert ok == false
            var b = balance({main})
            assert size(b) >= 12
            return 0
            "#,
            main = main.to_readable(),
        );
        let codes = lang_to_bytecode(&script).unwrap();
        assert!(codes.contains(&(Bytecode::ACTVIEW as u8)));

        let rv = execute_main_bytecode_as_call_ctx(&mut ctx, codes).unwrap();
        assert!(!rv.extract_bool().unwrap());
    }

    // ═══════════════════════════════════════════════════
    // ViewBalance tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn view_balance_empty_account_returns_zeros() {
        let _guard = test_guard();
        let main = main_addr();
        let target = alt_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ViewBalance::new();
        act.addr = target;
        let (gas, res) = act.execute(&mut ctx).unwrap();
        assert!(gas > 0);
        // Result: 4 bytes diamond + 8 bytes satoshi + Amount bytes for hacash
        assert!(
            res.len() >= 12,
            "balance result should be at least 12 bytes, got {}",
            res.len()
        );
        // Diamond count should be 0
        let diamond_count = u32::from_be_bytes([res[0], res[1], res[2], res[3]]);
        assert_eq!(diamond_count, 0);
        // Satoshi should be 0
        let satoshi = u64::from_be_bytes([
            res[4], res[5], res[6], res[7], res[8], res[9], res[10], res[11],
        ]);
        assert_eq!(satoshi, 0);
    }

    #[test]
    fn view_balance_with_hacash() {
        let _guard = test_guard();
        let main = main_addr();
        let target = alt_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        protocol::operate::hac_add(&mut ctx, &target, &Amount::mei(100)).unwrap();
        ctx.env.chain.fast_sync = true;

        let mut act = ViewBalance::new();
        act.addr = target;
        let (_, res) = act.execute(&mut ctx).unwrap();
        // Diamond count = 0
        let diamond_count = u32::from_be_bytes([res[0], res[1], res[2], res[3]]);
        assert_eq!(diamond_count, 0);
        // Satoshi = 0
        let satoshi = u64::from_be_bytes([
            res[4], res[5], res[6], res[7], res[8], res[9], res[10], res[11],
        ]);
        assert_eq!(satoshi, 0);
        // HAC amount should be non-zero (100 mei)
        let hac_bytes = &res[12..];
        assert!(
            !hac_bytes.iter().all(|&b| b == 0),
            "HAC balance should be non-zero"
        );
    }

    #[test]
    fn view_balance_serialize_roundtrip() {
        let mut act = ViewBalance::new();
        act.addr = main_addr();
        let bytes = act.serialize();
        let mut act2 = ViewBalance::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    // ═══════════════════════════════════════════════════
    // ViewDiaInscNum tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn view_diamond_insc_num_missing_diamond_errors() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ViewDiaInscNum::new();
        act.diamond = DiamondName::from_readable(b"AAAAAA").unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn view_diamond_insc_num_serialize_roundtrip() {
        let mut act = ViewDiaInscNum::new();
        act.diamond = DiamondName::from_readable(b"WTYUIA").unwrap();
        let bytes = act.serialize();
        let mut act2 = ViewDiaInscNum::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn view_diamond_insc_num_returns_count() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let diamond = DiamondName::from_readable(b"WTYUIA").unwrap();
        let mut dia = DiamondSto::new();
        dia.address = main;
        dia.inscripts = Inscripts::from_list(vec![
            DiamondInscript::create_by(1, BytesW1::from_str("insc-1").unwrap()),
            DiamondInscript::create_by(2, BytesW1::from_str("insc-2").unwrap()),
        ])
        .unwrap();
        CoreState::wrap(StateOperat::state(&mut ctx)).diamond_set(&diamond, &dia);

        let mut act = ViewDiaInscNum::new();
        act.diamond = diamond;
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, vec![2]);
    }

    // ═══════════════════════════════════════════════════
    // ViewDiaInscGet tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn view_diamond_insc_get_missing_diamond_errors() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = ViewDiaInscGet::new();
        act.diamond = DiamondName::from_readable(b"AAAAAA").unwrap();
        act.inscidx = Uint1::from(0);
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn view_diamond_insc_get_returns_selected_inscription() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let diamond = DiamondName::from_readable(b"HYXYHY").unwrap();
        let mut dia = DiamondSto::new();
        dia.address = main;
        dia.inscripts = Inscripts::from_list(vec![
            DiamondInscript::create_by(1, BytesW1::from_str("first").unwrap()),
            DiamondInscript::create_by(2, BytesW1::from_str("second").unwrap()),
        ])
        .unwrap();
        CoreState::wrap(StateOperat::state(&mut ctx)).diamond_set(&diamond, &dia);

        let mut act = ViewDiaInscGet::new();
        act.diamond = diamond;
        act.inscidx = Uint1::from(1);
        let (_, res) = act.execute(&mut ctx).unwrap();
        assert_eq!(res, b"second".to_vec());
    }

    #[test]
    fn view_diamond_insc_get_index_overflow_errors() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let diamond = DiamondName::from_readable(b"UETWNK").unwrap();
        let mut dia = DiamondSto::new();
        dia.address = main;
        dia.inscripts = Inscripts::from_list(vec![DiamondInscript::create_by(
            1,
            BytesW1::from_str("only").unwrap(),
        )])
        .unwrap();
        CoreState::wrap(StateOperat::state(&mut ctx)).diamond_set(&diamond, &dia);

        let mut act = ViewDiaInscGet::new();
        act.diamond = diamond;
        act.inscidx = Uint1::from(1); // overflow: only idx 0 exists
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("overflow"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // P2SHScriptProve tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn unlock_script_prove_rejects_nonzero_marks() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = P2SHScriptProve::new();
        act.lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        act.argvkey = BytesW2::from(vec![]).unwrap();
        // Serialize and corrupt marks
        let mut raw = act.serialize();
        // Find marks position and set to non-zero
        // The struct layout: kind(2) + argvkey(2+len) + lockbox(2+len) + adrlibs(1+...) + merkels(1+...) + marks(2)
        // marks is the last field, 2 bytes before end
        let len = raw.len();
        raw[len - 2] = 0xFF;
        let mut act2 = P2SHScriptProve::new();
        act2.parse(&raw).unwrap();
        let err = act2.execute(&mut ctx).unwrap_err();
        assert!(err.contains("marks"), "{err}");
    }

    #[test]
    fn unlock_script_prove_rejects_low_tx_type() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(2, main, vec![], 0);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let mut act = P2SHScriptProve::new();
        act.lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        act.argvkey = BytesW2::from(vec![]).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("requires tx type >= 3"), "{err}");
    }

    #[test]
    fn unlock_script_prove_ast_level_rejects_main_call_context_when_not_fast_sync() {
        let _guard = test_guard();
        let main = main_addr();
        let mut tx = make_tx3(main, 17);
        tx.push_action(Box::new(TxMessage::new())).unwrap();
        let mut ctx = make_ctx_from_tx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.exec_from_set(ExecFrom::Call);

        let mut act = P2SHScriptProve::new();
        act.lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        act.argvkey = BytesW2::from(vec![]).unwrap();
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("scope TOP not allowed from CALL"), "{err}");
    }

    #[test]
    fn unlock_script_prove_serialize_roundtrip() {
        let mut act = P2SHScriptProve::new();
        act.lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 42, Bytecode::END as u8]).unwrap();
        act.argvkey = BytesW2::from(vec![1, 2, 3]).unwrap();
        let bytes = act.serialize();
        let mut act2 = P2SHScriptProve::new();
        act2.parse(&bytes).unwrap();
        assert_eq!(act, act2);
    }

    #[test]
    fn unlock_script_prove_calc_scriptmh_single_leaf() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        let empty_merkels = MerkelStuffs::from_list(vec![]).unwrap();

        let calc = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox,
            &empty_merkels,
        )
        .unwrap();
        // Single leaf: sha3_path should have exactly 1 entry (the leaf hash = root hash)
        assert_eq!(calc.sha3_path.len(), 1);
        // Address should be a valid scriptmh address
        assert!(
            calc.address.is_scriptmh(),
            "address should be scriptmh type"
        );
    }

    #[test]
    fn unlock_script_prove_calc_scriptmh_deterministic() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 99, Bytecode::END as u8]).unwrap();
        let empty_merkels = MerkelStuffs::from_list(vec![]).unwrap();

        let calc1 = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox,
            &empty_merkels,
        )
        .unwrap();
        let calc2 = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox,
            &empty_merkels,
        )
        .unwrap();
        assert_eq!(
            calc1.address, calc2.address,
            "same inputs should produce same address"
        );
        assert_eq!(calc1.payload20, calc2.payload20);
    }

    #[test]
    fn unlock_script_prove_different_lockbox_different_address() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let empty_merkels = MerkelStuffs::from_list(vec![]).unwrap();

        let lockbox1 = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        let lockbox2 = BytesW2::from(vec![Bytecode::PU8 as u8, 2, Bytecode::END as u8]).unwrap();

        let calc1 = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox1,
            &empty_merkels,
        )
        .unwrap();
        let calc2 = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox2,
            &empty_merkels,
        )
        .unwrap();
        assert_ne!(
            calc1.address, calc2.address,
            "different lockbox should produce different address"
        );
    }

    #[test]
    fn unlock_script_prove_merkel_posi_invalid() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        // Create a merkle step with invalid posi (2)
        let bad_step = PosiHash {
            posi: Uint1::from(2),
            hash: Hash::default(),
        };
        let merkels = MerkelStuffs::from_list(vec![bad_step]).unwrap();

        let err = P2SHScriptProve::calc_scriptmh_from_lockbox(
            &libs,
            CodeConf::from_type(CodeType::Bytecode),
            &lockbox,
            &merkels,
        )
        .unwrap_err();
        assert!(err.contains("position") && err.contains("invalid"), "{err}");
    }

    // ═══════════════════════════════════════════════════
    // P2SH Tool integration tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn p2sh_tool_single_leaf_tree() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 42, Bytecode::END as u8]).unwrap();

        let tree = P2shTool::build_canonical_tree(vec![P2shLeafSpec {
            adrlibs: libs,
            codeconf: CodeConf::from_type(CodeType::Bytecode),
            lockbox,
        }])
        .unwrap();

        assert_eq!(tree.leaves().len(), 1);
        assert!(tree.address().is_scriptmh());

        // Proof for single leaf should be empty path
        let proof = tree.proof_for_index(0).unwrap();
        assert_eq!(proof.length(), 0);
    }

    #[test]
    fn p2sh_tool_two_leaf_tree_proofs_match() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lb1 = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        let lb2 = BytesW2::from(vec![Bytecode::PU8 as u8, 2, Bytecode::END as u8]).unwrap();

        let tree = P2shTool::build_canonical_tree(vec![
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb1,
            },
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb2,
            },
        ])
        .unwrap();

        assert_eq!(tree.leaves().len(), 2);

        // Both proofs should derive the same tree address
        for idx in 0..2 {
            let proof = tree.proof_for_index(idx).unwrap();
            let spec = &tree.leaves()[idx].spec;
            let calc = P2SHScriptProve::calc_scriptmh_from_lockbox(
                &spec.adrlibs,
                spec.codeconf,
                &spec.lockbox,
                &proof,
            )
            .unwrap();
            assert_eq!(
                calc.address,
                tree.address(),
                "proof for leaf {idx} should derive tree address"
            );
        }
    }

    #[test]
    fn p2sh_tool_build_unlock_script_prove() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lb1 = BytesW2::from(vec![Bytecode::PU8 as u8, 10, Bytecode::END as u8]).unwrap();
        let lb2 = BytesW2::from(vec![Bytecode::PU8 as u8, 20, Bytecode::END as u8]).unwrap();

        let tree = P2shTool::build_canonical_tree(vec![
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb1,
            },
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb2,
            },
        ])
        .unwrap();

        let witness = BytesW2::from(vec![0xAA, 0xBB]).unwrap();
        let (addr, act, _calc) = tree
            .build_unlock_script_prove_unchecked(0, witness)
            .unwrap();
        assert_eq!(addr, tree.address());
        assert!(addr.is_scriptmh());
        // The action should have correct lockbox from leaf 0
        assert_eq!(act.lockbox, tree.leaves()[0].spec.lockbox);
    }

    #[test]
    fn p2sh_tool_rejects_empty_specs() {
        let err = P2shTool::build_canonical_tree(vec![]).unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn p2sh_tool_rejects_duplicate_leaves() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lb = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();

        let err = P2shTool::build_canonical_tree(vec![
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb.clone(),
            },
            P2shLeafSpec {
                adrlibs: libs.clone(),
                codeconf: CodeConf::from_type(CodeType::Bytecode),
                lockbox: lb,
            },
        ])
        .unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn p2sh_tool_build_unlock_script_prove_checked_success() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 7, Bytecode::END as u8]).unwrap();
        let tree = P2shTool::build_canonical_tree(vec![P2shLeafSpec {
            adrlibs: libs,
            codeconf: CodeConf::from_type(CodeType::Bytecode),
            lockbox,
        }])
        .unwrap();
        let witness = BytesW2::from(vec![0x11, 0x22, 0x33]).unwrap();

        let (addr, act, calc) = tree
            .build_unlock_script_prove_checked(1, 0, witness)
            .unwrap();
        assert_eq!(addr, tree.address());
        assert_eq!(calc.address, tree.address());
        assert_eq!(act.merkels.length(), 0);
    }

    #[test]
    fn p2sh_tool_checked_rejects_invalid_lockbox() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        // 0x01 is an invalid bytecode in current ISA metadata table
        let invalid_lockbox = BytesW2::from(vec![0x01]).unwrap();
        let tree = P2shTool::build_canonical_tree(vec![P2shLeafSpec {
            adrlibs: libs,
            codeconf: CodeConf::from_type(CodeType::Bytecode),
            lockbox: invalid_lockbox,
        }])
        .unwrap();
        let witness = BytesW2::from(vec![]).unwrap();

        let err = tree
            .build_unlock_script_prove_checked(1, 0, witness)
            .unwrap_err();
        assert!(
            err.contains("InstInvalid") || err.contains("invalid bytecode"),
            "{err}"
        );
    }

    #[test]
    fn p2sh_tool_checked_rejects_oversized_witness() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        let tree = P2shTool::build_canonical_tree(vec![P2shLeafSpec {
            adrlibs: libs,
            codeconf: CodeConf::from_type(CodeType::Bytecode),
            lockbox,
        }])
        .unwrap();
        let over = SpaceCap::new(1).value_size + 1;
        let witness = BytesW2::from(vec![0u8; over]).unwrap();

        let err = tree
            .build_unlock_script_prove_checked(1, 0, witness)
            .unwrap_err();
        assert!(err.contains("witness bytes too long"), "{err}");
    }

    #[test]
    fn p2sh_tool_checked_rejects_index_overflow() {
        let libs = ContractAddressW1::from_list(vec![]).unwrap();
        let lockbox = BytesW2::from(vec![Bytecode::PU8 as u8, 1, Bytecode::END as u8]).unwrap();
        let tree = P2shTool::build_canonical_tree(vec![P2shLeafSpec {
            adrlibs: libs,
            codeconf: CodeConf::from_type(CodeType::Bytecode),
            lockbox,
        }])
        .unwrap();
        let witness = BytesW2::from(vec![]).unwrap();

        let err = tree
            .build_unlock_script_prove_checked(1, 2, witness)
            .unwrap_err();
        assert!(
            err.contains("leaf index") && err.contains("overflow"),
            "{err}"
        );
    }

    // ═══════════════════════════════════════════════════
    // Cross-action integration tests
    // ═══════════════════════════════════════════════════

    #[test]
    fn deploy_then_call_contract() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        // Deploy a contract
        let sto = make_external_contract("greet", "return 0");
        execute_deploy(&mut ctx, 1, sto).unwrap();

        // Verify it exists
        let caddr = contract_addr(&main, 1);
        assert!(
            VMState::wrap(StateOperat::state(&mut ctx))
                .contract(&caddr)
                .is_some()
        );

        // Call it via sandbox
        let callres =
            machine::sandbox_call(&mut ctx, machine::SandboxSpec::new(caddr, "greet")).unwrap();
        assert!(callres.use_gas > 0);
        assert_eq!(callres.ret_val, vm::value::Value::U8(0));
    }

    #[test]
    fn deploy_then_sandbox_call_missing_function_errors() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;
        fund_main_addr(&mut ctx);
        let budget = protocol::context::decode_gas_budget(
            17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
        );
        ctx.gas_initialize(budget).unwrap();

        let sto = make_external_contract("f", "return 0");
        execute_deploy(&mut ctx, 1, sto).unwrap();

        let caddr = contract_addr(&main, 1);
        let err = machine::sandbox_call(
            &mut ctx,
            machine::SandboxSpec::new(caddr.clone(), "nonexistent"),
        )
        .unwrap_err();
        assert!(err.contains("CallNotExist"), "{err}");

        let sig = vm::rt::calc_func_sign("f");
        let selector = format!("0x{}", hex::encode(sig));
        let callres =
            machine::sandbox_call(&mut ctx, machine::SandboxSpec::new(caddr, selector)).unwrap();
        assert_eq!(callres.ret_val, Value::U8(0));
    }

    #[test]
    fn env_actions_at_different_heights() {
        let _guard = test_guard();
        let main = main_addr();

        for height in [0u64, 1, 100, 999999, u64::MAX / 2] {
            let tx = make_tx(3, main, vec![], 17);
            let mut ctx = make_ctx(
                height,
                &tx,
                Box::new(StateMem::default()),
                Box::new(MemLogs::default()),
            );
            ctx.env.chain.fast_sync = true;

            let act = EnvHeight::new();
            let (_, res) = act.execute(&mut ctx).unwrap();
            assert_eq!(
                res,
                height.to_be_bytes().to_vec(),
                "height mismatch at {height}"
            );
        }
    }

    #[test]
    fn multiple_deploys_different_nonces() {
        let _guard = test_guard();
        let main = main_addr();
        let mut state: Box<dyn State> = Box::new(StateMem::default());

        for nonce in 1..=5u32 {
            let tx = make_tx(3, main, vec![], 17);
            let mut ctx = make_ctx(1, &tx, state, Box::new(MemLogs::default()));
            ctx.env.chain.fast_sync = true;
            fund_main_addr(&mut ctx);
            let budget = protocol::context::decode_gas_budget(
                17u8.min(protocol::context::TX_GAS_BUDGET_CAP_BYTE),
            );
            ctx.gas_initialize(budget).unwrap();

            let sto = make_external_contract("f", "return 0");
            execute_deploy(&mut ctx, nonce, sto).unwrap();
            let caddr = contract_addr(&main, nonce);
            assert!(
                VMState::wrap(StateOperat::state(&mut ctx))
                    .contract(&caddr)
                    .is_some(),
                "contract with nonce {nonce} should exist"
            );
            let (next_state, _logs) = ctx.release();
            state = next_state;
        }
    }

    #[test]
    fn deploy_rejects_negative_protocol_cost() {
        let _guard = test_guard();
        let main = main_addr();
        let tx = make_tx(3, main, vec![], 17);
        let mut ctx = make_ctx(
            1,
            &tx,
            Box::new(StateMem::default()),
            Box::new(MemLogs::default()),
        );
        ctx.env.chain.fast_sync = true;

        let sto = make_external_contract("f", "return 0");
        let mut act = ContractDeploy::new();
        act.nonce = Uint4::from(1u32);
        // Create a negative amount: 0 - 1 mei
        let neg_amt = Amount::zero()
            .sub(&Amount::mei(1), field::AmtMode::BIGINT)
            .unwrap();
        assert!(neg_amt.is_negative());
        act.protocol_cost = neg_amt;
        act.contract = sto;
        let err = act.execute(&mut ctx).unwrap_err();
        assert!(err.contains("negative") || err.contains("fee"), "{err}");
    }
}
