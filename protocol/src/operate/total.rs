#[inline]
fn u64_to_uint8(v: u64, name: &str) -> Ret<Uint8> {
    Uint8::from_checked(v).ok_or_else(|| format!("{name} overflow"))
}

#[inline]
fn u128_to_uint12(v: u128, name: &str) -> Ret<Uint12> {
    Uint12::from_checked(v).ok_or_else(|| format!("{name} overflow"))
}

#[inline]
pub fn with_total_count<R>(
    state: &mut CoreState,
    f: impl FnOnce(&mut TotalCount) -> Ret<R>,
) -> Ret<R> {
    let mut ttcount = state.get_total_count();
    let res = f(&mut ttcount)?;
    state.set_total_count(&ttcount);
    Ok(res)
}

#[inline]
pub fn preview_total_count(
    state: &CoreState<'_>,
    f: impl FnOnce(&mut TotalCount) -> Rerr,
) -> Ret<TotalCount> {
    let mut ttcount = state.get_total_count();
    f(&mut ttcount)?;
    Ok(ttcount)
}

#[inline]
pub fn total_add_u8(cur: &mut Uint8, add: u64, name: &str) -> Rerr {
    let next = (**cur)
        .checked_add(add)
        .ok_or_else(|| format!("{name} overflow"))?;
    *cur = u64_to_uint8(next, name)?;
    Ok(())
}

#[inline]
pub fn total_sub_u8(cur: &mut Uint8, sub: u64, name: &str) -> Rerr {
    let next = (**cur)
        .checked_sub(sub)
        .ok_or_else(|| format!("{name} underflow"))?;
    *cur = u64_to_uint8(next, name)?;
    Ok(())
}

#[inline]
pub fn total_add_u12(cur: &mut Uint12, add: u128, name: &str) -> Rerr {
    let next = (**cur)
        .checked_add(add)
        .ok_or_else(|| format!("{name} overflow"))?;
    *cur = u128_to_uint12(next, name)?;
    Ok(())
}

#[inline]
pub fn total_sub_u12(cur: &mut Uint12, sub: u128, name: &str) -> Rerr {
    let next = (**cur)
        .checked_sub(sub)
        .ok_or_else(|| format!("{name} underflow"))?;
    *cur = u128_to_uint12(next, name)?;
    Ok(())
}

#[inline]
pub fn total_add_amount_238(cur: &mut Uint12, amt: &Amount, name: &str) -> Rerr {
    let add = amt.to_238_u64()? as u128;
    total_add_u12(cur, add, name)
}

#[inline]
pub fn total_add_diamond_number(cur: &mut DiamondNumber, add: usize, name: &str) -> Rerr {
    let next = (cur.uint() as usize)
        .checked_add(add)
        .ok_or_else(|| format!("{name} overflow"))?;
    *cur = DiamondNumber::from_usize(next)?;
    Ok(())
}

#[inline]
pub fn total_add_tx_fee_pay(state: &mut CoreState, tx: &dyn TransactionRead) -> Rerr {
    let fee_pay_238 = tx.fee_pay().to_238_u64()? as u128;
    let fee_got_238 = tx.fee_got().to_238_u64()? as u128;
    with_total_count(state, |ttcount| {
        total_add_u12(
            &mut ttcount.tx_fee_pay_total_238,
            fee_pay_238,
            "tx_fee_pay_total_238",
        )?;
        total_add_u12(
            &mut ttcount.tx_fee_got_total_238,
            fee_got_238,
            "tx_fee_got_total_238",
        )?;
        Ok(())
    })?;
    Ok(())
}

#[inline]
pub fn total_record_blackhole_hac(state: &mut CoreState, amt: &Amount) -> Rerr {
    if !amt.is_positive() {
        return Ok(());
    }
    with_total_count(state, |ttcount| {
        total_add_amount_238(
            &mut ttcount.blackhole_hac_burn_238,
            amt,
            "blackhole_hac_burn_238",
        )
    })?;
    Ok(())
}

#[inline]
pub fn total_record_blackhole_sat(state: &mut CoreState, sat: &Satoshi) -> Rerr {
    if sat.uint() == 0 {
        return Ok(());
    }
    with_total_count(state, |ttcount| {
        total_add_u8(
            &mut ttcount.blackhole_sat_burn,
            sat.uint(),
            "blackhole_sat_burn",
        )
    })?;
    Ok(())
}

#[inline]
pub fn total_record_blackhole_asset(state: &mut CoreState) -> Rerr {
    with_total_count(state, |ttcount| {
        total_add_u8(
            &mut ttcount.blackhole_asset_burn_count,
            1,
            "blackhole_asset_burn_count",
        )
    })?;
    Ok(())
}

#[inline]
pub fn total_record_blackhole_hacd(state: &mut CoreState) -> Rerr {
    with_total_count(state, |ttcount| {
        total_add_u8(
            &mut ttcount.blackhole_hacd_burn_count,
            1,
            "blackhole_hacd_burn_count",
        )
    })?;
    Ok(())
}
