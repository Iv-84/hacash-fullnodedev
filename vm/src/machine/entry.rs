/*********************************/
/* VM entry request + guard state */

pub(crate) enum EntryRequest {
    Main {
        code_type: CodeType,
        codes: Arc<[u8]>,
    },
    P2sh {
        code_type: CodeType,
        state_addr: Address,
        libs: Vec<ContractAddress>,
        codes: ByteView,
        intent_scope: IntentScope,
        param: Value,
    },
    Abst {
        kind: AbstCall,
        contract_addr: ContractAddress,
        intent_scope: IntentScope,
        param: Value,
    },
}

impl EntryRequest {
    fn entry_kind(&self) -> EntryKind {
        match self {
            Self::Main { .. } => EntryKind::Main,
            Self::P2sh { .. } => EntryKind::P2sh,
            Self::Abst { .. } => EntryKind::Abst,
        }
    }

    fn call_base(&self, gas_extra: &GasExtra) -> i64 {
        self.entry_kind().call_base(gas_extra)
    }

    fn execute(
        self,
        machine: &mut Machine,
        runtime: &mut Runtime,
        ctx: &mut dyn Context,
    ) -> XRet<Value> {
        match self {
            Self::Main { code_type, codes } => machine.main_call(runtime, ctx, code_type, codes),
            Self::P2sh {
                code_type,
                state_addr,
                libs,
                codes,
                intent_scope,
                param,
            } => machine.p2sh_call(runtime, ctx, code_type, state_addr, libs, codes, intent_scope, param),
            Self::Abst {
                kind,
                contract_addr,
                intent_scope,
                param,
            } => machine.abst_call(runtime, ctx, kind, contract_addr, intent_scope, param),
        }
    }
}

struct VmEntryFrame {
    kind: EntryKind,
    gas_base: VmGasBuckets,
    /// Per-entry compute surcharge (`GasExtra::*_call_base`), added on top of measured work.
    call_base: i64,
}

struct VmEntryGuard {
    entries: *mut Vec<VmEntryFrame>,
    entry: VmEntryFrame,
    index: usize,
    armed: bool,
}

impl VmEntryGuard {
    fn push(
        entries: &mut Vec<VmEntryFrame>,
        max_reentry: u32,
        gas_base: VmGasBuckets,
        kind: EntryKind,
        call_base: i64,
    ) -> Ret<Self> {
        let next_level = entries
            .len()
            .checked_add(1)
            .ok_or_else(|| "re-entry level overflow".to_owned())?;
        if next_level as u32 > max_reentry + 1 {
            return Err(format!(
                "re-entry level {} exceeded limit {}",
                next_level - 1,
                max_reentry
            ));
        }
        let entry = VmEntryFrame {
            kind,
            gas_base,
            call_base,
        };
        let index = entries.len();
        entries.push(VmEntryFrame {
            kind: entry.kind,
            gas_base: entry.gas_base,
            call_base: entry.call_base,
        });
        Ok(Self {
            entries,
            entry,
            index,
            armed: true,
        })
    }

    fn entry(&self) -> VmEntryFrame {
        VmEntryFrame {
            kind: self.entry.kind,
            gas_base: self.entry.gas_base,
            call_base: self.entry.call_base,
        }
    }

    fn disarm(mut self) -> VmEntryFrame {
        // SAFETY: `entries` points to the current executor-owned entry stack captured at push time.
        // The guard never outlives that stack and only mutates it during unwind/disarm.
        let entries = unsafe { &mut *self.entries };
        let popped = entries
            .pop()
            .unwrap_or_else(|| panic!("vm entry frame missing during disarm"));
        debug_assert_eq!(entries.len(), self.index);
        debug_assert_eq!(popped.kind as u8, self.entry.kind as u8);
        self.armed = false;
        self.entry()
    }
}

impl Drop for VmEntryGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // SAFETY: same invariant as `disarm` — this guard only ever targets the executor entry stack it was pushed into.
        let entries = unsafe { &mut *self.entries };
        let popped = entries
            .pop()
            .expect("vm entry stack empty on guard drop");
        debug_assert_eq!(entries.len(), self.index);
        debug_assert_eq!(popped.kind as u8, self.entry.kind as u8);
    }
}

