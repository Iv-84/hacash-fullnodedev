
pub struct MsgHandler {
    pub(crate) engine: Arc<dyn Engine>,
    pub(crate) txpool: Arc<dyn TxPool>,
    pub(crate) p2pmng: StdMutex<Option<Box<dyn PeerManage>>>,

    blktx: Sender<BlockTxArrive>,
    blktxch: StdMutex<Option<Receiver<BlockTxArrive>>>,

    pub(crate) doing_sync: AtomicU64,
    pub(crate) sync_tracker: SyncTracker,
    pub(crate) knows: Knowledge,

    pub(crate) inserting: Arc<StdMutex<bool>>,
    handler_thread: StdMutex<Option<std::thread::ThreadId>>,
    exts_by_ty: StdMutex<HashMap<u16, Arc<dyn NodeP2PExtension>>>,
    exts_all: StdMutex<Vec<Arc<dyn NodeP2PExtension>>>,
}

impl MsgHandler {

    pub fn new(engine: Arc<dyn Engine>, txpool: Arc<dyn TxPool>) -> MsgHandler {
        let (tx, rx): (Sender<BlockTxArrive>, Receiver<BlockTxArrive>) = mpsc::channel(4000);
        MsgHandler{
            engine: engine,
            txpool: txpool,
            p2pmng: None.into(),
            blktx: tx,
            blktxch: Some(rx).into(),
            doing_sync: AtomicU64::new(0),
            sync_tracker: SyncTracker::new(),
            knows: Knowledge::new(2000),
            inserting: Arc::new(StdMutex::new(false)),
            handler_thread: StdMutex::new(None),
            exts_by_ty: StdMutex::new(HashMap::new()),
            exts_all: StdMutex::new(vec![]),
        }
    }

    pub fn switch_peer(&self, p: Arc<Peer>) -> Arc<Peer> {
        self.p2pmng.lock().unwrap().as_ref().unwrap().switch_peer(p)
    }

    pub fn set_p2p_mng(&self, mng: Box<dyn PeerManage>) {
        let mut mymng = self.p2pmng.lock().unwrap();
        *mymng = Some(mng);
    }

    pub fn register_p2p_extension(&self, tys: Vec<u16>, ext: Arc<dyn NodeP2PExtension>) -> Rerr {
        if tys.is_empty() {
            return errf!("empty p2p extension message types")
        }
        {
            let mut map = self.exts_by_ty.lock().unwrap();
            for ty in &tys {
                if is_inner_msg_ty(*ty) {
                    return errf!("message type {} is reserved by node", ty)
                }
                if map.contains_key(ty) {
                    return errf!("message type {} already registered", ty)
                }
            }
            for ty in tys {
                map.insert(ty, ext.clone());
            }
        }
        let mut all = self.exts_all.lock().unwrap();
        if !all.iter().any(|cur| Arc::ptr_eq(cur, &ext)) {
            all.push(ext);
        }
        Ok(())
    }

    fn extensions(&self) -> Vec<Arc<dyn NodeP2PExtension>> {
        self.exts_all.lock().unwrap().clone()
    }

    fn extension_for(&self, ty: u16) -> Option<Arc<dyn NodeP2PExtension>> {
        self.exts_by_ty.lock().unwrap().get(&ty).cloned()
    }

    pub fn broadcast_p2p_extension_message(&self, key: Hash, ty: u16, body: Vec<u8>) -> Rerr {
        if is_inner_msg_ty(ty) {
            return errf!("message type {} is reserved by node", ty)
        }
        let p2p = self.p2pmng.lock().unwrap();
        let Some(p2p) = p2p.as_ref() else {
            return errf!("p2p manager not initialized")
        };
        p2p.broadcast_message(0, key.into_array(), ty, body);
        Ok(())
    }

    pub async fn submit_transaction(&self, body: Vec<u8>) {
        let _ = self.blktx.send(BlockTxArrive::Tx(None, body, None)).await;
    }

    pub async fn submit_block(&self, body: Vec<u8>) {
        let _ = self.blktx.send(BlockTxArrive::Block(None, body, None)).await;
    }

