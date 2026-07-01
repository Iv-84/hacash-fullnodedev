fn impl_tx_submit(this: &HacashMinter, engine: &dyn EngineRead, txp: &TxPkg) -> Rerr {
    let txr = txp.tx_read();
    let curr_hei = engine.latest_block().height().uint();
    let next_hei = curr_hei + 1;
    let Some(diamintact) = action::pickout_diamond_mint_action(txr) else {
        return Ok(()) // other normal tx
    };
    if next_hei % 5 == 0 {
        return errf!("diamond mint transaction cannot be submitted after height ending in 4 or 9")
    }
    check_diamond_mint_minimum_bidding_fee(next_hei, txr, &diamintact)?;
    let mut biddings = this.bidding_prove.lock().unwrap();
    biddings.record(curr_hei, txp, &diamintact);
    Ok(())
}

fn impl_tx_pool_group(tx: &TxPkg) -> usize {
    let mut group_id = TXGID_NORMAL;
    if let Some(..) = action::pickout_diamond_mint_action(tx.tx_read()) {
        group_id = TXGID_DIAMINT;
    }
    group_id
}

fn impl_tx_pool_refresh(
    _this: &HacashMinter,
    eng: &dyn EngineRead,
    txpool: &dyn TxPool,
    txs: Vec<Hash>,
    blkhei: u64,
) {
    if blkhei % 15 == 0 {
        println!("{}.", txpool.print());
    }
    // drop all overdue diamond mint tx
    if blkhei % 5 == 0 {
        clean_invalid_diamond_mint_txs(eng, txpool, blkhei);
    }
    // drop all exist normal tx
    if txs.len() > 1 {
        let _ = txpool.drain(&txs[1..]); // over coinbase tx
    }
    // drop invalid normal
    if blkhei % 11 == 0 {
        // 1 hours
        clean_invalid_normal_txs(eng, txpool, blkhei);
    }
}

fn clean_invalid_normal_txs(eng: &dyn EngineRead, txpool: &dyn TxPool, blkhei: u64) {
    let pdhei = blkhei + 1;
    let mut sub_state = eng.fork_sub_state();
    let mut keep_rest_after_uncertain_type3 = false;
    let _ = txpool.retain_at(TXGID_NORMAL, &mut |a: &TxPkg| {
        if keep_rest_after_uncertain_type3 {
            return true;
        }
        let txr = a.tx_read();
        let exec = eng.try_execute_tx_by(txr, pdhei, &mut sub_state);
        if exec.is_ok() {
            return true;
        }
        if txr.ty() >= TransactionType3::TYPE {
            keep_rest_after_uncertain_type3 = true;
            return true;
        }
        false // delete legacy txs that still fail under deterministic precheck
    });
}

fn clean_invalid_diamond_mint_txs(eng: &dyn EngineRead, txpool: &dyn TxPool, _blkhei: u64) {
    let sta = eng.state();
    let sta = sta.as_ref();
    let curdn = CoreStateRead::wrap(sta.as_ref())
        .get_latest_diamond()
        .number
        .uint();
    let nextdn = curdn + 1;
    let _ = txpool.retain_at(TXGID_DIAMINT, &mut |a: &TxPkg| {
        nextdn == action::get_diamond_mint_number(a.tx_read())
    });
}
