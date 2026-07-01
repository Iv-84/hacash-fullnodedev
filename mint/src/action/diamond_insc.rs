/// HIP-22 unified inscription cooldown: 200 blocks
const INSCRIPTION_COOLDOWN_BLOCKS: u64 = 200;
const INSCRIPTION_CONTENT_MAX_BYTES: usize = 64;
const INSCRIPTION_READABLE_TYPE_MAX: u8 = 100;
pub const INSCRIPTION_MAX_PER_DIAMOND: usize = 200;
const APPEND_FREE_MAX_INSCRIPTIONS: usize = 10;
const APPEND_TIER1_MAX_INSCRIPTIONS: usize = 40;
const APPEND_TIER2_MAX_INSCRIPTIONS: usize = 100;

#[inline]
fn check_protocol_cost_4_long(pfee: &Amount) -> Rerr {
    pfee.check_4_long()
        .map_err(|_| "protocol cost amount size cannot exceed 4 bytes".to_string())
}

#[inline]
fn check_protocol_cost(pfee: &Amount) -> Rerr {
    if pfee.is_negative() {
        return errf!("protocol cost cannot be negative");
    }
    check_protocol_cost_4_long(pfee)
}

#[inline]
fn check_inscription_content(engraved_type: u8, content: &BytesW1) -> Rerr {
    let insc_len = content.length();
    if insc_len == 0 {
        return errf!("engraved content cannot be empty");
    }
    if insc_len > INSCRIPTION_CONTENT_MAX_BYTES {
        return errf!(
            "engraved content size cannot exceed {} bytes",
            INSCRIPTION_CONTENT_MAX_BYTES
        );
    }
    if engraved_type <= INSCRIPTION_READABLE_TYPE_MAX {
        if !check_readable_string(content) {
            return errf!("engraved content must be a readable string");
        }
    }
    Ok(())
}

#[inline]
fn create_diamond_inscript(engraved_type: u8, content: &BytesW1) -> DiamondInscript {
    DiamondInscript::create_by(engraved_type, content.clone())
}

#[inline]
fn check_inscription_owner_privakey(owner: &Address, diamond: &DiamondName) -> Rerr {
    if !owner.is_privakey() {
        return errf!(
            "diamond {} owner {} must be privakey address",
            diamond.to_readable(),
            owner
        );
    }
    Ok(())
}

#[inline]
fn check_diamond_status_for_inscription(
    state: &mut CoreState,
    owner: &Address,
    diamond: &DiamondName,
) -> XRet<DiamondSto> {
    check_inscription_owner_privakey(owner, diamond)?;
    let diasto = check_diamond_status(state, owner, diamond)?;
    Ok(diasto)
}

/// Load diamond, verify Normal status and PRIVAKEY owner, return (DiamondSto, owner).
/// Does NOT require a pre-known owner — used by Move where owners are discovered on-chain.
#[inline]
fn load_diamond_for_inscription(state: &mut CoreState, diamond: &DiamondName) -> Ret<DiamondSto> {
    let diasto = must_have!(
        format!("diamond status {}", diamond.to_readable()),
        state.diamond(diamond)
    );
    check_inscription_owner_privakey(&diasto.address, diamond)?;
    if diasto.status != DIAMOND_STATUS_NORMAL {
        return errf!(
            "diamond {} has been mortgaged and cannot operate inscription",
            diamond.to_readable()
        );
    }
    Ok(diasto)
}

#[inline]
fn check_inscription_cooldown(
    prev_engraved_height: u64,
    pending_height: u64,
    diamond: &DiamondName,
) -> Rerr {
    let next_height = prev_engraved_height.saturating_add(INSCRIPTION_COOLDOWN_BLOCKS);
    if next_height > pending_height {
        return errf!(
            "HACD {} inscription cooldown not met, need {} blocks",
            diamond.to_readable(),
            INSCRIPTION_COOLDOWN_BLOCKS
        );
    }
    Ok(())
}

#[inline]
fn load_diamond_average_bid_burn_mei(state: &mut CoreState, diamond: &DiamondName) -> Ret<u16> {
    let diaslt = must_have!(
        format!("diamond {}", diamond.to_readable()),
        state.diamond_smelt(diamond)
    );
    Ok(*diaslt.average_bid_burn)
}