    pub fn is_loop_started(&self) -> bool {
        self.blktxch.lock().unwrap().is_none()
    }

    pub fn enter_handler_thread(&self) {
        *self.handler_thread.lock().unwrap() = Some(std::thread::current().id());
    }

    pub fn leave_handler_thread(&self) {
        *self.handler_thread.lock().unwrap() = None;
    }

    fn is_handler_thread(&self) -> bool {
        let cur = std::thread::current().id();
        self.handler_thread
            .lock()
            .unwrap()
            .map(|id| id == cur)
            .unwrap_or(false)
    }

    fn submit_and_wait(&self, msg: BlockTxArrive) -> Rerr {
        if self.is_handler_thread() {
            return errf!("cannot synchronously submit from node message handler thread");
        }
        if !self.is_loop_started() {
            return errf!("node message handler not started");
        }
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        let msg = match msg {
            BlockTxArrive::Tx(peer, body, _) => BlockTxArrive::Tx(peer, body, Some(ack_tx)),
            BlockTxArrive::Block(peer, body, _) => BlockTxArrive::Block(peer, body, Some(ack_tx)),
        };
        self.blktx
            .try_send(msg)
            .map_err(|e| format!("node message queue submit failed: {}", e))?;
        ack_rx
            .recv()
            .map_err(|e| format!("node message handler response failed: {}", e))?
    }

    pub fn submit_transaction_wait(&self, body: Vec<u8>) -> Rerr {
        self.submit_and_wait(BlockTxArrive::Tx(None, body, None))
    }

    pub fn submit_block_wait(&self, body: Vec<u8>) -> Rerr {
        self.submit_and_wait(BlockTxArrive::Block(None, body, None))
    }

    pub fn exit(&self) {
        let lk = self.inserting.lock().unwrap();
        drop(lk)
    }

}


impl MsgHandler {

    pub async fn on_connect(&self, peer: Arc<Peer>) {
        let _ = peer.send_msg(MSG_REQ_STATUS, vec![]).await;
        let eng = self.engine.clone();
        let txp = self.txpool.clone();
        if let Err(e) = self.engine.minter().p2p_on_connect(peer.clone(), eng.clone(), txp.clone()) {
            println!("minter p2p on connect error: {}", e)
        }
        let peer_ext: Arc<dyn NPeer> = peer;
        for ext in self.extensions() {
            if let Err(e) = ext.on_connect(peer_ext.clone(), eng.clone(), txp.clone()) {
                println!("p2p extension on connect error: {}", e)
            }
        }
    }

    pub async fn on_disconnect(&self, peer: Arc<Peer>) {
        self.sync_tracker.clear_peer(&peer);
        let peer_ext: Arc<dyn NPeer> = peer;
        for ext in self.extensions() {
            ext.on_disconnect(peer_ext.clone());
        }
    }

    pub async fn on_message(&self, peer: Arc<Peer>, ty: u16, body: Vec<u8>) {
        match ty {
            MSG_TX_SUBMIT =>      { let _ = self.blktx.send(BlockTxArrive::Tx(Some(peer.clone()), body, None)).await; },
            MSG_BLOCK_DISCOVER => { let _ = self.blktx.send(BlockTxArrive::Block(Some(peer.clone()), body, None)).await; },
            MSG_BLOCK_HASH =>     { self.receive_hashs(peer, body).await; },
            MSG_REQ_BLOCK_HASH => { self.send_hashs(peer, body).await; },
            MSG_BLOCK =>          { self.receive_blocks(peer, body).await; },
            MSG_REQ_BLOCK =>      { self.send_blocks(peer, body).await; },
            MSG_REQ_STATUS =>     { self.send_status(peer).await; },
            MSG_STATUS =>         { self.receive_status(peer, body).await; },
            _ => {
                let ext = self.extension_for(ty);
                if let Some(ext) = ext {
                    let peer_ext: Arc<dyn NPeer> = peer;
                    let eng = self.engine.clone();
                    let txp = self.txpool.clone();
                    if let Err(e) = ext.on_message(peer_ext, eng, txp, ty, body) {
                        println!("p2p extension on message {} error: {}", ty, e)
                    }
                }
            },
        };
    }


}
