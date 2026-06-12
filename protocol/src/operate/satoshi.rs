

macro_rules! satoshi_operate_define {
    ($func_name: ident, $addr:ident, $sat:ident, $oldsat:ident,  $newsatblock:block) => (

        pub fn $func_name(ctx: &mut dyn Context, $addr: &Address, $sat: &Satoshi) -> XRet<Satoshi> {
            $addr.check_version()?;
            if $sat.uint() == 0 {
                return xerrf!("satoshi value cannot be zero")
            }    
            let mut state = CoreState::wrap(ctx.state());
            let mut userbls = state.balance( $addr ).unwrap_or_default();
            let $oldsat = &userbls.satoshi.to_satoshi();
            /* -------- */
            let newsat = $newsatblock;// operate
            /* -------- */
            // save
            userbls.satoshi = SatoshiAuto::from_satoshi( &newsat );
            state.balance_set($addr, &userbls);
            Ok(newsat)
        }

    )
}


/**************************** */

pub fn sat_add(ctx: &mut dyn Context, addr: &Address, sat: &Satoshi) -> XRet<Satoshi> {
    addr.check_version()?;
    if sat.uint() == 0 {
        return xerrf!("satoshi value cannot be zero")
    }
    let mut state = CoreState::wrap(ctx.state());
    let mut userbls = state.balance(addr).unwrap_or_default();
    let oldsat = &userbls.satoshi.to_satoshi();
    let Some(sum) = oldsat.uint().checked_add(sat.uint()) else {
        return xerrf!("address {} satoshi add overflow: {} + {}", addr, oldsat, sat)
    };
    let newsat = Satoshi::from(sum);
    userbls.satoshi = SatoshiAuto::from_satoshi(&newsat);
    state.balance_set(addr, &userbls);
    blackhole_engulf(&mut state, addr);
    Ok(newsat)
}

satoshi_operate_define!(sat_sub, addr, sat, oldsat, {  
    // check
    if *oldsat < *sat {
        return xerr_rf!("address {} satoshi {} is insufficient, at least {}", 
            addr, oldsat, sat)
    }
    // do sub
    *oldsat - *sat
});



/**************************** */


pub fn sat_transfer(ctx: &mut dyn Context, from: &Address, to: &Address, sat: &Satoshi
) -> XRet<Vec<u8>> {
    crate::upgrade::check_transfer_addr_online_open(
        ctx.env().chain.id,
        ctx.env().block.height,
        from,
        to,
    )?;
    if from == to {
		return xerrf!("cannot transfer to self")
    }
    // do transfer
    sat_sub(ctx, from, sat)?;
    sat_add(ctx, to,   sat)?; // sat_add includes blackhole_engulf
    // ok
    Ok(vec![])
}


pub fn sat_check(ctx: &mut dyn Context, addr: &Address, sat: &Satoshi) -> XRet<Satoshi> {
    addr.check_version()?;
    if sat.uint() == 0 {
        return xerrf!("satoshi check amount cannot be empty")
    }
    let state = CoreState::wrap(ctx.state());
    if let Some(bls) = state.balance( addr ) {
        let usrsat = bls.satoshi.to_satoshi();
        if usrsat >= *sat {
            return Ok(usrsat)
        }
    }
    xerr_rf!("address {} satoshi is insufficient", addr)
}
