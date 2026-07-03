fn impl_blk_verify(this: &HacashMinter, curblk: &dyn BlockRead, prevblk: &dyn BlockRead, src: &dyn BlockIntroSource) -> Rerr {
    let curhei = curblk.height().uint(); // u64
    let smaxh = this.cnf.sync_maxh;
    if smaxh > 0 && curhei > smaxh {
        return errf!("config [mint].height_max limit: {}", smaxh)
    }
    // verify mainnet prelude as coinbase
    let ptx = curblk.prelude_transaction()?;
    if ptx.ty() != crate::TransactionCoinbase::TYPE {
        return errf!("mainnet prelude tx must be coinbase")
    }
    verify_coinbase(curhei, ptx)?;
    // check difficulty
    let blkcln = this.cnf.difficulty_adjust_blocks; // 288
    if curhei < blkcln*200 && this.cnf.is_mainnet() {
        return Ok(()) // not check, compatible history code
    }
    // coinbase address must be PRIVAKEY type for modern blocks
    verify_coinbase_privakey(ptx)?;
    if this.difficulty.is_historical_lwma_height(curhei) {
        return Ok(())
    }
    let curn = curblk.difficulty().uint(); // u32
    let (tarn, tarhx, _tarbign) = this.next_difficulty(prevblk, curblk.timestamp().uint(), src);
    if tarn != curn {
        return errf!("height {} PoW difficulty check failed: expected {} but got {}", curhei, tarn, curn)
    }
    if hash_bigger_than(curblk.hash().as_ref(), &tarhx) {
        return errf!("height {} PoW hashrates check failed: must not exceed {} but got {}",
            curhei, hex::encode(tarhx),  hex::encode(curblk.hash()))
    }
    Ok(())
}

fn impl_blk_insert(this: &HacashMinter, curblk: &BlkPkg, _sta: &dyn State, prev: &dyn State) -> Rerr {
    check_highest_bid_of_block(this, curblk, prev)?;
    Ok(())
}

fn check_highest_bid_of_block(this: &HacashMinter, curblk: &BlkPkg, prevsta: &dyn State) -> Rerr {
    let curhei = curblk.hein();
    if curhei % 5 != 0 {
        return  Ok(())
    }
    let block = curblk.block_read();
    if let Some((tidx, txp, diamint)) = action::pickout_diamond_mint_action_from_block(block) {
        const CKN: u32 = DIAMOND_ABOVE_NUMBER_OF_MIN_FEE_AND_FORCE_CHECK_HIGHEST;
        if tidx != 1 && curhei > 600000 {
            return errf!("diamond mint transaction must be the first tx in block")
        }
        let dianum  = *diamint.d.number;
        let bidfee  = txp.fee().clone();
        check_diamond_mint_minimum_bidding_fee(curhei, txp.as_read(), &diamint)?;
        let mut bidrecord = this.bidding_prove.lock().unwrap();
        let t4blkt = bidrecord.prev_block_arrive_time(block.prevhash());
        let rhbf = bidrecord.highest(curhei, dianum, prevsta, t4blkt);
        if bidfee < rhbf {
            if dianum > CKN {
                if bidrecord.is_replay_allowed(&curblk.hash()) {
                    println!(
                        "[MintLowBid] replay low bid accepted height={} hash={} diamond={} fee={} fence={}",
                        curhei,
                        curblk.hash().half(),
                        dianum,
                        bidfee,
                        rhbf,
                    );
                } else {
                    if !bidrecord.add_low_bid_root(dianum, curblk.clone(), bidfee.clone()) {
                        return errf!("{}", LOW_BID_CACHE_FULL_ERR);
                    }
                    println!(
                        "[MintLowBid] low root detected height={} hash={} diamond={} fee={} fence={}",
                        curhei,
                        curblk.hash().half(),
                        dianum,
                        bidfee,
                        rhbf,
                    );
                    return errf!("{}", LOW_BID_PENDING_ERR)
                }
            }
        }
        bidrecord.remove_tx(dianum, txp.hash());
        bidrecord.roll(dianum);
    }
    Ok(())
}

fn check_diamond_mint_minimum_bidding_fee(next_hei: u64, tx: &dyn TransactionRead, dmact: &action::DiamondMint) -> Rerr {
    const CKN: u32 = DIAMOND_ABOVE_NUMBER_OF_MIN_FEE_AND_FORCE_CHECK_HIGHEST;
    let bidmin = genesis::block_reward(next_hei);
    let bidfee  = tx.fee().clone();
    let dianum  = *dmact.d.number;
    if bidfee < bidmin && dianum > CKN {
        return errf!("diamond bidding fee {} cannot be less than {} after number {}", bidfee, bidmin, CKN)
    }
    Ok(())
}
