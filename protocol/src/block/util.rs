

fn mrkl_merge(list: &Vec<Hash>) -> Vec<Hash> {
    let num = list.len();
    let mut res = vec![];
    let mut x = 0usize;
    loop {
        let lh = &list[x];
        let rh = maybe!(x + 1 < num, &list[x + 1], lh);
        let mut pair = Vec::with_capacity(lh.size() + rh.size());
        pair.extend_from_slice(lh.as_ref());
        pair.extend_from_slice(rh.as_ref());
        let hx = sys::calculate_hash(pair);
        res.push(Hash::must(&hx));
        x += 2;
        if x >= num {
            break
        }
    }
    res
}


/*
* 
*/
pub fn calculate_mrklroot(list: &Vec<Hash>) -> Hash {
    if list.len() == 0 {
        return Hash::DEFAULT
    }
    let mut reslist = list;
    let mut tmp: Vec<Hash>;
    loop {
        // println!("mrklroot len={}", list.len());
        if reslist.len() <= 1 {
            return reslist[0].clone()
        }
        tmp = mrkl_merge(&reslist);
        reslist = &tmp;
    }
}





/*
* 
*/
pub fn calculate_mrkl_prelude_modify(list: &Vec<Hash>) -> Vec<Hash> {
    let mut res = vec![];
    let hxl = list.len();
    if hxl == 0 {
        never!()
    }
    if hxl == 1 {
        return res
    }
    if hxl == 2 {
        res.push(list[1]);
        return res
    }

    let mut reslist = list;
    let mut tmp: Vec<Hash>;
    loop {
        // println!("mrklroot len={}", list.len());
        if reslist.len() == 1 {
            break
        }
        if reslist.len() >= 2 {
            res.push(reslist[1])
        }
        tmp = mrkl_merge(&reslist);
        reslist = &tmp;
    }
    res
}


/*
* return: newmrkl_
*/
pub fn calculate_mrkl_prelude_update(cbhx: Hash, list: &Vec<Hash>) -> Hash {
    let mut reshx = cbhx;
    for h in list {
        let mut pair = Vec::with_capacity(reshx.size() + h.size());
        pair.extend_from_slice(reshx.as_ref());
        pair.extend_from_slice(h.as_ref());
        reshx = Hash::from(sys::calculate_hash(pair));
    }
    reshx
}
