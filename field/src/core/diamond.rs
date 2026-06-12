#[repr(transparent)]
#[derive(Default, Debug, Hash, Copy, Clone, PartialEq, Eq)]
pub struct DiamondName(Fixed6);

impl DiamondName {
    pub const SIZE: usize = Fixed6::SIZE;

    pub fn is_valid(stuff: &[u8]) -> bool {
        const DIAMOND_NAME_VALID_CHARS: [u8; 16] = *b"WTYUIAHXVMEKBSZN";
        stuff.len() == 6 && stuff.iter().all(|&x| DIAMOND_NAME_VALID_CHARS.contains(&x))
    }

    fn check_bytes(stuff: &[u8]) -> Rerr {
        if Self::is_valid(stuff) {
            return Ok(());
        }
        errf!(
            "diamond name {} is not valid",
            String::from_utf8_lossy(stuff)
        )
    }

    pub fn name(&self) -> String {
        String::from_utf8(self.serialize()).unwrap()
    }
}

impl Deref for DiamondName {
    type Target = Fixed6;
    fn deref(&self) -> &Fixed6 {
        &self.0
    }
}

impl DerefMut for DiamondName {
    fn deref_mut(&mut self) -> &mut Fixed6 {
        &mut self.0
    }
}

impl AsRef<[u8]> for DiamondName {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Parse for DiamondName {
    fn parse(&mut self, buf: &[u8]) -> Ret<usize> {
        let mut raw = Fixed6::new();
        let sk = raw.parse(buf)?;
        Self::check_bytes(raw.as_ref())?;
        self.0 = raw;
        Ok(sk)
    }
}

impl Serialize for DiamondName {
    fn serialize_to(&self, out: &mut Vec<u8>) {
        self.0.serialize_to(out);
    }
    fn size(&self) -> usize {
        self.0.size()
    }
}

impl_field_only_new! {DiamondName}

impl ToJSON for DiamondName {
    fn to_json_fmt(&self, _fmt: &JSONFormater) -> String {
        format!("\"{}\"", self.name())
    }
}

impl FromJSON for DiamondName {
    fn from_json(&mut self, json: &str) -> Ret<()> {
        let raw = json_expect_quoted_decoded(json)?;
        let trimmed = raw.trim();
        if let Ok(dia) = Self::from_readable(trimmed.as_bytes()) {
            self.0 = dia.0;
            return Ok(());
        }
        let data = json_decode_binary(json)?;
        if data.len() != Self::SIZE {
            return errf!("DiamondName size mismatch: expected {} but got {}", Self::SIZE, data.len());
        }
        Self::check_bytes(&data)?;
        self.0 = Fixed6::from(data.try_into().unwrap());
        Ok(())
    }
}

impl Hex for DiamondName {
    fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl Base64 for DiamondName {
    fn to_base64(&self) -> String {
        self.0.to_base64()
    }
}

impl Readable for DiamondName {
    fn from_readable(v: &[u8]) -> Ret<Self> {
        let raw = Fixed6::from_readable(v)?;
        Self::check_bytes(raw.as_ref())?;
        Ok(Self(raw))
    }
    fn to_readable(&self) -> String {
        self.0.to_readable()
    }
    fn to_readable_left(&self) -> String {
        self.0.to_readable_left()
    }
    fn to_readable_or_hex(&self) -> String {
        self.0.to_readable_or_hex()
    }
}

impl From<[u8; 6]> for DiamondName {
    fn from(v: [u8; 6]) -> Self {
        Self(Fixed6::from(v))
    }
}

impl From<Fixed6> for DiamondName {
    fn from(v: Fixed6) -> Self {
        Self(v)
    }
}

impl std::fmt::Display for DiamondName {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.to_readable())
    }
}

impl PartialEq<Fixed6> for DiamondName {
    fn eq(&self, other: &Fixed6) -> bool {
        self.0 == *other
    }
}

impl PartialEq<DiamondName> for Fixed6 {
    fn eq(&self, other: &DiamondName) -> bool {
        *self == other.0
    }
}

// ******** DiamondNumberOptional and Auto ********

pub type DiamondNumberAuto = Fold64;
combi_optional! { DiamondNumberOptional,
    diamond: DiamondNumber
}
impl DiamondNumberAuto {
    pub fn to_diamond(&self) -> Ret<DiamondNumber> {
        let v = self.uint();
        if v > u32::MAX as u64 {
            return errf!("diamond number {} exceeds u32::MAX", v);
        }
        Ok(DiamondNumber::from(v as u32))
    }
    pub fn from_diamond(dia: &DiamondNumber) -> DiamondNumberAuto {
        DiamondNumberAuto::from(dia.uint() as u64).unwrap()
    }
}

