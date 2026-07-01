
fn try_execute_tx_by_author(
    this: &ChainEngine,
    tx: &dyn TransactionRead,
    pd_hei: u64,
    sub_state: &mut Box<dyn State>,
    author: Address,
) -> Rerr {
    let cnf = &this.cnf;
    if protocol::transaction::is_prelude_tx_type(tx.ty()) {
        return errf!("cannot submit author tx");
    }
    let an = tx.action_count();
    if an != tx.actions().len() {
        return errf!("tx action count does not match")
    }
    if an > cnf.max_tx_actions {
        return errf!("tx action count cannot exceed {}", cnf.max_tx_actions)
    }
    if tx.size() as usize > cnf.max_tx_size {
        return errf!("tx size cannot exceed {} bytes", cnf.max_tx_size)
    }
    let cur_time = curtimes();
    if tx.timestamp().uint() > cur_time {
        return errf!("tx timestamp {} cannot exceed now {}", tx.timestamp(), cur_time)
    }
    let hash = Hash::from([0u8; 32]);
    let env = Env {
        chain: ChainInfo {
            id: this.cnf.chain_id,
            diamond_form: this.cnf.diamond_form,
            fast_sync: false,
        },
        block: BlkInfo {
            height: pd_hei,
            hash,
            author,
        },
        tx: create_tx_info(tx),
    };
    // Isolate execution per tx:
    // - build an internal sub-state fork from current accumulated `sub_state`
    // - merge on success
    // - discard on failure
    let parent: Arc<Box<dyn State>> = sub_state.clone_state().into();
    let sub = parent.fork_sub(Arc::downgrade(&parent));
    let log = this.logs.next(0);
    let mut ctxobj = ctx::ContextInst::new(env, sub, Box::new(log), tx);
    let exec_res = tx.execute(&mut ctxobj);
    let (sta, _) = ctxobj.release();
    match exec_res {
        Ok(()) => {
            sub_state.merge_sub(sta);
            Ok(())
        }
        Err(e) => Err(e),
    }
}
