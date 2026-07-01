fn debug_block_txs(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let body = q_bool(&req, "body", false);
    let action = q_bool(&req, "action", false);
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);
    let include_coinbase = q_bool(&req, "coinbase", false);

    let mut key = q_string(&req, "hash", "");
    let height = req.query_u64("height", 0);
    if height > 0 {
        key = height.to_string();
    }

    let Ok(blkpkg) = load_block_by_key(ctx, &key) else {
        return api_error("cannot find block");
    };
    let blk = blkpkg.block();
    let lasthei = ctx.engine.latest_block().height().uint();
    let txs = blk.transactions();
    let txnum = blk.transaction_count().uint() as usize;
    let start = if include_coinbase { 0 } else { 1 };

    let mut list = Vec::with_capacity(txnum.saturating_sub(start));
    for (index, tx) in txs.iter().enumerate().take(txnum).skip(start) {
        let mut info = render_tx_info(
            tx.as_read(),
            Some(blk.as_read()),
            lasthei,
            &unit,
            body,
            signature,
            action,
            description,
        );
        info.insert("index".to_owned(), json!(index));
        list.push(json!(info));
    }

    api_data(serde_json::Map::from_iter([
        ("hash".to_owned(), json!(blkpkg.hash().to_hex())),
        ("height".to_owned(), json!(blk.height().uint())),
        ("transaction_count".to_owned(), json!(txnum.saturating_sub(start))),
        ("transactions".to_owned(), json!(list)),
    ]))
}

fn find_block_tx(
    ctx: &ApiExecCtx,
    txhx: &Hash,
) -> Ret<(Arc<BlkPkg>, usize, Box<dyn Transaction>)> {
    let state_ptr = read_mint_state(ctx);
    let state = CoreStateRead::wrap(state_ptr.as_ref().as_ref());
    let Some(txp) = state.tx_exist(txhx) else {
        return errf!("transaction not found");
    };
    let blkpkg = load_block_by_key(ctx, &txp.to_string())?;
    let blk = blkpkg.block();
    let txnum = blk.transaction_count().uint() as usize;
    for (index, tx) in blk.transactions().iter().enumerate().take(txnum).skip(1) {
        if *txhx == tx.hash() {
            return Ok((blkpkg.clone(), index, tx.clone()));
        }
    }
    errf!("transaction not found in the block")
}

fn debug_transaction_receipt(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let body = q_bool(&req, "body", false);
    let action = q_bool(&req, "action", false);
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);
    let hash = q_string(&req, "hash", "");
    let Ok(hx) = hex::decode(&hash) else {
        return api_error("transaction hash format invalid");
    };
    if hx.len() != Hash::SIZE {
        return api_error("transaction hash format invalid");
    }
    let txhx = Hash::must(&hx);
    let lasthei = ctx.engine.latest_block().height().uint();

    if let Some(txp) = ctx.hnoder.txpool().find(&txhx) {
        let mut info = render_tx_info(
            txp.tx_read(),
            None,
            lasthei,
            &unit,
            body,
            signature,
            action,
            description,
        );
        info.insert("pending".to_owned(), json!(true));
        info.insert("status".to_owned(), json!("pending"));
        return api_data(info);
    }

    match find_block_tx(ctx, &txhx) {
        Ok((blkpkg, index, tx)) => {
            let blk = blkpkg.block();
            let mut info = render_tx_info(
                tx.as_read(),
                Some(blk.as_read()),
                lasthei,
                &unit,
                body,
                signature,
                action,
                description,
            );
            info.insert("pending".to_owned(), json!(false));
            info.insert("status".to_owned(), json!("mined"));
            info.insert("success".to_owned(), json!(true));
            info.insert("block_hash".to_owned(), json!(blkpkg.hash().to_hex()));
            info.insert("block_height".to_owned(), json!(blk.height().uint()));
            info.insert("index".to_owned(), json!(index));
            api_data(info)
        }
        Err(e) => api_error(&e),
    }
}

fn debug_transaction_simulate(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let Ok(bddts) = body_data_may_hex(&req) else {
        return api_error("transaction body invalid");
    };
    let txpkg = protocol::transaction::build_tx_package(bddts);
    let Ok(txpkg) = txpkg else {
        return api_error("transaction parse failed");
    };
    if let Some(resp) = reject_api_tx_non_canonical_dia_insc_push_wire(txpkg.tx_read()) {
        return resp;
    }

    let mut data = serde_json::Map::new();
    data.insert("hash".to_owned(), json!(txpkg.hash().to_hex()));
    data.insert("hash_with_fee".to_owned(), json!(txpkg.tx_read().hash_with_fee().to_hex()));
    data.insert("size".to_owned(), json!(txpkg.data().len()));
    data.insert("fee_purity".to_owned(), json!(txpkg.fpur()));
    data.insert("pending_height".to_owned(), json!(ctx.engine.latest_block().height().uint() + 1));

    match ctx.engine.try_execute_tx(txpkg.tx_read()) {
        Ok(()) => {
            data.insert("ok".to_owned(), json!(true));
            data.insert("stage".to_owned(), json!("execute"));
        }
        Err(e) => {
            data.insert("ok".to_owned(), json!(false));
            data.insert("stage".to_owned(), json!("execute"));
            data.insert("error".to_owned(), json!(e));
        }
    }
    api_data(data)
}

fn debug_submit_transaction(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let engcnf = ctx.engine.config();
    let only_insert_txpool = q_bool(&req, "only_insert_txpool", false);
    let Ok(bddts) = body_data_may_hex(&req) else {
        return create_transaction_error_response(
            "debug_submit_invalid_body",
            "transaction body invalid",
            "parse_body",
            vec![],
        );
    };
    let txpkg = protocol::transaction::build_tx_package(bddts);
    let Ok(txpkg) = txpkg else {
        return create_transaction_error_response(
            "debug_submit_parse_failed",
            "transaction parse failed",
            "parse_transaction",
            vec![],
        );
    };
    if let Some(resp) = reject_api_tx_non_canonical_dia_insc_push_wire(txpkg.tx_read()) {
        return resp;
    }
    if txpkg.fpur() < engcnf.lowest_fee_purity {
        return create_transaction_error_response(
            "debug_submit_low_fee_purity",
            &format!(
                "The transaction fee purity {} is too low, the node minimum configuration is {}.",
                txpkg.fpur(), engcnf.lowest_fee_purity
            ),
            "admission_fee",
            vec![("fee_purity", json!(txpkg.fpur()))],
        );
    }
    if txpkg.data().len() > engcnf.max_tx_size {
        return create_transaction_error_response(
            "debug_submit_tx_too_large",
            &format!("tx size cannot exceed {} bytes", engcnf.max_tx_size),
            "admission_size",
            vec![("size", json!(txpkg.data().len()))],
        );
    }
    match ctx.hnoder.submit_transaction(&txpkg, true, only_insert_txpool) {
        Ok(()) => api_data(serde_json::Map::from_iter([
            ("hash".to_owned(), json!(txpkg.hash().to_hex())),
            ("hash_with_fee".to_owned(), json!(txpkg.tx_read().hash_with_fee().to_hex())),
            ("accepted".to_owned(), json!(true)),
            ("stage".to_owned(), json!("submit")),
            ("only_insert_txpool".to_owned(), json!(only_insert_txpool)),
        ])),
        Err(e) => create_transaction_error_response(
            "debug_submit_rejected",
            &e,
            "submit",
            vec![
                ("hash", json!(txpkg.hash().to_hex())),
                ("only_insert_txpool", json!(only_insert_txpool)),
            ],
        ),
    }
}