/// Bill this VM entry: measured buckets since `entry.gas_base`, plus `call_base` as compute surcharge (additive).
fn settle_vm_entry_return_cost(
    runtime: &mut Runtime,
    ctx: &mut dyn Context,
    entry: &VmEntryFrame,
) -> Result<VmGasBuckets, String> {
    let mut cost = runtime
        .gas_use()
        .checked_sub(entry.gas_base)
        .ok_or_else(|| {
            format!(
                "gas cost underflow: total={:?}, base={:?}",
                runtime.gas_use(),
                entry.gas_base
            )
        })?;
    if entry.call_base > 0 {
        runtime
            .settle_compute_gas(ctx, entry.call_base)
            .map_err(|e| e.to_string())?;
        cost.compute = cost.compute.saturating_add(entry.call_base);
    }
    if cost.total() <= 0 {
        return Err(format!(
            "{:?} gas cost invalid: {}",
            entry.kind,
            cost.total()
        ));
    }
    Ok(cost)
}

fn settle_vm_entry_return_cost_x(
    runtime: &mut Runtime,
    ctx: &mut dyn Context,
    entry: &VmEntryFrame,
) -> XRet<VmGasBuckets> {
    let mut cost = runtime
        .gas_use()
        .checked_sub(entry.gas_base)
        .ok_or_else(|| {
            XError::fault(format!(
                "gas cost underflow: total={:?}, base={:?}",
                runtime.gas_use(),
                entry.gas_base
            ))
        })?;
    if entry.call_base > 0 {
        runtime
            .settle_compute_gas(ctx, entry.call_base)
            .map_err(XError::from)?;
        cost.compute = cost.compute.saturating_add(entry.call_base);
    }
    if cost.total() <= 0 {
        xerr!(format!(
            "{:?} gas cost invalid: {}",
            entry.kind,
            cost.total()
        ))
    } else {
        Ok(cost)
    }
}

/*********************************/
/* Host-facing entry helpers */

fn ensure_vm_run_ready(ctx: &dyn Context) -> Rerr {
    const TY3: u8 = TransactionType3::TYPE;
    let txty = ctx.env().tx.ty;
    if txty < TY3 {
        return errf!(
            "current transaction type {} too low to setup vm, requires at least {}",
            txty,
            TY3
        );
    }
    Ok(())
}

/// Falsy return => success. Non-falsy or object return => recoverable (XError::revert). Runtime-only values crossing the VM boundary are unrecoverable (XError::fault).
pub fn check_vm_return_value(rv: &Value, err_msg: &str) -> XRerr {
    rv.check_vm_boundary_retv()
        .map_err(|e| XError::fault(format!("{} return cannot cross VM boundary: {}", err_msg, e)))?;
    use Value::*;
    let detail: Option<String> = match rv {
        Nil => None,
        Bool(false) => None,
        Bool(true) => Some("code 1".to_owned()),
        U8(n) => (*n != 0).then(|| format!("code {}", n)),
        U16(n) => (*n != 0).then(|| format!("code {}", n)),
        U32(n) => (*n != 0).then(|| format!("code {}", n)),
        U64(n) => (*n != 0).then(|| format!("code {}", n)),
        U128(n) => (*n != 0).then(|| format!("code {}", n)),
        Bytes(buf) => maybe!(
            buf_is_empty_or_all_zero(buf),
            None,
            Some(match ascii_show_string(buf) {
                Some(s) => format!("bytes {:?}", s),
                None => format!("bytes 0x{}", buf.to_hex()),
            })
        ),
        Value::Address(a) => maybe!(
            buf_is_empty_or_all_zero(a.as_bytes()),
            None,
            Some(format!("address {}", a.to_readable()))
        ),
        Handle(_) => never!(),
        Tuple(_) | Compo(_) => Some(format!("object {}", rv.to_json())),
    };
    match detail {
        None => Ok(()),
        Some(d) => Err(XError::revert(format!("{} return error {}", err_msg, d))),
    }
}

pub fn run_main_entry(
    ctx: &mut dyn Context,
    codeconf: u8,
    payload: Vec<u8>,
) -> XRet<(VmGasBuckets, Value)> {
    // Bytecode verification is intentionally handled by upper-layer action validators before calling run_main_entry.
    ensure_vm_run_ready(ctx)?;
    let (cost, rv) = ctx.vm_call(Box::new(EntryRequest::Main {
        code_type: CodeConf::parse(codeconf)?.code_type(),
        codes: Arc::from(payload),
    }))?;
    let Ok(rv) = rv.downcast::<Value>() else {
        return xerr!("vm call return type mismatch");
    };
    Ok((cost, *rv))
}

