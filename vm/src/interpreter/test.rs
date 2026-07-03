#[cfg(test)]
mod bounds_tests {
    use std::sync::Arc;

    use super::*;
    use crate::machine::{DeferCallbacks, DeferredEntry, VmHost};
    use crate::rt::{ExecCtx, GasExtra, GasTable, ItrErr, ItrErrCode, SpaceCap, VmrtErr, VmrtRes};
    use crate::space::{CtcKVMap, GKVMap, Heap, Stack};
    use crate::value::{CompoItem, TupleItem, Value, ValueTy};
    use crate::{ContractAddress, ContractEdition, ContractSto};
    use field::Address;
    use sys::{XError, XRet};

    trait TestGasHost {
        fn set_test_gas(&mut self, gas: i64);
        fn test_gas(&self) -> i64;
    }

    fn execute_code<H: VmHost + TestGasHost + ?Sized>(
        pc: &mut usize,
        codes: &[u8],
        exec: ExecCtx,
        operands: &mut Stack,
        locals: &mut Stack,
        heap: &mut Heap,
        context_addr: &Address,
        current_addr: &Address,
        gas_usable: &mut i64,
        gas_table: &GasTable,
        gas_extra: &GasExtra,
        space_cap: &SpaceCap,
        global_map: &mut GKVMap,
        memory_map: &mut CtcKVMap,
        host: &mut H,
    ) -> VmrtRes<CallExit> {
        host.set_test_gas(*gas_usable);
        let mut gas_use = basis::interface::VmGasBuckets::default();
        let mut log_bytes_total = 0usize;
        let mut defer_callbacks = DeferCallbacks::default();
        let res = super::execute_code(
            pc,
            codes,
            exec,
            operands,
            locals,
            heap,
            context_addr,
            current_addr,
            gas_table,
            gas_extra,
            space_cap,
            &mut gas_use,
            &mut log_bytes_total,
            global_map,
            memory_map,
            &mut defer_callbacks,
            host,
        );
        *gas_usable = host.test_gas();
        res
    }

    #[derive(Default)]
    struct DummyHost {
        gas_remaining: i64,
        gas_rebated: i64,
        act_res: Vec<u8>,
        act_gas: u32,
        act_err: Option<String>,
        act_body: Vec<u8>,
        srest_res: Option<Value>,
        sload_res: Option<Value>,
        sget_res: Option<Value>,
        sput_err: Option<ItrErrCode>,
        sdel_res: Option<i64>,
        sedit_res: Option<(i64, i64)>,
        log_calls: usize,
    }

    impl TestGasHost for DummyHost {
        fn set_test_gas(&mut self, gas: i64) {
            self.gas_remaining = gas;
        }

        fn test_gas(&self) -> i64 {
            self.gas_remaining
        }
    }

    impl VmHost for DummyHost {
        fn height(&self) -> u64 {
            1
        }

        fn main_entry_bindings(&self) -> FrameBindings {
            FrameBindings::root(Address::default(), Arc::<[Address]>::from(vec![]))
        }

        fn gas_remaining(&self) -> i64 {
            self.gas_remaining
        }

        fn gas_charge(&mut self, gas: i64) -> VmrtErr {
            if gas < 0 {
                return itr_err_fmt!(ItrErrCode::GasError, "gas cost invalid: {}", gas);
            }
            self.gas_remaining -= gas;
            if self.gas_remaining < 0 {
                return itr_err_code!(ItrErrCode::OutOfGas);
            }
            Ok(())
        }

        fn gas_rebate(&mut self, gas: i64) -> VmrtErr {
            self.gas_rebated = self
                .gas_rebated
                .checked_add(gas)
                .ok_or_else(|| ItrErr::new(ItrErrCode::GasError, "gas refund overflow"))?;
            Ok(())
        }

        fn contract_edition(&mut self, _addr: &ContractAddress) -> Option<ContractEdition> {
            None
        }

        fn contract(&mut self, _addr: &ContractAddress) -> Option<ContractSto> {
            None
        }

        fn action_call(&mut self, _kid: u16, body: Vec<u8>) -> XRet<(u32, Vec<u8>)> {
            if let Some(e) = &self.act_err {
                return Err(XError::revert(e.clone()));
            }
            self.act_body = body;
            Ok((self.act_gas, self.act_res.clone()))
        }

        fn log_push(&mut self, _cadr: &Address, _items: Vec<Value>) -> VmrtErr {
            self.log_calls += 1;
            Ok(())
        }

        fn sget(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: &Value,
        ) -> VmrtRes<Value> {
            match &self.sget_res {
                Some(v) => Ok(v.clone()),
                None => itr_err_code!(ItrErrCode::StorageError),
            }
        }

        fn sput(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
            _val: Value,
        ) -> VmrtErr {
            match self.sput_err {
                Some(ec) => itr_err_code!(ec),
                None => Ok(()),
            }
        }

        fn sstat(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: &Value,
        ) -> VmrtRes<Value> {
            match &self.srest_res {
                Some(v) => Ok(v.clone()),
                None => itr_err_code!(ItrErrCode::StorageError),
            }
        }

        fn sload(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: &Value,
        ) -> VmrtRes<Value> {
            match &self.sload_res {
                Some(v) => Ok(v.clone()),
                None => itr_err_code!(ItrErrCode::StorageError),
            }
        }

        fn sdel(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
        ) -> VmrtRes<i64> {
            match self.sdel_res {
                Some(v) => Ok(v),
                None => itr_err_code!(ItrErrCode::StorageError),
            }
        }

        fn snew(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
            _val: Value,
            _period: Value,
        ) -> VmrtRes<i64> {
            itr_err_code!(ItrErrCode::StorageError)
        }

        fn sedit(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
            _val: Value,
        ) -> VmrtRes<(i64, i64)> {
            match self.sedit_res {
                Some(v) => Ok(v),
                None => itr_err_code!(ItrErrCode::StorageError),
            }
        }

        fn srent(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
            _period: Value,
        ) -> VmrtRes<i64> {
            itr_err_code!(ItrErrCode::StorageError)
        }

        fn srecv(
            &mut self,
            _gst: &GasExtra,
            _cap: &SpaceCap,
            _cadr: &Address,
            _key: Value,
            _period: Value,
        ) -> VmrtRes<i64> {
            itr_err_code!(ItrErrCode::StorageError)
        }
    }

    fn run_call_opcode_gas(exec: ExecCtx, codes: Vec<u8>) -> i64 {
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        let exit = execute_code(
            &mut pc,
            &codes,
            exec,
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();
        assert!(matches!(exit, crate::rt::CallExit::Call(_)));
        1000 - gas
    }

    fn run_with_setup<F>(codes: Vec<u8>, host: DummyHost, setup: F) -> i64
    where
        F: FnOnce(&mut Stack, &mut Stack, &mut Heap, &mut GKVMap, &mut CtcKVMap, &ContractAddress),
    {
        run_with_setup_gas_extra(codes, host, GasExtra::new(1), setup).0
    }

    fn run_with_setup_host<F>(codes: Vec<u8>, host: DummyHost, setup: F) -> (i64, DummyHost)
    where
        F: FnOnce(&mut Stack, &mut Stack, &mut Heap, &mut GKVMap, &mut CtcKVMap, &ContractAddress),
    {
        run_with_setup_gas_extra(codes, host, GasExtra::new(1), setup)
    }

    fn run_with_setup_gas_extra<F>(
        codes: Vec<u8>,
        host: DummyHost,
        gas_extra: GasExtra,
        setup: F,
    ) -> (i64, DummyHost)
    where
        F: FnOnce(&mut Stack, &mut Stack, &mut Heap, &mut GKVMap, &mut CtcKVMap, &ContractAddress),
    {
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = host;

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        setup(
            &mut operands,
            &mut locals,
            &mut heap,
            &mut global_map,
            &mut memory_map,
            &cadr,
        );

        execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &gas_extra,
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();
        (1000 - gas, host)
    }

    #[test]
    fn execute_code_rejects_truncated_params() {
        use crate::rt::Bytecode;

        let codes = vec![Bytecode::PU16 as u8]; // missing 2 bytes param

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);

        let cadr = ContractAddress::default();

        let res = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );

