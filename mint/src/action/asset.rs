pub const ASSET_ALIVE_HEIGHT: u64 = 765432;

#[allow(unused)]
fn check_alive_blk_hei(ctx: &mut dyn Context) -> Ret<(u64, u64)> {
    let chei = ctx.env().block.height;
    let is_mainnet = ctx.env().chain.id == 0;
    let alive_hei: u64 = maybe!(is_mainnet, ASSET_ALIVE_HEIGHT, 0);
    let minsri: u64 = maybe!(is_mainnet, 1025, 5);
    Ok((alive_hei, minsri))
}

// By design tx.main plus protocol fee authorizes asset creation, while metadata.issuer is only the initial allocation target and does not need to sign.
action_define! { AssetCreate, 16,
    ActScope::TOP_ONLY, 2, false, [],
    {
        metadata: AssetSmelt
        protocol_cost: Amount
    },
    (self, format!("Register asset <{}>", self.metadata.ticket)),
    (self, ctx, _gas {
        let amd = self.metadata.clone();
        let serial = *amd.serial;
        // check serial
        let chei = ctx.env().block.height;
        let (alive_hei, minsri) = check_alive_blk_hei(ctx)?;
        if alive_hei > chei {
            return xerr!("The asset issuance has not yet begun")
        }
        if serial < minsri {
            return xerrf!("serial cannot be less than {}", minsri)
        }
        let serial_limit = minsri + (chei - alive_hei);
        if serial > serial_limit {
            return xerr!("asset serial overflow")
        }
        // check meta
        amd.issuer.check_version()?;
        if amd.issuer.is_privakey_unknown() {
            return xerrf!(
                "issuer cannot be system address {} (privakey unknown)",
                amd.issuer
            )
        }
        let tl = amd.ticket.length();
        let nl = amd.name.length();
        if tl < 1 || tl > 8 {
            return xerr!("ticket length must be 1 ~ 8")
        }
        if nl < 1 || nl > 32 {
            return xerr!("name length must be 1 ~ 32")
        }
        if !check_readable_string(&amd.ticket) {
            return xerr!("ticket must be ascii2 readable string")
        }
        if !check_readable_string(&amd.name) {
            return xerr!("name must be ascii2 readable string")
        }
        if *amd.decimal > 16 {
            return xerr!("decimal cannot exceed 16")
        }
        if amd.supply.is_zero() {
            return xerr!("supply must be greater than zero")
        }
        // check fee and burn
        let blkrw = super::genesis::block_reward(chei);
        let pfee = self.protocol_cost.clone();
        if pfee != blkrw {
            return xerrf!("Protocol fee must be {} but got {}", blkrw, pfee)
        }
        // sub main addr balance for protocol fee
        let main_addr = ctx.env().tx.main;
        hac_sub(ctx, &main_addr, &pfee)?;
        // state and check exists
        let mut sta = CoreState::wrap(ctx.state());
        if let Some(..) = sta.asset(&amd.serial) {
            return xerrf!("Asset serial {} already exists", serial)
        }
        sta.asset_set(&amd.serial, &amd); // store asset object
        // total count update
        with_total_count(&mut sta, |ttcount| {
            total_add_u8(&mut ttcount.created_asset, 1, "created_asset")?;
            total_add_amount_238(
                &mut ttcount.asset_issue_burn_238,
                &pfee,
                "asset_issue_burn_238",
            )?;
            Ok(())
        })?;
        // do mint
        let asset_obj = AssetAmt::from(amd.serial.uint(), amd.supply.uint())?;
        // issue
        let mut bls = sta.balance(&amd.issuer).unwrap_or_default();
        bls.asset_set(asset_obj)?;
        sta.balance_set(&amd.issuer, &bls);
        // ok finish
        Ok(vec![])
    })
}
