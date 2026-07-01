// close default
pub fn close_channel_default(
    pdhei: u64,
    ctx: &mut dyn Context,
    channel_id: &ChannelId,
    paychan: &ChannelSto,
) -> Ret<Vec<u8>> {
    close_channel_with_distribution(
        pdhei,
        ctx,
        channel_id,
        paychan,
        &paychan.left_bill.balance,
        &paychan.right_bill.balance,
        false,
    )
}

/**
 * close
 * pdhei = pending height
 */
pub fn close_channel_with_distribution(
    pdhei: u64,
    ctx: &mut dyn Context,
    channel_id: &ChannelId,
    paychan: &ChannelSto,
    left_bls: &Balance,
    right_bls: &Balance,
    is_final_closed: bool,
) -> Ret<Vec<u8>> {
    // bls
    let left_amt = &left_bls.hacash;
    let right_amt = &right_bls.hacash;
    let left_sat = &left_bls.satoshi;
    let right_sat = &right_bls.satoshi;

    // check
    if paychan.status != CHANNEL_STATUS_OPENING {
        return errf!("channel is not open");
    }
    let left_addr = &paychan.left_bill.address;
    let right_addr = &paychan.right_bill.address;
    if left_amt.is_negative() || right_amt.is_negative() {
        return errf!("channel distribution amount cannot be negative");
    }
    let ttamt = paychan
        .left_bill
        .balance
        .hacash
        .add_mode_u64(&paychan.right_bill.balance.hacash)?;
    if left_amt.add_mode_u64(right_amt)? != ttamt {
        return errf!("HAC distribution amount must match lock-in");
    }
    let ttamt_238 = ttamt.to_238_u64()?;
    let ttsat = paychan.left_bill.balance.satoshi + paychan.right_bill.balance.satoshi;
    if *left_sat + *right_sat != ttsat {
        return errf!("BTC distribution amount must match lock-in");
    }
    // Validate total-count deltas first, then apply them once at the end so
    // nested balance ops (e.g. blackhole engulf stats) cannot be overwritten
    // by a stale total_count snapshot.
    let mut interest_add_238 = 0u64;
    let mut deposit_sub_238 = 0u128;
    let mut closed_hac_volume_add_238 = 0u128;
    let mut deposit_sat_sub = 0u64;
    let mut ttcount_check = {
        let state = CoreState::wrap(ctx.state());
        preview_total_count(&state, |ttcount| {
            total_sub_u8(&mut ttcount.opening_channel, 1, "opening_channel")?;
            total_add_u8(&mut ttcount.channel_close_total, 1, "channel_close_total")?;
            Ok(())
        })?
    };
    // do close
    if ttamt.is_positive() {
        // calculate_interest
        let (newamt1, newamt2) = genesis::calculate_interest_of_height(
            pdhei,
            *paychan.open_height,
            paychan.interest_attribution,
            left_amt,
            right_amt,
        )?;
        let ttnewhac = newamt1.add_mode_u64(&newamt2)?;
        if ttnewhac < ttamt {
            return errf!("interest calculation failed");
        }
        let ttiesthac = ttnewhac.sub_mode_u64(&ttamt)?;
        interest_add_238 = ttiesthac.to_238_u64()?;
        deposit_sub_238 = ttamt_238 as u128;
        closed_hac_volume_add_238 = ttamt_238 as u128;
        total_add_u8(
            &mut ttcount_check.channel_interest_238,
            interest_add_238,
            "channel_interest_238",
        )?;
        total_sub_u12(
            &mut ttcount_check.channel_deposit_238,
            deposit_sub_238,
            "channel_deposit_238",
        )?;
        total_add_u12(
            &mut ttcount_check.channel_closed_hac_volume_238,
            closed_hac_volume_add_238,
            "channel_closed_hac_volume_238",
        )?;
        if newamt1.is_positive() {
            hac_add(ctx, left_addr, &newamt1)?;
        }
        if newamt2.is_positive() {
            hac_add(ctx, right_addr, &newamt2)?;
        }
    }
    if *ttsat > 0 {
        deposit_sat_sub = *ttsat;
        total_sub_u8(
            &mut ttcount_check.channel_deposit_sat,
            deposit_sat_sub,
            "channel_deposit_sat",
        )?;
        if left_sat.uint() > 0 {
            sat_add(ctx, left_addr, &left_sat.to_satoshi())?;
        }
        if right_sat.uint() > 0 {
            sat_add(ctx, right_addr, &right_sat.to_satoshi())?;
        }
    }
    // save channel
    let distribution = ClosedDistributionDataOptional::must(ClosedDistributionData {
        left_bill: Balance {
            hacash: left_amt.clone(),
            satoshi: left_sat.clone(),
            diamond: DiamondNumberAuto::new(),
            assets: AssetAmtW1::new(),
        },
        right_bill: Balance {
            hacash: right_amt.clone(),
            satoshi: right_sat.clone(),
            diamond: DiamondNumberAuto::new(),
            assets: AssetAmtW1::new(),
        },
    });
    let mut savechan = paychan.clone();
    savechan.status = maybe!(
        is_final_closed,
        CHANNEL_STATUS_FINAL_ARBITRATION_CLOSED,
        CHANNEL_STATUS_AGREEMENT_CLOSED
    );
    savechan.close_height = Uint5::from(pdhei);
    savechan.if_distribution = distribution;
    // save channel and count
    {
        let mut state = MintState::wrap(ctx.state());
        state.channel_set(&channel_id, &savechan);
    }
    {
        let mut state = CoreState::wrap(ctx.state());
        with_total_count(&mut state, |ttcount| {
            total_sub_u8(&mut ttcount.opening_channel, 1, "opening_channel")?;
            total_add_u8(&mut ttcount.channel_close_total, 1, "channel_close_total")?;
            total_add_u8(
                &mut ttcount.channel_interest_238,
                interest_add_238,
                "channel_interest_238",
            )?;
            total_sub_u12(
                &mut ttcount.channel_deposit_238,
                deposit_sub_238,
                "channel_deposit_238",
            )?;
            total_add_u12(
                &mut ttcount.channel_closed_hac_volume_238,
                closed_hac_volume_add_238,
                "channel_closed_hac_volume_238",
            )?;
            total_sub_u8(
                &mut ttcount.channel_deposit_sat,
                deposit_sat_sub,
                "channel_deposit_sat",
            )?;
            Ok(())
        })?;
    }
    // ok finish
    Ok(vec![])
}