fn extract_p2sh_witness(param: &Value) -> Ret<Vec<u8>> {
    match param {
        Value::Bytes(witness) => Ok(witness.clone()),
        Value::Tuple(items) => {
            let Some(first) = items.as_slice().first() else {
                return errf!("p2sh param tuple is empty");
            };
            let Value::Bytes(witness) = first else {
                return errf!("p2sh witness must be the first tuple item as bytes");
            };
            Ok(witness.clone())
        }
        _ => errf!("p2sh param must be bytes or a tuple starting with witness bytes"),
    }
}

pub(crate) fn run_p2sh_entry(
    ctx: &mut dyn Context,
    codeconf: u8,
    payload: Vec<u8>,
    param: Value,
) -> XRet<(VmGasBuckets, Value)> {
    // Canonical P2SH payload validation still happens in upper-layer action validators,
    // but this raw host-facing helper also revalidates the parsed payload to avoid trusting forged blobs.
    ensure_vm_run_ready(ctx)?;
    let hei = ctx.env().block.height;
    let (gst, cap) = peek_vm_runtime_limits(ctx, hei);
    let codeconf = CodeConf::parse(codeconf)?;
    let witness = BytesW2::from(extract_p2sh_witness(&param)?)?;
    let payload = ByteView::from_arc(Arc::from(payload));
    let payload_ref = payload.as_slice();
    let (state_addr, mv1) = Address::create(payload_ref)?;
    state_addr.must_scriptmh()?;
    let (libs, mv2) = ContractAddressW1::create(&payload_ref[mv1..])?;
    let mv = mv1 + mv2;
    let codes = BytesW2::from(payload_ref[mv..].to_vec())?;
    crate::action::P2SHScriptProve::verify_unlock_inputs(hei, &gst, &cap, &libs, codeconf, &codes, &witness)?;
    let intent_scope = ctx.vm_current_intent_scope();
    let (cost, rv) = ctx.vm_call(Box::new(EntryRequest::P2sh {
        code_type: codeconf.code_type(),
        state_addr,
        libs: libs.into_list(),
        codes: payload.slice(mv, payload.len())?,
        intent_scope,
        param,
    }))?;
    let Ok(rv) = rv.downcast::<Value>() else {
        return xerr!("vm call return type mismatch");
    };
    Ok((cost, *rv))
}

pub fn run_abst_entry(
    ctx: &mut dyn Context,
    target: AbstCall,
    payload: Address,
    param: Value,
) -> XRet<(VmGasBuckets, Value)> {
    ensure_vm_run_ready(ctx)?;
    let intent_scope = ctx.vm_current_intent_scope();
    let (cost, rv) = ctx.vm_call(Box::new(EntryRequest::Abst {
        kind: target,
        contract_addr: ContractAddress::from_addr(payload)?,
        intent_scope,
        param,
    }))?;
    let Ok(rv) = rv.downcast::<Value>() else {
        return xerr!("vm call return type mismatch");
    };
    Ok((cost, *rv))
}

/*********************************/
/* Entry-specific machine wrappers */

impl Machine {
    pub fn main_call<H: VmHost + ?Sized>(
        &mut self,
        runtime: &mut Runtime,
        host: &mut H,
        ctype: CodeType,
        codes: Arc<[u8]>,
    ) -> XRet<Value> {
        validate_vm_entry_param(runtime, &EntryKind::Main.root_exec(), None)?;
        let rv = self.main_call_raw(runtime, host, ctype, codes)?;
        validate_vm_entry_return(EntryKind::Main, &rv, "main call")?;
        Ok(rv)
    }

