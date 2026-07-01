
async fn handle_new_tx(this: Arc<MsgHandler>, peer: Option<Arc<Peer>>, body: Vec<u8>) -> Rerr {
    crate::core::handle_new_tx(this, peer, body).await
}

async fn handle_new_block(this: Arc<MsgHandler>, peer: Option<Arc<Peer>>, body: Vec<u8>) -> Rerr {
    crate::core::handle_new_block(this, peer, body).await
}
