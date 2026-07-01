macro_rules! uint_define {
    ($class:ident, $size:expr, $numlen:expr, $vty:ty ) => {
        concat_idents! { uint_zero = ZERO_, $class {
        #[allow(non_upper_case_globals)]
        static uint_zero: OnceLock<$class> = OnceLock::new();
        }}

        #[derive(Default, Debug, Hash, Copy, Clone, PartialEq, Eq)]
        pub struct $class {
            value: $vty,
        }

        impl Display for $class {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", self.value)
            }
        }

        impl Deref for $class {
            type Target = $vty;
            fn deref(&self) -> &$vty {
                &self.value
            }
        }

        impl AsRef<$vty> for $class {
            fn as_ref(&self) -> &$vty {
                &self.value
            }
        }

        ord_impl! {$class, value}
        compute_impl_checked! {$class, value, $vty}
        from_uint_all! {$class, value, $vty}

        impl Parse for $class {
            fn parse(&mut self, buf: &[u8]) -> Ret<usize> {
                let bts = bufeat_ref(buf, $size)?;
                let mut full = [0u8; $numlen];
                let start = $numlen - $size;
                full[start..].copy_from_slice(bts);
                self.value = <$vty>::from_be_bytes(full);
                if self.value > Self::MAX {
                    return errf!(
                        "{} parse value {} exceeds max {}",
                        stringify!($class),
                        self.value,
                        Self::MAX
                    );
                }
                Ok($size)
            }
        }

        impl Serialize for $class {
            fn serialize_to(&self, out: &mut Vec<u8>) {
                if self.value > Self::MAX {
                    never!() // fatal error!!!
                }
                let bts = self.to_bytes();
                out.extend_from_slice(&bts);
            }
            fn size(&self) -> usize {
                $size
            }
        }

        impl_field_only_new! {$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, _fmt: &JSONFormater) -> String {
                self.value.to_string()
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json: &str) -> Ret<()> {
                let s = json_expect_unquoted(json)?;
                if let Ok(v) = s.parse::<$vty>() {
                    if v > Self::MAX {
                        return errf!(
                            "{} value {} exceeds max {}",
                            stringify!($class),
                            v,
                            Self::MAX
                        );
                    }
                    self.value = v;
                    return Ok(());
                }
                errf!("cannot parse uint from: {}", s)
            }
        }

        impl $class {
            pub const MAX: $vty = maybe!(
                $size == $numlen,
                <$vty>::MAX,
                ((1u128 << ($size * 8)) - 1) as $vty
            );
            pub const SIZE: usize = $size as usize;

            pub fn zero_ref() -> &'static Self {
                concat_idents! { uint_zero = ZERO_, $class {
                uint_zero.get_or_init(||Self::from(0))
                }}
            }

            pub const fn from(v: $vty) -> Self {
                if v > Self::MAX {
                    panic!(concat!(stringify!($class), " overflow: value exceeds MAX"))
                }
                Self { value: v }
            }

            pub fn from_checked(v: $vty) -> Option<Self> {
                if v > Self::MAX {
                    return None;
                }
                Some(Self { value: v })
            }

            pub fn from_usize(v: usize) -> Ret<Self> {
                // Use u128 comparison to avoid truncation on 32-bit platforms
                if (v as u128) > (Self::MAX as u128) {
                    return errf!(
                        "{} value {} exceeds max {}",
                        stringify!($class),
                        v,
                        Self::MAX
                    );
                }
                Ok(Self { value: v as $vty })
            }

            pub fn uint(&self) -> $vty {
                self.value
            }

            pub fn to_uint(&self) -> $vty {
                self.value
            }

            pub fn as_uint(&self) -> &$vty {
                &self.value
            }

            pub fn to_vec(&self) -> Vec<u8> {
                self.to_bytes().into()
            }

            pub fn to_bytes(&self) -> [u8; $size] {
                if self.value > Self::MAX {
                    never!() // fatal error!!!
                }
                let mut real = [0u8; $size];
                let bts = <$vty>::to_be_bytes(self.value);
                for x in 1..=$size {
                    real[$size - x] = bts[$numlen - x];
                }
                // println!("Uint to_bytes size {} bts {} real {}", $size, hex::encode(bts), hex::encode(real));
                real
            }

            pub fn checked(self) -> Ret<Self> {
                if self.value > Self::MAX {
                    return errf!(
                        "{} value {} exceeds max {}",
                        stringify!($class),
                        self.value,
                        Self::MAX
                    );
                }
                Ok(self)
            }
        }
    };
}