    pub fn abst_call<H: VmHost + ?Sized>(
        &mut self,
        runtime: &mut Runtime,
        host: &mut H,
        cty: AbstCall,
        contract_addr: ContractAddress,
        intent_scope: IntentScope,
        param: Value,
    ) -> XRet<Value> {
        let exec = EntryKind::Abst.root_exec();
        validate_vm_entry_param(runtime, &exec, Some(&param))?;
        let adr = contract_addr.to_readable();
        let Some(hit) = runtime
            .resolve_abstfn(host, &contract_addr, cty)
            .map_err(XError::from)?
        else {
            return xerr!(format!("abst call {:?} not found in {}", cty, adr));
        };
        let defer_auth = cty.can_register_defer().then(|| contract_addr.clone());
        let old_defer_auth = runtime
            .volatile
            .deferred_registry
            .replace_defer_auth(defer_auth);
        let rv = self.do_call(
            runtime,
            host,
            exec,
            hit.fnobj.as_ref(),
            FrameBindings::contract(contract_addr, hit.owner, hit.lib_table)
                .with_intent_scope(intent_scope),
            Some(param),
        );
        runtime
            .volatile
            .deferred_registry
            .replace_defer_auth(old_defer_auth);
        let rv = rv.map_err(XError::from)?;
        validate_vm_entry_return(EntryKind::Abst, &rv, &format!("call {}.{:?}", adr, cty))?;
        Ok(rv)
    }

    pub fn p2sh_call<H: VmHost + ?Sized>(
        &mut self,
        runtime: &mut Runtime,
        host: &mut H,
        ctype: CodeType,
        p2sh_addr: Address,
        libs: Vec<ContractAddress>,
        codes: ByteView,
        intent_scope: IntentScope,
        param: Value,
    ) -> XRet<Value> {
        let exec = EntryKind::P2sh.root_exec();
        validate_vm_entry_param(runtime, &exec, Some(&param))?;
        let fnobj = FnObj::plain(ctype, codes, 0, None);
        let rv = self.do_call(
            runtime,
            host,
            exec,
            &fnobj,
            FrameBindings::root(
                p2sh_addr,
                libs.into_iter()
                    .map(|addr| addr.into_addr())
                    .collect::<Vec<_>>()
                    .into(),
            )
            .with_intent_scope(intent_scope),
            Some(param),
        ).map_err(XError::from)?;
        validate_vm_entry_return(EntryKind::P2sh, &rv, "p2sh call")?;
        Ok(rv)
    }
}

/*********************************/
/* VM trait entry bridge */

impl VM for Executor {
    fn current_intent_scope(&mut self) -> Option<Option<usize>> {
        self.machine.current_intent_scope()
    }

    fn runtime_config(&mut self) -> Option<Box<dyn Any>> {
        self.runtime_config_any()
    }

    fn snapshot_volatile(&mut self) -> Box<dyn Any> {
        self.snapshot_volatile_state()
    }

    fn restore_volatile(&mut self, snap: Box<dyn Any>) {
        self.restore_volatile_state(snap)
    }

    fn rollback_volatile_preserve_warm_and_gas(&mut self) {
        self.rollback_volatile_state_preserve_warm_and_gas()
    }

    fn invalidate_contract_cache(&mut self, addr: &Address) {
        self.invalidate_runtime_contract_cache(addr)
    }

    fn drain_deferred(&mut self, ctx: &mut dyn Context) -> Rerr {
        self.run_deferred_entries(ctx)
    }

    fn call(&mut self, ctx: &mut dyn Context, req: Box<dyn Any>) -> XRet<(VmGasBuckets, Box<dyn Any>)> {
        self.dispatch_entry_call(ctx, req)
    }
}

/*********************************/
/* Executor public entry APIs */

impl Executor {
    pub fn sandbox_main_call_raw(
        &mut self,
        ctx: &mut dyn Context,
        ctype: CodeType,
        codes: Arc<[u8]>,
    ) -> Ret<Value> {
        let (_, ret_val) = self.raw_main_entry(ctx, ctype, codes)?;
        Ok(ret_val)
    }

    pub fn main_call(
        &mut self,
        ctx: &mut dyn Context,
        ctype: CodeType,
        codes: Arc<[u8]>,
    ) -> XRet<Value> {
        let req = EntryRequest::Main { code_type: ctype, codes };
        let (_, retv) = self.execute_entry_req(ctx, req)?;
        Ok(retv)
    }

