

#[macro_export]
macro_rules! combi_dynlist {
    ($class:ident, $lenty:ty, $dynty:ident, $createfn:path, $json_createfn:path) => (


#[derive(Default, Clone)]
pub struct $class {
    count: $lenty,
    vlist: Vec<Box<dyn $dynty>>
}

impl Iterator for $class {
    type Item = Box<dyn $dynty>;
    fn next(&mut self) -> Option<Box<dyn $dynty>> {
        self.pop()
    }
}


impl std::fmt::Debug for $class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"[dyn list {}]", *self.count)
    }
}

impl PartialEq for $class {
    #[inline]
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl Eq for $class {}


impl Parse for $class {

    fn parse_from(&mut self, buf: &mut &[u8]) -> Ret<usize> {
        let mut count = <$lenty>::default();
        let mut seek = count.parse_from(buf)?;
        let count_usize = *count as usize;
        let mut cur = *buf;
        let mut new_vlist = Vec::with_capacity($crate::prealloc_cap(count_usize, cur.len()));
        for _ in 0..count_usize {
            let (obj, mvsk) = $createfn(cur)?;
            cur = &cur[mvsk..];
            seek += mvsk;
            new_vlist.push(obj);
        }
        *buf = cur;
        self.count = count;
        self.vlist = new_vlist;
        Ok(seek)
    }
}

impl Serialize for $class {
    
    fn serialize_to(&self, out: &mut Vec<u8>) {
        self.count.serialize_to(out);
        for i in 0 .. *self.count as usize {
            self.vlist[i].as_ref().serialize_to(out);
        }
    }

    fn size(&self) -> usize {
        let mut sznum = self.count.size();
        for i in 0 .. *self.count as usize {
            sznum += self.vlist[i as usize].as_ref().size();
        }
        sznum
    }

}

impl_field_only_new!{$class}

impl ToJSON for $class {
    fn to_json_fmt(&self, fmt: &JSONFormater) -> String {
        let mut res = String::from("[");
        for i in 0..self.vlist.len() {
            if i > 0 { res.push(','); }
            res.push_str(&self.vlist[i].as_ref().to_json_fmt(fmt));
        }
        res.push(']');
        res
    }
}

impl FromJSON for $class {
    fn from_json(&mut self, json: &str) -> Ret<()> {
        let (list, _) = $crate::json_decode_array(json)?;
        let mut vlist: Vec<Box<dyn $dynty>> = Vec::with_capacity(list.len());
        for item_json in list {
            if let Some(item) = $json_createfn(&item_json)? {
                vlist.push(item);
            } else {
                return errf!("dynamic object JSON decode failed: {}", item_json);
            }
        }
        let count = <$lenty>::from_usize(vlist.len())?;
        self.count = count;
        self.vlist = vlist;
        Ok(())
    }
}

impl $class {

    pub fn from_list(list: Vec<Box<dyn $dynty>>) -> Ret<Self> {
        let n = list.len();
        if n > <$lenty>::MAX as usize {
            return errf!("list data length overflow")
        }
        Ok(Self{
            count: <$lenty>::from_usize(n)?,
            vlist: list,
        })
    }

    pub fn replace(&mut self, i: usize, v: Box<dyn $dynty>) -> Rerr {
        let tl = self.length() as usize;
        if i >= tl {
            return errf!("list index overflow")
        }
        self.vlist[i] = v;
        Ok(())
    }

	pub fn push(&mut self, v: Box<dyn $dynty>) -> Rerr {
        if *self.count >= <$lenty>::MAX {
            return errf!("list length overflow");
        }
		self.count += 1u8;
        self.vlist.push(v);
        Ok(())
	}

	pub fn pop(&mut self) -> Option<Box<dyn $dynty>> {
        let n = *self.count;
        match n {
            0 => None,
            _ => {
                self.count -= 1u8;
                self.vlist.pop()
            }
        }
	}

	pub fn length(&self) -> usize {
		*self.count as usize
	}

	pub fn count(&self) -> &$lenty {
		&self.count
	}

    pub fn as_list(&self) -> &Vec<Box<dyn $dynty>> {
        &self.vlist
    }



}



    )
}


/********************* test **********************/ 
pub trait Test7354846353856 : Field + DynClone {
    fn getv(&self) -> u8 { 0 }
}
clone_trait_object!{Test7354846353856}

fn test_create_823646394734(_a:&[u8])->Ret<(Box<dyn Test7354846353856>, usize)>{errf!("")}
fn test_json_create_823646394734(_b:&str)->Ret<Option<Box<dyn Test7354846353856>>>{errf!("")}
combi_dynlist!{ Test8364856695623,
    Uint1, Test7354846353856, test_create_823646394734, test_json_create_823646394734
}


