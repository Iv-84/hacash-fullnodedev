/*
* simple hac to
*/
action_define! { TexCellAct, 22,
    ActScope::TOP, 3, self.cells.has_asset_transfer_cell(), [],
    {
        addr  : Address
        cells : DnyTexCellW1
        sign  : Sign
    },
    (self, format!("Execute {} tex cells by {}", self.cells.length(), self.addr)),
    (self, ctx, _gas {
        if ctx.exec_from() != ExecFrom::Top {
            return xerrf!("TexCellAct can only run in TOP context, got {}", ctx.exec_from())
        }
        self.addr.must_privakey()?;
        // Check signature on the standalone TEX bundle.
        let thx = self.get_sign_stuff();
        if ! verify_signature(&thx, &self.addr, &self.sign) {
            return xerrf!("address {} signature verification failed in tex cell action", self.addr)
        }
        // Condition/business failures are intentionally surfaced as plain text errors, which map to Fault here.
        self.cells.execute(ctx, &self.addr).map(|_| vec![]).map_err(XError::from)
    })
}

impl TexCellAct {
    fn get_sign_stuff(&self) -> Hash {
        // Intentionally sign only addr+cells.
        // TEX bundles are designed to be replayable across transactions.
        let mut stf = Vec::with_capacity(self.addr.size() + self.cells.size());
        self.addr.serialize_to(&mut stf);
        self.cells.serialize_to(&mut stf);
        Hash::from(sha3(&stf))
    }

    pub fn create_by(addr: Address) -> Self {
        Self {
            addr,
            ..Self::new()
        }
    }

    pub fn do_sign(&mut self, acc: &Account) -> Rerr {
        acc.check_addr(self.addr.as_bytes())?;
        let thx = self.get_sign_stuff();
        self.sign = Sign::create_by(acc, &thx);
        Ok(())
    }

    pub fn add_cell(&mut self, cell: Box<dyn TexCell>) -> Rerr {
        self.cells.push(cell)?;
        Ok(())
    }
}