#[inline]
fn check_inscription_index(
    diamond: &DiamondName,
    idx: usize,
    insc_len: usize,
    role_prefix: &str,
) -> Rerr {
    if insc_len == 0 {
        if role_prefix.is_empty() {
            return errf!("no inscriptions in diamond {}", diamond.to_readable());
        }
        return errf!(
            "no inscriptions in {} HACD {}",
            role_prefix,
            diamond.to_readable()
        );
    }
    if idx >= insc_len {
        return errf!(
            "inscription index {} out of range, HACD {} has {} inscriptions",
            idx,
            diamond.to_readable(),
            insc_len
        );
    }
    Ok(())
}

#[inline]
fn load_diamond_owner_for_inscription_index(
    state: &mut CoreState,
    diamond: &DiamondName,
    idx: usize,
    pending_height: u64,
) -> XRet<(DiamondSto, Address)> {
    let diasto = load_diamond_for_inscription(state, diamond)?;
    let owner = diasto.address.clone();
    let insc_len = diasto.inscripts.length();
    check_inscription_index(diamond, idx, insc_len, "")?;
    check_inscription_cooldown(*diasto.prev_engraved_height, pending_height, diamond)?;
    Ok((diasto, owner))
}

#[inline]
fn add_dia_insc_u8(
    state: &mut CoreState,
    field: fn(&mut TotalCount) -> &mut Uint8,
    add: u64,
    name: &str,
) -> Rerr {
    if add == 0 {
        return Ok(());
    }
    with_total_count(state, |ttcount| total_add_u8(field(ttcount), add, name))?;
    Ok(())
}

#[inline]
fn add_diamond_insc_burn_count(state: &mut CoreState, pfee: &Amount) -> Rerr {
    if !pfee.is_positive() {
        return Ok(());
    }
    with_total_count(state, |ttcount| {
        total_add_amount_238(
            &mut ttcount.diamond_insc_burn_238,
            pfee,
            "diamond_insc_burn_238",
        )
    })?;
    Ok(())
}

#[inline]
fn saturating_sub_dia_insc_live_diamond(state: &mut CoreState, sub: u64) -> Rerr {
    if sub == 0 {
        return Ok(());
    }
    with_total_count(state, |ttcount| {
        let cur = ttcount.dia_insc_live_diamond.uint();
        let next = cur.saturating_sub(sub);
        ttcount.dia_insc_live_diamond =
            Uint8::from_checked(next).ok_or_else(|| "dia_insc_live_diamond overflow".to_string())?;
        Ok(())
    })?;
    Ok(())
}

/// Reject non-canonical `protocol_cost` wire on new tx paths (API / mempool).
/// Historical block replay still uses permissive [`WireAmount`] parse.
pub fn reject_tx_dia_insc_push_non_canonical_protocol_cost_wire(tx: &dyn TransactionRead) -> Rerr {
    for act in tx.actions() {
        if let Some(a) = DiaInscPush::downcast(act) {
            a.protocol_cost.require_canonical_wire().map_err(|e| {
                format!(
                    "DiaInscPush protocol_cost must use canonical amount encoding: {}",
                    e
                )
            })?;
        }
    }
    Ok(())
}

/*
*
*/
action_define! { DiaInscPush, 32,
    ActScope::TOP, 2, true, [],
    {
        diamonds         : DiamondNameListMax200
        protocol_cost    : WireAmount
        engraved_type    : Uint1
        engraved_content : BytesW1
    },
    (self, {
        let a = self;
        let dia_num = a.diamonds.length();
        let cost_str = a.protocol_cost.to_fin_string();
        let ins_str = a.engraved_content.to_readable_or_hex();
        let mut desc = format!("Inscript {} HACD ({}) with \"{}\"",
            dia_num, a.diamonds.splitstr(), ins_str);
        if a.protocol_cost.is_positive() {
            desc += &format!(" cost {} HAC fee", cost_str);
        }
        desc
    }),
    (self, ctx, _gas {
        diamond_inscription(self, ctx)
    })
}

