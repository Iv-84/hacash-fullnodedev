
pub const MSG_REQ_STATUS:          u16 = 1;
pub const MSG_STATUS:              u16 = 2;

pub const MSG_REQ_BLOCK_HASH:      u16 = 3;
pub const MSG_BLOCK_HASH:          u16 = 4;

pub const MSG_REQ_BLOCK:           u16 = 5;
pub const MSG_BLOCK:               u16 = 6;

pub const MSG_TX_SUBMIT:           u16 = basis::P2P_MSG_TX_SUBMIT;
pub const MSG_BLOCK_DISCOVER:      u16 = 8;


pub fn is_inner_msg_ty(ty: u16) -> bool {
    ty < 2048
}



pub enum BlockTxArrive {
    Block(Option<Arc<Peer>>, Vec<u8>, Option<std::sync::mpsc::SyncSender<Rerr>>),
    Tx(Option<Arc<Peer>>, Vec<u8>, Option<std::sync::mpsc::SyncSender<Rerr>>),
}

combi_struct!{ HandshakeStatus,
    genesis_hash:            Hash
    block_version:           Uint1
    transaction_type:        Uint1
    action_kind:             Uint2
    repair_serial:           Uint2
    __mark:                  Uint3
    latest_height:           BlockHeight
    latest_hash:             Hash
}
