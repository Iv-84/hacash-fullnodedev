fn miner_success(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    if !ctx.engine.config().miner_enable {
        return api_error("miner not enabled");
    }

    let height = req.query_u64("height", 0);
    let block_nonce = q_u32(&req, "block_nonce", 0);
    let coinbase_nonce = q_string(&req, "coinbase_nonce", "");

    let success_stuff = {
        let stf = MINER_PENDING_BLOCK.lock().unwrap();
        if stf.is_empty() {
            return api_error("pending block not ready");
        }
        let mut found_idx = None;
        for i in 0..stf.len() {
            if *stf[i].height == height {
                found_idx = Some(i);
                break;
            }
        }
        let Some(stfidx) = found_idx else {
            return api_error(&format!("pending block height {} not found", height));
        };
        let tarstf = &stf[stfidx];

        let Ok(cb_nonce_bytes) = hex::decode(coinbase_nonce.as_bytes()) else {
            return api_error("coinbase nonce format invalid");
        };
        if cb_nonce_bytes.len() != Hash::SIZE {
            return api_error("coinbase nonce length invalid");
        }

        let mut local_block = tarstf.block.clone();
        let mut local_coinbase_tx = tarstf.coinbase_tx.clone();
        let mut target_hash = tarstf.target_hash.to_vec();
        let mrklrts = tarstf.mrklrts.clone();

        right_00_to_ff(&mut target_hash);
        let target_hash = Hash::from(target_hash.try_into().unwrap());

        local_block.set_nonce(Uint4::from(block_nonce));
        local_coinbase_tx.set_nonce(Hash::from(cb_nonce_bytes.try_into().unwrap()));
        
        let cbhx = local_coinbase_tx.hash();
        let mkrl = calculate_mrkl_prelude_update(cbhx, &mrklrts);
        local_block.set_mrklroot(mkrl);
        
        let blkhx = local_block.hash();
        if 1 == hash_diff(&blkhx, &target_hash) {
            return api_error(&format!(
                "difficulty check failed: expected at least {} but got {}",
                target_hash.to_hex(),
                blkhx.to_hex()
            ));
        }
        
        (local_block, local_coinbase_tx)
    };

    let (mut block, coinbase_tx) = success_stuff;
    let done_height = block.height().uint();
    
    block.replace_transaction(0, Box::new(coinbase_tx)).unwrap();
    let blkpkg = BlkPkg::create(Box::new(block));
    if let Err(e) = ctx.hnoder.submit_block(&blkpkg, false) {
        return api_error(&format!("submit block failed: {}", e));
    }

    {
        let mut stf = MINER_PENDING_BLOCK.lock().unwrap();
        stf.retain(|it| *it.height != done_height);
    }

    api_ok(vec![
        ("height", json!(done_height)),
        ("mining", json!("success")),
    ])
}