fn diamond_inscription(this: &DiaInscPush, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let env = ctx.env().clone();
    let main_addr = env.tx.main;
    let pfee = this.protocol_cost.amount();
    check_protocol_cost(pfee)?;
    // check
    this.diamonds.check()?;
    ctx.check_sign(&main_addr)?;
    // check inscription content
    check_inscription_content(*this.engraved_type, &this.engraved_content)?;
    // cost
    let mut ttcost = Amount::zero();
    let mut live_diamond_add = 0u64;
    let pdhei = env.block.height;
    // do
    let mut state = CoreState::wrap(ctx.state());
    for dia in this.diamonds.as_list() {
        let prev_len = load_diamond_for_inscription(&mut state, &dia)?.inscripts.length();
        let cc = engraved_one_diamond(
            pdhei,
            &mut state,
            &main_addr,
            &dia,
            *this.engraved_type,
            &this.engraved_content,
        )?;
        ttcost = ttcost.add_mode_u64(&cc)?;
        if prev_len == 0 {
            live_diamond_add += 1;
        }
    }
    // check cost
    if pfee < &ttcost {
        return xerrf!(
            "diamond inscription cost expected {:?} but got {:?}",
            ttcost,
            pfee
        );
    }
    // change count
    add_dia_insc_u8(&mut state, |t| &mut t.diamond_engraved, 1, "diamond_engraved")?;
    add_dia_insc_u8(
        &mut state,
        |t| &mut t.dia_insc_push,
        this.diamonds.length() as u64,
        "dia_insc_push",
    )?;
    add_diamond_insc_burn_count(&mut state, pfee)?;
    add_dia_insc_u8(
        &mut state,
        |t| &mut t.dia_insc_live_diamond,
        live_diamond_add,
        "dia_insc_live_diamond",
    )?;
    // sub main addr balance
    if pfee.is_positive() {
        hac_sub(ctx, &main_addr, pfee)?;
    }
    // ok
    Ok(vec![])
}

/************************************ */

action_define! { DiaInscClean, 33,
    ActScope::TOP, 2, true, [],
    {
        diamonds      : DiamondNameListMax200
        protocol_cost : Amount
    },
    (self, {
        let a = self;
        let dia_num = a.diamonds.length();
        let cost_str = a.protocol_cost.to_fin_string();
        format!(
            "Clean inscript {} HACD ({}) cost {} HAC fee",
            dia_num, a.diamonds.splitstr(), cost_str
        )
    }),
    (self, ctx, _gas {
        diamond_inscription_clean(self, ctx)
    })
}

fn diamond_inscription_clean(this: &DiaInscClean, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let env = ctx.env().clone();
    let main_addr = env.tx.main;
    let pfee = &this.protocol_cost;
    check_protocol_cost(pfee)?;
    // check
    this.diamonds.check()?;
    ctx.check_sign(&main_addr)?;
    // cost
    let mut ttcost = Amount::zero();
    let mut cleared_entries = 0u64;
    let mut cleared_diamonds = 0u64;
    let pdhei = env.block.height;
    // do
    let mut state = CoreState::wrap(ctx.state());
    for dia in this.diamonds.as_list() {
        let prev_len = load_diamond_for_inscription(&mut state, &dia)?.inscripts.length();
        // Clear semantics: full wipe of inscription traces, including cooldown trace.
        let cc = engraved_clean_one_diamond(pdhei, &mut state, &main_addr, &dia)?;
        ttcost = ttcost.add_mode_u64(&cc)?;
        cleared_entries += prev_len as u64;
        if prev_len > 0 {
            cleared_diamonds += 1;
        }
    }
    // check cost
    if pfee < &ttcost {
        return xerrf!(
            "diamond inscription cost expected {:?} but got {:?}",
            ttcost,
            pfee
        );
    }
    // change count and sub hac
    add_diamond_insc_burn_count(&mut state, pfee)?;
    add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_clean, 1, "dia_insc_clean")?;
    add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_drop, cleared_entries, "dia_insc_drop")?;
    saturating_sub_dia_insc_live_diamond(&mut state, cleared_diamonds)?;
    if pfee.is_positive() {
        hac_sub(ctx, &main_addr, pfee)?;
    }
    // finish
    Ok(vec![])
}

/************************************ */

/*
* HIP-22: Upgrade HACD Inscriptions
* Transfer / Per-entry Delete / Single-entry Update
*/