macro_rules! define_diamond_name_list {
    ( $class: ident, $nty: ty, $max: expr ) => {
        #[derive(Default, Clone, PartialEq, Eq)]
        pub struct $class {
            count: $nty,
            lists: Vec<DiamondName>,
        }

        impl Iterator for $class {
            type Item = DiamondName;
            fn next(&mut self) -> Option<DiamondName> {
                self.pop()
            }
        }

        impl std::fmt::Debug for $class {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "[list {}]", *self.count)
            }
        }

        impl Parse for $class {
            fn parse_from(&mut self, buf: &mut &[u8]) -> Ret<usize> {
                let mut count = <$nty>::default();
                let mut seek = count.parse_from(buf)?;
                let mut lists = Vec::with_capacity($crate::prealloc_cap(*count as usize, buf.len()));
                for _ in 0..*count as usize {
                    let mut obj = DiamondName::new();
                    seek += obj.parse_from(buf)?;
                    lists.push(obj);
                }
                let tmp = Self { count, lists };
                tmp.check()?;
                self.count = tmp.count;
                self.lists = tmp.lists;
                Ok(seek)
            }
        }

        impl Serialize for $class {
            fn serialize_to(&self, out: &mut Vec<u8>) {
                self.count.serialize_to(out);
                for v in &self.lists {
                    v.serialize_to(out);
                }
            }
            fn size(&self) -> usize {
                self.count.size() + self.lists.iter().map(|v| v.size()).sum::<usize>()
            }
        }

        impl_field_only_new! {$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, _fmt: &JSONFormater) -> String {
                format!("\"{}\"", self.splitstr())
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json_str: &str) -> Ret<()> {
                let tmp = if json_str.trim().starts_with('[') {
                    let items = json_split_array(json_str)?;
                    let mut lists = Vec::with_capacity(items.len());
                    for item in items {
                        let mut obj = DiamondName::new();
                        obj.from_json(item)?;
                        lists.push(obj);
                    }
                    let count = <$nty>::from_usize(lists.len())?;
                    Self { count, lists }
                } else {
                    Self::from_readable(&json_expect_quoted_decoded(json_str)?)?
                };
                tmp.check()?;
                self.count = tmp.count;
                self.lists = tmp.lists;
                Ok(())
            }
        }

        impl $class {
            pub fn length(&self) -> usize {
                *self.count as usize
            }

            pub fn as_list(&self) -> &Vec<DiamondName> {
                &self.lists
            }

            pub fn into_list(self) -> Vec<DiamondName> {
                self.lists
            }

            pub fn replace(&mut self, i: usize, v: DiamondName) -> Rerr {
                if i >= self.length() {
                    return errf!("list index overflow");
                }
                self.lists[i] = v;
                Ok(())
            }

            pub fn drop(&mut self, i: usize) -> Rerr {
                if i >= self.length() {
                    return errf!("list index overflow");
                }
                self.count = <$nty>::from_usize(self.length() - 1)?;
                self.lists.remove(i);
                Ok(())
            }

            pub fn push(&mut self, v: DiamondName) -> Rerr {
                let n = self.length() + 1;
                self.count = <$nty>::from_usize(n)?;
                self.lists.push(v);
                Ok(())
            }

            pub fn append(&mut self, mut list: Vec<DiamondName>) -> Rerr {
                let n = self.length() + list.len();
                self.count = <$nty>::from_usize(n)?;
                self.lists.append(&mut list);
                Ok(())
            }

            pub fn pop(&mut self) -> Option<DiamondName> {
                if self.length() == 0 {
                    return None;
                }
                self.count = <$nty>::from_usize(self.length() - 1).ok()?;
                self.lists.pop()
            }

            pub fn as_mut(&mut self) -> &mut [DiamondName] {
                self.lists.as_mut_slice()
            }

            pub fn fetch_list(&mut self, n: usize) -> Ret<Vec<DiamondName>> {
                let m = self.length();
                if n > m {
                    return errf!("list data length is {} but will fetch {}", m, n);
                }
                let l = m - n;
                self.count = <$nty>::from_usize(l)?;
                Ok(self.lists.split_off(l))
            }

            pub fn fetch_head_list(&mut self, n: usize) -> Ret<Vec<DiamondName>> {
                let m = self.length();
                if n > m {
                    return errf!("list data length is {} but will fetch {}", m, n);
                }
                if n == 0 {
                    return Ok(vec![]);
                }
                if n == m {
                    self.count = <$nty>::from_usize(0)?;
                    return Ok(std::mem::take(&mut self.lists));
                }
                // Preserve FIFO extraction without repeated front-removes.
                let mut head = std::mem::take(&mut self.lists);
                let tail = head.split_off(n);
                self.count = <$nty>::from_usize(tail.len())?;
                self.lists = tail;
                Ok(head)
            }

            pub fn one(dia: DiamondName) -> Self {
                let mut obj = Self::default();
                obj.push_checked(dia).unwrap();
                obj
            }

            pub fn push_checked(&mut self, dia: DiamondName) -> Rerr {
                DiamondName::check_bytes(dia.as_ref())?;
                if self.contains(dia.as_ref()) {
                    return errf!("diamond name {} is duplicated", dia.to_readable());
                }
                if self.length() + 1 > $max {
                    return errf!("diamond list max {} overflow", $max);
                }
                self.count = <$nty>::from_usize(self.length() + 1)?;
                self.lists.push(dia);
                Ok(())
            }

            pub fn check(&self) -> Ret<usize> {
                // check len
                let setlen = *self.count as usize;
                let reallen = self.lists.len() as usize;
                if setlen != reallen {
                    return errf!(
                        "check failed: length expected {} but got {}",
                        setlen,
                        reallen
                    );
                }
                if reallen == 0 {
                    return errf!("diamonds quantity cannot be zero");
                }
                if reallen > $max {
                    return errf!("diamonds quantity cannot exceed {}", $max);
                }
                // check name + duplicate
                let mut seen = HashSet::with_capacity(reallen);
                for v in &self.lists {
                    if !DiamondName::is_valid(v.as_ref()) {
                        return errf!("diamond name {} is not valid", v.to_readable());
                    }
                    if !seen.insert(*v) {
                        return errf!("diamond name {} is duplicated", v.to_readable());
                    }
                }
                // success
                Ok(reallen)
            }

            pub fn contains(&self, x: &[u8]) -> bool {
                for v in &self.lists {
                    if x == v.as_ref() {
                        return true;
                    }
                }
                false // not find
            }

            pub fn splitstr(&self) -> String {
                self.lists
                    .iter()
                    .map(|a| a.to_readable())
                    .collect::<Vec<_>>()
                    .join(",")
            }

            pub fn readable(&self) -> String {
                self.lists
                    .iter()
                    .map(|a| a.to_readable())
                    .collect::<Vec<_>>()
                    .concat()
            }

            pub fn form(&self) -> Vec<u8> {
                self.lists
                    .iter()
                    .map(|a| a.serialize())
                    .collect::<Vec<_>>()
                    .concat()
            }

            pub fn hashset(&self) -> HashSet<DiamondName> {
                self.lists.iter().map(|a| a.clone()).collect::<HashSet<_>>()
            }

            pub fn from_readable(stuff: &str) -> Ret<$class> {
                let s = stuff
                    .replace(" ", "")
                    .replace("\n", "")
                    .replace("|", "")
                    .replace(",", "");
                if s.len() == 0 {
                    return errf!("diamond list is empty");
                }
                if s.len() % 6 != 0 {
                    return errf!("diamond list format invalid");
                }
                let num = s.len() / 6;
                if num > $max {
                    return errf!("diamond list max {} overflow", $max);
                }
                let mut obj = $class::default();
                let bs = s.as_bytes();
                for i in 0..num {
                    let x = i * 6;
                    let raw: [u8; 6] = bs[x..x + 6].try_into().unwrap();
                    let name = DiamondName::from(raw);
                    obj.push_checked(name)?;
                }
                obj.check()?;
                Ok(obj)
            }

            pub fn checked_append(&mut self, dias: Vec<DiamondName>) -> Rerr {
                for d in &dias {
                    DiamondName::check_bytes(d.as_ref())?;
                    if self.contains(d.as_ref()) {
                        return errf!("diamond name {} is duplicated", d.to_readable());
                    }
                }
                let n = self.lists.len() + dias.len();
                if n > $max {
                    return errf!("diamond list max {} overflow", $max);
                }
                let set: std::collections::HashSet<_> = dias.iter().collect();
                if set.len() != dias.len() {
                    return errf!("diamond name list contains duplicates");
                }
                self.count = <$nty>::from_usize(n)?;
                self.lists.extend(dias);
                Ok(())
            }

            pub fn from_list_checked(v: Vec<DiamondName>) -> Ret<Self> {
                let num = v.len();
                if num > $max {
                    return errf!("diamond list max {} overflow", $max);
                }
                for i in 0..num {
                    DiamondName::check_bytes(v[i].as_ref())?;
                    for j in (i + 1)..num {
                        if v[i] == v[j] {
                            return errf!("diamond name list contains duplicates");
                        }
                    }
                }
                Ok(Self {
                    count: <$nty>::from_usize(num)?,
                    lists: v,
                })
            }
        }
    };
}

define_diamond_name_list! { DiamondNameListMax200,   Uint1, 200 }
define_diamond_name_list! { DiamondNameListMax60000, Uint2, 60000 }
