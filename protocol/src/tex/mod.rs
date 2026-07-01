use std::any::*;

use dyn_clone::*;

use basis::component::*;
use basis::interface::*;
use basis::method::*;
use field::*;
use sys::*;

use super::action::*;
use super::operate::*;
use super::setup::*;
use super::state::*;
use super::*;

static SETTLEMENT_ADDR: Address = ADDRESS_ONEX;

#[inline]
fn tex_check_settlement_addr_privakey() -> Rerr {
    if !SETTLEMENT_ADDR.is_privakey() {
        return errf!(
            "tex settlement address {} must be PRIVAKEY type",
            SETTLEMENT_ADDR
        );
    }
    if !SETTLEMENT_ADDR.is_privakey_unknown() {
        return errf!(
            "tex settlement address {} must be a system address (value < u32::MAX)",
            SETTLEMENT_ADDR
        );
    }
    Ok(())
}

#[inline]
fn tex_check_asset_serial(ctx: &mut dyn Context, serial: Fold64) -> Rerr {
    if serial.is_zero() {
        return errf!("tex asset serial cannot be zero");
    }
    {
        let tex = ctx.tex_ledger();
        if tex.asset_checked.contains(&serial) {
            return Ok(());
        }
    }
    let exist = {
        let state = CoreState::wrap(ctx.state());
        state.asset(&serial).is_some()
    };
    if !exist {
        return errf!("tex asset <{}> does not exist", serial.uint());
    }
    ctx.tex_ledger_mut_top()?.asset_checked.insert(serial);
    Ok(())
}

include! {"interface.rs"}
include! {"transfer.rs"}
include! {"condition.rs"}
include! {"settle.rs"}
include! {"cell.rs"}
include! {"action.rs"}

/*
    action register
*/
pub fn register(setup: &mut ProtocolSetup) {
    setup.action_codec(ACTION_CODEC_KINDS, try_create, try_json_decode)
}

action_register! {

    TexCellAct   // 22

}