action_define! { DiaInscEdit, 34,
    ActScope::CALL, 2, true, [],
    {
        diamond           : DiamondName
        index             : Uint1
        protocol_cost     : Amount
        engraved_type     : Uint1
        engraved_content  : BytesW1
    },
    (self, {
        let a = self;
        let ins_str = a.engraved_content.to_readable_or_hex();
        let mut desc = format!("Edit inscription #{} of HACD {} to \"{}\"",
            *a.index, a.diamond.to_readable(), ins_str);
        if a.protocol_cost.is_positive() {
            desc += &format!(" cost {} HAC fee", a.protocol_cost.to_fin_string());
        }
        desc
    }),
    (self, ctx, _gas {
        diamond_inscription_edit(self, ctx)
    })
}

fn diamond_inscription_edit(this: &DiaInscEdit, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let env = ctx.env().clone();
    let main_addr = env.tx.main;
    let pfee = &this.protocol_cost;
    check_protocol_cost(pfee)?;
    // fee payer must sign via tx.main signature already bound to tx execution
    ctx.check_sign(&main_addr)?;
    // check inscription content
    check_inscription_content(*this.engraved_type, &this.engraved_content)?;
    let idx = *this.index as usize;
    let pdhei = env.block.height;
    // check diamond owner signature and status/index/cooldown
    let (mut diasto, owner) = {
        let mut state = CoreState::wrap(ctx.state());
        load_diamond_owner_for_inscription_index(&mut state, &this.diamond, idx, pdhei)?
    };
    ctx.check_sign(&owner)?;
    let mut state = CoreState::wrap(ctx.state());
    // protocol cost: average_bid_burn / 100
    let avg_bid_burn_mei = load_diamond_average_bid_burn_mei(&mut state, &this.diamond)?;
    let cost = calc_edit_inscription_protocol_cost(avg_bid_burn_mei);
    if pfee < &cost {
        return xerrf!(
            "inscription edit cost expected {:?} but got {:?}",
            cost,
            pfee
        );
    }
    // replace the inscription entry
    diasto.inscripts.replace(
        idx,
        create_diamond_inscript(*this.engraved_type, &this.engraved_content),
    )?;
    diasto.prev_engraved_height = BlockHeight::from(pdhei);
    state.diamond_set(&this.diamond, &diasto);
    // burn protocol cost
    add_diamond_insc_burn_count(&mut state, pfee)?;
    add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_edit, 1, "dia_insc_edit")?;
    if pfee.is_positive() {
        hac_sub(ctx, &main_addr, pfee)?;
    }
    // ok
    Ok(vec![])
}

/************************************** */

action_define! { DiaInscMove, 35,
    ActScope::AST, 2, true, [],
    {
        from_diamond    : DiamondName
        to_diamond      : DiamondName
        index           : Uint1
        protocol_cost   : Amount
    },
    (self, {
        let a = self;
        let mut desc = format!("Move inscription #{} from HACD {} to HACD {}",
            *a.index, a.from_diamond.to_readable(),
            a.to_diamond.to_readable());
        if a.protocol_cost.is_positive() {
            desc += &format!(" cost {} HAC fee", a.protocol_cost.to_fin_string());
        }
        desc
    }),
    (self, ctx, _gas {
        diamond_inscription_move(self, ctx)
    })
}