    pub fn p2sh_call(
        &mut self,
        ctx: &mut dyn Context,
        ctype: CodeType,
        state_addr: Address,
        libs: Vec<ContractAddress>,
        codes: ByteView,
        intent_scope: IntentScope,
        param: Value,
    ) -> XRet<Value> {
        let req = EntryRequest::P2sh {
            code_type: ctype,
            state_addr,
            libs,
            codes,
            intent_scope,
            param,
        };
        let (_, retv) = self.execute_entry_req(ctx, req)?;
        Ok(retv)
    }

    pub fn abst_call(
        &mut self,
        ctx: &mut dyn Context,
        kind: AbstCall,
        contract_addr: ContractAddress,
        intent_scope: IntentScope,
        param: Value,
    ) -> XRet<Value> {
        let req = EntryRequest::Abst {
            kind,
            contract_addr,
            intent_scope,
            param,
        };
        let (_, retv) = self.execute_entry_req(ctx, req)?;
        Ok(retv)
    }

    /*********************************/
    /* Executor internal entry pipeline */

    fn append_secondary_message(primary: String, secondary: String) -> String {
        if secondary.is_empty() || primary.contains(&secondary) {
            primary
        } else {
            format!("{} | secondary: {}", primary, secondary)
        }
    }

    fn merge_ret_failure(exec_err: String, settle_err: String) -> String {
        Self::append_secondary_message(exec_err, settle_err)
    }

    fn merge_xret_failure(exec_err: XError, settle_err: XError) -> XError {
        let exec_is_revert = exec_err.is_revert();
        let settle_is_revert = settle_err.is_revert();
        if exec_is_revert && !settle_is_revert {
            match settle_err {
                sys::ExecError::Revert(msg) => XError::fault(Self::append_secondary_message(msg, exec_err.to_string())),
                sys::ExecError::Fault(msg) => XError::fault(Self::append_secondary_message(msg, exec_err.to_string())),
            }
        } else {
            match exec_err {
                sys::ExecError::Revert(msg) => XError::revert(Self::append_secondary_message(msg, settle_err.to_string())),
                sys::ExecError::Fault(msg) => XError::fault(Self::append_secondary_message(msg, settle_err.to_string())),
            }
        }
    }

    fn run_vm_entry_ret<T>(
        &mut self,
        ctx: &mut dyn Context,
        kind: EntryKind,
        call_base: i64,
        preserve_exec_from: bool,
        execute: impl FnOnce(&mut Machine, &mut Runtime, &mut dyn Context) -> Ret<T>,
    ) -> Ret<(VmGasBuckets, T)> {
        let guard = VmEntryGuard::push(
            &mut self.entries,
            self.runtime.warm.space_cap.reentry_level,
            self.runtime.gas_use(),
            kind,
            call_base,
        )?;
        let result = if preserve_exec_from {
            execute(&mut self.machine, &mut self.runtime, ctx)
        } else {
            basis::interface::with_exec_from(ctx, basis::component::ExecFrom::Call, |ctx| {
                execute(&mut self.machine, &mut self.runtime, ctx)
            })
        };
        let entry = guard.disarm();
        let settle = settle_vm_entry_return_cost(&mut self.runtime, ctx, &entry);
        match (result, settle) {
            (Err(exec_err), Err(settle_err)) => Err(Self::merge_ret_failure(exec_err, settle_err)),
            (Err(exec_err), _) => Err(exec_err),
            (Ok(_), Err(settle_err)) => Err(settle_err),
            (Ok(retv), Ok(cost)) => Ok((cost, retv)),
        }
    }

    fn run_vm_entry_xret<T>(
        &mut self,
        ctx: &mut dyn Context,
        kind: EntryKind,
        call_base: i64,
        preserve_exec_from: bool,
        execute: impl FnOnce(&mut Machine, &mut Runtime, &mut dyn Context) -> XRet<T>,
    ) -> XRet<(VmGasBuckets, T)> {
        let guard = VmEntryGuard::push(
            &mut self.entries,
            self.runtime.warm.space_cap.reentry_level,
            self.runtime.gas_use(),
            kind,
            call_base,
        )
        .map_err(XError::fault)?;
        let result = if preserve_exec_from {
            execute(&mut self.machine, &mut self.runtime, ctx)
        } else {
            basis::interface::with_exec_from(ctx, basis::component::ExecFrom::Call, |ctx| {
                execute(&mut self.machine, &mut self.runtime, ctx)
            })
        };
        let entry = guard.disarm();
        let settle = settle_vm_entry_return_cost_x(&mut self.runtime, ctx, &entry);
        match (result, settle) {
            (Err(exec_err), Err(settle_err)) => Err(Self::merge_xret_failure(exec_err, settle_err)),
            (Err(exec_err), _) => Err(exec_err),
            (Ok(_), Err(settle_err)) => Err(settle_err),
            (Ok(retv), Ok(cost)) => Ok((cost, retv)),
        }
    }