/*
* define
*/
uint_define! {Uint1, 1, 1, u8}
uint_define! {Uint2, 2, 2, u16}
uint_define! {Uint3, 3, 4, u32}
uint_define! {Uint4, 4, 4, u32}
uint_define! {Uint5, 5, 8, u64}
uint_define! {Uint6, 6, 8, u64}
uint_define! {Uint7, 7, 8, u64}
uint_define! {Uint8, 8, 8, u64}
uint_define! {Uint10, 10, 16, u128}
uint_define! {Uint12, 12, 16, u128}
uint_define! {Uint16, 16, 16, u128}

impl ParsePrefix for Uint1 {
    fn create_with_prefix(prefix: &[u8], _rest: &[u8]) -> Ret<(Self, usize)> {
        if prefix.is_empty() {
            return errf!("Uint1 prefix is empty");
        }
        let v = Uint1::from(prefix[0]);
        Ok((v, 1))
    }
}

/************************ test ************************/

#[cfg(test)]
mod uint_tests {
    use super::*;

    macro_rules! uint_test_one {
        ($ty: ty, $v: expr) => {{
            let u1 = <$ty>::from($v);
            let mut u1f = <$ty>::from(0);
            let _ = u1f.parse(&u1.serialize());
            assert_eq!(u1, u1f);
        }};
    }

    #[test]
    fn test2() {
        uint_test_one!(Uint1, 0);
        uint_test_one!(Uint1, 1);
        uint_test_one!(Uint1, 2);
        uint_test_one!(Uint1, 3);
        uint_test_one!(Uint1, 77);
        uint_test_one!(Uint1, 100);
        uint_test_one!(Uint1, 150);
        uint_test_one!(Uint1, 254);
        uint_test_one!(Uint1, 255);

        uint_test_one!(Uint2, 0);
        uint_test_one!(Uint2, 1);
        uint_test_one!(Uint2, 2);
        uint_test_one!(Uint2, 3);
        uint_test_one!(Uint2, 65534);
        uint_test_one!(Uint2, 65535);

        let m3: u32 = 256 * 256 * 256 - 1;
        uint_test_one!(Uint3, 0);
        uint_test_one!(Uint3, 1);
        uint_test_one!(Uint3, 1000);
        uint_test_one!(Uint3, m3 - 2);
        uint_test_one!(Uint3, m3 - 1);
        uint_test_one!(Uint3, m3);

        let m4: u32 = u32::MAX;
        uint_test_one!(Uint4, 0);
        uint_test_one!(Uint4, 1);
        uint_test_one!(Uint4, 74563000);
        uint_test_one!(Uint4, m4 - 2);
        uint_test_one!(Uint4, m4 - 1);
        uint_test_one!(Uint4, m4);

        let m5: u64 = 256 * 256 * 256 * 256 * 256 - 1;
        uint_test_one!(Uint5, 0);
        uint_test_one!(Uint5, 1);
        uint_test_one!(Uint5, 740345600);
        uint_test_one!(Uint5, m5 - 2);
        uint_test_one!(Uint5, m5 - 1);
        uint_test_one!(Uint5, m5);

        let m6: u64 = 256 * 256 * 256 * 256 * 256 * 256 - 1;
        uint_test_one!(Uint6, 0);
        uint_test_one!(Uint6, 1);
        uint_test_one!(Uint6, 7404534566600);
        uint_test_one!(Uint6, m6 - 2);
        uint_test_one!(Uint6, m6 - 1);
        uint_test_one!(Uint6, m6);

        let m7: u64 = 256 * 256 * 256 * 256 * 256 * 256 * 256 - 1;
        uint_test_one!(Uint7, 0);
        uint_test_one!(Uint7, 1);
        uint_test_one!(Uint7, 7434505674564500);
        uint_test_one!(Uint7, m7 - 2);
        uint_test_one!(Uint7, m7 - 1);
        uint_test_one!(Uint7, m7);

        let m8: u64 = u64::MAX;
        uint_test_one!(Uint8, 0);
        uint_test_one!(Uint8, 1);
        uint_test_one!(Uint8, 7408487745635624600);
        uint_test_one!(Uint8, m8 - 2);
        uint_test_one!(Uint8, m8 - 1);
        uint_test_one!(Uint8, m8);
    }

    #[test]
    fn test_checked_overflow() {
        // Create invalid Uint via transmute to verify checked() rejects overflow
        let ov3: Uint3 = unsafe { std::mem::transmute(1u32 << 24) };
        assert!(ov3.checked().is_err());
        let ov5: Uint5 = unsafe { std::mem::transmute(1u64 << 40) };
        assert!(ov5.checked().is_err());
    }

