pub struct EmptyLogs {}
impl Logs for EmptyLogs {}

pub struct BlockLogs {
    bhei: u64,
    logs: Vec<Vec<u8>>,
    disk: Arc<dyn DiskDB>,
}

impl Logs for BlockLogs {
    fn push(&mut self, stuff: &dyn Serialize) {
        if self.bhei == 0 {
            return; // not save
        }
        self.logs.push(stuff.serialize())
    }

    fn load(&self, hei: u64, idx: usize) -> Option<Vec<u8>> {
        let key = Self::wnk(hei, idx);
        self.disk.read(&key)
    }

    fn remove(&self, height: u64) {
        let hei = Uint8::from(height);
        let mut lnk = Vec::with_capacity(hei.size() + 1);
        hei.serialize_to(&mut lnk);
        lnk.push(b'n');
        let num = self.read_len(&lnk);
        let mut batch = MemKV::new();
        for i in 0..num {
            let k = Self::wnk(height, i);
            batch.del(k);
        }
        batch.del(lnk);
        self.disk.write(&batch);
    }

    fn height(&self) -> u64 {
        self.bhei
    }

    fn write_to_disk(&self) {
        let m = self.logs.len();
        let old_len = self.read_len(&self.lnk());
        let mut batch = MemKV::new();
        for i in 0..m {
            batch.put(self.nk(i), self.logs[i].clone());
        }
        for i in m..old_len {
            batch.del(self.nk(i));
        }
        if m > 0 {
            let num = Uint8::from(m as u64);
            batch.put(self.lnk(), num.serialize());
        } else {
            batch.del(self.lnk());
        }
        self.disk.write(&batch);
    }

    fn snapshot_len(&self) -> usize {
        self.logs.len()
    }

    fn truncate(&mut self, len: usize) {
        self.logs.truncate(len);
    }
}

impl BlockLogs {
    fn lnk(&self) -> Vec<u8> {
        let hei = Uint8::from(self.bhei);
        let mut out = Vec::with_capacity(hei.size() + 1);
        hei.serialize_to(&mut out);
        out.push(b'n');
        out
    }

    fn wnk(hei: u64, idx: usize) -> Vec<u8> {
        let hei = Uint8::from(hei);
        let num = Uint8::from(idx as u64);
        let mut out = Vec::with_capacity(hei.size() + num.size());
        hei.serialize_to(&mut out);
        num.serialize_to(&mut out);
        out
    }

    fn nk(&self, n: usize) -> Vec<u8> {
        Self::wnk(self.bhei, n)
    }

    pub fn wrap(disk: Arc<dyn DiskDB>) -> Self {
        Self {
            disk,
            bhei: 0,
            logs: Vec::new(),
        }
    }

    pub fn next(&self, hei: u64) -> Self {
        Self {
            disk: self.disk.clone(),
            bhei: hei,
            logs: Vec::new(),
        }
    }
}

impl BlockLogs {
    fn read_len(&self, lnk: &Vec<u8>) -> usize {
        let mut num = Uint8::from(0);
        match self.disk.read(lnk) {
            None => num,
            Some(v) => {
                num.parse(&v).unwrap(); // must
                num
            }
        }
        .uint() as usize
    }

    pub fn len(&self) -> usize {
        let l = self.logs.len();
        if l > 0 {
            return l;
        }
        self.read_len(&self.lnk())
    }