fn diamond_inscription_move(this: &DiaInscMove, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let env = ctx.env().clone();
    let main_addr = env.tx.main;
    let pfee = &this.protocol_cost;
    check_protocol_cost(pfee)?;
    let idx = *this.index as usize;
    let pdhei = env.block.height;
    if this.from_diamond == this.to_diamond {
        return xerrf!("source and target HACD cannot be the same");
    }
    // validate source and target diamonds
    let (mut from_sto, mut to_sto, from_owner, to_owner, move_cost, from_len, to_len) = {
        let mut state = CoreState::wrap(ctx.state());
        let from_sto = load_diamond_for_inscription(&mut state, &this.from_diamond)?;
        let from_owner = from_sto.address.clone();
        let from_insc_len = from_sto.inscripts.length();
        check_inscription_index(&this.from_diamond, idx, from_insc_len, "source ")?;
        check_inscription_cooldown(*from_sto.prev_engraved_height, pdhei, &this.from_diamond)?;
        let to_sto = load_diamond_for_inscription(&mut state, &this.to_diamond)?;
        let to_owner = to_sto.address.clone();
        check_inscription_cooldown(*to_sto.prev_engraved_height, pdhei, &this.to_diamond)?;
        if to_sto.inscripts.length() >= INSCRIPTION_MAX_PER_DIAMOND {
            return xerrf!(
                "target HACD {} inscriptions full (max {})",
                this.to_diamond.to_readable(),
                INSCRIPTION_MAX_PER_DIAMOND
            );
        }
        let to_insc_len = to_sto.inscripts.length();
        let move_cost = {
            let avg_bid_burn_mei = load_diamond_average_bid_burn_mei(&mut state, &this.to_diamond)?;
            calc_move_inscription_protocol_cost(to_insc_len, avg_bid_burn_mei)
        };
        (from_sto, to_sto, from_owner, to_owner, move_cost, from_insc_len, to_insc_len)
    };
    // both owners must sign
    ctx.check_sign(&from_owner)?;
    ctx.check_sign(&to_owner)?;
    // check protocol cost
    if pfee < &move_cost {
        return xerrf!(
            "inscription move cost expected {:?} but got {:?}",
            move_cost,
            pfee
        );
    }
    // extract inscription from source
    let inscript = from_sto.inscripts.as_list()[idx].clone();
    from_sto.inscripts.drop(idx)?;
    from_sto.prev_engraved_height = BlockHeight::from(pdhei);
    let mut state = CoreState::wrap(ctx.state());
    state.diamond_set(&this.from_diamond, &from_sto);
    // push inscription to target
    to_sto.inscripts.push(inscript)?;
    to_sto.prev_engraved_height = BlockHeight::from(pdhei);
    state.diamond_set(&this.to_diamond, &to_sto);
    // burn protocol cost
    add_diamond_insc_burn_count(&mut state, pfee)?;
    add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_move, 1, "dia_insc_move")?;
    if from_len == 1 {
        saturating_sub_dia_insc_live_diamond(&mut state, 1)?;
    }
    if to_len == 0 {
        add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_live_diamond, 1, "dia_insc_live_diamond")?;
    }
    if pfee.is_positive() {
        hac_sub(ctx, &main_addr, pfee)?;
    }
    // ok
    Ok(vec![])
}

/************************************ */

action_define! { DiaInscDrop, 36,
    ActScope::TOP, 2, true, [],
    {
        diamond           : DiamondName
        index             : Uint1
        protocol_cost     : Amount
    },
    (self, {
        let a = self;
        format!("Drop inscription #{} from HACD {} cost {} HAC fee",
            *a.index, a.diamond.to_readable(), a.protocol_cost.to_fin_string())
    }),
    (self, ctx, _gas {
        diamond_inscription_drop(self, ctx)
    })
}

fn diamond_inscription_drop(this: &DiaInscDrop, ctx: &mut dyn Context) -> XRet<Vec<u8>> {
    let env = ctx.env().clone();
    let main_addr = env.tx.main;
    let pfee = &this.protocol_cost;
    check_protocol_cost(pfee)?;
    // fee payer must sign via tx.main signature already bound to tx execution
    ctx.check_sign(&main_addr)?;
    let idx = *this.index as usize;
    let pdhei = env.block.height;
    // check diamond owner signature and status/index/cooldown
    let (mut diasto, owner) = {
        let mut state = CoreState::wrap(ctx.state());
        load_diamond_owner_for_inscription_index(&mut state, &this.diamond, idx, pdhei)?
    };
    ctx.check_sign(&owner)?;
    let mut state = CoreState::wrap(ctx.state());
    // cost: average_bid_burn / 50
    let avg_bid_burn_mei = load_diamond_average_bid_burn_mei(&mut state, &this.diamond)?;
    let prev_len = diasto.inscripts.length();
    let cost = calc_drop_inscription_protocol_cost(avg_bid_burn_mei);
    if pfee < &cost {
        return xerrf!(
            "inscription drop cost expected {:?} but got {:?}",
            cost,
            pfee
        );
    }
    // drop the inscription entry
    diasto.inscripts.drop(idx)?;
    diasto.prev_engraved_height = BlockHeight::from(pdhei);
    state.diamond_set(&this.diamond, &diasto);
    // burn fee
    add_diamond_insc_burn_count(&mut state, pfee)?;
    add_dia_insc_u8(&mut state, |t| &mut t.dia_insc_drop, 1, "dia_insc_drop")?;
    if prev_len == 1 {
        saturating_sub_dia_insc_live_diamond(&mut state, 1)?;
    }
    if pfee.is_positive() {
        hac_sub(ctx, &main_addr, pfee)?;
    }
    Ok(vec![])
}

