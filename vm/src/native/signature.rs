fn verify_signature(_: u64, buf: &[u8]) -> VmrtRes<Value> {
    let mut seek = 0usize;

    let (hash, used) = Hash::create(buf).map_ire(NativeFuncError)?;
    seek += used;

    let (addr, used) = field::Address::create(&buf[seek..]).map_ire(NativeFuncError)?;
    seek += used;

    let (sign, used) = Sign::create(&buf[seek..]).map_ire(NativeFuncError)?;
    seek += used;

    if seek != buf.len() {
        return itr_err_fmt!(
            NativeFuncError,
            "call verify_signature parse length mismatch: used {}, total {}",
            seek,
            buf.len()
        )
    }

    Ok(Value::Bool(basis::method::verify_signature(&hash, &addr, &sign)))
}

#[cfg(test)]
mod signature_native_tests {
    use super::*;

    fn argv(hash: &Hash, addr: &field::Address, sign: &Sign) -> Vec<u8> {
        let mut out = Vec::with_capacity(hash.size() + addr.size() + sign.size());
        hash.serialize_to(&mut out);
        addr.serialize_to(&mut out);
        sign.serialize_to(&mut out);
        out
    }

    #[test]
    fn verify_signature_returns_expected_bool() {
        let acc = sys::Account::create_by("vm-native-verify-signature").unwrap();
        let addr = field::Address::from(*acc.address());
        let hash = Hash::from(sys::sha3("vm-native-verify-signature"));
        let sign = Sign::create_by(&acc, &hash);

        let (ret, _) = NativeFunc::call(0, NativeFunc::idx_verify_signature, &argv(&hash, &addr, &sign))
            .unwrap();
        assert_eq!(ret, Value::Bool(true));

        let bad_hash = Hash::from(sys::sha3("vm-native-verify-signature-bad-hash"));
        let (ret, _) = NativeFunc::call(
            0,
            NativeFunc::idx_verify_signature,
            &argv(&bad_hash, &addr, &sign),
        )
        .unwrap();
        assert_eq!(ret, Value::Bool(false));

        let other = sys::Account::create_by("vm-native-verify-signature-other").unwrap();
        let other_addr = field::Address::from(*other.address());
        let (ret, _) = NativeFunc::call(
            0,
            NativeFunc::idx_verify_signature,
            &argv(&hash, &other_addr, &sign),
        )
        .unwrap();
        assert_eq!(ret, Value::Bool(false));
    }

    #[test]
    fn verify_signature_rejects_invalid_argv_length() {
        let acc = sys::Account::create_by("vm-native-verify-signature-len").unwrap();
        let addr = field::Address::from(*acc.address());
        let hash = Hash::from(sys::sha3("vm-native-verify-signature-len"));
        let sign = Sign::create_by(&acc, &hash);
        let mut data = argv(&hash, &addr, &sign);
        data.push(0xaa);
        assert!(NativeFunc::call(0, NativeFunc::idx_verify_signature, &data).is_err());
        assert!(NativeFunc::call(0, NativeFunc::idx_verify_signature, &data[..data.len() - 2]).is_err());
    }
}