    // load
    pub fn read(&self, idx: usize) -> Option<Vec<u8>> {
        if idx < self.logs.len() {
            return Some(self.logs[idx].clone());
        }
        let i = &self.nk(idx);
        self.disk.read(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemDisk {
        kv: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    }

    impl DiskDB for MemDisk {
        fn read(&self, key: &[u8]) -> Option<Vec<u8>> {
            self.kv.lock().unwrap().get(key).cloned()
        }

        fn write(&self, batch: &dyn MemDB) {
            let mut kv = self.kv.lock().unwrap();
            batch.for_each(&mut |key, val| match val {
                Some(v) => {
                    kv.insert(key.to_vec(), v.to_vec());
                }
                None => {
                    kv.remove(key);
                }
            });
        }

        fn save(&self, key: &[u8], val: &[u8]) {
            self.kv.lock().unwrap().insert(key.to_vec(), val.to_vec());
        }

        fn remove(&self, key: &[u8]) {
            self.kv.lock().unwrap().remove(key);
        }

        fn for_each(&self, each: &mut dyn FnMut(&[u8], &[u8]) -> bool) -> Result<(), String> {
            let rows: Vec<(Vec<u8>, Vec<u8>)> = self
                .kv
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (k, v) in rows {
                if !each(&k, &v) {
                    break;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn remove_clears_items_and_length_key() {
        let disk: Arc<dyn DiskDB> = Arc::new(MemDisk::default());
        let height = 88u64;

        let mut wr = BlockLogs::wrap(disk.clone()).next(height);
        wr.push(&Uint1::from(7));
        wr.push(&Uint1::from(8));
        wr.write_to_disk();

        let rd = BlockLogs::wrap(disk.clone()).next(height);
        assert_eq!(rd.len(), 2);
        assert!(rd.load(height, 0).is_some());
        rd.remove(height);

        let after = BlockLogs::wrap(disk.clone()).next(height);
        assert_eq!(after.len(), 0);
        assert!(after.load(height, 0).is_none());

        after.remove(height);
        let final_view = BlockLogs::wrap(disk).next(height);
        assert_eq!(final_view.len(), 0);
    }

    #[test]
    fn write_to_disk_persists_logs_and_length_key() {
        let disk: Arc<dyn DiskDB> = Arc::new(MemDisk::default());
        let height = 99u64;

        let mut wr = BlockLogs::wrap(disk.clone()).next(height);
        wr.push(&Uint1::from(3));
        wr.push(&Uint1::from(4));
        wr.write_to_disk();

        let rd = BlockLogs::wrap(disk).next(height);
        assert_eq!(rd.len(), 2);
        assert_eq!(rd.load(height, 0), Some(Uint1::from(3).serialize()));
        assert_eq!(rd.load(height, 1), Some(Uint1::from(4).serialize()));
    }

    #[test]
    fn rewrite_shorter_clears_stale_tail_items() {
        let disk: Arc<dyn DiskDB> = Arc::new(MemDisk::default());
        let height = 100u64;

        let mut wr = BlockLogs::wrap(disk.clone()).next(height);
        wr.push(&Uint1::from(1));
        wr.push(&Uint1::from(2));
        wr.push(&Uint1::from(3));
        wr.write_to_disk();

        let mut rewrite = BlockLogs::wrap(disk.clone()).next(height);
        rewrite.push(&Uint1::from(9));
        rewrite.write_to_disk();

        let rd = BlockLogs::wrap(disk).next(height);
        assert_eq!(rd.len(), 1);
        assert_eq!(rd.load(height, 0), Some(Uint1::from(9).serialize()));
        assert!(rd.load(height, 1).is_none());
        assert!(rd.load(height, 2).is_none());
    }

    #[test]
    fn rewrite_empty_clears_all_items_and_length_key() {
        let disk: Arc<dyn DiskDB> = Arc::new(MemDisk::default());
        let height = 101u64;

        let mut wr = BlockLogs::wrap(disk.clone()).next(height);
        wr.push(&Uint1::from(5));
        wr.push(&Uint1::from(6));
        wr.write_to_disk();

        let rewrite = BlockLogs::wrap(disk.clone()).next(height);
        rewrite.write_to_disk();

        let rd = BlockLogs::wrap(disk).next(height);
        assert_eq!(rd.len(), 0);
        assert!(rd.load(height, 0).is_none());
        assert!(rd.load(height, 1).is_none());
    }
}