        assert!(matches!(res, Err(ItrErr(ItrErrCode::CodeOverflow, _))));
    }

    #[test]
    fn execute_code_rejects_unknown_type_id_for_tis_and_cto() {
        use crate::rt::Bytecode;

        for raw in [7u8, 10u8, 12u8] {
            for inst in [Bytecode::TIS, Bytecode::CTO] {
                let codes = vec![Bytecode::P0 as u8, inst as u8, raw, Bytecode::END as u8];

                let mut pc: usize = 0;
                let mut gas: i64 = 1000;
                let mut host = DummyHost::default();

                let mut operands = Stack::new(256);
                let mut locals = Stack::new(256);
                let mut heap = Heap::new(64);
                let mut global_map = GKVMap::new(20);
                let mut memory_map = CtcKVMap::new(12);

                let cadr = ContractAddress::default();

                let res = execute_code(
                    &mut pc,
                    &codes,
                    ExecCtx::main(),
                    &mut operands,
                    &mut locals,
                    &mut heap,
                    &cadr,
                    &cadr,
                    &mut gas,
                    &GasTable::new(1),
                    &GasExtra::new(1),
                    &SpaceCap::new(1),
                    &mut global_map,
                    &mut memory_map,
                    &mut host,
                );

                assert!(
                    matches!(res, Err(ItrErr(ItrErrCode::InstParamsErr, _))),
                    "instruction {:?} with type id {} should fail with InstParamsErr",
                    inst,
                    raw
                );
            }
        }
    }

    #[test]
    fn execute_code_rejects_cto_targets_outside_cast_set() {
        use crate::rt::Bytecode;

        let non_castable_targets = [
            ValueTy::Nil,
            ValueTy::Tuple,
            ValueTy::Handle,
            ValueTy::Compo,
        ];
        for ty in non_castable_targets {
            let codes = vec![
                Bytecode::P0 as u8,
                Bytecode::CTO as u8,
                ty as u8,
                Bytecode::END as u8,
            ];

            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();

            let res = execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            );

            assert!(
                matches!(res, Err(ItrErr(ItrErrCode::InstParamsErr, _))),
                "CTO with target {:?} should fail with InstParamsErr",
                ty
            );
        }
    }

    #[test]
    fn execute_code_tis_accepts_declared_non_cast_types() {
        use crate::rt::Bytecode;

        let run = |stack_v: Value, ty: ValueTy| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::TIS as u8, ty as u8, Bytecode::END as u8];

            operands.push(stack_v)?;
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        assert_eq!(run(Value::Nil, ValueTy::Nil).unwrap(), Value::Bool(true));
        assert_eq!(run(Value::U8(0), ValueTy::Nil).unwrap(), Value::Bool(false));
        assert_eq!(
            run(Value::Compo(CompoItem::new_list()), ValueTy::Compo).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            run(Value::handle(7u32), ValueTy::Handle).unwrap(),
            Value::Bool(true)
        );

        let args = TupleItem::new(vec![Value::U8(1)]).unwrap();
        assert_eq!(
            run(Value::Tuple(args.clone()), ValueTy::Tuple).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn execute_code_cto_accepts_cast_set_targets() {
        use crate::rt::Bytecode;

        let run = |stack_v: Value, ty: ValueTy| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::CTO as u8, ty as u8, Bytecode::END as u8];

            operands.push(stack_v)?;
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        let addr = Address::default();
        assert_eq!(run(Value::Nil, ValueTy::Bool).unwrap(), Value::Bool(false));
        assert_eq!(run(Value::Bool(true), ValueTy::U16).unwrap(), Value::U16(1));
        assert_eq!(
            run(Value::U16(0x0102), ValueTy::Bytes).unwrap(),
            Value::Bytes(vec![0x01, 0x02])
        );
        assert_eq!(
            run(Value::Bytes(addr.to_vec()), ValueTy::Address).unwrap(),
            Value::Address(addr)
        );
    }

    #[test]
    fn execute_code_tis_and_cto_diverge_on_non_cast_targets() {
        use crate::rt::Bytecode;

        let run = |inst: Bytecode, stack_v: Value, ty: ValueTy| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![inst as u8, ty as u8, Bytecode::END as u8];

            operands.push(stack_v)?;
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        assert_eq!(
            run(Bytecode::TIS, Value::Nil, ValueTy::Nil).unwrap(),
            Value::Bool(true)
        );
        assert!(matches!(
            run(Bytecode::CTO, Value::Nil, ValueTy::Nil),
            Err(ItrErr(ItrErrCode::InstParamsErr, _))
        ));

        let args = TupleItem::new(vec![Value::U8(1)]).unwrap();
        assert_eq!(
            run(Bytecode::TIS, Value::Tuple(args.clone()), ValueTy::Tuple).unwrap(),
            Value::Bool(true)
        );
        assert!(matches!(
            run(Bytecode::CTO, Value::Tuple(args), ValueTy::Tuple),
            Err(ItrErr(ItrErrCode::InstParamsErr, _))
        ));

        let handle = Value::handle(7u32);
        assert_eq!(
            run(Bytecode::TIS, handle.clone(), ValueTy::Handle).unwrap(),
            Value::Bool(true)
        );
        assert!(matches!(
            run(Bytecode::CTO, handle, ValueTy::Handle),
            Err(ItrErr(ItrErrCode::InstParamsErr, _))
        ));

        let list = CompoItem::new_list();
        assert_eq!(
            run(Bytecode::TIS, Value::Compo(list.clone()), ValueTy::Compo).unwrap(),
            Value::Bool(true)
        );
        assert!(matches!(
            run(Bytecode::CTO, Value::Compo(list), ValueTy::Compo),
            Err(ItrErr(ItrErrCode::InstParamsErr, _))
        ));
    }

    #[test]
    fn execute_code_cto_valid_target_with_invalid_operand_fails_castfail() {
        use crate::rt::Bytecode;

        let run = |stack_v: Value, ty: ValueTy| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::CTO as u8, ty as u8, Bytecode::END as u8];

            operands.push(stack_v)?;
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        assert!(matches!(
            run(Value::U8(1), ValueTy::Address),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
        assert!(matches!(
            run(Value::Bytes(vec![1, 2, 3]), ValueTy::U16),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
        assert!(matches!(
            run(Value::Nil, ValueTy::Bytes),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
    }

    #[test]
    fn execute_code_cbytes_rejects_nil() {
        use crate::rt::Bytecode;

        let run = |stack_v: Value| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::CBYTES as u8, Bytecode::END as u8];

            operands.push(stack_v)?;
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        assert!(matches!(
            run(Value::Nil),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
        assert_eq!(
            run(Value::Bytes(vec![])).unwrap(),
            Value::Bytes(vec![])
        );
    }

    #[test]
    fn xlg_ordering_marks_execute_with_display_semantics() {
        use crate::rt::Bytecode;

        let run = |mark: u8, stack_v: Value, local_v: Value| -> (Value, Value) {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::XLG as u8, mark, Bytecode::END as u8];

            let idx = (mark & 0b0001_1111) as u16;
            locals.alloc((idx + 1) as u8).unwrap();
            for i in 0..=idx {
                locals.save(i, Value::U8(0)).unwrap();
            }
            locals.save(idx, local_v).unwrap();
            operands.push(stack_v).unwrap();

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();

            let out = operands.pop().unwrap();
            let local_after = locals.load(idx as usize).unwrap();
            (out, local_after)
        };

        let (v40, l40) = run((4 << 5) | 0, Value::U8(3), Value::U8(2)); // 2 > 3
        assert_eq!(v40, Value::Bool(false));
        assert_eq!(l40, Value::U8(2));
        let (v41, l41) = run((4 << 5) | 0, Value::U8(1), Value::U8(2)); // 2 > 1
        assert_eq!(v41, Value::Bool(true));
        assert_eq!(l41, Value::U8(2));
        let (v50, l50) = run((5 << 5) | 0, Value::U8(2), Value::U8(2)); // 2 >= 2
        assert_eq!(v50, Value::Bool(true));
        assert_eq!(l50, Value::U8(2));
        let (v61, l61) = run((6 << 5) | 1, Value::U8(1), Value::U8(2)); // 2 < 1
        assert_eq!(v61, Value::Bool(false));
        assert_eq!(l61, Value::U8(2));
        let (v60, l60) = run((6 << 5) | 0, Value::U8(3), Value::U8(2)); // 2 < 3
        assert_eq!(v60, Value::Bool(true));
        assert_eq!(l60, Value::U8(2));
        let (v70, l70) = run((7 << 5) | 0, Value::U8(2), Value::U8(2)); // 2 <= 2
        assert_eq!(v70, Value::Bool(true));
        assert_eq!(l70, Value::U8(2));
    }

    #[test]
    fn eq_tuple_uses_content_compare_and_ptr_fast_path_gas() {
        use crate::rt::Bytecode;

        let payload = vec![9u8; 64];
        let shared = TupleItem::new(vec![
            Value::Bytes(payload.clone()),
            Value::Bytes(payload.clone()),
        ])
        .unwrap();
        let distinct =
            TupleItem::new(vec![Value::Bytes(payload.clone()), Value::Bytes(payload)]).unwrap();

        let shared_gas = run_with_setup(
            vec![Bytecode::EQ as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(Value::Tuple(shared.clone())).unwrap();
                ops.push(Value::Tuple(shared.clone())).unwrap();
            },
        );
        let distinct_gas = run_with_setup(
            vec![Bytecode::EQ as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(Value::Tuple(shared.clone())).unwrap();
                ops.push(Value::Tuple(distinct.clone())).unwrap();
            },
        );
        let gst = GasTable::new(1);
        let gex = GasExtra::new(1);
        let compare_fee = value_compare_fee(
            &Value::Tuple(shared.clone()),
            &Value::Tuple(shared.clone()),
            gex.container_cmp_header,
        );
        let shared_expected =
            gst.gas(Bytecode::EQ as u8) + gst.gas(Bytecode::END as u8) + gex.stack_cmp(compare_fee);
        assert_eq!(shared_gas, shared_expected);
        assert!(distinct_gas > shared_gas);

        let run_result = |rhs: Value| -> Value {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            operands.push(Value::Tuple(shared.clone())).unwrap();
            operands.push(rhs).unwrap();
            execute_code(
                &mut pc,
                &[Bytecode::EQ as u8, Bytecode::END as u8],
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &gst,
                &gex,
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            operands.pop().unwrap()
        };

        assert_eq!(run_result(Value::Tuple(shared.clone())), Value::Bool(true));
        assert_eq!(run_result(Value::Tuple(distinct)), Value::Bool(true));
    }

    #[test]
    fn xlg_eq_tuple_uses_content_compare_and_ptr_fast_path_gas() {
        use crate::rt::Bytecode;

        let payload = vec![9u8; 64];
        let shared = TupleItem::new(vec![
            Value::Bytes(payload.clone()),
            Value::Bytes(payload.clone()),
        ])
        .unwrap();
        let distinct =
            TupleItem::new(vec![Value::Bytes(payload.clone()), Value::Bytes(payload)]).unwrap();
        let mark = (2 << 5) | 0;

        let run = |local_v: Value, stack_v: Value| -> (Value, i64) {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            locals.alloc(1).unwrap();
            locals.save(0, local_v).unwrap();
            operands.push(stack_v).unwrap();

            execute_code(
                &mut pc,
                &[Bytecode::XLG as u8, mark, Bytecode::END as u8],
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();

            (operands.pop().unwrap(), 1000 - gas)
        };

        let gst = GasTable::new(1);
        let gex = GasExtra::new(1);
        let compare_fee = value_compare_fee(
            &Value::Tuple(shared.clone()),
            &Value::Tuple(shared.clone()),
            gex.container_cmp_header,
        );
        let shared_expected = gst.gas(Bytecode::XLG as u8)
            + gst.gas(Bytecode::END as u8)
            + gex.stack_cmp(compare_fee);

        let (out_shared, gas_shared) =
            run(Value::Tuple(shared.clone()), Value::Tuple(shared.clone()));
        let (out_distinct, gas_distinct) = run(Value::Tuple(shared), Value::Tuple(distinct));
        assert_eq!(out_shared, Value::Bool(true));
        assert_eq!(out_distinct, Value::Bool(true));
        assert_eq!(gas_shared, shared_expected);
        assert!(gas_distinct > gas_shared);
    }

    #[test]
    fn size_rejects_tuple_compo_and_handle() {
        use crate::rt::Bytecode;

        let run_err = |input: Value| -> ItrErrCode {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            operands.push(input).unwrap();
            execute_code(
                &mut pc,
                &[Bytecode::SIZE as u8, Bytecode::END as u8],
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap_err()
            .0
        };

        assert_eq!(
            run_err(Value::Tuple(TupleItem::new(vec![Value::U8(1)]).unwrap())),
            ItrErrCode::ItemNoSize
        );
        assert_eq!(
            run_err(Value::Compo(
                CompoItem::list(std::collections::VecDeque::from([Value::U8(1)])).unwrap()
            )),
            ItrErrCode::ItemNoSize
        );
        assert_eq!(run_err(Value::handle(7u32)), ItrErrCode::ItemNoSize);
        assert_eq!(run_err(Value::handle(1u32)), ItrErrCode::ItemNoSize);
    }

    #[test]
    fn hreadul_charges_dynamic_read_bytes() {
        use crate::rt::Bytecode;

        let run = |mark_hi: u8, mark_lo: u8| -> i64 {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            heap.grow(1, &GasExtra::new(1)).unwrap();
            heap.write(0, Value::Bytes(vec![0u8; 16])).unwrap();
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![
                Bytecode::HREADUL as u8,
                mark_hi,
                mark_lo,
                Bytecode::END as u8,
            ];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            1000 - gas
        };

        // mark=0x0000 -> u8 at segment 0, len=1
        let gas_u8 = run(0x00, 0x00);
        // mark=0x8000 -> u128 at segment 0, len=16
        let gas_u128 = run(0x80, 0x00);

        assert_eq!(
            gas_u128, gas_u8,
            "HREADUL u8 and u128 are both in the first ceil bucket (<=16 bytes)"
        );
    }

    #[test]
    fn actenv_return_value_is_metered() {
        use crate::rt::Bytecode;

        let run = |idx: u8, ret: Vec<u8>| -> i64 {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost {
                act_res: ret,
                ..Default::default()
            };

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::ACTENV as u8, idx, Bytecode::END as u8];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            1000 - gas
        };

        // idx=1 -> EnvHeight (U64, 8 bytes), 8/16 = 0 extra
        let gas_u64 = run(1, 1u64.to_be_bytes().to_vec());
        // idx=2 -> EnvMainAddr (Address, 21 bytes), 21/16 = 1 extra
        let gas_addr = run(2, Address::default().to_vec());

        assert_eq!(
            gas_addr,
            gas_u64 + 1,
            "ACTENV should meter return value bytes"
        );
    }

    #[test]
    fn srest_is_fixed_gas_without_dynamic_bytes() {
        use crate::rt::Bytecode;

        let run = |retv: Value| -> i64 {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost {
                srest_res: Some(retv),
                ..Default::default()
            };

            let mut operands = Stack::new(256);
            operands.push(Value::U8(1)).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::SSTAT as u8, Bytecode::END as u8];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            1000 - gas
        };

        let gas_u8 = run(Value::U8(7));
        let gas_addr = run(Value::Address(Address::default()));
        assert_eq!(
            gas_addr, gas_u8,
            "SREST should be fixed gas without return-size dynamic billing"
        );

        let gst = GasTable::new(1);
        let expect = gst.gas(Bytecode::SSTAT as u8) + gst.gas(Bytecode::END as u8);
        assert_eq!(gas_u8, expect);
    }

    #[test]
    fn ntenv_return_value_is_metered() {
        use crate::native::NativeEnv;
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);

        let cadr = ContractAddress::default();
        let idx = NativeEnv::idx_context_address;
        let codes = vec![Bytecode::NTENV as u8, idx, Bytecode::END as u8];

        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &gas_table,
            &gas_extra,
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        let expect = gas_table.gas(Bytecode::NTENV as u8)
            + NativeEnv::gas(idx).unwrap()
            + gas_extra.nt_bytes(Address::SIZE)
            + gas_table.gas(Bytecode::END as u8);
        assert_eq!(1000 - gas, expect);
    }

    #[test]
    fn max_log_bytes_zero_disables_log_opcode() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        let mut cap = SpaceCap::new(1);
        cap.log_size = 0;

        operands.push(Value::Nil).unwrap();
        operands.push(Value::Nil).unwrap();

        let err = execute_code(
            &mut pc,
            &[Bytecode::LOG1 as u8, Bytecode::END as u8],
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &cap,
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::OutOfLogSize);
        assert_eq!(host.log_calls, 0);
    }

    #[test]
    fn ntreg_defer_registers_current_contract() {
        use crate::machine::DeferCallbacks;
        use crate::native::NativeCtl;
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let mut defer_callbacks = DeferCallbacks::default();
        let mut gas_use = basis::interface::VmGasBuckets::default();

        let cadr = ContractAddress::from_unchecked(Address::create_contract([7u8; 20]));
        let codes = vec![
            Bytecode::PNIL as u8,
            Bytecode::NTCTL as u8,
            NativeCtl::idx_defer,
            Bytecode::END as u8,
        ];

        let mut bindings =
            FrameBindings::contract(cadr.clone(), cadr.clone(), Vec::<Address>::new().into());
        let mut intent_state = crate::frame::IntentScopeState::default();
        let mut log_bytes_total = 0usize;
        defer_callbacks.replace_defer_auth(Some(cadr.clone()));
        super::execute_code_in_frame(
            &mut pc,
            &codes,
            ExecCtx::external(),
            &mut operands,
            &mut locals,
            &mut heap,
            &mut bindings,
            &mut intent_state,
            &cadr.to_addr(),
            &cadr.to_addr(),
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut gas_use,
            &mut log_bytes_total,
            &mut global_map,
            &mut memory_map,
            &mut crate::machine::IntentRuntime::default(),
            &mut defer_callbacks,
            &mut host,
        )
        .unwrap();

        assert_eq!(
            defer_callbacks.drain_lifo(),
            vec![DeferredEntry {
                addr: cadr,
                intent_scope: Some(None),
            }]
        );
    }

    #[test]
    fn ntreg_defer_requires_intent_owner_even_when_abst_authorized() {
        use crate::machine::DeferCallbacks;
        use crate::native::NativeCtl;
        use crate::rt::Bytecode;
        use crate::value::IntentId;

        let mut pc: usize = 0;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let owner = ContractAddress::from_unchecked(Address::create_contract([7u8; 20]));
        let foreign = ContractAddress::from_unchecked(Address::create_contract([8u8; 20]));
        let mut intents = crate::machine::IntentRuntime::new(crate::machine::IntentRuntimeLimits {
            create_limit: 8,
            keys_per_intent: 8,
            val_size_limit: 128,
            key_max_bytes: 128,
        });
        let iid = intents.create(owner, b"owned".to_vec()).unwrap();

        let mut operands = Stack::new(256);
        operands.push(Value::handle(IntentId(iid))).unwrap();
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let mut defer_callbacks = DeferCallbacks::default();
        let mut gas_use = basis::interface::VmGasBuckets::default();

        let codes = vec![
            Bytecode::NTCTL as u8,
            NativeCtl::idx_defer,
            Bytecode::END as u8,
        ];

        let mut bindings =
            FrameBindings::contract(foreign.clone(), foreign.clone(), Vec::<Address>::new().into());
        let mut intent_state = crate::frame::IntentScopeState::default();
        let mut log_bytes_total = 0usize;
        defer_callbacks.replace_defer_auth(Some(foreign.clone()));
        let err = super::execute_code_in_frame(
            &mut pc,
            &codes,
            ExecCtx::external(),
            &mut operands,
            &mut locals,
            &mut heap,
            &mut bindings,
            &mut intent_state,
            &foreign.to_addr(),
            &foreign.to_addr(),
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut gas_use,
            &mut log_bytes_total,
            &mut global_map,
            &mut memory_map,
            &mut intents,
            &mut defer_callbacks,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::DeferredError);
        assert!(
            err.1.contains("not owned by contract"),
            "unexpected error: {}",
            err.1
        );
        assert!(defer_callbacks.drain_lifo().is_empty());
    }

    #[test]
    fn ntreg_defer_requires_contract_context() {
        use crate::machine::DeferCallbacks;
        use crate::native::NativeCtl;
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let mut defer_callbacks = DeferCallbacks::default();
        let mut gas_use = basis::interface::VmGasBuckets::default();
        let mut log_bytes_total = 0usize;

        let main = Address::create_privakey([3u8; 20]);
        let codes = vec![
            Bytecode::PNIL as u8,
            Bytecode::NTCTL as u8,
            NativeCtl::idx_defer,
            Bytecode::END as u8,
        ];

        let err = super::execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &main,
            &main,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut gas_use,
            &mut log_bytes_total,
            &mut global_map,
            &mut memory_map,
            &mut defer_callbacks,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::DeferredError);
        assert!(
            err.1.contains("contract context"),
            "unexpected error: {}",
            err.1
        );
    }

    #[test]
    fn get0_stack_copy_uses_dup_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            locals.alloc(1).unwrap();
            locals.save(0, v).unwrap();

            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::GET0 as u8, Bytecode::END as u8];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            1000 - gas
        };

        let gas_32 = run(Value::Bytes(vec![0u8; 32]));
        let gas_33 = run(Value::Bytes(vec![0u8; 33]));
        assert_eq!(gas_33, gas_32 + 1);
    }

    #[test]
    fn get0_stack_copy_uses_dup_size_for_handle() {
        use crate::rt::Bytecode;

        let run = |_len: u32| -> i64 {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            locals.alloc(1).unwrap();
            locals.save(0, Value::handle(7u32)).unwrap();

            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![Bytecode::GET0 as u8, Bytecode::END as u8];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            1000 - gas
        };

        assert_eq!(run(1), run(4096));
    }

    #[test]
    fn itemget_compo_bytes_counts_non_bytes_values() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();

        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);

        let mut map = CompoItem::new_map();
        let cap = SpaceCap::new(1);
        map.insert(&cap, Value::U8(1), Value::Address(Address::default()))
            .unwrap();
        operands.push(Value::Compo(map)).unwrap();
        operands.push(Value::U8(1)).unwrap();

        let cadr = ContractAddress::default();
        let codes = vec![Bytecode::ITEMGET as u8, Bytecode::END as u8];

        execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &cap,
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        let expect = gas_table.gas(Bytecode::ITEMGET as u8)
            + gas_table.gas(Bytecode::END as u8)
            + gas_extra.compo_items_read(1)
            + gas_extra.compo_bytes(Address::SIZE);
        assert_eq!(1000 - gas, expect);
    }

    #[test]
    fn args_support_list_read_opcodes() {
        use crate::rt::Bytecode;

        let run = |codes: Vec<u8>, seed: Value, extra: Option<Value>| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            operands.push(seed).unwrap();
            if let Some(v) = extra {
                operands.push(v).unwrap();
            }
            let _ = execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        let args = || {
            Value::Tuple(
                TupleItem::new(vec![Value::Compo(CompoItem::new_map()), Value::U16(9)]).unwrap(),
            )
        };

        assert_eq!(
            run(
                vec![Bytecode::LENGTH as u8, Bytecode::END as u8],
                args(),
                None
            )
            .unwrap(),
            Value::U32(2)
        );
        assert_eq!(
            run(
                vec![Bytecode::HASKEY as u8, Bytecode::END as u8],
                args(),
                Some(Value::U8(1))
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert!(matches!(
            run(
                vec![Bytecode::ITEMGET as u8, Bytecode::END as u8],
                args(),
                Some(Value::U8(0))
            )
            .unwrap(),
            Value::Compo(_)
        ));
        assert!(matches!(
            run(
                vec![Bytecode::TAKEFIRST as u8, Bytecode::END as u8],
                args(),
                None
            ),
            Err(ItrErr(ItrErrCode::CompoOpNotMatch, _))
        ));
        assert!(matches!(
            run(
                vec![Bytecode::TAKELAST as u8, Bytecode::END as u8],
                args(),
                None
            ),
            Err(ItrErr(ItrErrCode::CompoOpNotMatch, _))
        ));
    }

    #[test]
    fn mget_stack_copy_uses_dup_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::MGET as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, memory_map, cadr| {
                    let key = Value::Bytes(vec![1u8]);
                    memory_map
                        .entry_mut(cadr)
                        .unwrap()
                        .put(key.clone(), v)
                        .unwrap();
                    ops.push(key).unwrap();
                },
            )
        };

        let gas_32 = run(Value::Bytes(vec![0u8; 32]));
        let gas_33 = run(Value::Bytes(vec![0u8; 33]));
        assert_eq!(gas_33, gas_32 + 1);
    }
    #[test]
    fn dup_stack_copy_uses_dup_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::DUP as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(v).unwrap();
                },
            )
        };

        let gas_32 = run(Value::Bytes(vec![0u8; 32]));
        let gas_33 = run(Value::Bytes(vec![0u8; 33]));
        assert_eq!(gas_33, gas_32 + 1);
    }

    #[test]
    fn dup_stack_copy_uses_dup_size_for_args_handle() {
        use crate::rt::Bytecode;

        let run = |len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::DUP as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    let args =
                        crate::value::TupleItem::new(vec![Value::Bytes(vec![0u8; len])]).unwrap();
                    ops.push(Value::Tuple(args)).unwrap();
                },
            )
        };

        assert_eq!(run(1), run(256));
    }

    #[test]
    fn dupn_stack_copy_uses_dup_size_for_ref_handles() {
        use crate::rt::Bytecode;

        let run = |len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::DUPN as u8, 5, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    for _ in 0..5 {
                        let args = crate::value::TupleItem::new(vec![Value::Bytes(vec![0u8; len])])
                            .unwrap();
                        ops.push(Value::Tuple(args)).unwrap();
                    }
                },
            )
        };

        assert_eq!(run(1), run(256));
    }

    #[test]
    fn gget_stack_copy_uses_dup_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::GGET as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, global_map, _memory_map, _cadr| {
                    let key = Value::Bytes(vec![2u8]);
                    global_map.put(key.clone(), v).unwrap();
                    ops.push(key).unwrap();
                },
            )
        };

        let gas_32 = run(Value::Bytes(vec![0u8; 32]));
        let gas_33 = run(Value::Bytes(vec![0u8; 33]));
        assert_eq!(gas_33, gas_32 + 1);
    }

    #[test]
    fn getx_stack_copy_uses_dup_size_for_handle() {
        use crate::rt::Bytecode;

        let run = |_len: u32| -> i64 {
            run_with_setup(
                vec![
                    Bytecode::P0 as u8,
                    Bytecode::GETX as u8,
                    Bytecode::END as u8,
                ],
                DummyHost::default(),
                |_ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(1).unwrap();
                    locals.save(0, Value::handle(9u32)).unwrap();
                },
            )
        };

        assert_eq!(run(1), run(4096));
    }

    #[test]
    fn put_space_write_uses_val_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::PUT as u8, 0, Bytecode::END as u8],
                DummyHost::default(),
                |ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(1).unwrap();
                    ops.push(v).unwrap();
                },
            )
        };

        let gas_28 = run(Value::Bytes(vec![0u8; 28]));
        let gas_29 = run(Value::Bytes(vec![0u8; 29]));
        assert_eq!(gas_29, gas_28 + 1);
    }

    #[test]
    fn xop_stack_write_uses_result_val_size() {
        use crate::rt::Bytecode;
        use crate::rt::{encode_local_operand_mark, LxOp};

        let mark = encode_local_operand_mark(LxOp::Add, 0).unwrap();
        let run = |local: Value, rhs: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::XOP as u8, mark, Bytecode::END as u8],
                DummyHost::default(),
                |ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(1).unwrap();
                    locals.save(0, local).unwrap();
                    ops.push(rhs).unwrap();
                },
            )
        };

        let gas = run(Value::U8(0), Value::U8(1));
        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        let base = gas_table.gas(Bytecode::XOP as u8) + gas_table.gas(Bytecode::END as u8);
        let result_sz = Value::U8(1).val_size();
        assert_eq!(gas, base + gas_extra.stack_write(result_sz));
    }

    #[test]
    fn putx_space_write_uses_val_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::PUTX as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(1).unwrap();
                    ops.push(Value::U16(0)).unwrap();
                    ops.push(v).unwrap();
                },
            )
        };

        let gas_28 = run(Value::Bytes(vec![0u8; 28]));
        let gas_29 = run(Value::Bytes(vec![0u8; 29]));
        assert_eq!(gas_29, gas_28 + 1);
    }

    #[test]
    fn mput_space_write_uses_val_size() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x35u8]);
        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::MPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, memory_map, cadr| {
                    memory_map
                        .entry_mut(cadr)
                        .unwrap()
                        .put(key.clone(), Value::U8(1))
                        .unwrap();
                    ops.push(key.clone()).unwrap();
                    ops.push(v).unwrap();
                },
            )
        };

        let gas_28 = run(Value::Bytes(vec![0u8; 28]));
        let gas_29 = run(Value::Bytes(vec![0u8; 29]));
        assert_eq!(gas_29, gas_28 + 1);
    }

    #[test]
    fn gput_space_write_uses_val_size() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x36u8]);
        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::GPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, global_map, _memory_map, _cadr| {
                    global_map.put(key.clone(), Value::U8(1)).unwrap();
                    ops.push(key.clone()).unwrap();
                    ops.push(v).unwrap();
                },
            )
        };

        let gas_28 = run(Value::Bytes(vec![0u8; 28]));
        let gas_29 = run(Value::Bytes(vec![0u8; 29]));
        assert_eq!(gas_29, gas_28 + 1);
    }

    #[test]
    fn clear_charges_edit_items_and_bytes() {
        use crate::rt::Bytecode;
        use std::collections::VecDeque;

        let run = |blen: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::CLEAR as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    let list = Value::Compo(
                        CompoItem::list(VecDeque::from(vec![Value::Bytes(vec![0u8; blen])]))
                            .unwrap(),
                    );
                    ops.push(list).unwrap();
                },
            )
        };

        assert_eq!(run(41), run(40) + 1);
    }

    #[test]
    fn insert_map_charges_key_bytes() {
        use crate::rt::Bytecode;

        let run = |klen: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::INSERT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Compo(CompoItem::new_map())).unwrap();
                    ops.push(Value::Bytes(vec![1u8; klen])).unwrap();
                    ops.push(Value::U8(7)).unwrap();
                },
            )
        };

        assert_eq!(run(41), run(40) + 1);
    }

    #[test]
    fn gput_charges_key_bytes_by_stack_write_div() {
        use crate::rt::Bytecode;

        let run = |klen: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::GPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, global_map, _memory_map, _cadr| {
                    let key = Value::Bytes(vec![0x55u8; klen]);
                    global_map.put(key.clone(), Value::U8(1)).unwrap();
                    ops.push(key).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(29), run(28) + 1);
    }

    #[test]
    fn mput_nil_removes_existing_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x71u8]);
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        memory_map
            .entry_mut(&cadr)
            .unwrap()
            .put(key.clone(), Value::U8(7))
            .unwrap();
        operands.push(key.clone()).unwrap();
        operands.push(Value::Nil).unwrap();

        execute_code(
            &mut pc,
            &[Bytecode::MPUT as u8, Bytecode::END as u8],
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert_eq!(memory_map.get(&cadr, &key).unwrap(), Value::Nil);
        assert!(memory_map.contains_addr(&cadr).unwrap());
        assert_eq!(memory_map.entry(&cadr).unwrap().unwrap().len(), 0);
    }

    #[test]
    fn gput_nil_removes_existing_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x72u8]);
        let mut global_map = GKVMap::new(20);
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        global_map.put(key.clone(), Value::U8(9)).unwrap();
        operands.push(key.clone()).unwrap();
        operands.push(Value::Nil).unwrap();

        execute_code(
            &mut pc,
            &[Bytecode::GPUT as u8, Bytecode::END as u8],
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert_eq!(global_map.get(&key).unwrap(), Value::Nil);
        assert_eq!(global_map.len(), 0);
    }

    #[test]
    fn gput_nil_on_missing_key_does_not_consume_slot() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x73u8]);
        let run = || {
            let mut global_map = GKVMap::new(1);
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();

            operands.push(key.clone()).unwrap();
            operands.push(Value::Nil).unwrap();
            execute_code(
                &mut pc,
                &[Bytecode::GPUT as u8, Bytecode::END as u8],
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            global_map
        };

        let mut global_map = run();
        assert_eq!(global_map.len(), 0);
        global_map
            .put(Value::Bytes(vec![0x74u8]), Value::U8(1))
            .unwrap();
        assert_eq!(global_map.len(), 1);
    }

    #[test]
    fn mput_nil_on_missing_key_does_not_consume_slot() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x75u8]);
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(1);
        let cadr = ContractAddress::default();

        operands.push(key.clone()).unwrap();
        operands.push(Value::Nil).unwrap();
        execute_code(
            &mut pc,
            &[Bytecode::MPUT as u8, Bytecode::END as u8],
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert_eq!(memory_map.addr_len(), 0);
        assert!(!memory_map.contains_addr(&cadr).unwrap());

        let mem = memory_map.entry_mut(&cadr).unwrap();
        mem.put(Value::Bytes(vec![0x76u8]), Value::U8(1)).unwrap();
        assert_eq!(mem.len(), 1);
    }

    #[test]
    fn gput_nil_then_reinsert_counts_as_new_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x77u8]);
        let run = |clear_first: bool| -> i64 {
            run_with_setup(
                vec![Bytecode::GPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, global_map, _memory_map, _cadr| {
                    global_map.put(key.clone(), Value::U8(1)).unwrap();
                    if clear_first {
                        global_map.remove(&key).unwrap();
                    }
                    ops.push(key.clone()).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(true), run(false) + 32);
    }

    #[test]
    fn mput_nil_then_reinsert_counts_as_new_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x78u8]);
        let run = |clear_first: bool| -> i64 {
            run_with_setup(
                vec![Bytecode::MPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, memory_map, cadr| {
                    let mem = memory_map.entry_mut(cadr).unwrap();
                    mem.put(key.clone(), Value::U8(1)).unwrap();
                    if clear_first {
                        mem.remove(&key).unwrap();
                    }
                    ops.push(key.clone()).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(true), run(false) + 20);
    }

    #[test]
    fn mput_charges_key_bytes_by_stack_write_div() {
        use crate::rt::Bytecode;

        let run = |klen: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::MPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, memory_map, cadr| {
                    let key = Value::Bytes(vec![0x66u8; klen]);
                    memory_map
                        .entry_mut(cadr)
                        .unwrap()
                        .put(key.clone(), Value::U8(1))
                        .unwrap();
                    ops.push(key).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(29), run(28) + 1);
    }

    #[test]
    fn rev_has_no_dynamic_copy_gas() {
        use crate::rt::Bytecode;

        let run = |v1: Value, v2: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::REV as u8, 2, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(v1).unwrap();
                    ops.push(v2).unwrap();
                },
            )
        };

        let gas_small = run(Value::U8(1), Value::U8(2));
        let gas_large = run(Value::Bytes(vec![0u8; 64]), Value::Bytes(vec![0u8; 64]));
        assert_eq!(gas_large, gas_small, "REV should not meter dynamic bytes");
    }

    #[test]
    fn popn_gas_increases_by_one_per_item() {
        use crate::rt::Bytecode;

        let run = |n: u8| -> i64 {
            run_with_setup(
                vec![Bytecode::POPN as u8, n, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap();
                    ops.push(Value::U8(2)).unwrap();
                    ops.push(Value::U8(3)).unwrap();
                },
            )
        };

        assert_eq!(run(2), run(1) + 1);
    }

    #[test]
    fn roll_gas_increases_by_one_per_moved_item() {
        use crate::rt::Bytecode;

        let run_roll = |n: u8| -> i64 {
            run_with_setup(
                vec![Bytecode::ROLL as u8, n, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap();
                    ops.push(Value::U8(2)).unwrap();
                    ops.push(Value::U8(3)).unwrap();
                    ops.push(Value::U8(4)).unwrap();
                },
            )
        };
        let run_roll0 = || -> i64 {
            run_with_setup(
                vec![Bytecode::ROLL0 as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap();
                    ops.push(Value::U8(2)).unwrap();
                    ops.push(Value::U8(3)).unwrap();
                },
            )
        };

        assert_eq!(run_roll(2), run_roll(1) + 1);
        assert_eq!(run_roll(0), run_roll0());
    }

    #[test]
    fn swap_charges_two_moved_items() {
        use crate::rt::Bytecode;

        let gas = run_with_setup(
            vec![Bytecode::SWAP as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(Value::U8(1)).unwrap();
                ops.push(Value::U8(2)).unwrap();
            },
        );

        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        assert_eq!(
            gas,
            gas_table.gas(Bytecode::SWAP as u8)
                + gas_table.gas(Bytecode::END as u8)
                + gas_extra.stack_move_items(2)
        );
    }

    #[test]
    fn choose_charges_two_stack_move_items() {
        use crate::rt::Bytecode;

        let gas = run_with_setup(
            vec![
                Bytecode::PTRUE as u8,
                Bytecode::P1 as u8,
                Bytecode::P0 as u8,
                Bytecode::CHOOSE as u8,
                Bytecode::END as u8,
            ],
            DummyHost::default(),
            |_ops, _locals, _heap, _global_map, _memory_map, _cadr| {},
        );

        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        assert_eq!(
            gas,
            gas_table.gas(Bytecode::PTRUE as u8)
                + gas_table.gas(Bytecode::P1 as u8)
                + gas_table.gas(Bytecode::P0 as u8)
                + gas_table.gas(Bytecode::CHOOSE as u8)
                + gas_table.gas(Bytecode::END as u8)
                + gas_extra.stack_move_items(2)
        );
    }

    #[test]
    fn unpack_charges_compo_items_read_on_container_len() {
        use crate::rt::Bytecode;

        let run = |n: usize| -> i64 {
            let items: Vec<Value> = (0..n).map(|i| Value::U8(i as u8)).collect();
            let tuple = Value::Tuple(TupleItem::new(items).unwrap());
            run_with_setup(
                vec![Bytecode::UNPACK as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(n as u8).unwrap();
                    ops.push(tuple).unwrap();
                    ops.push(Value::U8(0)).unwrap();
                },
            )
        };

        assert_eq!(run(4), run(3) + 1);
        assert_eq!(run(5), run(4) + 2);
    }

    #[test]
    fn rev_gas_increases_by_one_per_item() {
        use crate::rt::Bytecode;

        let run = |n: u8| -> i64 {
            run_with_setup(
                vec![Bytecode::REV as u8, n, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap();
                    ops.push(Value::U8(2)).unwrap();
                    ops.push(Value::U8(3)).unwrap();
                    ops.push(Value::U8(4)).unwrap();
                },
            )
        };

        assert_eq!(run(3), run(2) + 1);
    }

    #[test]
    fn cat_dynamic_gas_uses_stack_op_div_20() {
        use crate::rt::Bytecode;

        let run = |l1: usize, l2: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::CAT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![1u8; l1])).unwrap();
                    ops.push(Value::Bytes(vec![2u8; l2])).unwrap();
                },
            )
        };

        assert_eq!(run(11, 10), run(10, 10) + 1);
    }

    #[test]
    fn left_dynamic_gas_uses_stack_op_div_20() {
        use crate::rt::Bytecode;

        let run = |take: u8| -> i64 {
            run_with_setup(
                vec![Bytecode::LEFT as u8, take, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![9u8; 64])).unwrap();
                },
            )
        };

        assert_eq!(run(21), run(20), "LEFT currently meters by input size");
    }

    #[test]
    fn byte_dynamic_gas_uses_source_value_size_not_index_size() {
        use crate::rt::Bytecode;

        let run = |buf_len: usize, idx: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::BYTE as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![9u8; buf_len])).unwrap();
                    ops.push(idx).unwrap();
                },
            )
        };

        assert_eq!(run(21, Value::U8(0)), run(20, Value::U8(0)) + 1);
        assert_eq!(run(64, Value::U8(0)), run(64, Value::U16(0)));
    }

    #[test]
    fn actview_meters_input_and_return_bytes() {
        use crate::rt::Bytecode;

        // view idx=1 -> returns Bytes; input body comes from stack top bytes.
        let run = |input_len: usize, ret_len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::ACTVIEW as u8, 1, Bytecode::END as u8],
                DummyHost {
                    act_res: vec![0u8; ret_len],
                    ..Default::default()
                },
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![9u8; input_len])).unwrap();
                },
            )
        };

        assert_eq!(run(17, 0), run(16, 0));
        assert_eq!(run(0, 17), run(0, 16));
    }

    #[test]
    fn action_meters_input_body_bytes() {
        use crate::rt::Bytecode;

        // action idx=1 is valid in allowlist.
        let run = |input_len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::ACTION as u8, 1, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![7u8; input_len])).unwrap();
                },
            )
        };

        assert_eq!(run(11), run(10));
        assert_eq!(run(21), run(20));
    }

    #[test]
    fn action_host_error_still_charges_input_dynamic_gas() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            act_err: Some("mock action recoverable fail".to_owned()),
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        let codes = vec![Bytecode::ACTION as u8, 1, Bytecode::END as u8];
        operands.push(Value::Bytes(vec![7u8; 21])).unwrap();
        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);

        let res = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &gas_table,
            &gas_extra,
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );

        assert!(matches!(
            res,
            Err(ItrErr(ItrErrCode::ActCallRevert, _)) | Err(ItrErr(ItrErrCode::OutOfGas, _))
        ));
        let used = 1000 - gas;
        let expected = gas_table.gas(Bytecode::ACTION as u8) + gas_extra.act_bytes(21);
        assert_eq!(used, expected);
    }

    #[test]
    fn action_host_error_out_of_gas_has_higher_priority() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut host = DummyHost {
            act_err: Some("mock action recoverable fail".to_owned()),
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        let codes = vec![Bytecode::ACTION as u8, 1, Bytecode::END as u8];
        operands.push(Value::Bytes(vec![7u8; 21])).unwrap();
        let gas_table = GasTable::new(1);
        let gas_extra = GasExtra::new(1);
        let expected = gas_table.gas(Bytecode::ACTION as u8) + gas_extra.act_bytes(21);
        let mut gas = expected - 1;

        let res = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &gas_table,
            &gas_extra,
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );

        assert!(res.is_err());
    }

    #[test]
    fn bytes_can_be_used_as_action_and_nt_call_param() {
        use crate::native::NativeFunc;
        use crate::rt::Bytecode;

        let cadr = ContractAddress::default();

        {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            operands.push(Value::Bytes(vec![8, 7])).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let codes = vec![Bytecode::ACTION as u8, 1, Bytecode::END as u8];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            assert_eq!(host.act_body, vec![8, 7]);
        }

        {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            operands.push(Value::Bytes(vec![8, 7])).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let codes = vec![
                Bytecode::NTFUNC as u8,
                NativeFunc::idx_sha2,
                Bytecode::END as u8,
            ];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                operands.pop().unwrap(),
                Value::Bytes(sys::sha2(&[8, 7]).to_vec())
            );
        }

        {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            operands.push(Value::Bytes(vec![8, 7])).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let codes = vec![
                Bytecode::NTFUNC as u8,
                NativeFunc::idx_keccak256,
                Bytecode::END as u8,
            ];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                operands.pop().unwrap(),
                Value::Bytes(sys::keccak256(&[8, 7]).to_vec())
            );
        }

        {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            operands.push(Value::Bytes(vec![8, 7])).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let codes = vec![
                Bytecode::NTFUNC as u8,
                NativeFunc::idx_blake2s256,
                Bytecode::END as u8,
            ];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                operands.pop().unwrap(),
                Value::Bytes(sys::blake2s256(&[8, 7]).to_vec())
            );
        }

        {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            operands.push(Value::Bytes(vec![8, 7])).unwrap();
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let codes = vec![
                Bytecode::NTFUNC as u8,
                NativeFunc::idx_blake2b256,
                Bytecode::END as u8,
            ];

            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                operands.pop().unwrap(),
                Value::Bytes(sys::blake2b256(&[8, 7]).to_vec())
            );
        }
    }

    #[test]
    fn hread_uses_start_length_stack_order() {
        use crate::rt::Bytecode;

        let cadr = ContractAddress::default();
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        heap.grow(1, &GasExtra::new(1)).unwrap();
        heap.write(0, Value::Bytes(vec![9, 8, 7, 6])).unwrap();
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let codes = vec![
            Bytecode::HREAD as u8,
            Bytecode::ACTION as u8,
            1,
            Bytecode::END as u8,
        ];
        operands.push(Value::U32(1)).unwrap(); // start
        operands.push(Value::U16(2)).unwrap(); // length

        execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert_eq!(host.act_body, vec![8, 7]);
    }

    #[test]
    fn hread_dynamic_gas_uses_read_length() {
        use crate::rt::Bytecode;

        let run = |len: u16| -> i64 {
            run_with_setup(
                vec![Bytecode::HREAD as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, heap, _global_map, _memory_map, _cadr| {
                    heap.grow(1, &GasExtra::new(1)).unwrap();
                    ops.push(Value::U16(0)).unwrap(); // start
                    ops.push(Value::U16(len)).unwrap(); // len
                },
            )
        };

        assert_eq!(run(17), run(16) + 1);
    }

    #[test]
    fn hwrite_dynamic_gas_uses_value_size() {
        use crate::rt::Bytecode;

        let run = |v: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::HWRITE as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, heap, _global_map, _memory_map, _cadr| {
                    heap.grow(1, &GasExtra::new(1)).unwrap();
                    ops.push(Value::U16(0)).unwrap(); // start
                    ops.push(v).unwrap(); // value
                },
            )
        };

        // heap write byte/12
        let gas_u8 = run(Value::U8(1)); // 1/12 = 0
        let gas_16 = run(Value::U128(1)); // 16/12 = 1
        assert_eq!(gas_16, gas_u8 + 1);
    }

    #[test]
    fn hwrite_accepts_u32_runtime_offsets() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1_000_000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(300);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        for _ in 0..17 {
            heap.grow(16, &GasExtra::new(1)).unwrap();
        }

        let start = 66_000u32;
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        operands.push(Value::U32(start)).unwrap();
        operands.push(Value::Bytes(bytes.clone())).unwrap();

        execute_code(
            &mut pc,
            &[Bytecode::HWRITE as u8, Bytecode::END as u8],
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert_eq!(
            heap.do_read(start as usize, bytes.len()).unwrap(),
            Value::Bytes(bytes)
        );
    }

    #[test]
    fn log_dynamic_gas_uses_sum_of_value_bytes() {
        use crate::rt::Bytecode;

        let run = |data_len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::LOG2 as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    // LOG2 pops 3 values: topic1, topic2, data.
                    ops.push(Value::U8(1)).unwrap();
                    ops.push(Value::U8(2)).unwrap();
                    ops.push(Value::Bytes(vec![0u8; data_len])).unwrap();
                },
            )
        };

        // log byte/1
        assert!(run(10) >= run(9));
        assert!(run(100) >= run(99));
    }

    #[test]
    fn sget_dynamic_gas_uses_status_gas_extra() {
        use crate::rt::Bytecode;

        let run = |retv: Value, gst: GasExtra| -> i64 {
            run_with_setup_gas_extra(
                vec![Bytecode::SGET as u8, Bytecode::END as u8],
                DummyHost {
                    sget_res: Some(retv),
                    ..Default::default()
                },
                gst,
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap();
                },
            )
            .0
        };

        let mut gst = GasExtra::new(1);
        gst.status_read_byte_mul = 4;
        let base =
            GasTable::new(1).gas(Bytecode::SGET as u8) + GasTable::new(1).gas(Bytecode::END as u8);

        let gas_8 = run(Value::Bytes(vec![0u8; 8]), gst.clone());
        let gas_9 = run(Value::Bytes(vec![0u8; 9]), gst.clone());
        let gas_nil = run(Value::Nil, gst.clone());
        assert_eq!(gas_9, gas_8 + gst.status_read_byte_mul);
        assert_eq!(gas_8 - base, gst.status_read(8));
        assert_eq!(gas_nil - base, 0);
    }

    #[test]
    fn sput_dynamic_gas_uses_status_gas_extra() {
        use crate::rt::Bytecode;

        let run = |klen: usize, vlen: usize, gst: GasExtra| -> i64 {
            run_with_setup_gas_extra(
                vec![Bytecode::SPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                gst,
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Bytes(vec![1u8; klen])).unwrap();
                    ops.push(Value::Bytes(vec![2u8; vlen])).unwrap();
                },
            )
            .0
        };

        let mut gst = GasExtra::new(1);
        gst.status_write_key_byte_mul = 8;
        gst.status_write_value_byte_mul = 8;
        let base =
            GasTable::new(1).gas(Bytecode::SPUT as u8) + GasTable::new(1).gas(Bytecode::END as u8);

        let gas_33 = run(3, 3, gst.clone());
        let gas_34 = run(3, 4, gst.clone());
        let gas_44 = run(4, 4, gst.clone());
        assert_eq!(gas_34, gas_33 + gst.status_write_value_byte_mul);
        assert_eq!(gas_44, gas_34 + gst.status_write_key_byte_mul);
        assert_eq!(gas_34 - base, gst.status_write(3, 4));
    }

    #[test]
    fn sload_dynamic_gas_uses_return_value_size() {
        use crate::rt::Bytecode;

        let run = |retv: Value| -> i64 {
            run_with_setup(
                vec![Bytecode::SLOAD as u8, Bytecode::END as u8],
                DummyHost {
                    sload_res: Some(retv),
                    ..Default::default()
                },
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::U8(1)).unwrap(); // key placeholder
                },
            )
        };

        let gas_8 = run(Value::Bytes(vec![0u8; 8]));
        let gas_9 = run(Value::Bytes(vec![0u8; 9]));
        assert_eq!(gas_9, gas_8 + 1);
    }

    #[test]
    fn sget_is_disabled_in_pure_mode() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            sget_res: Some(Value::U8(1)),
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        operands.push(Value::U8(1)).unwrap();

        let res = execute_code(
            &mut pc,
            &[Bytecode::SGET as u8, Bytecode::END as u8],
            ExecCtx::pure(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );
        assert!(matches!(res, Err(ItrErr(ItrErrCode::InstDisabled, _))));
    }

    #[test]
    fn sput_is_disabled_in_view_mode() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost {
            gas_remaining: 1000,
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        operands.push(Value::Bytes(vec![1])).unwrap();
        operands.push(Value::Bytes(vec![2])).unwrap();

        let res = execute_code(
            &mut pc,
            &[Bytecode::SPUT as u8, Bytecode::END as u8],
            ExecCtx::view(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );
        assert!(matches!(res, Err(ItrErr(ItrErrCode::InstDisabled, _))));
    }

    #[test]
    fn sedit_rebate_is_applied_after_step_charge_succeeds() {
        use crate::rt::Bytecode;

        let (_gas_used, host) = run_with_setup_host(
            vec![Bytecode::SEDIT as u8, Bytecode::END as u8],
            DummyHost {
                sedit_res: Some((7, 5)),
                ..Default::default()
            },
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(Value::U8(1)).unwrap();
                ops.push(Value::Bytes(vec![8])).unwrap();
            },
        );

        assert_eq!(host.gas_rebated, 5);
    }

    #[test]
    fn sedit_rebate_is_not_applied_when_step_runs_out_of_gas() {
        use crate::rt::Bytecode;

        let codes = vec![Bytecode::SEDIT as u8, Bytecode::END as u8];
        let mut pc: usize = 0;
        let mut gas: i64 = 1;
        let mut host = DummyHost {
            gas_remaining: 1,
            sedit_res: Some((10, 6)),
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        operands.push(Value::U8(1)).unwrap();
        operands.push(Value::Bytes(vec![8])).unwrap();

        let res = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );

        assert!(matches!(res, Err(ItrErr(ItrErrCode::OutOfGas, _))));
        assert_eq!(host.gas_rebated, 0);
    }

    #[test]
    fn sdel_rebate_is_not_applied_when_step_runs_out_of_gas() {
        use crate::rt::Bytecode;

        let codes = vec![Bytecode::SDEL as u8, Bytecode::END as u8];
        let mut pc: usize = 0;
        let mut gas: i64 = 1;
        let mut host = DummyHost {
            gas_remaining: 1,
            sdel_res: Some(9),
            ..Default::default()
        };
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        operands.push(Value::U8(1)).unwrap();

        let res = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        );

        assert!(matches!(res, Err(ItrErr(ItrErrCode::OutOfGas, _))));
        assert_eq!(host.gas_rebated, 0);
    }

    #[test]
    fn keys_and_clone_compo_bytes_include_map_key_bytes() {
        use crate::rt::Bytecode;
        use std::collections::BTreeMap;

        let map_with_key_len = |klen: usize| -> Value {
            let mut m = BTreeMap::new();
            m.insert(vec![7u8; klen], Value::U8(1));
            Value::Compo(CompoItem::map(m).unwrap())
        };

        let gas_keys_40 = run_with_setup(
            vec![Bytecode::KEYS as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(map_with_key_len(40)).unwrap();
            },
        );
        let gas_keys_41 = run_with_setup(
            vec![Bytecode::KEYS as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(map_with_key_len(41)).unwrap();
            },
        );
        assert_eq!(
            gas_keys_41,
            gas_keys_40 + 1,
            "KEYS byte/40 should include key bytes"
        );

        let gas_clone_39 = run_with_setup(
            vec![Bytecode::CLONE as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(map_with_key_len(39)).unwrap(); // 39(key)+1(val)=40
            },
        );
        let gas_clone_40 = run_with_setup(
            vec![Bytecode::CLONE as u8, Bytecode::END as u8],
            DummyHost::default(),
            |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                ops.push(map_with_key_len(40)).unwrap(); // 40+1=41
            },
        );
        assert_eq!(
            gas_clone_40,
            gas_clone_39 + 1,
            "CLONE byte/40 should include key bytes"
        );
    }

    #[test]
    fn alloc_uses_local_slot_cost() {
        use crate::rt::Bytecode;

        let run = |n: u8| -> i64 {
            run_with_setup(
                vec![Bytecode::ALLOC as u8, n, Bytecode::END as u8],
                DummyHost::default(),
                |_ops, _locals, _heap, _global_map, _memory_map, _cadr| {},
            )
        };

        // local alloc: 5 gas per slot
        assert!(run(3) > run(2));
        assert!(run(10) > run(3));
    }

    #[test]
    fn uplist_space_write_charges_like_put_per_item() {
        use crate::rt::Bytecode;
        use std::collections::VecDeque;

        let run = |item_len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::UNPACK as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, locals, _heap, _global_map, _memory_map, _cadr| {
                    locals.alloc(2).unwrap();
                    let list = Value::Compo(
                        CompoItem::list(VecDeque::from(vec![
                            Value::Bytes(vec![0u8; item_len]),
                            Value::Bytes(vec![0u8; item_len]),
                        ]))
                        .unwrap(),
                    );
                    ops.push(list).unwrap();
                    ops.push(Value::U8(0)).unwrap();
                },
            )
        };

        assert_eq!(run(29), run(28) + 2);
    }

    #[test]
    fn zero_length_byte_slice_ops_are_allowed() {
        let mut v = Value::Bytes(vec![1, 2, 3]);
        v.cutleft(0).unwrap();
        assert_eq!(v, Value::Bytes(vec![]));

        let mut v = Value::Bytes(vec![1, 2, 3]);
        v.cutright(0).unwrap();
        assert_eq!(v, Value::Bytes(vec![]));

        let mut v = Value::Bytes(vec![1, 2, 3]);
        v.cutout(Value::U8(0), Value::U8(2)).unwrap();
        assert_eq!(v, Value::Bytes(vec![]));

        let mut v = Value::Bytes(vec![1, 2, 3]);
        v.dropleft(0).unwrap();
        assert_eq!(v, Value::Bytes(vec![1, 2, 3]));

        let mut v = Value::Bytes(vec![1, 2, 3]);
        v.dropright(0).unwrap();
        assert_eq!(v, Value::Bytes(vec![1, 2, 3]));
    }

    #[test]
    fn unpack_rejects_oversize_list_value_by_spacecap() {
        use crate::rt::Bytecode;
        use std::collections::VecDeque;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();

        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        let mut locals = Stack::new(256);
        locals.alloc(2).unwrap();
        locals.save(0, Value::U8(7)).unwrap();
        locals.save(1, Value::U8(9)).unwrap();

        let mut operands = Stack::new(256);
        let list = Value::Compo(
            CompoItem::list(VecDeque::from(vec![
                Value::U8(1),
                Value::Bytes(vec![0u8; 1281]),
            ]))
            .unwrap(),
        );
        operands.push(list).unwrap();
        operands.push(Value::U8(0)).unwrap();

        let codes = vec![Bytecode::UNPACK as u8, Bytecode::END as u8];
        let err = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::OutOfValueSize);
    }

    #[test]
    fn unpack_accepts_tuple_wrapper() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();

        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        let mut locals = Stack::new(256);
        locals.alloc(2).unwrap();

        let mut operands = Stack::new(256);
        let args = Value::Tuple(
            TupleItem::new(vec![Value::Compo(CompoItem::new_list()), Value::U16(9)]).unwrap(),
        );
        operands.push(args).unwrap();
        operands.push(Value::U8(0)).unwrap();

        let codes = vec![Bytecode::UNPACK as u8, Bytecode::END as u8];
        let exit = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();

        assert!(matches!(exit, crate::rt::CallExit::Finish));
        assert!(matches!(locals.load(0).unwrap(), Value::Compo(_)));
        assert_eq!(locals.load(1).unwrap(), Value::U16(9));
    }

    #[test]
    fn unpack_rejects_oversize_tuple_value_by_spacecap() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();

        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        let mut locals = Stack::new(256);
        locals.alloc(2).unwrap();

        let mut operands = Stack::new(256);
        let args = Value::Tuple(
            TupleItem::new(vec![Value::U8(1), Value::Bytes(vec![0u8; 1281])]).unwrap(),
        );
        operands.push(args).unwrap();
        operands.push(Value::U8(0)).unwrap();

        let codes = vec![Bytecode::UNPACK as u8, Bytecode::END as u8];
        let err = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::OutOfValueSize);
    }

    #[test]
    fn unpack_rejects_nested_container_over_cap() {
        use crate::rt::Bytecode;
        use std::collections::VecDeque;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();

        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();

        let mut locals = Stack::new(256);
        locals.alloc(1).unwrap();

        let mut operands = Stack::new(256);
        let nested =
            Value::Compo(CompoItem::list(VecDeque::from([Value::U8(1), Value::U8(2)])).unwrap());
        let args = Value::Tuple(TupleItem::new(vec![nested]).unwrap());
        operands.push(args).unwrap();
        operands.push(Value::U8(0)).unwrap();

        let mut cap = SpaceCap::new(1);
        cap.compo_length = 1;
        let codes = vec![Bytecode::UNPACK as u8, Bytecode::END as u8];
        let err = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &cap,
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::OutOfCompoLen);
    }

    #[test]
    fn insert_gput_and_mput_reject_empty_keys() {
        use crate::rt::Bytecode;

        let run = |codes: Vec<u8>, setup: fn(&mut Stack)| {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            setup(&mut operands);
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
            .unwrap_err()
        };

        let insert_err = run(vec![Bytecode::INSERT as u8, Bytecode::END as u8], |ops| {
            ops.push(Value::Compo(CompoItem::new_map())).unwrap();
            ops.push(Value::Bytes(vec![])).unwrap();
            ops.push(Value::U8(7)).unwrap();
        });
        assert_eq!(insert_err.0, ItrErrCode::CastBeKeyFail);

        let gput_err = run(vec![Bytecode::GPUT as u8, Bytecode::END as u8], |ops| {
            ops.push(Value::Bytes(vec![])).unwrap();
            ops.push(Value::U8(7)).unwrap();
        });
        assert_eq!(gput_err.0, ItrErrCode::GlobalError);

        let mput_err = run(vec![Bytecode::MPUT as u8, Bytecode::END as u8], |ops| {
            ops.push(Value::Bytes(vec![])).unwrap();
            ops.push(Value::U8(7)).unwrap();
        });
        assert_eq!(mput_err.0, ItrErrCode::MemoryError);
    }

    #[test]
    fn compiler_generated_multi_param_prelude_accepts_list_sequence() {
        let codes = crate::lang::lang_to_bytecode("param { a b }\nend").unwrap();

        let run = |argv: Value| -> VmrtRes<crate::rt::CallExit> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            operands.push(argv).unwrap();
            execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )
        };

        let list = Value::Compo(
            CompoItem::list(std::collections::VecDeque::from([
                Value::U8(1),
                Value::U8(2),
            ]))
            .unwrap(),
        );
        assert!(matches!(run(list).unwrap(), crate::rt::CallExit::Finish));

        let tuple = Value::Tuple(TupleItem::new(vec![Value::U8(1), Value::U8(2)]).unwrap());
        assert!(matches!(run(tuple).unwrap(), crate::rt::CallExit::Finish));
    }

    #[test]
    fn args2list_converts_plain_args_and_charges_copy_gas() {
        use crate::rt::Bytecode;

        let run = |tail_len: usize| -> i64 {
            run_with_setup(
                vec![Bytecode::TUPLE2LIST as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, _memory_map, _cadr| {
                    ops.push(Value::Tuple(
                        TupleItem::new(vec![Value::U8(7), Value::Bytes(vec![0u8; tail_len])])
                            .unwrap(),
                    ))
                    .unwrap();
                },
            )
        };

        assert_eq!(run(40), run(39) + 1);
    }

    #[test]
    fn args2list_rejects_non_args_and_nested_compo_items() {
        use crate::rt::Bytecode;

        let run = |value: Value| -> VmrtRes<Value> {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();
            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);
            let cadr = ContractAddress::default();
            operands.push(value).unwrap();
            let codes = vec![Bytecode::TUPLE2LIST as u8, Bytecode::END as u8];
            let _ = execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            )?;
            operands.pop()
        };

        assert!(matches!(
            run(Value::U8(1)),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
        assert!(matches!(
            run(Value::Tuple(
                TupleItem::new(vec![Value::Compo(CompoItem::new_map())]).unwrap()
            )),
            Err(ItrErr(ItrErrCode::CastFail, _))
        ));
        let out = run(Value::Tuple(
            TupleItem::new(vec![Value::U8(1), Value::Bytes(vec![2])]).unwrap(),
        ))
        .unwrap();
        assert!(matches!(out, Value::Compo(_)));
    }

    #[test]
    fn args2list_checks_compo_cap() {
        use crate::rt::Bytecode;

        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        let mut cap = SpaceCap::new(1);
        cap.compo_length = 1;

        operands
            .push(Value::Tuple(
                TupleItem::new(vec![Value::U8(1), Value::U8(2)]).unwrap(),
            ))
            .unwrap();

        let codes = vec![Bytecode::TUPLE2LIST as u8, Bytecode::END as u8];
        let err = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &cap,
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap_err();

        assert_eq!(err.0, ItrErrCode::OutOfCompoLen);
    }

    #[test]
    fn dup_on_args_shares_storage_like_compo_clone() {
        use crate::rt::Bytecode;

        let seed = TupleItem::new(vec![Value::U8(7), Value::Bytes(vec![1, 2, 3])]).unwrap();
        let value = Value::Tuple(seed.clone());
        let mut pc: usize = 0;
        let mut gas: i64 = 1000;
        let mut host = DummyHost::default();
        let mut operands = Stack::new(256);
        let mut locals = Stack::new(256);
        let mut heap = Heap::new(64);
        let mut global_map = GKVMap::new(20);
        let mut memory_map = CtcKVMap::new(12);
        let cadr = ContractAddress::default();
        operands.push(value).unwrap();
        let codes = vec![Bytecode::DUP as u8, Bytecode::END as u8];
        let exit = execute_code(
            &mut pc,
            &codes,
            ExecCtx::main(),
            &mut operands,
            &mut locals,
            &mut heap,
            &cadr,
            &cadr,
            &mut gas,
            &GasTable::new(1),
            &GasExtra::new(1),
            &SpaceCap::new(1),
            &mut global_map,
            &mut memory_map,
            &mut host,
        )
        .unwrap();
        assert!(matches!(exit, crate::rt::CallExit::Finish));
        let Value::Tuple(top) = operands.pop().unwrap() else {
            panic!("must be args")
        };
        let Value::Tuple(bottom) = operands.pop().unwrap() else {
            panic!("must be args")
        };
        assert_eq!(seed.shared_count(), 3);
        assert_eq!(top.shared_count(), 3);
        assert_eq!(bottom.shared_count(), 3);
    }

    #[test]
    fn mput_charges_memory_key_cost_only_for_new_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x33u8]);
        let run = |preload: bool| -> i64 {
            run_with_setup(
                vec![Bytecode::MPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, _global_map, memory_map, cadr| {
                    if preload {
                        memory_map
                            .entry_mut(cadr)
                            .unwrap()
                            .put(key.clone(), Value::U8(1))
                            .unwrap();
                    }
                    ops.push(key.clone()).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(false), run(true) + 20);
    }

    #[test]
    fn gput_charges_global_key_cost_only_for_new_key() {
        use crate::rt::Bytecode;

        let key = Value::Bytes(vec![0x44u8]);
        let run = |preload: bool| -> i64 {
            run_with_setup(
                vec![Bytecode::GPUT as u8, Bytecode::END as u8],
                DummyHost::default(),
                |ops, _locals, _heap, global_map, _memory_map, _cadr| {
                    if preload {
                        global_map.put(key.clone(), Value::U8(1)).unwrap();
                    }
                    ops.push(key.clone()).unwrap();
                    ops.push(Value::U8(9)).unwrap();
                },
            )
        };

        assert_eq!(run(false), run(true) + 32);
    }

    #[test]
    fn merge_rejects_self_merge_after_dup() {
        use crate::rt::Bytecode;

        for init in [Bytecode::NEWLIST, Bytecode::NEWMAP] {
            let mut pc: usize = 0;
            let mut gas: i64 = 1000;
            let mut host = DummyHost::default();

            let mut operands = Stack::new(256);
            let mut locals = Stack::new(256);
            let mut heap = Heap::new(64);
            let mut global_map = GKVMap::new(20);
            let mut memory_map = CtcKVMap::new(12);

            let cadr = ContractAddress::default();
            let codes = vec![
                init as u8,
                Bytecode::DUP as u8,
                Bytecode::MERGE as u8,
                Bytecode::END as u8,
            ];
            let res = execute_code(
                &mut pc,
                &codes,
                ExecCtx::main(),
                &mut operands,
                &mut locals,
                &mut heap,
                &cadr,
                &cadr,
                &mut gas,
                &GasTable::new(1),
                &GasExtra::new(1),
                &SpaceCap::new(1),
                &mut global_map,
                &mut memory_map,
                &mut host,
            );
            assert!(matches!(res, Err(ItrErr(ItrErrCode::CompoOpInvalid, _))));
        }
    }

    #[test]
    fn shortcut_call_gas_matches_opcode_tiers() {
        use crate::rt::{
            CallTarget, EffectMode, ExecCtx, calc_func_sign, encode_call_body, encode_splice_body,
        };

        let sign = calc_func_sign("jump");
        let cases = [
            (
                {
                    let mut codes = vec![Bytecode::CALL as u8];
                    codes.extend_from_slice(&encode_call_body(
                        CallTarget::Ext(1),
                        EffectMode::Edit,
                        sign,
                    ));
                    codes
                },
                GasTable::new(1).gas(Bytecode::CALL as u8),
            ),
            (
                vec![Bytecode::CALLTHIS as u8, sign[0], sign[1], sign[2], sign[3]],
                GasTable::new(1).gas(Bytecode::CALLTHIS as u8),
            ),
            (
                vec![Bytecode::CALLSELF as u8, sign[0], sign[1], sign[2], sign[3]],
                GasTable::new(1).gas(Bytecode::CALLSELF as u8),
            ),
            (
                vec![
                    Bytecode::CALLSUPER as u8,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLSUPER as u8),
            ),
            (
                vec![
                    Bytecode::CALLSELFVIEW as u8,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLSELFVIEW as u8),
            ),
            (
                vec![
                    Bytecode::CALLSELFPURE as u8,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLSELFPURE as u8),
            ),
            (
                {
                    let mut codes = vec![Bytecode::CODECALL as u8];
                    codes.extend_from_slice(&encode_splice_body(1, sign));
                    codes
                },
                GasTable::new(1).gas(Bytecode::CODECALL as u8),
            ),
            (
                vec![
                    Bytecode::CALLUSEPURE as u8,
                    1,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLUSEPURE as u8),
            ),
            (
                vec![
                    Bytecode::CALLUSEVIEW as u8,
                    1,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLUSEVIEW as u8),
            ),
            (
                vec![
                    Bytecode::CALLEXTVIEW as u8,
                    1,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLEXTVIEW as u8),
            ),
            (
                vec![
                    Bytecode::CALLEXT as u8,
                    1,
                    sign[0],
                    sign[1],
                    sign[2],
                    sign[3],
                ],
                GasTable::new(1).gas(Bytecode::CALLEXT as u8),
            ),
        ];

        for (codes, expected) in cases {
            assert_eq!(run_call_opcode_gas(ExecCtx::external(), codes), expected);
        }
    }
}
