macro_rules! transaction_define_legacy {
    ($class:ident, $tyid:expr) => {
        field::combi_struct! { $class,
            ty         : Uint1
            timestamp  : Timestamp
            addrlist   : AddrOrList
            fee        : Amount
            actions    : DynListActionW2
            signs      : SignW2
            gas_max    : Uint1
            ano_mark   : Fixed1
        }

        impl TransactionRead for $class {
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

            fn fee(&self) -> &Amount {
                &self.fee
            }

            fn timestamp(&self) -> &Timestamp {
                &self.timestamp
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

            fn fee_pay(&self) -> Amount {
                self.fee().clone()
            }

            fn fee_got(&self) -> Amount {
                let mut gfee = self.fee().clone();
                // Legacy Type1/Type2 extra9 semantics: any top-level extra9 action lowers
                // the miner-side fee view by one unit level. Type3 does not use this path.
                if self.actions().iter().any(|a| a.extra9()) && gfee.unit() > 1 {
                    gfee = gfee.unit_sub(1).unwrap();
                }
                gfee
            }

            fn gas_max_byte(&self) -> Option<u8> {
                None
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

            fn fee_purity(&self) -> u64 {
                let txsz = self.size() as u64;
                if txsz == 0 {
                    return 0;
                }
                let fee238 = self.fee_got().to_238_u64().unwrap_or(0);
                fee238 / txsz
            }
        }

        impl Transaction for $class {
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

        impl TxExec for $class {
            fn execute(&self, ctx: &mut dyn Context) -> Rerr {
                do_tx_execute_legacy(self, ctx)
            }
        }

        impl $class {
            pub const TYPE: u8 = $tyid;

            pub fn new_by(addr: Address, fee: Amount, ts: u64) -> $class {
                $class {
                    ty: Uint1::from($tyid),
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
                let stuff = vec![
                    self.ty.serialize(),
                    self.timestamp.serialize(),
                    self.addrlist.serialize(),
                    adfe,
                    self.actions.serialize(),
                ]
                .concat();
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
    };
}

trait LegacyTransactionRead: TransactionRead {
    fn legacy_gas_max_field(&self) -> u8;
    fn legacy_ano_mark_field(&self) -> u8;
}

impl LegacyTransactionRead for TransactionType1 {
    fn legacy_gas_max_field(&self) -> u8 {
        *self.gas_max
    }

    fn legacy_ano_mark_field(&self) -> u8 {
        self.ano_mark[0]
    }
}

impl LegacyTransactionRead for TransactionType2 {
    fn legacy_gas_max_field(&self) -> u8 {
        *self.gas_max
    }

    fn legacy_ano_mark_field(&self) -> u8 {
        self.ano_mark[0]
    }
}

struct TxExecutePrep {
    blkhei: u64,
    txty: u8,
    hx: Hash,
    main: Address,
    fee: Amount,
    has_ast_control: bool,
}

fn prepare_tx_execute(tx: &dyn Transaction, ctx: &mut dyn Context) -> Ret<TxExecutePrep> {
    const TXTY1: u8 = TransactionType1::TYPE;
    let env = ctx.env();
    let blkhei = env.block.height;
    crate::upgrade::check_gated_tx(env.chain.id, blkhei, tx.ty())?;
    let not_fast_sync = !env.chain.fast_sync;
    let hx = tx.hash();
    let main = tx.main();
    let fee = tx.fee().clone();
    let has_ast_control = tx
        .actions()
        .iter()
        .any(|a| crate::action::is_ast_container_action(a.as_ref()));
    precheck_tx_actions(tx.ty(), tx.actions())?;
    let state = CoreState::wrap(ctx.state());
    if not_fast_sync {
        if !main.is_privakey() {
            return errf!("tx fee address version must be PRIVAKEY type.");
        }
        if main.is_privakey_unknown() {
            return errf!(
                "tx main address {} is a system address with unknown private key",
                main
            );
        }
        for adr in tx.addrs() {
            adr.check_version()?;
        }
        let txty = tx.ty();
        if blkhei > 20_0000 {
            fee.check_6_long().map_err(|_| {
                "tx fee size cannot exceed 6 bytes when block height above 200,000".to_string()
            })?;
        }
        if blkhei > 33033 && txty <= TXTY1 {
            return errf!("Type 1 transactions have been deprecated after height 33,033");
        }
        tx.verify_signature()?;
        if let Some(exhei) = state.tx_exist(&hx) {
            let bugtx =
                Hash::from_hex(b"f22deb27dd2893397c2bc203ddc9bc9034e455fe630d8ee310e8b5ecc6dc5628")
                    .unwrap();
            if *exhei != 63448 || hx != bugtx {
                return errf!("tx {} already exists in height {}", hx, *exhei);
            }
        }
    }
    Ok(TxExecutePrep {
        blkhei,
        txty: tx.ty(),
        hx,
        main,
        fee,
        has_ast_control,
    })
}

fn mark_tx_exist(ctx: &mut dyn Context, hx: &Hash, blkhei: u64) {
    let mut state = CoreState::wrap(ctx.state());
    state.tx_exist_set(hx, &BlockHeight::from(blkhei));
}

// Legacy Type1/Type2 extra9 burned-fee accounting. This records the fee delta
// created by `fee_got()` and is not part of Type3 returned-gas surcharge semantics.
fn record_legacy_extra9_burn_fee(
    ctx: &mut dyn Context,
    fee: &Amount,
    fee_got: &Amount,
) -> Rerr {
    let burn_fee = fee.sub_mode_u128(fee_got)?;
    if burn_fee.is_positive() {
        let burn_238 = burn_fee.to_238_u64()?;
        if burn_238 > 0 {
            let mut state = CoreState::wrap(ctx.state());
            let mut ttcount = state.get_total_count();
            let next_burn = (*ttcount.tx_fee_burn90_238)
                .checked_add(burn_238 as u128)
                .ok_or_else(|| "legacy_tx_extra9_burn_238 overflow".to_string())?;
            ttcount.tx_fee_burn90_238 = Uint12::from(next_burn);
            state.set_total_count(&ttcount);
        }
    }
    Ok(())
}

fn do_tx_execute_legacy<T: Transaction + LegacyTransactionRead>(
    tx: &T,
    ctx: &mut dyn Context,
) -> Rerr {
    const TXTY3: u8 = TransactionType3::TYPE;
    let prep = prepare_tx_execute(tx, ctx)?;
    if prep.has_ast_control {
        return errf!(
            "tx type {} cannot include AST control-flow actions; requires at least type {}",
            prep.txty,
            TXTY3
        );
    }
    if tx.legacy_gas_max_field() != 0 {
        return errf!("tx type {} gas_max must be zero", prep.txty);
    }
    if tx.legacy_ano_mark_field() != 0 {
        return errf!("tx type {} ano_mark must be zero", prep.txty);
    }
    mark_tx_exist(ctx, &prep.hx, prep.blkhei);
    for action in tx.actions() {
        ctx.exec_from_set(ExecFrom::Top);
        // Legacy top-level execution ignores returned gas. Legacy extra9 fee semantics
        // are handled through `fee_got()` / `record_legacy_extra9_burn_fee()` instead.
        let (_ret_gas, _retv) = action.execute(ctx)?;
    }
    super::tex::do_settlement(ctx)?;
    operate::hac_sub(ctx, &prep.main, &prep.fee)?;
    // Safety: clear leaked HAC/SAT/Asset on SETTLEMENT_ADDR after all balance operations.
    crate::tex::settlement_addr_postsettle_cleanup(ctx);
    let fee_got = tx.fee_got();
    record_legacy_extra9_burn_fee(ctx, &prep.fee, &fee_got)?;
    Ok(())
}