    #[test]
    fn test_from_rejects_overflow() {
        let result = std::panic::catch_unwind(|| {
            let _ = Uint3::from(1u32 << 24);
        });
        assert!(result.is_err());
        let result = std::panic::catch_unwind(|| {
            let _ = Uint5::from(1u64 << 40);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_max_constants() {
        assert_eq!(Uint3::MAX, (1u32 << 24) - 1);
        assert_eq!(Uint5::MAX, (1u64 << 40) - 1);
        assert_eq!(Uint7::MAX, (1u64 << 56) - 1);
        assert_eq!(Uint8::MAX, u64::MAX);
    }

    #[test]
    fn test_from_signed_rejects_negative() {
        let result = std::panic::catch_unwind(|| {
            let _: Uint3 = (-1i32).into();
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_arithmetic_overflow_panics() {
        let a = Uint3::from(15_000_000);
        let b = Uint3::from(5_000_000);
        let result = std::panic::catch_unwind(|| {
            let _ = a + b; // 20M > Uint3::MAX
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_from_json_rejects_overflow() {
        let mut u = Uint3::default();
        assert!(u.from_json("16777216").is_err());
    }

    #[test]
    fn test_from_usize_platform_independent() {
        // Use u128 comparison so 32/64-bit platforms behave the same
        assert!(Uint3::from_usize(16777215).is_ok());
        assert!(Uint3::from_usize(16777216).is_err());
        assert!(Uint5::from_usize(1099511627775).is_ok()); // 2^40 - 1
        assert!(Uint5::from_usize(1099511627776).is_err());
    }

    #[test]
    fn test_from_unsigned_rejects_overflow() {
        let result = std::panic::catch_unwind(|| {
            let _: Uint3 = 0xFFFFFFFFu32.into();
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_add_assign_overflow_panics() {
        let result = std::panic::catch_unwind(|| {
            let mut a = Uint3::from(15_000_000);
            a += Uint3::from(5_000_000);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_div_by_zero_panics() {
        let result = std::panic::catch_unwind(|| {
            let _ = Uint3::from(1) / Uint3::from(0);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_div_by_zero_scalar_panics() {
        let result = std::panic::catch_unwind(|| {
            let _ = Uint3::from(1) / 0u32;
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_serialize_roundtrip_at_max() {
        let u = Uint1::from(Uint1::MAX);
        let ser = u.serialize();
        let mut p = Uint1::default();
        assert_eq!(p.parse(&ser).unwrap(), Uint1::SIZE);
        assert_eq!(p.uint(), Uint1::MAX);

        let u = Uint2::from(Uint2::MAX);
        let ser = u.serialize();
        let mut p = Uint2::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint2::MAX);

        let u = Uint3::from(Uint3::MAX);
        let ser = u.serialize();
        let mut p = Uint3::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint3::MAX);

        let u = Uint4::from(Uint4::MAX);
        let ser = u.serialize();
        let mut p = Uint4::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint4::MAX);

        let u = Uint5::from(Uint5::MAX);
        let ser = u.serialize();
        let mut p = Uint5::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint5::MAX);

        let u = Uint6::from(Uint6::MAX);
        let ser = u.serialize();
        let mut p = Uint6::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint6::MAX);

        let u = Uint7::from(Uint7::MAX);
        let ser = u.serialize();
        let mut p = Uint7::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint7::MAX);

        let u = Uint8::from(Uint8::MAX);
        let ser = u.serialize();
        let mut p = Uint8::default();
        p.parse(&ser).unwrap();
        assert_eq!(p.uint(), Uint8::MAX);
    }

    #[test]
    fn test_parse_rejects_insufficient_buffer() {
        let mut u = Uint4::default();
        assert!(u.parse(&[0u8; 1]).is_err());
        assert!(u.parse(&[0u8; 2]).is_err());
        assert!(u.parse(&[0u8; 3]).is_err());
    }

    #[test]
    fn test_sub_overflow_panics() {
        let result = std::panic::catch_unwind(|| {
            let _ = Uint3::from(0) - Uint3::from(1);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_mul_overflow_panics() {
        let result = std::panic::catch_unwind(|| {
            let _ = Uint3::from(5000) * Uint3::from(5000);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_size_matches_len() {
        for v in [0u32, 1, 100, 1000000, Uint3::MAX] {
            let u = Uint3::from(v);
            let ser = u.serialize();
            assert_eq!(ser.len(), Uint3::SIZE);
        }
    }

    #[test]
    fn test_uint_boundary_values_roundtrip() {
        let boundaries: [(u32, u32); 4] = [
            (0, Uint3::MAX),
            (1, Uint3::MAX - 1),
            (256 * 256 - 1, 256 * 256),
            (256 * 256 * 256 - 2, 256 * 256 * 256 - 1),
        ];
        for (a, b) in boundaries {
            let u = Uint3::from(a);
            let ser = u.serialize();
            let mut p = Uint3::default();
            p.parse(&ser).unwrap();
            assert_eq!(p.uint(), a);

            let u = Uint3::from(b);
            let ser = u.serialize();
            let mut p = Uint3::default();
            p.parse(&ser).unwrap();
            assert_eq!(p.uint(), b);
        }
    }
}
