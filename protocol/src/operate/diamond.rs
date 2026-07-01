


macro_rules! diamond_operate_define {
    ($func_name: ident, $addr:ident, $hacd:ident, $oldhacd:ident, $newhacdblock:block) => (

pub fn $func_name(state: &mut CoreState, $addr: &Address, $hacd: &DiamondNumber) -> XRet<DiamondNumber> {
    $addr.check_version()?;
    let mut userbls = state.balance( $addr ).unwrap_or_default();
    let oldhacd_val = userbls.diamond.to_diamond()
        .map_err(|e| format!("address {} diamond count decode failed: {}", $addr, e))?;
    let $oldhacd = &oldhacd_val;
    /* -------- */
    let newhacd = $newhacdblock;// operate
    /* -------- */
    // save
    userbls.diamond = DiamondNumberAuto::from_diamond( &newhacd );
    state.balance_set($addr, &userbls);
    Ok(newhacd)
}

    )
}


/**************************** */

pub fn hacd_add(state: &mut CoreState, addr: &Address, hacd: &DiamondNumber) -> XRet<DiamondNumber> {
    addr.check_version()?;
    let mut userbls = state.balance(addr).unwrap_or_default();
    let oldhacd_val = userbls.diamond.to_diamond()
        .map_err(|e| format!("address {} diamond count decode failed: {}", addr, e))?;
    let oldhacd = &oldhacd_val;
    let Some(sum) = oldhacd.uint().checked_add(hacd.uint()) else {
        return xerrf!("address {} diamond add overflow: {} + {}", addr, oldhacd, hacd)
    };
    let newhacd = DiamondNumber::from_usize(sum as usize)?;
    userbls.diamond = DiamondNumberAuto::from_diamond(&newhacd);
    state.balance_set(addr, &userbls);
    if blackhole_engulf(state, addr) {
        total_record_blackhole_hacd(state)?;
    }
    Ok(newhacd)
}

diamond_operate_define!(hacd_sub, addr, hacd, oldhacd, {  
    // check
    if *oldhacd < *hacd {
        return xerr_rf!("address {} diamond {} is insufficient, at least {}", 
            addr, oldhacd, hacd)
    }
    // do sub
    *oldhacd - *hacd
});



/**************************** */


pub fn hacd_transfer(state: &mut CoreState,
    from: &Address, to: &Address, hacd: &DiamondNumber, _dlist: &DiamondNameListMax200
) -> XRet<Vec<u8>> {
    if from == to {
		return xerrf!("cannot transfer to self")
    }
    // do transfer
    hacd_sub(state, from, hacd)?;
    hacd_add(state, to,   hacd)?; // hacd_add includes blackhole_engulf
    // ok
    Ok(vec![])
}


/*********************************** */


pub fn hacd_move_one_diamond(state: &mut CoreState, addr_from: &Address, addr_to: &Address, hacd_name: &DiamondName) -> XRerr {
    addr_from.check_version()?;
    addr_to.check_version()?;
    if addr_from == addr_to {
		return xerrf!("cannot transfer to self")
    }
    // query
    let mut diaitem = check_diamond_status(state, addr_from, hacd_name)?;
	// transfer diamond
    diaitem.address = addr_to.clone();
    state.diamond_set(hacd_name, &diaitem);
    // ok
    Ok(())
}


pub fn check_diamond_status(state: &mut CoreState, addr_from: &Address, hacd_name: &DiamondName) -> XRet<DiamondSto> {
    addr_from.check_version()?;
    // query
    let Some(diaitem) = state.diamond(hacd_name) else {
        return xerrf!("diamond status {} not found", hacd_name.to_readable());
    };
    if diaitem.status != DIAMOND_STATUS_NORMAL {
        return xerr_rf!("diamond {} has been mortgaged and cannot be transferred", hacd_name.to_readable())
    }
    if *addr_from != diaitem.address {
        return xerr_rf!("diamond {} does not belong to address {}", hacd_name.to_readable(), addr_from)
    }
    // ok
    Ok(diaitem)
}


/**
* diamond owned push or drop
*/
pub fn diamond_owned_push_one(state: &mut CoreState, address: &Address, name: &DiamondName) {
    let mut owned = state.diamond_owned(address).unwrap_or_default();
    owned.push_one(name);
    state.diamond_owned_set(address, &owned);
}


/**
* diamond owned push or drop
*/
pub fn diamond_owned_append(state: &mut CoreState, address: &Address, list: DiamondNameListMax60000) {
    let mut owned = state.diamond_owned(address).unwrap_or_default();
    for name in list.into_iter() {
        owned.push_one(&name);
    }
    state.diamond_owned_set(address, &owned);
}


pub fn diamond_owned_move(state: &mut CoreState, from: &Address, to: &Address, list: &DiamondNameListMax200) -> XRerr {
    if from == to {
        return xerrf!("cannot transfer to self")
    }
    // do drop
    let from_owned = state.diamond_owned(from);
    if let None = from_owned {
        return xerrf!("diamond owner record not found")
    }
    let mut from_owned = from_owned.unwrap();
    let _blsnum = from_owned.drop(list)?;
    if from_owned.names.length() > 0 {
        state.diamond_owned_set(from, &from_owned);
    }else{
        state.diamond_owned_del(from);
    }
    // do push
    let mut to_owned = state.diamond_owned(to).unwrap_or_default();
    to_owned.push(list);
    state.diamond_owned_set(to, &to_owned);
    Ok(())
}