    pub(crate) fn raw_main_entry(
        &mut self,
        ctx: &mut dyn Context,
        ctype: CodeType,
        codes: Arc<[u8]>,
    ) -> Ret<(VmGasBuckets, Value)> {
        let call_base = EntryKind::Main.call_base(&self.runtime.warm.gas_extra);
        self.run_vm_entry_ret(
            ctx,
            EntryKind::Main,
            call_base,
            true,
            move |machine, runtime, ctx| machine.main_call_raw(runtime, ctx, ctype, codes),
        )
    }

    fn execute_entry_req(
        &mut self,
        ctx: &mut dyn Context,
        req: EntryRequest,
    ) -> XRet<(VmGasBuckets, Value)> {
        let kind = req.entry_kind();
        let call_base = req.call_base(&self.runtime.warm.gas_extra);
        self.run_vm_entry_xret(
            ctx,
            kind,
            call_base,
            false,
            move |machine, runtime, ctx| req.execute(machine, runtime, ctx),
        )
    }

    fn dispatch_entry_call(
        &mut self,
        ctx: &mut dyn Context,
        req: Box<dyn Any>,
    ) -> XRet<(VmGasBuckets, Box<dyn Any>)> {
        let Ok(req) = req.downcast::<EntryRequest>() else {
            return xerrf!("vm call request type mismatch");
        };
        let (cost, resv) = self.execute_entry_req(ctx, *req)?;
        Ok((cost, Box::new(resv)))
    }

