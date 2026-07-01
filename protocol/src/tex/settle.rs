// use crate::operate::hacd_transfer;

pub fn do_settlement(ctx: &mut dyn Context) -> Rerr {
    tex_check_settlement_addr_privakey()?;
    // Validate and materialize settlement operations while holding a short borrow to the ledger.
    // Then execute state mutations without holding `&mut TexLedger` to avoid borrow cycles.
    let mut diamond_trs: Vec<(Address, DiamondNameListMax200)> = vec![];
    {
        let t = ctx.tex_ledger_mut_top()?;
        if t.zhu != 0 || t.sat != 0 || t.dia != 0 {
            return errf!("coin settlement check failed");
        }
        for (a, v) in t.assets.iter() {
            if *v != 0 {
                return errf!("asset <{}> settlement check failed", a.uint());
            }
        }
        // settle diamonds in FIFO order
        for (adr, dn) in &t.diatrs {
            let dialist =
                DiamondNameListMax200::from_list_checked(t.diamonds.fetch_head_list(*dn)?)?;
            diamond_trs.push((adr.clone(), dialist));
        }
        // after fetch_list(), ledger should have no diamonds remaining
        if t.diamonds.length() > 0 {
            return errf!("diamonds settlement check failed");
        }
    }
    for (adr, dialist) in diamond_trs {
        let _ = do_diamonds_transfer(&dialist, &SETTLEMENT_ADDR, &adr, ctx)?;
    }
    Ok(())
}

/// After all balance mutations for a transaction are committed (fee, refund, actions,
/// TEX settlement), zero out any residual HAC/SAT/Asset balance left on the TEX
/// settlement address.
///
/// TEX cells use the off-chain `TexLedger` for HAC/SAT/Asset tracking.  The on-chain
/// balance of `SETTLEMENT_ADDR` is never touched by TEX operations — it can only be
/// modified by manual user transfers.  Any such leaked balance is permanently
/// unspendable (nobody holds the private key for system address value 1).  This
/// function reclaims that dust so it cannot confuse future invariant checks.
///
/// Diamond ownership on `SETTLEMENT_ADDR` is NOT cleared here because:
///   1. Individual `DiamondSto` records are managed by `do_diamonds_transfer`
///      during settlement and already point away from `SETTLEMENT_ADDR`.
///   2. The aggregate diamond count in the `Balance` struct is a derived
///      counter that must remain consistent with `DiamondSto` across rollback.
pub fn settlement_addr_postsettle_cleanup(ctx: &mut dyn Context) {
    let state = &mut CoreState::wrap(ctx.state());
    if let Some(mut bls) = state.balance(&SETTLEMENT_ADDR) {
        let mut dirty = false;
        if bls.hacash > Amount::zero() {
            bls.hacash = Amount::zero();
            dirty = true;
        }
        if bls.satoshi.uint() > 0 {
            bls.satoshi = SatoshiAuto::default();
            dirty = true;
        }
        if bls.assets.length() > 0 {
            bls.assets = AssetAmtW1::new();
            dirty = true;
        }
        // DO NOT clear bls.diamond — see doc comment above.
        if dirty {
            state.balance_set(&SETTLEMENT_ADDR, &bls);
        }
    }
}
