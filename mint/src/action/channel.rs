/*
*
*/
action_define! { ChannelOpen, 2,
    ActScope::TOP, 2, false,
    [
        self.left_bill.address.into(),
        self.right_bill.address.into()
    ],
    {
        channel_id     : ChannelId
        left_bill      : AddrHac
        right_bill     : AddrHac
    },
    (self, format!("Open channel {} for {} and {}", self.channel_id, self.left_bill.address, self.right_bill.address)),
    (self, ctx, _gas {
        channel_open(self, ctx)
    })
}

fn channel_open(this: &ChannelOpen, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    this.left_bill.address.must_privakey()?;
    this.right_bill.address.must_privakey()?;

    let (cid, left_addr, left_amt, right_addr, right_amt) = (
        &this.channel_id,
        &this.left_bill.address,
        &this.left_bill.amount,
        &this.right_bill.address,
        &this.right_bill.amount,
    );

    let env = ctx.env().clone();

    // check id size
    check_valid_store_item_key("channel", &cid, ChannelId::SIZE)?;
    // check format
    if left_addr == right_addr {
        return xerrf!("left address cannot be equal to right address");
    }
    left_amt
        .check_6_long()
        .map_err(|_| "left amount bytes too long".to_string())?;
    right_amt
        .check_6_long()
        .map_err(|_| "right amount bytes too long".to_string())?;
    if left_amt.is_negative()
        || right_amt.is_negative()
        || (left_amt.is_zero() && right_amt.is_zero())
    {
        return xerrf!("left or right amount must be positive, or both are empty");
    }
    // sub balance
    if left_amt.not_zero() {
        hac_sub(ctx, left_addr, left_amt)?;
    }
    if right_amt.not_zero() {
        hac_sub(ctx, right_addr, right_amt)?;
    }

    let lock_total = left_amt.add_mode_u64(right_amt)?;
    // TotalCount is tracked in HAC unit238 (u64).
    let lock_total_238 = lock_total.to_238_u64()?;

    // check exist
    let mut reuse_version = Uint4::from(1);
    // channel ID with the same left and right addresses and closed by consensus can be reused
    {
        let state = MintState::wrap(ctx.state());
        let havcha = state.channel(cid);
        if let Some(chan) = havcha {
            let chan_stat = chan.status;
            let samebothaddr =
                *left_addr == chan.left_bill.address && *right_addr == chan.right_bill.address;
            if !samebothaddr || CHANNEL_STATUS_AGREEMENT_CLOSED != chan_stat {
                // exist or cannot reuse
                return xerrf!("channel {} is opening or cannot be reused", cid);
            }
            reuse_version = chan.reuse_version.clone();
            let nv = (*reuse_version)
                .checked_add(1)
                .ok_or_else(|| "channel reuse_version overflow".to_string())?;
            reuse_version = Uint4::from(nv);
        }
    }

    // save channel
    let pd_hei = env.block.height;
    let channel = ChannelSto {
        status: CHANNEL_STATUS_OPENING,
        reuse_version: reuse_version,
        open_height: Uint5::from(pd_hei),
        close_height: Uint5::from(0),
        arbitration_lock_block: Uint2::from(5000), // lock period is about 17 days
        interest_attribution: CHANNEL_INTEREST_ATTRIBUTION_TYPE_DEFAULT,
        left_bill: AddrBalance {
            address: left_addr.clone(),
            balance: Balance::hac(left_amt.clone()),
        },
        right_bill: AddrBalance {
            address: right_addr.clone(),
            balance: Balance::hac(right_amt.clone()),
        },
        if_challenging: ChallengePeriodDataOptional::default(), // none
        if_distribution: ClosedDistributionDataOptional::default(), // none
    };
    {
        let mut state = MintState::wrap(ctx.state());
        state.channel_set(cid, &channel);
    }

    // update total count
    let mut cstate = CoreState::wrap(ctx.state());
    with_total_count(&mut cstate, |ttcount| {
        total_add_u8(&mut ttcount.opening_channel, 1, "opening_channel")?;
        total_add_u12(
            &mut ttcount.channel_deposit_238,
            lock_total_238 as u128,
            "channel_deposit_238",
        )?;
        total_add_u8(&mut ttcount.channel_open_total, 1, "channel_open_total")?;
        Ok(())
    })?;

    // ok finish
    Ok(vec![])
}

/*******************************************/

action_define! { ChannelClose, 3,
    ActScope::TOP, 2, false, [],
    {
        channel_id     : ChannelId
    },
    (self, format!("Close channel {}", self.channel_id)),
    (self, ctx, _gas {
        channel_close(self, ctx)
    })
}

fn channel_close(this: &ChannelClose, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let cid = &this.channel_id;
    check_valid_store_item_key("channel", cid, ChannelId::SIZE)?;

    let pending_height = ctx.env().block.height;
    let state = MintState::wrap(ctx.state());

    // query
    let Some(chan) = state.channel(cid) else {
        return xerrf!("channel not found");
    };
    chan.left_bill.address.must_privakey()?;
    chan.right_bill.address.must_privakey()?;

    // verify two address sign
    ctx.check_sign(&chan.left_bill.address)?;
    ctx.check_sign(&chan.right_bill.address)?;

    // do close
    Ok(close_channel_default(pending_height, ctx, cid, &chan)?)
}