    fn run_deferred_entries(&mut self, ctx: &mut dyn Context) -> Rerr {
        let callbacks = {
            // Deferred phase currently uses strict one-shot consumption: once drained, callbacks are consumed
            // even if a later deferred callback fails. This keeps deferred dispatch non-reentrant and matches the
            // existing transaction semantics where warmups/gas remain monotonic after the phase begins.
            self.runtime.volatile.deferred_registry.drain_lifo()
        };
        for caddr in callbacks {
            let _ = self
                .run_vm_entry_ret(
                    ctx,
                    EntryKind::Abst,
                    EntryKind::Abst.call_base(&self.runtime.warm.gas_extra),
                    false,
                    move |machine, runtime, ctx| {
                        EntryRequest::Abst {
                            kind: AbstCall::Deferred,
                            contract_addr: caddr.addr,
                            intent_scope: caddr.intent_scope,
                            param: Value::Nil,
                        }
                        .execute(machine, runtime, ctx)
                        .map_err(|e| e.to_string())
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/*********************************/
/* Entry boundary validation */

fn validate_vm_entry_param(
    runtime: &Runtime,
    exec: &ExecCtx,
    param: Option<&Value>,
) -> XRerr {
    exec.ensure_call_depth(&runtime.warm.space_cap).map_err(XError::from)?;
    if let Some(param) = param {
        param.check_vm_boundary_argv().map_err(XError::from)?;
        param
            .check_boundary_value_cap(&runtime.warm.space_cap)
            .map_err(XError::from)?;
        param
            .check_container_cap(&runtime.warm.space_cap)
            .map_err(XError::from)?;
    }
    Ok(())
}

fn validate_vm_entry_return(kind: EntryKind, rv: &Value, err_msg: &str) -> XRerr {
    match kind {
        EntryKind::Main | EntryKind::P2sh | EntryKind::Abst => check_vm_return_value(rv, err_msg),
    }
}

#[cfg(test)]
mod entry_tests {
    use super::*;
    use basis::component::{Env, ExecFrom, TexLedger};
    use basis::interface::{Context, Logs, State, StateOperat, TransactionRead};
    use field::{AddrOrPtr, Amount, Hash};
    use protocol::context::EmptyState;
    use std::sync::{Arc, Weak};
    use sys::XRet;

    #[derive(Default)]
    struct NoopLogs;
    impl Logs for NoopLogs {}

    #[derive(Default)]
    struct DummyState;
    impl State for DummyState {
        fn fork_sub(&self, _: Weak<Box<dyn State>>) -> Box<dyn State> {
            Box::new(Self)
        }
        fn merge_sub(&mut self, _: Box<dyn State>) {}
        fn detach(&mut self) {}
        fn clone_state(&self) -> Box<dyn State> {
            Box::new(Self)
        }
    }

    #[derive(Debug, Clone, Default)]
    struct DummyTx {
        main: Address,
        addrs: Vec<Address>,
    }

    impl field::Serialize for DummyTx {
        fn size(&self) -> usize {
            0
        }
        fn serialize(&self) -> Vec<u8> {
            vec![]
        }
    }

    impl basis::interface::TxExec for DummyTx {}

    impl TransactionRead for DummyTx {
        fn ty(&self) -> u8 {
            3
        }
        fn hash(&self) -> Hash {
            Hash::default()
        }
        fn hash_with_fee(&self) -> Hash {
            Hash::default()
        }
        fn main(&self) -> Address {
            self.main
        }
        fn addrs(&self) -> Vec<Address> {
            self.addrs.clone()
        }
        fn fee(&self) -> &Amount {
            Amount::zero_ref()
        }
        fn fee_purity(&self) -> u64 {
            1
        }
        fn gas_max_byte(&self) -> Option<u8> {
            Some(1)
        }
    }

    struct CaptureCtx {
        env: Env,
        tx: DummyTx,
        state: Box<dyn State>,
        logs: NoopLogs,
        tex: TexLedger,
        calls: usize,
        exec_from: ExecFrom,
    }

    impl CaptureCtx {
        fn new(main: Address) -> Self {
            let tx = DummyTx {
                main,
                addrs: vec![main],
            };
            let mut env = Env::default();
            env.tx.ty = 3;
            env.tx.main = main;
            env.tx.addrs = tx.addrs.clone();
            Self {
                env,
                tx,
                state: Box::new(DummyState),
                logs: NoopLogs,
                tex: TexLedger::default(),
                calls: 0,
                exec_from: ExecFrom::Top,
            }
        }
    }

    impl StateOperat for CaptureCtx {
        fn state(&mut self) -> &mut dyn State {
            self.state.as_mut()
        }
        fn state_fork(&mut self) -> Arc<Box<dyn State>> {
            Arc::new(Box::new(EmptyState {}))
        }
        fn state_merge(&mut self, _: Arc<Box<dyn State>>) {}
        fn state_recover(&mut self, _: Arc<Box<dyn State>>) {}
    }

    impl Context for CaptureCtx {
        fn action_call(&mut self, _: u16, _: Vec<u8>) -> XRet<(u32, Vec<u8>)> {
            Ok((0, vec![]))
        }
        fn exec_from(&self) -> ExecFrom {
            self.exec_from
        }
        fn exec_from_set(&mut self, from: ExecFrom) {
            self.exec_from = from;
        }
        fn env(&self) -> &Env {
            &self.env
        }
        fn addr(&self, ptr: &AddrOrPtr) -> Ret<Address> {
            ptr.real(&self.env.tx.addrs)
        }
        fn check_sign(&mut self, _: &Address) -> Rerr {
            Ok(())
        }
        fn tx(&self) -> &dyn TransactionRead {
            &self.tx
        }
        fn vm_call(&mut self, _: Box<dyn Any>) -> XRet<(VmGasBuckets, Box<dyn Any>)> {
            self.calls += 1;
            Ok((VmGasBuckets { compute: 1, resource: 0, storage: 0 }, Box::new(Value::Nil)))
        }
        fn vm_current_intent_scope(&mut self) -> IntentScope {
            None
        }
        fn tex_ledger(&self) -> &TexLedger {
            &self.tex
        }
        fn tex_ledger_mut_top(&mut self) -> Ret<&mut TexLedger> {
            Ok(&mut self.tex)
        }
        fn logs(&mut self) -> &mut dyn Logs {
            &mut self.logs
        }
    }

    fn valid_p2sh_payload(state_addr: Address, libs: ContractAddressW1, codes: Vec<u8>) -> Vec<u8> {
        let mut payload = state_addr.as_bytes().to_vec();
        payload.extend_from_slice(&libs.serialize());
        payload.extend_from_slice(&codes);
        payload
    }

    #[test]
    fn validate_vm_entry_param_rejects_oversize_scalar_value() {
        let mut runtime = Runtime::create(1);
        runtime.warm.space_cap.value_size = 2;
        let exec = ExecCtx::main();
        assert!(validate_vm_entry_param(&runtime, &exec, Some(&Value::Bytes(vec![0u8; 3]))).is_err());
    }

    #[test]
    fn check_vm_return_value_faults_handle_inside_tuple() {
        let rv = Value::Tuple(
            TupleItem::new(vec![Value::U8(1), Value::handle(7u32)]).unwrap(),
        );
        let err = check_vm_return_value(&rv, "main call").unwrap_err();
        assert!(err.is_fault());
        assert!(err.to_string().contains("cannot cross VM boundary"));
    }

    #[test]
    fn run_p2sh_entry_rejects_non_scriptmh_state_addr_before_vm_dispatch() {
        let main = Address::create_privakey([1u8; 20]);
        let mut ctx = CaptureCtx::new(main);
        let payload = valid_p2sh_payload(
            Address::create_privakey([2u8; 20]),
            ContractAddressW1::from_list(vec![]).unwrap(),
            vec![Bytecode::END as u8],
        );
        let param = Value::Tuple(TupleItem::new(vec![Value::Bytes(vec![])]).unwrap());

        let err = run_p2sh_entry(
            &mut ctx,
            CodeConf::from_type(CodeType::Bytecode).raw(),
            payload,
            param,
        )
        .unwrap_err();

        assert!(err.contains("is not SCRIPTMH type"), "{err}");
        assert_eq!(ctx.calls, 0);
    }

    #[test]
    fn run_p2sh_entry_rejects_duplicate_libs_before_vm_dispatch() {
        let main = Address::create_privakey([1u8; 20]);
        let mut ctx = CaptureCtx::new(main);
        let lib = ContractAddress::from_unchecked(Address::create_contract([9u8; 20]));
        let libs = ContractAddressW1::from_list(vec![lib.clone(), lib]).unwrap();
        let payload = valid_p2sh_payload(
            Address::create_scriptmh([2u8; 20]),
            libs,
            vec![Bytecode::END as u8],
        );
        let param = Value::Tuple(TupleItem::new(vec![Value::Bytes(vec![])]).unwrap());

        let err = run_p2sh_entry(
            &mut ctx,
            CodeConf::from_type(CodeType::Bytecode).raw(),
            payload,
            param,
        )
        .unwrap_err();

        assert!(err.contains("duplicate"), "{err}");
        assert_eq!(ctx.calls, 0);
    }

    #[test]
    fn run_p2sh_entry_rejects_malformed_witness_param_before_vm_dispatch() {
        let main = Address::create_privakey([1u8; 20]);
        let mut ctx = CaptureCtx::new(main);
        let payload = valid_p2sh_payload(
            Address::create_scriptmh([2u8; 20]),
            ContractAddressW1::from_list(vec![]).unwrap(),
            vec![Bytecode::END as u8],
        );
        let param = Value::Tuple(TupleItem::new(vec![Value::U8(7)]).unwrap());

        let err = run_p2sh_entry(
            &mut ctx,
            CodeConf::from_type(CodeType::Bytecode).raw(),
            payload,
            param,
        )
        .unwrap_err();

        assert!(err.contains("witness must be the first tuple item as bytes"), "{err}");
        assert_eq!(ctx.calls, 0);
    }

    #[test]
    fn run_p2sh_entry_rejects_invalid_code_before_vm_dispatch() {
        let main = Address::create_privakey([1u8; 20]);
        let mut ctx = CaptureCtx::new(main);
        let payload = valid_p2sh_payload(
            Address::create_scriptmh([2u8; 20]),
            ContractAddressW1::from_list(vec![]).unwrap(),
            vec![],
        );
        let param = Value::Tuple(TupleItem::new(vec![Value::Bytes(vec![])]).unwrap());

        assert!(run_p2sh_entry(
            &mut ctx,
            CodeConf::from_type(CodeType::Bytecode).raw(),
            payload,
            param,
        )
        .is_err());
        assert_eq!(ctx.calls, 0);
    }
}
