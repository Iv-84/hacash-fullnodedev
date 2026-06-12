field::combi_struct! { TransactionType3,
    ty         : Uint1
    timestamp  : Timestamp
    addrlist   : AddrOrList
    fee        : Amount
    actions    : DynListActionW2
    signs      : SignW2
    gas_max    : Uint1
    ano_mark   : Fixed1
}

impl TransactionRead for TransactionType3 {
    fn hash(&self) -> Hash {
        self.hash_ex(vec![])
    }

    fn hash_with_fee(&self) -> Hash {
        self.hash_ex(self.fee.serialize())
    }

    fn ty(&self) -> u8 {
        *self.ty
    }

    fn main(&self) -> Address {
        self.addrs()[0]
    }

    fn addrs(&self) -> Vec<Address> {
        self.addrlist.to_list()
    }

    fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    fn fee(&self) -> &Amount {
        &self.fee
    }

    fn fee_pay(&self) -> Amount {
        self.fee.clone()
    }

    fn fee_got(&self) -> Amount {
        // Type3 does not use the legacy extra9 fee-split / burn path.
        // Miner-side fee view stays equal to the full transaction fee.
        self.fee.clone()
    }

    fn gas_max_byte(&self) -> Option<u8> {
        Some(*self.gas_max)
    }

    fn fee_purity(&self) -> u64 {
        let txsz = self.size() as u64;
        if txsz == 0 {
            return 0;
        }
        self.fee.to_238_u64().unwrap_or(0) / txsz
    }

    fn action_count(&self) -> usize {
        self.actions.length()
    }

    fn actions(&self) -> &Vec<Box<dyn Action>> {
        self.actions.as_list()
    }

    fn signs(&self) -> &Vec<Sign> {
        self.signs.as_list()
    }

    fn req_sign(&self) -> Ret<HashSet<Address>> {
        let addrs = &self.addrs();
        let mut adrsets = HashSet::from([self.main()]);
        for act in self.actions() {
            for ptr in act.req_sign() {
                let adr = ptr.real(addrs)?;
                if adr.is_privakey() {
                    adrsets.insert(adr);
                }
            }
        }
        Ok(adrsets)
    }

    fn verify_signature(&self) -> Rerr {
        verify_tx_signature(self)
    }
}

impl Transaction for TransactionType3 {
    fn as_read(&self) -> &dyn TransactionRead {
        self
    }

    fn set_fee(&mut self, fee: Amount) {
        self.fee = fee;
    }

    fn fill_sign(&mut self, acc: &Account) -> Ret<Sign> {
        let mut fhx = self.hash();
        if acc.address() == self.main().as_bytes() {
            fhx = self.hash_with_fee();
        }
        let signobj = Sign::create_by(acc, &fhx);
        self.insert_sign(signobj.clone())?;
        Ok(signobj)
    }

    fn push_sign(&mut self, signobj: Sign) -> Rerr {
        self.insert_sign(signobj)
    }

    fn push_action(&mut self, act: Box<dyn Action>) -> Rerr {
        self.actions.push(act)
    }
}

impl TxExec for TransactionType3 {
    fn execute(&self, ctx: &mut dyn Context) -> Rerr {
        do_tx_execute_type3(self, ctx)
    }
}

impl TransactionType3 {
    pub const TYPE: u8 = 3u8;

    pub fn new_by(addr: Address, fee: Amount, ts: u64) -> Self {
        Self {
            ty: Uint1::from(Self::TYPE),
            timestamp: Timestamp::from(ts),
            addrlist: AddrOrList::from_addr(addr),
            fee,
            actions: DynListActionW2::default(),
            signs: SignW2::default(),
            gas_max: Uint1::default(),
            ano_mark: Fixed1::default(),
        }
    }

    fn hash_ex(&self, adfe: Vec<u8>) -> Hash {
        let mut stuff = vec![
            self.ty.serialize(),
            self.timestamp.serialize(),
            self.addrlist.serialize(),
            adfe,
            self.actions.serialize(),
        ]
        .concat();
        stuff.append(&mut self.gas_max.serialize());
        stuff.append(&mut self.ano_mark.serialize());
        let hx = sys::calculate_hash(stuff);
        Hash::must(&hx[..])
    }

    fn insert_sign(&mut self, signobj: Sign) -> Rerr {
        let plen = self.signs.length();
        if plen >= u16::MAX as usize - 1 {
            return errf!("too many sign objects");
        }
        let curaddr = Address::from(Account::get_address_by_public_key(*signobj.publickey));
        let apbk = signobj.publickey.as_ref();
        let mut istid = usize::MAX;
        let sglist = self.signs.as_list();
        for i in 0..plen {
            let pbk = sglist[i].publickey.as_bytes();
            if apbk == pbk {
                istid = i;
                break;
            }
        }
        if istid == usize::MAX {
            self.signs.push(signobj)?;
        } else {
            self.signs.as_mut()[istid] = signobj;
        }
        if let Ok(yes) = verify_target_signature(&curaddr, self) {
            if yes {
                return Ok(());
            }
        }
        errf!("address {} signature verification failed", curaddr)
    }
}

fn do_tx_execute_type3(tx: &TransactionType3, ctx: &mut dyn Context) -> Rerr {
    let prep = prepare_tx_execute(tx, ctx)?;
    if tx.ano_mark[0] != 0 {
        return errf!("tx type {} ano_mark must be zero", prep.txty);
    }
    mark_tx_exist(ctx, &prep.hx, prep.blkhei);
    let gas_initialized = tx_gas_initialize(ctx)?;
    for action in tx.actions() {
        ctx.exec_from_set(ExecFrom::Top);
        let (ret_gas, _retv) = action.execute(ctx)?;
        // Type3 applies extra9 only as a delta-only returned-gas surcharge here;
        // it does not reuse legacy fee-split semantics.
        ctx.gas_charge(extra9_surcharge(action.extra9(), ret_gas) as i64)?;
    }
    super::tex::do_settlement(ctx)?;
    ctx.run_deferred_phase()?;
    // Commit semantics: gas settlement/statistics are committed only on the tx success path.
    // Upper layers roll back failed transaction state, so refund is only executed on success and cannot leave inconsistent state behind.
    if gas_initialized {
        ctx.gas_refund()?;
    }
    operate::hac_sub(ctx, &prep.main, &prep.fee)?;
    // Safety: clear leaked HAC/SAT/Asset on SETTLEMENT_ADDR after all balance operations.
    crate::tex::settlement_addr_postsettle_cleanup(ctx);
    Ok(())
}



// init gas
pub fn tx_gas_initialize(ctx: &mut dyn Context) -> Ret<bool> {
    let tx = ctx.tx();
    let txty = tx.ty();
    let Some(gas_max_byte) = tx.gas_max_byte() else {
        return errf!("tx type {} gas_max must exist", txty);
    };
    let budget = decode_gas_budget(gas_max_byte.min(TX_GAS_BUDGET_CAP_BYTE));
    if budget <= 0 {
        // `gas_max=0` is intentional and means "do not initialize tx gas".
        // This is valid because not every action path consumes gas. Callers must not
        // reinterpret this branch as an invalid transaction; if a later action actually
        // charges gas, the execution path will fail with the normal "gas not initialized"
        // error at the first real gas use.
        return Ok(false);
    }
    ctx.gas_initialize(budget)?;
    Ok(true)
}
