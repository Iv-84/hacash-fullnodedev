



pub const BLACKHOLE_ADDR: Address = ADDRESS_ZERO;

/// Clear ALL state for the blackhole address (value 0):
///
/// 1. **Balance**: zeroes HAC/SAT/HACD/Asset counts — prevents dust accumulation.
/// 2. **Diamond owned list**: deleted — prevents unbounded state bloat from
///    repeated diamond-transfers-to-blackhole.  Individual `DiamondSto` records
///    remain intact as permanent burn markers (they still point to BLACKHOLE_ADDR
///    and are provably unspendable).
///
/// NOTE: only address value == 0 is engulfed.  Other system addresses
/// (e.g. ADDRESS_ONEX = 1 used by TEX settlement) must NOT be cleared.
#[inline(always)]
pub fn blackhole_engulf(sta: &mut CoreState, addr: &Address) {
    if *addr == BLACKHOLE_ADDR {
        sta.balance_set(addr, &Balance::new());
        // Prune the diamond owned list to prevent infinite growth.
        // DiamondSto records survive as permanent burn markers.
        if sta.diamond_owned_exist(addr) {
            sta.diamond_owned_del(addr);
        }
    }
}

