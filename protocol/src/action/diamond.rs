/*
*
*/
action_define! { DiaSingleTrs, 5,
    ActScope::CALL, 2, false, [],
    {
        diamond   : DiamondName
        to        : AddrOrPtr
    },
    (self, format!("Transfer 1 HACD ({}) to {}", self.diamond.to_readable(), self.to.to_readable())),
    (self, ctx, _gas {
        let from  = ctx.env().tx.main;
        let to    = ctx.addr(&self.to)?;
        if to.is_privakey_unknown() {
            return xerrf!("cannot transfer diamond to system address {} (privakey unknown)", to)
        }
        let dlist = DiamondNameListMax200::one(self.diamond);
        do_diamonds_transfer(&dlist, &from, &to, ctx)
    })
}

/*
*
*/
action_define! { DiaFromToTrs, 6,
    ActScope::CALL, 2, false,
    [
        self.from
    ],
    {
        from      : AddrOrPtr
        to        : AddrOrPtr
        diamonds  : DiamondNameListMax200
    },
    (self, format!("Transfer {} HACD ({}) from {} to {}", self.diamonds.length(), self.diamonds.splitstr(), self.from.to_readable(), self.to.to_readable())),
    (self, ctx, _gas {
        let from = ctx.addr(&self.from)?;
        let to   = ctx.addr(&self.to)?;
        if to.is_privakey_unknown() {
            return xerrf!("cannot transfer diamond to system address {} (privakey unknown)", to)
        }
        do_diamonds_transfer(&self.diamonds, &from, &to, ctx)
    })
}

/*
*
*/
action_define! { DiaToTrs, 7,
    ActScope::CALL, 2, false, [],
    {
        to        : AddrOrPtr
        diamonds  : DiamondNameListMax200
    },
    (self, format!("Transfer {} HACD ({}) to {}", self.diamonds.length(), self.diamonds.splitstr(), self.to.to_readable())),
    (self, ctx, _gas {
        let from = ctx.env().tx.main;
        let to   = ctx.addr(&self.to)?;
        if to.is_privakey_unknown() {
            return xerrf!("cannot transfer diamond to system address {} (privakey unknown)", to)
        }
        do_diamonds_transfer(&self.diamonds, &from, &to, ctx)
    })
}

/*
*
*/
action_define! { DiaFromTrs, 8,
    ActScope::CALL, 2, false,
    [
        self.from
    ],
    {
        from      : AddrOrPtr
        diamonds  : DiamondNameListMax200
    },
    (self, format!("Transfer {} HACD ({}) from {}", self.diamonds.length(), self.diamonds.splitstr(), self.from.to_readable())),
    (self, ctx, _gas {
        let from = ctx.addr(&self.from)?;
        let to   = ctx.env().tx.main;
        do_diamonds_transfer(&self.diamonds, &from, &to, ctx)
    })
}

/**************************/

pub fn do_diamonds_transfer(
    diamonds: &DiamondNameListMax200,
    from: &Address,
    to: &Address,
    ctx: &mut dyn Context,
) -> XRet<Vec<u8>> {
    crate::upgrade::check_transfer_addr_online_open(
        ctx.env().chain.id,
        ctx.env().block.height,
        from,
        to,
    )?;
    // check
    let dianum = diamonds.check()?;
    let isdf = ctx.env().chain.diamond_form;
    //transfer
    let mut state = CoreState::wrap(ctx.state());
    for dianame in diamonds.as_list() {
        hacd_move_one_diamond(&mut state, from, to, &dianame)?; // move one
    }
    if isdf {
        diamond_owned_move(&mut state, from, to, diamonds)?;
    }
    // transfer
    hacd_transfer(
        &mut state,
        from,
        to,
        &DiamondNumber::from(dianum as u32),
        &diamonds,
    )
}
