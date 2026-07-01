fn supply(ctx: &ApiExecCtx, _req: ApiRequest) -> ApiResponse {
    let staptr = read_mint_state(ctx);
    let state = CoreStateRead::wrap(staptr.as_ref().as_ref());
    let lasthei = ctx.engine.latest_block().height().uint();
    const UNIT_238: u128 = 100_0000_0000;
    let supply = state.get_total_count();
    let blk_rwd = match (cumulative_block_reward(lasthei) as u128).checked_mul(UNIT_238) {
        Some(v) => v,
        None => return api_error("block_reward overflow"),
    };
    let burned_diamond_bid = *supply.hacd_bid_burn_238;
    let burned_diamond_insc = *supply.diamond_insc_burn_238;
    let burned_legacy_tx_extra9 = *supply.tx_fee_burn90_238;
    let burned_vm_ast_gas = *supply.ast_vm_gas_burn_238;
    let burned_asset_issue = *supply.asset_issue_burn_238;
    let burned_contract_protocol_cost = *supply.contract_protocol_cost_burn_238;
    let burned_blackhole_hac = *supply.blackhole_hac_burn_238;
    // `burned_diamond_bid` is a subset of legacy tx extra9 burn, so do not add it again.
    let burn_fee = match burned_legacy_tx_extra9
        .checked_add(burned_diamond_insc)
        .and_then(|v| v.checked_add(burned_vm_ast_gas))
        .and_then(|v| v.checked_add(burned_asset_issue))
        .and_then(|v| v.checked_add(burned_contract_protocol_cost))
    {
        Some(v) => v,
        None => return api_error("burned_fee overflow"),
    };
    let curr_ccl = match blk_rwd
        .checked_add(*supply.channel_interest_238 as u128)
        .and_then(|v| v.checked_sub(burn_fee))
        .and_then(|v| v.checked_sub(burned_blackhole_hac))
    {
        Some(v) => v,
        None => return api_error("current_circulation overflow"),
    };
    let z2m = |v238: u128| v238 as f64 / UNIT_238 as f64;
    let dia_insc_created = supply.dia_insc_push.uint();
    let dia_insc_destroyed = supply.dia_insc_drop.uint();
    let dia_insc_live = dia_insc_created.saturating_sub(dia_insc_destroyed);
    api_ok(vec![
        ("latest_height", json!(lasthei)),
        ("current_circulation", json!(z2m(curr_ccl))),
        ("block_reward", json!(z2m(blk_rwd))),
        ("burned_fee", json!(z2m(burn_fee))),
        // diamond mint
        ("minted_diamond", json!(*supply.minted_diamond)),
        ("burned_diamond_bid", json!(z2m(burned_diamond_bid))),
        // channel
        ("channel_opening", json!(*supply.opening_channel)),
        ("channel_deposit", json!(z2m(*supply.channel_deposit_238))),
        ("channel_interest", json!(z2m(*supply.channel_interest_238 as u128))),
        ("channel_open_total", json!(*supply.channel_open_total)),
        ("channel_close_total", json!(*supply.channel_close_total)),
        ("channel_closed_hac_volume", json!(z2m(*supply.channel_closed_hac_volume_238))),
        // asset
        ("burned_asset_issue", json!(z2m(burned_asset_issue))),
        // fee burn
        ("burned_legacy_tx_extra9_fee", json!(z2m(burned_legacy_tx_extra9))),
        ("burned_ast_vm_gas", json!(z2m(burned_vm_ast_gas))),
        ("tx_fee_pay_total", json!(z2m(*supply.tx_fee_pay_total_238))),
        ("tx_fee_got_total", json!(z2m(*supply.tx_fee_got_total_238))),
        // diamond inscription
        ("diamond_engraved", json!(*supply.diamond_engraved)),
        ("burned_diamond_insc", json!(z2m(burned_diamond_insc))),
        ("diamond_inscription_created", json!(dia_insc_created)),
        ("diamond_inscription_destroyed", json!(dia_insc_destroyed)),
        ("diamond_inscription_live", json!(dia_insc_live)),
        ("diamond_inscription_clean", json!(*supply.dia_insc_clean)),
        ("diamond_inscription_edit", json!(*supply.dia_insc_edit)),
        ("diamond_inscription_move", json!(*supply.dia_insc_move)),
        ("diamond_inscription_live_diamond", json!(*supply.dia_insc_live_diamond)),
        // contract
        ("burned_contract_protocol_cost", json!(z2m(burned_contract_protocol_cost))),
        ("contract_deploy_count", json!(*supply.contract_deploy_count)),
        ("contract_update_count", json!(*supply.contract_update_count)),
        ("contract_charge_bytes_total", json!(*supply.contract_charge_bytes_total)),
        // blackhole
        ("burned_blackhole_hac", json!(z2m(burned_blackhole_hac))),
        ("blackhole_sat_burn", json!(*supply.blackhole_sat_burn)),
        ("blackhole_asset_burn_count", json!(*supply.blackhole_asset_burn_count)),
        ("blackhole_hacd_burn_count", json!(*supply.blackhole_hacd_burn_count)),
        // legacy placeholders
        ("transferred_bitcoin", json!(0)),
        ("trsbtc_subsidy", json!(0)),
    ])
}
