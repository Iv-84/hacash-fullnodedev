
#[cfg(test)]
mod machine_file_test {

    use super::*;
    use crate::contract::{Abst, Contract, Func};
    use crate::lang::lang_to_bytecode;
    use crate::rt::CodeType;

    use crate::value::ValueTy as VT;
    use basis::component::Env;
    use basis::interface::Context;
    use field::{Address, Amount, Uint4};
    use testkit::sim::context::make_ctx_with_state;
    use testkit::sim::state::FlatMemState as StateMem;
    use testkit::sim::tx::StubTxBuilder;

    fn test_base_addr() -> Address {
        Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap()
    }

    fn test_contract(base: &Address, nonce: u32) -> crate::ContractAddress {
        crate::ContractAddress::calculate(base, &Uint4::from(nonce))
    }

    fn new_test_machine() -> (Machine, Runtime) {
        (Machine::new(), Runtime::create(1))
    }

    struct TestVmHost<'a> {
        ctx: &'a mut dyn Context,
        gas_remaining: i64,
    }

    impl<'a> TestVmHost<'a> {
        fn new(ctx: &'a mut dyn Context, gas_remaining: i64) -> Self {
            Self { ctx, gas_remaining }
        }
    }

    impl VmHost for TestVmHost<'_> {
        fn height(&self) -> u64 {
            self.ctx.env().block.height
        }

        fn main_entry_bindings(&self) -> FrameBindings {
            FrameBindings::root(self.ctx.tx().main(), self.ctx.env().tx.addrs.clone().into())
        }

        fn gas_remaining(&self) -> i64 {
            self.gas_remaining
        }

        fn gas_charge(&mut self, gas: i64) -> VmrtErr {
            if gas < 0 {
                return itr_err_fmt!(GasError, "gas cost invalid: {}", gas);
            }
            self.gas_remaining -= gas;
            if self.gas_remaining < 0 {
                return itr_err_code!(OutOfGas);
            }
            Ok(())
        }

        fn gas_rebate(&mut self, gas: i64) -> VmrtErr {
            let _ = gas;
            Ok(())
        }

        fn contract_edition(&mut self, addr: &ContractAddress) -> Option<ContractEdition> {
            crate::VMState::wrap(self.ctx.state()).contract_edition(addr)
        }

        fn contract(&mut self, addr: &ContractAddress) -> Option<ContractSto> {
            crate::VMState::wrap(self.ctx.state()).contract(addr)
        }

        fn action_call(&mut self, kid: u16, body: Vec<u8>) -> XRet<(u32, Vec<u8>)> {
            self.ctx.action_call(kid, body)
        }

        fn log_push(&mut self, addr: &Address, items: Vec<Value>) -> VmrtErr {
            let lgdt = crate::VmLog::new(*addr, items)?;
            self.ctx.logs().push(&lgdt);
            Ok(())
        }

        fn sget(&mut self, gst: &GasExtra, cap: &SpaceCap, addr: &Address, key: &Value) -> VmrtRes<Value> {
            let _ = gst;
            crate::VMState::wrap(self.ctx.state()).sget(cap, addr, key)
        }

        fn sput(&mut self, gst: &GasExtra, cap: &SpaceCap, addr: &Address, key: Value, val: Value) -> VmrtErr {
            let _ = gst;
            crate::VMState::wrap(self.ctx.state()).sput(cap, addr, key, val)
        }

        fn sstat(&mut self, gst: &GasExtra, cap: &SpaceCap, addr: &Address, key: &Value) -> VmrtRes<Value> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).sstat(gst, cap, hei, addr, key)
        }

        fn sload(&mut self, gst: &GasExtra, cap: &SpaceCap, addr: &Address, key: &Value) -> VmrtRes<Value> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).sload(gst, cap, hei, addr, key)
        }

        fn sdel(&mut self, gst: &GasExtra, cap: &SpaceCap, addr: &Address, key: Value) -> VmrtRes<i64> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).sdel(gst, cap, hei, addr, key)
        }

        fn snew(
            &mut self,
            gst: &GasExtra,
            cap: &SpaceCap,
            addr: &Address,
            key: Value,
            val: Value,
            period: Value,
        ) -> VmrtRes<i64> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).snew(gst, cap, hei, addr, key, val, period)
        }

        fn sedit(
            &mut self,
            gst: &GasExtra,
            cap: &SpaceCap,
            addr: &Address,
            key: Value,
            val: Value,
        ) -> VmrtRes<(i64, i64)> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).sedit(gst, cap, hei, addr, key, val)
        }

        fn srent(
            &mut self,
            gst: &GasExtra,
            cap: &SpaceCap,
            addr: &Address,
            key: Value,
            period: Value,
        ) -> VmrtRes<i64> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).srent(gst, cap, hei, addr, key, period)
        }

        fn srecv(
            &mut self,
            gst: &GasExtra,
            cap: &SpaceCap,
            addr: &Address,
            key: Value,
            period: Value,
        ) -> VmrtRes<i64> {
            let hei = self.ctx.env().block.height;
            crate::VMState::wrap(self.ctx.state()).srecv(gst, cap, hei, addr, key, period)
        }
    }

    fn run_main_script_with(
        base_addr: Address,
        tx_libs: Vec<crate::ContractAddress>,
        mut ext_state: StateMem,
        main_script: &str,
        raw: bool,
    ) -> Ret<Value> {
        let main_codes = lang_to_bytecode(main_script).unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = tx_libs.iter().map(|a| a.clone().into_addr()).collect();

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(std::mem::take(&mut ext_state)), &tx);

        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();
        if raw {
            machine.main_call_raw(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())
        } else {
            let rv = machine.main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())?;
            Ok(rv)
        }
    }

    fn run_main_script(
        base_addr: Address,
        tx_libs: Vec<crate::ContractAddress>,
        ext_state: StateMem,
        main_script: &str,
    ) -> Ret<Value> {
        run_main_script_with(base_addr, tx_libs, ext_state, main_script, false)
    }

    #[test]
    fn main_call_intent_new_requires_contract_context() {
        let base_addr = test_base_addr();
        let res = run_main_script_raw(
            base_addr,
            vec![],
            StateMem::default(),
            r##"return intent_new("basic")"##,
        );
        assert_err_contains(res, "contract context");
    }

    #[test]
    fn main_call_intent_bind_depth_overflow_fails() {
        let base_addr = test_base_addr();
        let mut body = String::new();
        for _ in 0..10 {
            body.push_str("intent_use(nil)\n");
        }
        body.push_str("intent_use(nil)\nreturn 0\n");
        let err = run_main_script_with(base_addr, vec![], StateMem::default(), &body, true)
            .expect_err("intent bind depth overflow must fail");
        assert!(err.contains("IntentError"), "unexpected error: {err}");
        assert!(err.contains("intent bind depth exceeded max 10"), "unexpected error: {err}");
    }

    #[test]
    fn main_call_intent_use_rejects_non_handle() {
        let base_addr = test_base_addr();
        let res = run_main_script_raw(
            base_addr,
            vec![],
            StateMem::default(),
            "intent_use(1)\nreturn 0",
        );
        assert_err_contains(res, "requires intent handle");
    }

    fn run_main_script_raw(
        base_addr: Address,
        tx_libs: Vec<crate::ContractAddress>,
        ext_state: StateMem,
        main_script: &str,
    ) -> Ret<Value> {
        run_main_script_with(base_addr, tx_libs, ext_state, main_script, true)
    }

    fn setup_intent_owner_contracts(
        base_addr: &Address,
    ) -> (crate::ContractAddress, crate::ContractAddress, StateMem) {
        let contract_owner = test_contract(base_addr, 141);
        let contract_foreign = test_contract(base_addr, 142);
        let owner_sto = Contract::new()
            .func(
                Func::new("make")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("owned")
                        intent_use(iid)
                        intent_put("flag", 7)
                        return iid
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("read_flag")
                    .unwrap()
                    .external()
                    .fitsh(r##"return intent_require("flag")"##)
                    .unwrap(),
            )
            .func(
                Func::new("register_defer")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        param { iid }
                        defer(iid)
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let foreign_sto = Contract::new()
            .func(
                Func::new("read_flag")
                    .unwrap()
                    .external()
                    .fitsh(r##"return intent_require("flag")"##)
                    .unwrap(),
            )
            .func(
                Func::new("register_defer")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        param { iid }
                        defer(iid)
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_owner, &owner_sto);
            vmsta.contract_set_sync_edition(&contract_foreign, &foreign_sto);
        }
        (contract_owner, contract_foreign, ext_state)
    }

    #[test]
    fn intent_use_accepts_existing_foreign_handle_and_defers_owner_failure_until_access() {
        let base_addr = test_base_addr();
        let (contract_owner, contract_foreign, ext_state) = setup_intent_owner_contracts(&base_addr);

        let owner_script = r##"
            lib O = 0
            var iid = O.make()
            intent_use(iid)
            assert O.read_flag() == 7
            return 0
        "##;
        let rv = run_main_script_raw(
            base_addr.clone(),
            vec![contract_owner.clone()],
            ext_state.clone(),
            owner_script,
        )
        .unwrap();
        assert_eq!(rv.extract_u8().unwrap(), 0);
        let foreign_script = r##"
            lib O = 0
            lib F = 1
            var iid = O.make()
            intent_use(iid)
            return F.read_flag()
        "##;
        let foreign_res = run_main_script_raw(
            base_addr,
            vec![contract_owner, contract_foreign],
            ext_state,
            foreign_script,
        );
        assert_err_contains(foreign_res, "not owned by contract");
    }

    #[test]
    fn user_func_defer_without_abst_auth_is_rejected() {
        let base_addr = test_base_addr();
        let (contract_owner, _contract_foreign, ext_state) =
            setup_intent_owner_contracts(&base_addr);

        let owner_script = r##"
            lib O = 0
            var iid = O.make()
            return O.register_defer(iid)
        "##;
        let res = run_main_script_raw(
            base_addr,
            vec![contract_owner],
            ext_state,
            owner_script,
        );
        assert_err_contains(res, "defer can only be registered from Permit*/Payable* abst entries");
    }

    #[test]
    fn intent_put_flat_kv_accepts_flat_kv_list() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 166);
        let contract_sto = Contract::new()
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("kv")
                        intent_use(iid)
                        intent_put_flat_kv(list { "a" 3 "b" 5 })
                        return intent_len()
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }
        let rv = run_main_script_raw(
            base_addr,
            vec![contract],
            ext_state,
            r##"
                lib C = 0
                return C.run()
            "##,
        )
        .unwrap();
        assert_eq!(rv, Value::U64(2));
    }

    #[test]
    fn intent_keys_page_returns_u32_cursor_and_accepts_uint_family_args() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 168);
        let contract_sto = Contract::new()
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("pg")
                        intent_use(iid)
                        intent_put("a", 1)
                        intent_put("b", 2)
                        intent_put("c", 3)
                        return intent_keys_page(0 as u8, 2 as u16)
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }
        let rv = run_main_script_raw(
            base_addr,
            vec![contract],
            ext_state,
            r##"
                lib C = 0
                return C.run()
            "##,
        )
        .unwrap();
        let Value::Tuple(ret) = rv else {
            panic!("expected tuple");
        };
        let items = ret.to_vec();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], Value::U32(2));
        let Value::Compo(list) = &items[1] else {
            panic!("expected list");
        };
        assert_eq!(list.list_ref().unwrap().len(), 2);
    }

    #[test]
    fn intent_keys_after_requires_bytes_or_nil_token_and_accepts_uint_family_limit() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 169);
        let contract_sto = Contract::new()
            .func(
                Func::new("run_ok")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("pg")
                        intent_use(iid)
                        intent_put("a", 1)
                        intent_put("b", 2)
                        intent_put("c", 3)
                        return intent_keys_after("a", 2 as u8)
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("run_bad")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("pg")
                        intent_use(iid)
                        intent_put("a", 1)
                        return intent_keys_after(1, 1)
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("run_bad_empty")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("pg")
                        intent_use(iid)
                        return intent_keys_after("", 1)
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }
        let ok = run_main_script_raw(
            base_addr.clone(),
            vec![contract.clone()],
            ext_state.clone(),
            r##"
                lib C = 0
                return C.run_ok()
            "##,
        )
        .unwrap();
        let Value::Tuple(ok_ret) = ok else {
            panic!("expected tuple");
        };
        let ok_items = ok_ret.to_vec();
        assert_eq!(ok_items.len(), 2);
        assert_eq!(ok_items[0], Value::Nil);

        let bad = run_main_script_raw(
            base_addr.clone(),
            vec![contract.clone()],
            ext_state.clone(),
            r##"
                lib C = 0
                return C.run_bad()
            "##,
        );
        assert_err_contains(bad, "requires after_key bytes argument");

        let bad_empty = run_main_script_raw(
            base_addr,
            vec![contract],
            ext_state,
            r##"
                lib C = 0
                return C.run_bad_empty()
            "##,
        );
        assert!(bad_empty.is_err(), "empty-intent invalid after_key must still fail");
    }

    #[test]
    fn intent_put_flat_kv_list_tuple_form_hits_scalar_boundary_error() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 167);
        let contract_sto = Contract::new()
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("kv")
                        intent_use(iid)
                        intent_put_flat_kv(list { tuple("a", 3) tuple("b", 5) })
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }
        let res = run_main_script_raw(
            base_addr,
            vec![contract],
            ext_state,
            r##"
                lib C = 0
                return C.run()
            "##,
        );
        assert_err_contains(res, "CastBeValueFail");
    }

    fn run_p2sh_script(
        p2sh_addr: Address,
        tx_libs: Vec<crate::ContractAddress>,
        mut ext_state: StateMem,
        p2sh_script: &str,
    ) -> Ret<Value> {
        let p2sh_codes = lang_to_bytecode(p2sh_script).unwrap();
        let main_addr = test_base_addr();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main_addr;
        env.tx.addrs = tx_libs.iter().map(|a| a.clone().into_addr()).collect();

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(main_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(std::mem::take(&mut ext_state)), &tx);

        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();
        let rv = machine.p2sh_call(
            &mut runtime,
            &mut host,
            CodeType::Bytecode,
            p2sh_addr,
            tx_libs,
            p2sh_codes.into(),
            None,
            Value::Nil,
        )?;
        Ok(rv)
    }

    fn assert_err_contains(res: Ret<Value>, needle: &str) {
        let err = res.expect_err("expected error");
        assert!(
            err.contains(needle),
            "expected error to contain '{needle}', got '{err}'"
        );
    }

    #[test]
    fn main_call_raw_accepts_object_return() {
        let base_addr = test_base_addr();
        let rv = run_main_script_raw(
            base_addr,
            vec![],
            StateMem::default(),
            r##"
                return map { "kind": "hnft", "mint": 1 }
            "##,
        )
        .unwrap();
        assert_eq!(rv.to_json(), r#"{"kind":"hnft","mint":1}"#);
    }

    #[test]
    fn main_call_still_rejects_object_return() {
        let base_addr = test_base_addr();
        let res = run_main_script(
            base_addr,
            vec![],
            StateMem::default(),
            r##"
                return map { "kind": "hnft" }
            "##,
        );
        assert_err_contains(res, "main call return error");
    }

    #[test]
    fn main_call_still_rejects_tuple_return() {
        let base_addr = test_base_addr();
        let res = run_main_script(
            base_addr,
            vec![],
            StateMem::default(),
            r##"
                return tuple(7, 8)
            "##,
        );
        assert_err_contains(res, "main call return error");
    }

    #[test]
    fn sandbox_call_displays_object_return() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 31);
        let contract_sto = Contract::new()
            .func(
                Func::new("probe")
                    .unwrap()
                    .external()
                    .fitsh(r##"return map { "kind": "hnft", "mint": 1 }"##)
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        protocol::operate::hac_add(&mut ctx, &base_addr, &Amount::unit238(1_000_000_000)).unwrap();

        let callres = sandbox_call(&mut ctx, SandboxSpec::new(contract, "probe")).unwrap();
        assert_eq!(callres.ret_val.to_debug_json(), r#"{"kind":"hnft","mint":1}"#);
    }

    #[test]
    fn calltargets_resolve_under_ext_view_and_inherits() {
        // Arrange addresses.
        let base_addr = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let contract_child = crate::ContractAddress::calculate(&base_addr, &Uint4::from(1));
        let contract_parent = crate::ContractAddress::calculate(&base_addr, &Uint4::from(2));
        let contract_base = crate::ContractAddress::calculate(&base_addr, &Uint4::from(3));

        // Build an inheritance chain: Child -> Parent -> Base. The key trick is: `super.f()` moves code_contract to Parent, while this_contract stays Child. Then inside Parent.f(), `this.g()` must resolve in this_contract (Child), `self.g()` in code_contract (Parent), and `super.g()` in Parent's direct base (Base).

        let base = Contract::new().func(Func::new("g").unwrap().fitsh("return 3").unwrap());

        let parent = Contract::new()
            .inh(contract_base.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 2").unwrap())
            .func(
                Func::new("f")
                    .unwrap()
                    .fitsh(
                        r##"
                        return this.g() * 10000 + self.g() * 100 + super.g()
                        "##,
                    )
                    .unwrap(),
            );

        let child = Contract::new()
            .inh(contract_parent.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 1").unwrap())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        let v = super.f()
                        assert v == 10203
                        return 0
                        "##,
                    )
                    .unwrap(),
            );

        // Put contracts into state, then move it into Context.
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_base, &base.into_sto());
            vmsta.contract_set_sync_edition(&contract_parent, &parent.into_sto());
            vmsta.contract_set_sync_edition(&contract_child, &child.into_sto());
        }

        // Main script calls contract_main.run() using tx-provided libs (index 0).
        let main_script = r##"
            lib C = 0
            return C.run()
        "##;
        let main_codes = lang_to_bytecode(main_script).unwrap();

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = vec![contract_child.into_addr()];

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);

        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);

        let (mut machine, mut runtime) = new_test_machine();
        let rv = machine
            .main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())
            .unwrap();

        assert!(
            !rv.extract_bool().unwrap(),
            "main call should return success (nil/0)"
        );
    }

    #[test]
    fn callupper_uses_code_owner_then_parent_chain() {
        let base_addr = test_base_addr();
        let contract_child = test_contract(&base_addr, 4);
        let contract_parent = test_contract(&base_addr, 5);
        let contract_base = test_contract(&base_addr, 6);

        let base = Contract::new().func(Func::new("g").unwrap().fitsh("return 3").unwrap());
        let parent = Contract::new()
            .inh(contract_base.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 2").unwrap())
            .func(
                Func::new("f")
                    .unwrap()
                    .fitsh(
                        r##"
                        return this.g() * 10000 + self.g() * 100 + call edit upper.g() * 10 + super.g()
                        "##,
                    )
                    .unwrap(),
            );
        let child = Contract::new()
            .inh(contract_parent.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 1").unwrap())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        assert super.f() == 10223
                        return 0
                        "##,
                    )
                    .unwrap(),
            );

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_base, &base.into_sto());
            vmsta.contract_set_sync_edition(&contract_parent, &parent.into_sto());
            vmsta.contract_set_sync_edition(&contract_child, &child.into_sto());
        }

        let script = r##"
            lib C = 0
            return C.run()
        "##;
        let res = run_main_script(base_addr, vec![contract_child], ext_state, script);
        assert!(
            res.is_ok(),
            "upper should resolve on code_owner first, then parent chain: {res:?}"
        );
    }

    #[test]
    fn callupper_falls_back_to_parent_when_code_owner_lacks_match() {
        let base_addr = test_base_addr();
        let contract_child = test_contract(&base_addr, 7);
        let contract_parent = test_contract(&base_addr, 8);
        let contract_base = test_contract(&base_addr, 9);

        let base = Contract::new().func(Func::new("h").unwrap().fitsh("return 7").unwrap());
        let parent = Contract::new()
            .inh(contract_base.to_addr())
            .func(
                Func::new("f")
                    .unwrap()
                    .fitsh(
                        r##"
                        return call edit upper.h()
                        "##,
                    )
                    .unwrap(),
            );
        let child = Contract::new()
            .inh(contract_parent.to_addr())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        assert super.f() == 7
                        return 0
                        "##,
                    )
                    .unwrap(),
            );

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_base, &base.into_sto());
            vmsta.contract_set_sync_edition(&contract_parent, &parent.into_sto());
            vmsta.contract_set_sync_edition(&contract_child, &child.into_sto());
        }

        let script = r##"
            lib C = 0
            return C.run()
        "##;
        let res = run_main_script(base_addr, vec![contract_child], ext_state, script);
        assert!(
            res.is_ok(),
            "upper should fall back into the code_owner parent chain when owner lacks the selector: {res:?}"
        );
    }

    #[test]
    fn codecall_upper_uses_code_owner_chain_not_this_contract() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 10);
        let contract_code = test_contract(&base_addr, 11);
        let contract_base = test_contract(&base_addr, 12);

        let base_sto = Contract::new()
            .func(Func::new("g").unwrap().fitsh("return 3").unwrap())
            .into_sto();
        let code_sto = Contract::new()
            .inh(contract_base.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 2").unwrap())
            .func(
                Func::new("probe_upper")
                    .unwrap()
                    .fitsh(
                        r##"
                        return this.g() * 10000 + self.g() * 100 + call edit upper.g() * 10 + super.g()
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_code.to_addr())
            .func(Func::new("g").unwrap().fitsh("return 1").unwrap())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Code = 0
                        codecall Code.probe_upper
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_base, &base_sto);
            vmsta.contract_set_sync_edition(&contract_code, &code_sto);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
        }

        let res = run_main_script(
            base_addr,
            vec![contract_entry],
            ext_state,
            r##"
                lib C = 0
                assert C.run() == 10223
                return 0
            "##,
        );
        assert!(
            res.is_ok(),
            "codecall upper should use the code_owner chain, not this_contract inheritance: {res:?}"
        );
    }

    #[test]
    fn internal_contract_call_accepts_tuple_return_value() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 88);
        let contract_sto = Contract::new()
            .func(
                Func::new("build")
                    .unwrap()
                    .types(Some(VT::Tuple), vec![])
                    .fitsh(r##"return tuple(7, map { "kind": "hnft" })"##)
                    .unwrap(),
            )
            .func(
                Func::new("consume")
                    .unwrap()
                    .types(Some(VT::U8), vec![VT::U8, VT::Compo])
                    .fitsh(
                        r##"
                        param { num doc }
                        assert doc is map
                        return num
                    "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("relay")
                    .unwrap()
                    .external()
                    .types(Some(VT::U8), vec![])
                    .fitsh(r##"return this.consume(this.build())"##)
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }
        let rv = run_main_script_raw(
            base_addr,
            vec![contract],
            ext_state,
            r##"
                lib C = 0
                return C.relay()
            "##,
        )
        .unwrap();
        assert_eq!(rv, Value::U8(7));
    }

    #[test]
    fn call_external_view_pure_use_inherits_but_codecall_keeps_local_lookup() {
        let base_addr = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let contract_child = crate::ContractAddress::calculate(&base_addr, &Uint4::from(11));
        let contract_parent = crate::ContractAddress::calculate(&base_addr, &Uint4::from(12));

        let parent_sto = Contract::new()
            .func(Func::new("id").unwrap().fitsh("return 2").unwrap())
            .func(
                Func::new("probe")
                    .unwrap()
                    .external()
                    .fitsh("return 201")
                    .unwrap(),
            )
            .into_sto();
        let child_sto = Contract::new()
            .inh(contract_parent.to_addr())
            .func(Func::new("id").unwrap().fitsh("return 1").unwrap())
            .func(
                Func::new("noop")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();

        let run_main = |main_script: &str| -> Ret<Value> {
            let main_codes = lang_to_bytecode(main_script).unwrap();
            let mut ext_state = StateMem::default();
            {
                let mut vmsta = crate::VMState::wrap(&mut ext_state);
                vmsta.contract_set_sync_edition(&contract_parent, &parent_sto.clone());
                vmsta.contract_set_sync_edition(&contract_child, &child_sto.clone());
            }

            let mut env = Env::default();
            env.block.height = 1;
            env.tx.main = base_addr;
            env.tx.addrs = vec![contract_child.clone().into_addr()];

            let tx = StubTxBuilder::new()
                .ty(0)
                .main(base_addr)
                .addrs(env.tx.addrs.clone())
                .fee(Amount::zero())
                .gas_max(1)
                .tx_size(128)
                .fee_purity(1)
                .build();
            let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);

            let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
            let (mut machine, mut runtime) = new_test_machine();
            let rv = machine.main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())?;
            Ok(rv)
        };

        // CALLEXT (External): should resolve inherited `probe` on parent.
        let external_script = r##"
            lib C = 0
            let v = C.probe()
            assert v == 201
            return 0
        "##;
        assert!(
            run_main(external_script).is_ok(),
            "CALLEXT should resolve through inherit chain"
        );

        // CALLEXTVIEW and ext-pure generic calls should also resolve the inherit chain; CODECALL stays local-only.
        let view_script = r##"
            lib C = 0
            assert C:probe() == 201
            return 0
        "##;
        assert!(
            run_main(view_script).is_ok(),
            "CALLEXTVIEW should resolve through inherit chain"
        );

        let pure_script = r##"
            lib C = 0
            assert call pure C.probe() == 201
            return 0
        "##;
        assert!(
            run_main(pure_script).is_ok(),
            "generic Ext(lib)+Pure should resolve through inherit chain"
        );

        let codecall_script = r##"
            lib C = 0
            codecall C.probe
        "##;
        assert!(
            run_main(codecall_script).is_err(),
            "CODECALL should not resolve inherit chain"
        );
    }

    #[test]
    fn call_external_requires_external_visibility() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 21);

        let target_sto = Contract::new()
            .func(Func::new("hidden").unwrap().fitsh("return 1").unwrap())
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let script = r##"
            lib C = 0
            return C.hidden()
        "##;
        let res = run_main_script(base_addr, vec![contract_target], ext_state, script);
        assert_err_contains(res, "CallNotExternal");
    }

    #[test]
    fn call_libidx_overflow_is_reported() {
        let base_addr = test_base_addr();
        let script = r##"
            lib C = 0
            return C.anything()
        "##;
        let res = run_main_script(base_addr, vec![], StateMem::default(), script);
        assert_err_contains(res, "CallLibIdxOverflow");
    }

    #[test]
    fn callthis_callself_callsuper_are_forbidden_in_main_mode() {
        let base_addr = test_base_addr();
        let scripts = [
            "return this.nope()",
            "return self.nope()",
            "return super.nope()",
        ];
        for sc in scripts {
            let res = run_main_script(base_addr, vec![], StateMem::default(), sc);
            assert_err_contains(res, "CallOtherInMain");
        }
    }

    #[test]
    fn abst_this_and_self_follow_this_contract_and_code_contract() {
        let base_addr = test_base_addr();
        let contract_child = test_contract(&base_addr, 28);
        let contract_parent = test_contract(&base_addr, 29);

        let parent_sto = Contract::new()
            .syst(
                Abst::new(AbstCall::Construct)
                    .fitsh(
                        r##"
                        let v = this.id() * 100 + self.id()
                        assert v == 102
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(Func::new("id").unwrap().fitsh("return 2").unwrap())
            .into_sto();
        let child_sto = Contract::new()
            .inh(contract_parent.to_addr())
            .func(Func::new("id").unwrap().fitsh("return 1").unwrap())
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_parent, &parent_sto);
            vmsta.contract_set_sync_edition(&contract_child, &child_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);

        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();
        let rv = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Construct,
                contract_child,
                None,
                Value::Bytes(vec![]),
            )
            .unwrap();
        assert!(
            !rv.extract_bool().unwrap(),
            "abst call should finish without assertion failure"
        );
    }

    #[test]
    fn abst_call_first_cold_load_costs_more_than_second_warm_call() {
        let base_addr = test_base_addr();
        let contract_child = test_contract(&base_addr, 31);
        let contract_parent = test_contract(&base_addr, 32);

        let parent_sto = Contract::new()
            .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
            .into_sto();
        let child_sto = Contract::new().inh(contract_parent.to_addr()).into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_parent, &parent_sto);
            vmsta.contract_set_sync_edition(&contract_child, &child_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let (mut machine, mut runtime) = new_test_machine();
        let gas_budget = 1_000_000i64;
        let mut host_1 = TestVmHost::new(&mut ctx as &mut dyn Context, gas_budget);
        machine
            .abst_call(
                &mut runtime,
                &mut host_1,
                AbstCall::Construct,
                contract_child.clone(),
                None,
                Value::Bytes(vec![]),
            )
            .unwrap();
        let used_1 = gas_budget - host_1.gas_remaining;

        let mut host_2 = TestVmHost::new(&mut ctx as &mut dyn Context, gas_budget);
        machine
            .abst_call(
                &mut runtime,
                &mut host_2,
                AbstCall::Construct,
                contract_child,
                None,
                Value::Bytes(vec![]),
            )
            .unwrap();
        let used_2 = gas_budget - host_2.gas_remaining;

        assert!(
            used_1 > used_2,
            "first cold abst_call should consume more gas than second warm call"
        );
    }

    #[test]
    fn abst_external_call_fails_before_loading_lib_target() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 33);
        let contract_target = test_contract(&base_addr, 34);

        let target_sto = Contract::new()
            .func(
                Func::new("probe")
                    .unwrap()
                    .external()
                    .fitsh("return 7")
                    .unwrap(),
            )
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .syst(
                Abst::new(AbstCall::Construct)
                    .fitsh(
                        r##"
                        lib T = 0
                        return T.probe()
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let err = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Construct,
                contract_entry.clone(),
                None,
                Value::Bytes(vec![]),
            )
            .expect_err("abst external call must be rejected before target load");
        assert!(err.contains("CallInAbst"), "unexpected error: {err}");
        assert!(runtime.warm.contracts.contains_key(&contract_entry));
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn abst_defer_registration_follows_abst_type_boundary() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 35);
        let contract_sto = Contract::new()
            .syst(
                Abst::new(AbstCall::Append)
                    .fitsh("defer(nil)\nreturn 0")
                    .unwrap(),
            )
            .syst(
                Abst::new(AbstCall::PayableHAC)
                    .fitsh(
                        r##"
                        param { from, hac }
                        defer(nil)
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .syst(
                Abst::new(AbstCall::Deferred)
                    .fitsh("defer(nil)\nreturn 0")
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let append_err = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Append,
                contract.clone(),
                None,
                Value::Nil,
            )
            .expect_err("Append must not be able to register defer");
        assert!(
            append_err.contains("defer can only be registered from Permit*/Payable* abst entries"),
            "unexpected error: {append_err}"
        );
        assert!(runtime.volatile.deferred_registry.drain_lifo().is_empty());

        let param = Value::pack_call_args(vec![Value::Address(base_addr), Value::Bytes(vec![])]).unwrap();
        let rv = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::PayableHAC,
                contract.clone(),
                None,
                param,
            )
            .unwrap();
        assert_eq!(rv.extract_u8().unwrap(), 0);
        assert_eq!(
            runtime.volatile.deferred_registry.drain_lifo(),
            vec![DeferredEntry {
                addr: contract.clone(),
                intent_scope: Some(None),
            }]
        );

        let deferred_err = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Deferred,
                contract.clone(),
                None,
                Value::Nil,
            )
            .expect_err("Deferred must not be able to register another defer");
        assert!(
            deferred_err.contains("defer can only be registered from Permit*/Payable* abst entries"),
            "unexpected error: {deferred_err}"
        );
        assert!(runtime.volatile.deferred_registry.drain_lifo().is_empty());
    }

    #[test]
    fn callextview_and_ext_pure_enforce_mode_call_whitelist() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 22);
        let target_sto = Contract::new()
            .func(
                Func::new("bad_view")
                    .unwrap()
                    .external()
                    .fitsh("return this.nope()")
                    .unwrap(),
            )
            .func(
                Func::new("bad_pure")
                    .unwrap()
                    .external()
                    .fitsh("return this.nope()")
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let view_script = r##"
            lib C = 0
            return C:bad_view()
        "##;
        let pure_script = r##"
            lib C = 0
            return C::bad_pure()
        "##;

        let view_res = run_main_script(
            base_addr.clone(),
            vec![contract_target.clone()],
            ext_state.clone(),
            view_script,
        );
        assert_err_contains(view_res, "CallLocInView");

        let pure_res = run_main_script(base_addr, vec![contract_target], ext_state, pure_script);
        assert_err_contains(pure_res, "CallInPure");
    }

    #[test]
    fn codecall_cannot_reenable_action_from_nested_frame() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 221);
        let target_sto = Contract::new()
            .func(
                Func::new("bad_act")
                    .unwrap()
                    .fitsh(
                        r##"
                        transfer_hac_to(1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9, 1)
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let script = r##"
            lib C = 0
            codecall C.bad_act
        "##;
        let res = run_main_script(base_addr, vec![contract_target], ext_state, script);
        assert_err_contains(res, "ActDisabled");
    }

    #[test]
    fn p2sh_codecall_cannot_reenable_nested_edit_call() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 231);
        let contract_target = test_contract(&base_addr, 232);

        let target_sto = Contract::new()
            .func(
                Func::new("id")
                    .unwrap()
                    .external()
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("jump_ext")
                    .unwrap()
                    .fitsh(
                        r##"
                        lib Dep = 0
                        assert Dep.id() == 0
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
        }

        let script = r##"
            lib C = 0
            codecall C.jump_ext
        "##;
        let res = run_p2sh_script(
            Address::create_scriptmh([23u8; 20]),
            vec![contract_entry],
            ext_state,
            script,
        );
        assert_err_contains(res, "CallOtherInP2sh");
    }

    #[test]
    fn p2sh_codecall_still_allows_nested_view_and_pure_calls() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 233);
        let contract_target = test_contract(&base_addr, 234);

        let target_sto = Contract::new()
            .func(Func::new("view_id").unwrap().fitsh("return 7").unwrap())
            .func(Func::new("pure_id").unwrap().fitsh("return 8").unwrap())
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("jump_readonly")
                    .unwrap()
                    .fitsh(
                        r##"
                        lib Dep = 0
                        assert Dep:view_id() == 7
                        assert Dep::pure_id() == 8
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
        }

        let script = r##"
            lib C = 0
            codecall C.jump_readonly
        "##;
        let res = run_p2sh_script(
            Address::create_scriptmh([24u8; 20]),
            vec![contract_entry],
            ext_state,
            script,
        );
        assert!(
            res.is_ok(),
            "P2SH codecall should still allow nested view/pure calls: {res:?}"
        );
    }

    #[test]
    fn codecall_inherits_current_intent_state() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 168);
        let target_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("read_flag")
                    .unwrap()
                    .fitsh(r##"return intent_require("flag")"##)
                    .unwrap(),
            )
            .func(
                Func::new("jump_read_flag")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib C = 0
                        codecall C.read_flag
                        end
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("seed_and_jump")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        var iid = intent_new("owned")
                        intent_use(iid)
                        intent_put("flag", 7)
                        return this.jump_read_flag()
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }
        let rv = run_main_script_raw(
            base_addr,
            vec![contract_target],
            ext_state,
            r##"
                lib C = 0
                return C.seed_and_jump()
            "##,
        )
        .unwrap();
        assert_eq!(rv, Value::U8(7));
    }

    #[test]
    fn codecall_uses_explicit_argv_and_allows_nested_calls() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 23);
        let target_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("need_arg")
                    .unwrap()
                    .types(Some(VT::U8), vec![VT::U8])
                    .fitsh(
                        "param { x }
return x
end",
                    )
                    .unwrap(),
            )
            .func(Func::new("leaf").unwrap().fitsh("return 7").unwrap())
            .func(
                Func::new("nested")
                    .unwrap()
                    .fitsh("return this.leaf()")
                    .unwrap(),
            )
            .func(
                Func::new("jump_need_arg")
                    .unwrap()
                    .external()
                    .types(Some(VT::U8), vec![])
                    .fitsh(
                        r##"
                        lib C = 0
                        codecall C.need_arg(9)
                        end
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("jump_nested")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib C = 0
                        codecall C.nested
                        end
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let arg_script = r##"
            lib C = 0
            assert C.jump_need_arg() == 9
            return 0
        "##;
        let nested_script = r##"
            lib C = 0
            assert C.jump_nested() == 7
            return 0
        "##;

        let arg_res = run_main_script(
            base_addr.clone(),
            vec![contract_target.clone()],
            ext_state.clone(),
            arg_script,
        );
        assert!(
            arg_res.is_ok(),
            "codecall should use explicit argv expression: {arg_res:?}"
        );

        let nested_res =
            run_main_script(base_addr, vec![contract_target], ext_state, nested_script);
        assert!(
            nested_res.is_ok(),
            "codecall should allow nested calls: {nested_res:?}"
        );
    }

    #[test]
    fn codecall_without_caller_ret_contract_ignores_callee_ret_contract() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 33);
        let target_sto = Contract::new()
            .func(
                Func::new("ret_mismatch")
                    .unwrap()
                    .types(Some(VT::Address), vec![])
                    .fitsh("return 0")
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let script = r##"
            lib C = 0
            codecall C.ret_mismatch
        "##;
        let rv = run_main_script(base_addr, vec![contract_target], ext_state, script)
            .expect("codecall should follow caller(no contract) return policy");
        assert_eq!(rv, Value::U8(0));
    }

    #[test]
    fn nested_codecall_preserves_outer_caller_return_contract() {
        let base_addr = test_base_addr();
        let contract_outer = test_contract(&base_addr, 34);
        let contract_middle = test_contract(&base_addr, 35);
        let contract_leaf = test_contract(&base_addr, 36);

        let leaf_sto = Contract::new()
            .func(
                Func::new("leaf")
                    .unwrap()
                    .types(Some(VT::U8), vec![])
                    .fitsh("return 7")
                    .unwrap(),
            )
            .into_sto();
        let middle_sto = Contract::new()
            .lib(contract_leaf.to_addr())
            .func(
                Func::new("middle")
                    .unwrap()
                    .types(Some(VT::Bool), vec![])
                    .fitsh(
                        r##"
                        lib L = 0
                        codecall L.leaf
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let outer_sto = Contract::new()
            .lib(contract_middle.to_addr())
            .func(
                Func::new("outer")
                    .unwrap()
                    .external()
                    .types(Some(VT::U8), vec![])
                    .fitsh(
                        r##"
                        lib M = 0
                        codecall M.middle
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_leaf, &leaf_sto);
            vmsta.contract_set_sync_edition(&contract_middle, &middle_sto);
            vmsta.contract_set_sync_edition(&contract_outer, &outer_sto);
        }

        let script = r##"
            lib O = 0
            assert O.outer() == 7
            return 0
        "##;
        let res = run_main_script(base_addr, vec![contract_outer], ext_state, script);
        assert!(
            res.is_ok(),
            "nested codecall must keep outer caller return contract: {res:?}"
        );
    }

    #[test]
    fn tail_call_revert_uses_frozen_caller_return_contract() {
        let base_addr = test_base_addr();
        let contract_outer = test_contract(&base_addr, 37);
        let contract_middle = test_contract(&base_addr, 38);
        let contract_leaf = test_contract(&base_addr, 39);

        let leaf_sto = Contract::new()
            .func(
                Func::new("leaf")
                    .unwrap()
                    .external()
                    .types(Some(VT::U8), vec![])
                    .fitsh("return 7")
                    .unwrap(),
            )
            .into_sto();
        let mut middle_codes = vec![crate::rt::Bytecode::CALLEXT as u8, 0];
        middle_codes.extend_from_slice(&crate::rt::calc_func_sign("leaf"));
        let middle_sto = Contract::new()
            .lib(contract_leaf.to_addr())
            .func(
                Func::new("middle")
                    .unwrap()
                    .types(Some(VT::Bool), vec![])
                    .bytecode(middle_codes)
                    .unwrap(),
            )
            .into_sto();
        let outer_sto = Contract::new()
            .lib(contract_middle.to_addr())
            .func(
                Func::new("outer")
                    .unwrap()
                    .external()
                    .types(Some(VT::U8), vec![])
                    .fitsh(
                        r##"
                        lib M = 0
                        codecall M.middle
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_leaf, &leaf_sto);
            vmsta.contract_set_sync_edition(&contract_middle, &middle_sto);
            vmsta.contract_set_sync_edition(&contract_outer, &outer_sto);
        }

        let script = r##"
            lib O = 0
            assert O.outer() == 7
            return 0
        "##;
        let res = run_main_script(base_addr, vec![contract_outer], ext_state, script);
        assert!(
            res.is_ok(),
            "tail revert must keep frozen caller return contract: {res:?}"
        );
    }

    #[test]
    fn irnode_tail_invoke_propagates_entry_failure_code() {
        let base_addr = test_base_addr();
        let contract_addr = test_contract(&base_addr, 40);

        let mut tail_call = vec![crate::rt::Bytecode::PNIL as u8, crate::rt::Bytecode::CALLSELF as u8];
        tail_call.extend_from_slice(&crate::rt::calc_func_sign("fail"));
        let mut run_ir = vec![crate::rt::Bytecode::IRBYTECODE as u8];
        run_ir.extend_from_slice(&(tail_call.len() as u16).to_be_bytes());
        run_ir.extend_from_slice(&tail_call);

        let contract_sto = Contract::new()
            .func(Func::new("fail").unwrap().fitsh("return 7").unwrap())
            .func(Func::new("run").unwrap().external().ircode(run_ir).unwrap())
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_addr, &contract_sto);
        }

        let script = r##"
            lib C = 0
            return C.run()
        "##;
        assert_err_contains(
            run_main_script(base_addr, vec![contract_addr], ext_state, script),
            "main call return error code 7",
        );
    }

    #[test]
    fn callsuper_uses_direct_parent_order() {
        let base_addr = test_base_addr();
        let contract_a = test_contract(&base_addr, 24);
        let contract_b = test_contract(&base_addr, 25);
        let contract_child = test_contract(&base_addr, 26);

        let a_sto = Contract::new()
            .func(Func::new("f").unwrap().fitsh("return 10").unwrap())
            .into_sto();
        let b_sto = Contract::new()
            .func(Func::new("f").unwrap().fitsh("return 20").unwrap())
            .into_sto();
        let child_sto = Contract::new()
            .inh(contract_a.to_addr())
            .inh(contract_b.to_addr())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        assert super.f() == 10
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_a, &a_sto);
            vmsta.contract_set_sync_edition(&contract_b, &b_sto);
            vmsta.contract_set_sync_edition(&contract_child, &child_sto);
        }

        let script = r##"
            lib C = 0
            return C.run()
        "##;
        let res = run_main_script(base_addr, vec![contract_child], ext_state, script);
        assert!(
            res.is_ok(),
            "super should choose first direct parent in inherit order"
        );
    }

    #[test]
    fn callsuper_never_resolves_back_to_current_owner() {
        let base_addr = test_base_addr();
        let contract_parent = test_contract(&base_addr, 41);
        let contract_child = test_contract(&base_addr, 42);

        let parent_sto = Contract::new().inh(contract_child.to_addr()).into_sto();
        let child_sto = Contract::new()
            .inh(contract_parent.to_addr())
            .func(Func::new("f").unwrap().fitsh("return 7").unwrap())
            .func(
                Func::new("run")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        let _ = super.f()
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_parent, &parent_sto);
            vmsta.contract_set_sync_edition(&contract_child, &child_sto);
        }

        let script = r##"
            lib C = 0
            return C.run()
        "##;
        let res = run_main_script(base_addr, vec![contract_child], ext_state, script);
        assert_err_contains(res, "CallNotExist");
    }

    #[test]
    fn callextview_callusepure_and_codecall_local_lookup_positive_paths() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 27);
        let target_sto = Contract::new()
            .func(Func::new("view_ok").unwrap().fitsh("return 7").unwrap())
            .func(Func::new("pure_ok").unwrap().fitsh("return 8").unwrap())
            .func(Func::new("code_ok").unwrap().fitsh("return 0").unwrap())
            .func(
                Func::new("self_ok")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        assert self:view_ok() == 7
                        assert self::pure_ok() == 8
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let script = r##"
            lib C = 0
            assert C:view_ok() == 7
            assert C::pure_ok() == 8
            assert C.self_ok() == 0
            codecall C.code_ok
            end
        "##;
        let res = run_main_script(base_addr, vec![contract_target], ext_state, script);
        assert!(
            res.is_ok(),
            "local lookup should succeed for ext-view/use-pure/codecall/self shortcuts"
        );
    }

    #[test]
    fn newframe_calls_rebind_current_libctx_for_callee_lib_calls() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 71);
        let contract_entry_lib = test_contract(&base_addr, 72);
        let contract_code_lib = test_contract(&base_addr, 73);

        let entry_lib_sto = Contract::new()
            .func(
                Func::new("id")
                    .unwrap()
                    .external()
                    .fitsh("return 20")
                    .unwrap(),
            )
            .func(Func::new("view_id").unwrap().fitsh("return 21").unwrap())
            .func(Func::new("pure_id").unwrap().fitsh("return 22").unwrap())
            .into_sto();
        let code_lib_sto = Contract::new()
            .func(
                Func::new("id")
                    .unwrap()
                    .external()
                    .fitsh("return 30")
                    .unwrap(),
            )
            .func(Func::new("view_id").unwrap().fitsh("return 31").unwrap())
            .func(Func::new("pure_id").unwrap().fitsh("return 32").unwrap())
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_entry_lib.to_addr())
            .lib(contract_code_lib.to_addr())
            .func(
                Func::new("ext_probe")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep.id() == 30
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("view_probe")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep:view_id() == 31
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("pure_probe")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep::pure_id() == 32
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry_lib, &entry_lib_sto);
            vmsta.contract_set_sync_edition(&contract_code_lib, &code_lib_sto);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
        }

        let script = r##"
            lib C = 0
            assert C.ext_probe() == 0
            assert C:view_probe() == 0
            assert C::pure_probe() == 0
            return 0
        "##;
        let res = run_main_script(
            base_addr,
            vec![contract_entry, contract_entry_lib],
            ext_state,
            script,
        );
        assert!(
            res.is_ok(),
            "new-frame calls should resolve callee lib lookups on code_contract: {res:?}"
        );
    }

    #[test]
    fn codecall_rebinds_callee_libs_for_nested_calls() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 81);
        let contract_entry_lib = test_contract(&base_addr, 82);
        let contract_code_lib = test_contract(&base_addr, 83);

        let entry_lib_sto = Contract::new()
            .func(
                Func::new("id")
                    .unwrap()
                    .external()
                    .fitsh("return 20")
                    .unwrap(),
            )
            .func(Func::new("view_id").unwrap().fitsh("return 21").unwrap())
            .func(Func::new("pure_id").unwrap().fitsh("return 22").unwrap())
            .into_sto();
        let code_lib_sto = Contract::new()
            .func(
                Func::new("id")
                    .unwrap()
                    .external()
                    .fitsh("return 30")
                    .unwrap(),
            )
            .func(Func::new("view_id").unwrap().fitsh("return 31").unwrap())
            .func(Func::new("pure_id").unwrap().fitsh("return 32").unwrap())
            .func(Func::new("code_ok").unwrap().fitsh("return 0").unwrap())
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_entry_lib.to_addr())
            .lib(contract_code_lib.to_addr())
            .func(
                Func::new("jump_ext")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep.id() == 30
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("jump_view")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep:view_id() == 31
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("jump_pure")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        assert Dep::pure_id() == 32
                        return 0
                        "##,
                    )
                    .unwrap(),
            )
            .func(
                Func::new("jump_code")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib Dep = 1
                        codecall Dep.code_ok
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry_lib, &entry_lib_sto);
            vmsta.contract_set_sync_edition(&contract_code_lib, &code_lib_sto);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
        }

        let tx_libs = vec![contract_entry.clone(), contract_entry_lib.clone()];
        let run_codecall = |func: &str| -> Ret<Value> {
            let script = format!(
                r##"
                lib C = 0
                codecall C.{func}
                "##,
            );
            run_main_script(
                base_addr.clone(),
                tx_libs.clone(),
                ext_state.clone(),
                &script,
            )
        };
        for func in ["jump_ext", "jump_view", "jump_pure", "jump_code"] {
            let res = run_codecall(func);
            assert!(
                res.is_ok(),
                "codecall should rebind callee libs for {func}: {res:?}"
            );
        }
    }

    #[test]
    fn call_depth_overflow_fails_before_loading_target_contract() {
        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 84);
        let contract_target = test_contract(&base_addr, 85);

        let target_sto = Contract::new()
            .func(
                Func::new("probe")
                    .unwrap()
                    .external()
                    .fitsh("return 7")
                    .unwrap(),
            )
            .into_sto();
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("deep")
                    .unwrap()
                    .external()
                    .fitsh(
                        r##"
                        lib T = 0
                        return T.probe()
                        "##,
                    )
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let main_codes = lang_to_bytecode(
            r##"
            lib E = 0
            return E.deep()
            "##,
        )
        .unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = vec![contract_entry.clone().into_addr()];

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();
        runtime.warm.space_cap.call_depth = 2;

        let err = machine
            .main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())
            .expect_err("nested call must exceed call_depth limit");
        assert!(err.contains("OutOfCallDepth"), "unexpected error: {err}");
        assert!(runtime.warm.contracts.contains_key(&contract_entry));
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn user_call_missing_argv_fails_before_loading_target_contract() {
        use crate::rt::Bytecode;

        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 86);
        let contract_target = test_contract(&base_addr, 87);

        let target_sto = Contract::new()
            .func(
                Func::new("probe")
                    .unwrap()
                    .external()
                    .fitsh("return 7")
                    .unwrap(),
            )
            .into_sto();
        let mut entry_codes = vec![Bytecode::POP as u8, Bytecode::CALLEXT as u8, 0];
        entry_codes.extend_from_slice(&crate::rt::calc_func_sign("probe"));
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("deep")
                    .unwrap()
                    .external()
                    .bytecode(entry_codes)
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let main_codes = lang_to_bytecode(
            r##"
            lib E = 0
            return E.deep()
            "##,
        )
        .unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = vec![contract_entry.clone().into_addr()];

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let err = machine
            .main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())
            .expect_err("nested call without argv must fail locally");
        assert!(err.contains("Read empty stack"), "unexpected error: {err}");
        assert!(runtime.warm.contracts.contains_key(&contract_entry));
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn codecall_missing_argv_fails_before_loading_target_contract() {
        use crate::rt::Bytecode;

        let base_addr = test_base_addr();
        let contract_entry = test_contract(&base_addr, 89);
        let contract_target = test_contract(&base_addr, 90);

        let target_sto = Contract::new()
            .func(Func::new("probe").unwrap().fitsh("return 7").unwrap())
            .into_sto();
        let mut entry_codes = vec![Bytecode::POP as u8, Bytecode::CODECALL as u8, 0];
        entry_codes.extend_from_slice(&crate::rt::calc_func_sign("probe"));
        let entry_sto = Contract::new()
            .lib(contract_target.to_addr())
            .func(
                Func::new("deep")
                    .unwrap()
                    .external()
                    .bytecode(entry_codes)
                    .unwrap(),
            )
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_entry, &entry_sto);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let main_codes = lang_to_bytecode(
            r##"
            lib E = 0
            return E.deep()
            "##,
        )
        .unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = vec![contract_entry.clone().into_addr()];

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .addrs(env.tx.addrs.clone())
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let err = machine
            .main_call(&mut runtime, &mut host, CodeType::Bytecode, main_codes.into())
            .expect_err("codecall without argv must fail locally");
        assert!(err.contains("Read empty stack"), "unexpected error: {err}");
        assert!(runtime.warm.contracts.contains_key(&contract_entry));
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn abst_call_invalid_param_fails_before_loading_target_contract() {
        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 88);

        let target_sto = Contract::new()
            .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let err = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Construct,
                contract_target.clone(),
                None,
                Value::handle(0u32),
            )
            .expect_err("invalid abst argv must fail locally");
        assert!(err.contains("CastBeFnArgvFail"), "unexpected error: {err}");
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn abst_call_oversize_compo_param_fails_before_loading_target_contract() {
        use std::collections::VecDeque;

        let base_addr = test_base_addr();
        let contract_target = test_contract(&base_addr, 89);

        let target_sto = Contract::new()
            .syst(Abst::new(AbstCall::Construct).fitsh("return 0").unwrap())
            .into_sto();

        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract_target, &target_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;

        let tx = StubTxBuilder::new()
            .ty(0)
            .main(base_addr)
            .fee(Amount::zero())
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        let mut host = TestVmHost::new(&mut ctx as &mut dyn Context, 1_000_000);
        let (mut machine, mut runtime) = new_test_machine();

        let over = runtime.warm.space_cap.compo_length + 1;
        let list = CompoItem::list(VecDeque::from(vec![Value::U8(1); over])).unwrap();
        let err = machine
            .abst_call(
                &mut runtime,
                &mut host,
                AbstCall::Construct,
                contract_target.clone(),
                None,
                Value::Compo(list),
            )
            .expect_err("oversize compo argv must fail locally");
        assert!(err.contains("OutOfCompoLen"), "unexpected error: {err}");
        assert!(!runtime.warm.contracts.contains_key(&contract_target));
    }

    #[test]
    fn call_base_gas_is_consumed_from_shared_counter() {
        use crate::rt::Bytecode;

        // Prepare a state with enough HAC for gas spending.
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(3200)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        // END is a minimal "return nil" program; actual instruction gas is tiny.
        let codes = vec![Bytecode::END as u8];

        vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: codes.clone().into(),
            }),
        )
        .unwrap();

        let gsext = GasExtra::new(1);
        let base = gsext.main_call_base;
        let budget = decode_gas_budget(17); // lookup-table decoded budget
        let spent = budget - Context::gas_remaining(&ctx);
        // END adds measured compute work; entry then bills `call_base` surcharge on top (additive).
        assert!(
            spent >= base + 1,
            "spent gas {} should include opcode work plus call_base surcharge (base={})",
            spent,
            base
        );
    }

    #[test]
    fn top_level_calls_keep_tx_cumulative_vm_gas_use() {
        use crate::rt::Bytecode;

        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(3200)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let fail_codes = lang_to_bytecode("return 1").unwrap();
        let ok_codes = vec![Bytecode::END as u8];

        let failed = vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: fail_codes.into(),
            }),
        );
        assert!(failed.is_err(), "first call must fail");

        let gas_after_fail = vm.runtime.gas_use();
        assert!(
            gas_after_fail.total() > 0,
            "failed top-level call must leave cumulative vm gas usage"
        );

        let (ok_cost, _rv) = vm
            .call(
                &mut ctx,
                Box::new(EntryRequest::Main {
                    code_type: CodeType::Bytecode,
                    codes: ok_codes.into(),
                }),
            )
            .unwrap();

        let gas_after_ok = vm.runtime.gas_use();
        assert!(
            gas_after_ok.total() > gas_after_fail.total(),
            "later top-level call must add onto tx cumulative vm gas usage"
        );
        assert_eq!(
            gas_after_ok.checked_sub(gas_after_fail).unwrap(),
            ok_cost,
            "returned gas must stay delta-only even when tx cumulative usage already exists"
        );
    }

    #[test]
    fn top_level_calls_share_tx_compute_bucket() {
        use crate::rt::Bytecode;

        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(3200)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let limit = GasExtra::new(1).main_call_base * 2 - 1;
        vm.runtime.warm.gas_extra.compute_limit = limit;
        let codes = vec![Bytecode::END as u8];

        vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: codes.clone().into(),
            }),
        )
        .unwrap();

        let err = vm
            .call(
                &mut ctx,
                Box::new(EntryRequest::Main {
                    code_type: CodeType::Bytecode,
                    codes: codes.into(),
                }),
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("compute gas limit exceeded"),
            "second top-level call must observe tx cumulative compute bucket: {err}"
        );
    }

    #[test]
    fn snapshot_restore_volatile_fields_only_except_gas_and_warmups() {
        use crate::rt::Bytecode;
        use std::sync::Arc;

        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let codes = vec![Bytecode::END as u8];
        vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: codes.clone().into(),
            }),
        )
        .unwrap();

        let warm_a = test_contract(&main, 201);
        let warm_b = test_contract(&main, 202);
        vm.runtime
            .warm
            .contracts
            .insert(warm_a.clone(), Arc::new(ContractObj::default()));
        let snap = vm.snapshot_volatile();

        // Mutate gas remaining in context (outside VM volatile snapshot).
        let gas_to_consume = Context::gas_remaining(&ctx) - 1;
        Context::gas_charge(&mut ctx, gas_to_consume).unwrap();
        // Mutate volatile fields (should be restored)
        vm.runtime
            .warm
            .contracts
            .insert(warm_b.clone(), Arc::new(ContractObj::default()));

        vm.restore_volatile(snap);

        // Gas remaining is NOT restored: gas usage must stay monotonic in one tx.
        assert_eq!(Context::gas_remaining(&ctx), 1);
        // Warmup accounting is NOT restored: gas-charged preload state must remain monotonic.
        assert_eq!(vm.runtime.warm.contracts.len(), 2);
        assert!(vm.runtime.warm.contracts.contains_key(&warm_b));
    }

    #[test]
    fn low_fee_does_not_panic_in_settle() {
        use crate::rt::Bytecode;

        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(1))
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(1)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let codes = vec![Bytecode::END as u8];

        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vm.call(
                &mut ctx,
                Box::new(EntryRequest::Main {
                    code_type: CodeType::Bytecode,
                    codes: codes.clone().into(),
                }),
            )
        }));
        assert!(res.is_ok(), "settle must not panic");
        // low fee may cause an error return, but must never panic
        let _ = res.unwrap();
    }

    #[test]
    fn reentry_level_restores_after_early_return() {
        use crate::rt::Bytecode;

        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        // Invalid request type should still unwind the re-entry guard.
        let early = vm.call(&mut ctx, Box::new(()));
        assert!(early.is_err(), "invalid request type must fail");
        assert_eq!(
            vm.entries.len(),
            0,
            "re-entry level must be restored after early return"
        );

        // Next normal call should still behave as an outermost call.
        let codes = vec![Bytecode::END as u8];
        let ok = vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: codes.into(),
            }),
        );
        assert!(
            ok.is_ok(),
            "subsequent call must not be poisoned by previous early return"
        );
        assert_eq!(
            vm.entries.len(),
            0,
            "re-entry level must remain balanced after successful call"
        );
    }

    #[test]
    fn call_base_surcharge_adds_to_measured_work_ignores_ctx_rebate() {
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let call_base = EntryKind::Main.call_base(&vm.runtime.warm.gas_extra);
        let gas_before = Context::gas_remaining(&ctx);
        let work = 1i64;

        let (cost, ()) = vm
            .run_vm_entry_ret(
                &mut ctx,
                EntryKind::Main,
                call_base,
                true,
                |_machine, runtime, ctx| {
                    ctx.gas_charge(1)?;
                    ctx.gas_rebate(1)?;
                    runtime.warm.gas_use.compute = runtime
                        .next_compute_used(work)
                        .map_err(|e| e.to_string())?;
                    Ok(())
                },
            )
            .unwrap();

        let expect = work + call_base;
        assert_eq!(cost.total(), expect);
        assert_eq!(vm.runtime.gas_use().total(), expect);
        assert_eq!(Context::gas_remaining(&ctx), gas_before - expect);
        assert_eq!(ctx.exec_from(), basis::component::ExecFrom::Top);
        assert!(vm.entries.is_empty(), "vm entry stack must unwind after settle");
    }

    #[test]
    fn call_base_stacks_with_nested_entry_work_on_shared_meter() {
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let outer_base = EntryKind::Main.call_base(&vm.runtime.warm.gas_extra);
        let inner_base = EntryKind::Abst.call_base(&vm.runtime.warm.gas_extra);
        let inner_work = 3i64;
        let outer_work = 2i64;
        let gas_before = Context::gas_remaining(&ctx);

        let (cost, ()) = vm
            .run_vm_entry_ret(
                &mut ctx,
                EntryKind::Main,
                outer_base,
                true,
                |_machine, runtime, ctx| {
                    // Simulate an inner entry that already measured and settled its own surcharge.
                    runtime.warm.gas_use.compute = runtime
                        .next_compute_used(inner_work)
                        .map_err(|e| e.to_string())?;
                    ctx.gas_charge(inner_work)?;
                    runtime
                        .settle_compute_gas(ctx, inner_base)
                        .map_err(|e| e.to_string())?;

                    // Outer entry self work after inner returns.
                    runtime.warm.gas_use.compute = runtime
                        .next_compute_used(outer_work)
                        .map_err(|e| e.to_string())?;
                    ctx.gas_charge(outer_work)?;
                    Ok(())
                },
            )
            .unwrap();

        let expect = inner_work + inner_base + outer_work + outer_base;
        assert_eq!(cost.total(), expect);
        assert_eq!(vm.runtime.gas_use().total(), expect);
        assert_eq!(Context::gas_remaining(&ctx), gas_before - expect);
        assert!(vm.entries.is_empty(), "vm entry stack must unwind after settle");
    }

    #[test]
    fn drain_deferred_is_one_shot_and_restores_exec_from_and_entry_stack_on_error() {
        let base_addr = test_base_addr();
        let contract = test_contract(&base_addr, 250);
        let contract_sto = Contract::new()
            .syst(Abst::new(AbstCall::Deferred).fitsh("return 1").unwrap())
            .into_sto();
        let mut ext_state = StateMem::default();
        {
            let mut vmsta = crate::VMState::wrap(&mut ext_state);
            vmsta.contract_set_sync_edition(&contract, &contract_sto);
        }

        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = base_addr;
        env.tx.addrs = vec![base_addr];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(base_addr)
            .addrs(vec![base_addr])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(ext_state), &tx);
        protocol::operate::hac_add(&mut ctx, &base_addr, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        vm.runtime
            .volatile
            .deferred_registry
            .replace_defer_auth(Some(contract.clone()));
        vm.runtime
            .volatile
            .deferred_registry
            .register_current(&contract, Some(None))
            .unwrap();

        let err = vm.drain_deferred(&mut ctx).unwrap_err();
        assert!(err.contains("Deferred"), "unexpected error: {err}");
        assert_eq!(ctx.exec_from(), basis::component::ExecFrom::Top);
        assert!(vm.entries.is_empty(), "deferred failure must unwind vm entry stack");
        let rest = vm.runtime.volatile.deferred_registry.drain_lifo();
        assert!(
            rest.is_empty(),
            "deferred phase is strict one-shot: drained callbacks are not restored after failure"
        );
    }

    fn read_hac_balance(ctx: &mut dyn Context, addr: &Address) -> Amount {
        protocol::state::CoreState::wrap(ctx.state())
            .balance(addr)
            .unwrap_or_default()
            .hacash
    }

    #[test]
    fn outermost_failed_call_reduces_remaining_before_tx_settlement() {
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(10_000_000))
            .gas_max(17)
            .tx_size(128)
            .fee_purity(3200)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(17)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let fail_codes = lang_to_bytecode("return 1").unwrap();

        let bal_before = read_hac_balance(&mut ctx, &main);
        let call = vm.call(
            &mut ctx,
            Box::new(EntryRequest::Main {
                code_type: CodeType::Bytecode,
                codes: fail_codes.into(),
            }),
        );
        let bal_after = read_hac_balance(&mut ctx, &main);
        let err = call.expect_err("vm call should fail when script returns non-zero");

        assert!(
            err.to_string().contains("main call return error"),
            "business failure should win over settle failure: {err}"
        );
        assert!(
            Context::gas_remaining(&ctx) < decode_gas_budget(17),
            "remaining gas should decrease even when call fails"
        );
        assert_eq!(
            bal_before, bal_after,
            "direct vm.call failure does not settle protocol billing inside this helper path"
        );
    }

    #[test]
    fn business_failure_wins_over_settle_failure() {
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let mut env = Env::default();
        env.block.height = 1;
        env.tx.main = main;
        env.tx.addrs = vec![main];

        let tx = StubTxBuilder::new()
            .ty(TransactionType3::TYPE)
            .main(main)
            .addrs(vec![main])
            .fee(Amount::unit238(1))
            .gas_max(1)
            .tx_size(128)
            .fee_purity(1)
            .build();
        let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
        protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
        ctx.gas_initialize(decode_gas_budget(1)).unwrap();

        let mut vm = Executor::from_runtime(Runtime::create(1));
        let fail_codes = lang_to_bytecode("return 1").unwrap();

        let err = vm
            .call(
                &mut ctx,
                Box::new(EntryRequest::Main {
                    code_type: CodeType::Bytecode,
                    codes: fail_codes.into(),
                }),
            )
            .expect_err("vm call should fail");
        assert!(
            err.to_string().contains("main call return error"),
            "execute error should take precedence over settle error: {err}"
        );
    }

    #[test]
    fn fail_then_success_keeps_same_balance_until_tx_level_settlement() {
        let main = Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        let fail_codes = lang_to_bytecode("return 1").unwrap();
        let ok_codes = vec![crate::rt::Bytecode::END as u8];

        let run = |run_failed_first: bool| -> (Amount, i64) {
            let mut env = Env::default();
            env.block.height = 1;
            env.tx.main = main;
            env.tx.addrs = vec![main];

            let tx = StubTxBuilder::new()
                .ty(TransactionType3::TYPE)
                .main(main)
                .addrs(vec![main])
                .fee(Amount::unit238(10_000_000))
                .gas_max(17)
                .tx_size(128)
                .fee_purity(3200)
                .build();
            let mut ctx = make_ctx_with_state(env, Box::new(StateMem::default()), &tx);
            protocol::operate::hac_add(&mut ctx, &main, &Amount::unit238(1_000_000_000)).unwrap();
            ctx.gas_initialize(decode_gas_budget(17)).unwrap();

            let mut vm = Executor::from_runtime(Runtime::create(1));
            if run_failed_first {
                let failed = vm.call(
                    &mut ctx,
                    Box::new(EntryRequest::Main {
                        code_type: CodeType::Bytecode,
                        codes: fail_codes.clone().into(),
                    }),
                );
                assert!(failed.is_err(), "prelude failed call must fail");
            }

            let ok = vm.call(
                &mut ctx,
                Box::new(EntryRequest::Main {
                    code_type: CodeType::Bytecode,
                    codes: ok_codes.clone().into(),
                }),
            );
            assert!(ok.is_ok(), "final success call must succeed");

            (read_hac_balance(&mut ctx, &main), Context::gas_remaining(&ctx))
        };

        let (bal_success_only, rem_success_only) = run(false);
        let (bal_fail_then_success, rem_fail_then_success) = run(true);

        assert_eq!(
            bal_fail_then_success, bal_success_only,
            "direct vm.call failure alone does not settle protocol billing before tx-level settlement"
        );
        assert!(
            rem_fail_then_success < rem_success_only,
            "failed call should still consume shared remaining gas"
        );
    }

    /* i64::MAX  = 9223372036854775807 10000 HAC =   10000000000000000:236 0.00000001 = 1:240 = 10000:236 */
}
