
impl MsgHandler {

    pub async fn start(this: Arc<MsgHandler>, mut worker: Worker) {
        let mut blktxch = {
            this.blktxch.lock().unwrap().take().unwrap()
        };
        this.enter_handler_thread();
        loop {
            tokio::select! {
                _ = worker.wait() => {
                    break
                },
                msg = blktxch.recv() => {
                    let Some(msg) = msg else {
                        break
                    };
                    match msg {
                        BlockTxArrive::Tx(peer, tx, ack) => {
                            let res = handle_new_tx(this.clone(), peer, tx).await;
                            if let Some(ack) = ack {
                                let _ = ack.send(res);
                            }
                        },
                        BlockTxArrive::Block(peer, blk, ack) => {
                            let res = handle_new_block(this.clone(), peer, blk).await;
                            if let Some(ack) = ack {
                                let _ = ack.send(res);
                            }
                        },
                    }
                }
            }
        }
        this.leave_handler_thread();
        println!("[MsgHandler] loop end.");
    }
}