/************************************ */

#[inline]
pub fn calc_append_inscription_protocol_cost(
    cur_inscriptions: usize,
    average_bid_burn_mei: u16,
) -> Amount {
    if cur_inscriptions < APPEND_FREE_MAX_INSCRIPTIONS {
        return Amount::zero();
    }
    if cur_inscriptions < APPEND_TIER1_MAX_INSCRIPTIONS {
        // 11~40: average_bid_burn / 50
        return Amount::coin(average_bid_burn_mei as u64 * 2, 246);
    }
    if cur_inscriptions < APPEND_TIER2_MAX_INSCRIPTIONS {
        // 41~100: average_bid_burn / 20
        return Amount::coin(average_bid_burn_mei as u64 * 5, 246);
    }
    // 101~200: average_bid_burn / 10
    Amount::coin(average_bid_burn_mei as u64 * 10, 246)
}

#[inline]
pub fn calc_move_inscription_protocol_cost(
    target_cur_inscriptions: usize,
    average_bid_burn_mei: u16,
) -> Amount {
    calc_append_inscription_protocol_cost(target_cur_inscriptions, average_bid_burn_mei)
}

#[inline]
pub fn calc_edit_inscription_protocol_cost(average_bid_burn_mei: u16) -> Amount {
    // average_bid_burn / 100
    Amount::coin(average_bid_burn_mei as u64, 246)
}

#[inline]
pub fn calc_drop_inscription_protocol_cost(average_bid_burn_mei: u16) -> Amount {
    // average_bid_burn / 50
    Amount::coin(average_bid_burn_mei as u64 * 2, 246)
}

/**
*
* return total cost
*/
pub fn engraved_one_diamond(
    pending_height: u64,
    state: &mut CoreState,
    addr: &Address,
    diamond: &DiamondName,
    engraved_type: u8,
    content: &BytesW1,
) -> Ret<Amount> {
    let mut diasto = check_diamond_status_for_inscription(state, addr, diamond)?;
    // check height
    check_inscription_cooldown(*diasto.prev_engraved_height, pending_height, diamond)?;
    // check insc
    let haveng = diasto.inscripts.length();
    if haveng >= INSCRIPTION_MAX_PER_DIAMOND {
        return errf!(
            "maximum inscriptions for one diamond is {}",
            INSCRIPTION_MAX_PER_DIAMOND
        );
    }
    let diaslt = must_have!(
        format!("diamond {}", diamond.to_readable()),
        state.diamond_smelt(&diamond)
    );
    // cost: stepped append protocol cost
    let cost = calc_append_inscription_protocol_cost(haveng, *diaslt.average_bid_burn);
    // do engraved
    diasto.prev_engraved_height = BlockHeight::from(pending_height);
    diasto
        .inscripts
        .push(create_diamond_inscript(engraved_type, content))?;
    // save
    state.diamond_set(diamond, &diasto);
    // ok finish
    Ok(cost)
}

/// Clear all inscriptions of one diamond and return protocol cost.
/// Unlike append/drop/edit/move, clear does not enforce cooldown check.
/// It also removes cooldown trace by resetting `prev_engraved_height` to 0.
pub fn engraved_clean_one_diamond(
    _pending_height_ignored: u64,
    state: &mut CoreState,
    addr: &Address,
    diamond: &DiamondName,
) -> Ret<Amount> {
    let mut diasto = check_diamond_status_for_inscription(state, addr, diamond)?;
    let diaslt = must_have!(
        format!("diamond {}", diamond.to_readable()),
        state.diamond_smelt(&diamond)
    );
    // check
    if diasto.inscripts.length() == 0 {
        return errf!(
            "cannot find any inscriptions in HACD {}",
            diamond.to_readable()
        );
    }
    // burning cost bid fee
    let cost = Amount::mei(*diaslt.average_bid_burn as u64);
    // Clear intentionally resets both inscriptions and cooldown trace, allowing an immediate fresh append path.
    diasto.prev_engraved_height = BlockHeight::from(0);
    diasto.inscripts = Inscripts::default();
    // save
    state.diamond_set(diamond, &diasto);
    // ok finish
    Ok(cost)
}
