/////////////////////// opset ///////////////////////

fn locop_arithmetic<F>(x: &mut Value, y: &mut Value, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value) -> VmrtRes<Value>,
{
    let (lx, ry) = Value::arithmetic_args2(x, y)?;
    let v = f(&lx, &ry)?;
    *x = v;
    Ok(())
}

/* * *   such as: v = x + y */
fn binop_arithmetic<F>(operand_stack: &mut Stack, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value) -> VmrtRes<Value>,
{
    let mut y = operand_stack.pop()?;
    let x = operand_stack.peek()?;
    locop_arithmetic(x, &mut y, f)
}

fn locop_arithmetic3<F>(x: &mut Value, y: &mut Value, z: &mut Value, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value, &Value) -> VmrtRes<Value>,
{
    let (lx, my, rz) = Value::arithmetic_args3(x, y, z)?;
    let v = f(&lx, &my, &rz)?;
    *x = v;
    Ok(())
}

fn triop_arithmetic<F>(operand_stack: &mut Stack, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value, &Value) -> VmrtRes<Value>,
{
    let mut z = operand_stack.pop()?;
    let mut y = operand_stack.pop()?;
    let x = operand_stack.peek()?;
    locop_arithmetic3(x, &mut y, &mut z, f)
}

fn locop_arithmetic4<F>(x: &mut Value, y: &mut Value, z: &mut Value, w: &mut Value, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value, &Value, &Value) -> VmrtRes<Value>,
{
    let (lx, my, rz, qw) = Value::arithmetic_args4(x, y, z, w)?;
    let v = f(&lx, &my, &rz, &qw)?;
    *x = v;
    Ok(())
}

fn quadop_arithmetic<F>(operand_stack: &mut Stack, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value, &Value, &Value) -> VmrtRes<Value>,
{
    let mut w = operand_stack.pop()?;
    let mut z = operand_stack.pop()?;
    let mut y = operand_stack.pop()?;
    let x = operand_stack.peek()?;
    locop_arithmetic4(x, &mut y, &mut z, &mut w, f)
}

/* * *   binop_between *   such as: v = x && y */

fn locop_btw<F>(x: &mut Value, y: &mut Value, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value) -> VmrtRes<Value>,
{
    let v = f(&x, &y)?;
    *x = v;
    Ok(())
}

fn binop_btw<F>(operand_stack: &mut Stack, f: F) -> VmrtErr
where
    F: FnOnce(&Value, &Value) -> VmrtRes<Value>,
{
    let mut y = operand_stack.pop()?;
    let x = operand_stack.peek()?;
    locop_btw(x, &mut y, f)
}

macro_rules! bitop {
    ( $x: expr, $y: expr, $op: ident ) => {
        Ok(match ($x, $y) {
            (U8(l), U8(r)) => Value::U8((*l).$op(*r)),
            (U16(l), U16(r)) => Value::U16((*l).$op(*r)),
            (U32(l), U32(r)) => Value::U32((*l).$op(*r)),
            (U64(l), U64(r)) => Value::U64((*l).$op(*r)),
            (U128(l), U128(r)) => Value::U128((*l).$op(*r)),
            (_, _) => {
                return itr_err_fmt!(
                    Arithmetic,
                    "cannot do bit ops between {:?} and {:?}",
                    $x,
                    $y
                )
            }
        })
    };
}

macro_rules! ahmtdo {
    ( $x: expr, $y: expr, $op: ident ) => {
        match ($x, $y) {
            (U8(l), U8(r)) => <u8>::$op(*l, *r).map(Value::U8),
            (U16(l), U16(r)) => <u16>::$op(*l, *r).map(Value::U16),
            (U32(l), U32(r)) => <u32>::$op(*l, *r).map(Value::U32),
            (U64(l), U64(r)) => <u64>::$op(*l, *r).map(Value::U64),
            (U128(l), U128(r)) => <u128>::$op(*l, *r).map(Value::U128),
            (_, _) => {
                return itr_err_fmt!(
                    Arithmetic,
                    "cannot do arithmetic between {:?} and {:?}",
                    $x,
                    $y
                )
            }
        }
    };
}

/////////////////////// logic ///////////////////////

fn check_failed_tip(op: &str, x: &Value, y: &Value) -> String {
    format!("arithmetic {} check failed with {:?} and {:?}", op, x, y)
}

fn check_failed_tip3(op: &str, x: &Value, y: &Value, z: &Value) -> String {
    format!(
        "arithmetic {} check failed with {:?}, {:?} and {:?}",
        op, x, y, z
    )
}

fn check_failed_tip4(op: &str, x: &Value, y: &Value, z: &Value, w: &Value) -> String {
    format!(
        "arithmetic {} check failed with {:?}, {:?}, {:?} and {:?}",
        op, x, y, z, w
    )
}

fn check_failed_tip1(op: &str, x: &Value) -> String {
    format!("arithmetic {} check failed with {:?}", op, x)
}

#[inline]
fn cast_uint_like_sample(sample: &Value, out: u128, err: impl Fn() -> ItrErr) -> VmrtRes<Value> {
    Ok(match sample {
        U8(..) => Value::U8(u8::try_from(out).map_err(|_| err())?),
        U16(..) => Value::U16(u16::try_from(out).map_err(|_| err())?),
        U32(..) => Value::U32(u32::try_from(out).map_err(|_| err())?),
        U64(..) => Value::U64(u64::try_from(out).map_err(|_| err())?),
        U128(..) => Value::U128(out),
        _ => return Err(err()),
    })
}

#[inline]
fn require_nonzero_u128(d: u128, err: impl Fn() -> ItrErr) -> VmrtRes<u128> {
    if d == 0 {
        Err(err())
    } else {
        Ok(d)
    }
}

#[inline]
fn half_up_round_u128_threshold(div: u128) -> u128 {
    div - div / 2
}

#[inline]
fn ceil_quot_if_rem_u128(quo: u128, rem: u128, err: impl Fn() -> ItrErr) -> VmrtRes<u128> {
    if rem == 0 {
        Ok(quo)
    } else {
        quo.checked_add(1).ok_or_else(err)
    }
}

fn lgc_and(x: &Value, y: &Value) -> VmrtRes<Value> {
    let lx = x.extract_bool()?;
    let ry = y.extract_bool()?;
    Ok(Value::bool(lx && ry))
}

fn lgc_or(x: &Value, y: &Value) -> VmrtRes<Value> {
    let lx = x.extract_bool()?;
    let ry = y.extract_bool()?;
    Ok(Value::bool(lx || ry))
}

#[allow(unused)]
fn lgc_not(x: &Value) -> VmrtRes<Value> {
    let v = x.extract_bool()?;
    Ok(Value::bool(!v))
}

fn lgc_equal_bool(x: &Value, y: &Value) -> VmrtRes<bool> {
    value_content_eq(x, y)
}

fn lgc_compare_fee(x: &Value, y: &Value, gas_extra: &GasExtra) -> usize {
    value_compare_fee(x, y, gas_extra.container_cmp_header)
}

fn lgc_equal(x: &Value, y: &Value) -> VmrtRes<Value> {
    Ok(Value::bool(lgc_equal_bool(x, y)?))
}

fn lgc_not_equal(x: &Value, y: &Value) -> VmrtRes<Value> {
    Ok(Value::bool(!lgc_equal_bool(x, y)?))
}

fn lgc_ord_cmp<F>(x: &Value, y: &Value, f: F) -> VmrtRes<Value>
where
    F: FnOnce(u128, u128) -> bool,
{
    if !x.is_uint() || !y.is_uint() {
        return itr_err_fmt!(
            Arithmetic,
            "ordered compare only supports uint operands, got {:?} and {:?}",
            x,
            y
        );
    }
    let lx = x.extract_u128()?;
    let ry = y.extract_u128()?;
    Ok(Value::bool(f(lx, ry)))
}

fn lgc_less(x: &Value, y: &Value) -> VmrtRes<Value> {
    lgc_ord_cmp(x, y, |l, r| l < r)
}

fn lgc_less_equal(x: &Value, y: &Value) -> VmrtRes<Value> {
    lgc_ord_cmp(x, y, |l, r| l <= r)
}

fn lgc_greater(x: &Value, y: &Value) -> VmrtRes<Value> {
    lgc_ord_cmp(x, y, |l, r| l > r)
}

fn lgc_greater_equal(x: &Value, y: &Value) -> VmrtRes<Value> {
    lgc_ord_cmp(x, y, |l, r| l >= r)
}

fn bit_and(x: &Value, y: &Value) -> VmrtRes<Value> {
    bitop!(x, y, bitand)
}

fn bit_or(x: &Value, y: &Value) -> VmrtRes<Value> {
    bitop!(x, y, bitor)
}

fn bit_xor(x: &Value, y: &Value) -> VmrtRes<Value> {
    bitop!(x, y, bitxor)
}

fn bit_shift_overflow(op: &str, x: &Value, y: &Value) -> ItrErr {
    ItrErr::new(
        Arithmetic,
        &format!("bit {} shift overflow between {:?} and {:?}", op, x, y),
    )
}

fn bit_shl(x: &Value, y: &Value) -> VmrtRes<Value> {
    let res = match (x, y) {
        (U8(l), U8(r)) => <u8>::checked_shl(*l, *r as u32).map(Value::U8),
        (U16(l), U16(r)) => <u16>::checked_shl(*l, *r as u32).map(Value::U16),
        (U32(l), U32(r)) => <u32>::checked_shl(*l, *r as u32).map(Value::U32),
        (U64(l), U64(r)) => {
            let s = u32::try_from(*r).map_err(|_| bit_shift_overflow("left", x, y))?;
            <u64>::checked_shl(*l, s).map(Value::U64)
        }
        (U128(l), U128(r)) => {
            if *r > u32::MAX as u128 {
                return Err(bit_shift_overflow("left", x, y));
            }
            <u128>::checked_shl(*l, *r as u32).map(Value::U128)
        }
        (_, _) => return itr_err_fmt!(Arithmetic, "cannot do bit ops between {:?} and {:?}", x, y),
    };
    res.ok_or_else(|| bit_shift_overflow("left", x, y))
}

fn bit_shr(x: &Value, y: &Value) -> VmrtRes<Value> {
    let res = match (x, y) {
        (U8(l), U8(r)) => <u8>::checked_shr(*l, *r as u32).map(Value::U8),
        (U16(l), U16(r)) => <u16>::checked_shr(*l, *r as u32).map(Value::U16),
        (U32(l), U32(r)) => <u32>::checked_shr(*l, *r as u32).map(Value::U32),
        (U64(l), U64(r)) => {
            let s = u32::try_from(*r).map_err(|_| bit_shift_overflow("right", x, y))?;
            <u64>::checked_shr(*l, s).map(Value::U64)
        }
        (U128(l), U128(r)) => {
            if *r > u32::MAX as u128 {
                return Err(bit_shift_overflow("right", x, y));
            }
            <u128>::checked_shr(*l, *r as u32).map(Value::U128)
        }
        (_, _) => return itr_err_fmt!(Arithmetic, "cannot do bit ops between {:?} and {:?}", x, y),
    };
    res.ok_or_else(|| bit_shift_overflow("right", x, y))
}

/////////////////////// arithmetic ///////////////////////

macro_rules! ahmtdocheck {
    ( $x: expr, $y: expr, $op: ident, $tip: expr ) => {
        ahmtdo!($x, $y, $op).ok_or_else(|| ItrErr::new(Arithmetic, &check_failed_tip($tip, $x, $y)))
    };
}

fn add_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    ahmtdocheck!(x, y, checked_add, "add")
}

fn sub_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    ahmtdocheck!(x, y, checked_sub, "sub")
}

fn mul_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    ahmtdocheck!(x, y, checked_mul, "mul")
}

fn div_checked(x: &Value, y: &Value, op: &'static str) -> VmrtRes<Value> {
    ahmtdocheck!(x, y, checked_div, op)
}

fn div_up_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    div_with_round_checked(x, y, FinRoundPolicy::Ceil, "div_up")
}

fn div_exact_op_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    div_with_round_checked(x, y, FinRoundPolicy::Exact, "div_exact_op")
}

fn mod_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    ahmtdocheck!(x, y, checked_rem, "mod") // rem = mod
}

#[inline(always)]
fn add_mod_u128(a: u128, b: u128, m: u128) -> u128 {
    let lhs = a % m;
    let rhs = b % m;
    let gap = m - rhs;
    if lhs >= gap {
        lhs - gap
    } else {
        lhs + rhs
    }
}

fn mul_mod_u128(a: u128, b: u128, m: u128) -> u128 {
    let mut lhs = a % m;
    let mut rhs = b % m;
    let mut out = 0u128;
    while rhs != 0 {
        if rhs & 1 == 1 {
            out = add_mod_u128(out, lhs, m);
        }
        rhs >>= 1;
        if rhs != 0 {
            lhs = add_mod_u128(lhs, lhs, m);
        }
    }
    out
}

#[inline(always)]
fn low_bits_mask(bits: u32) -> u128 {
    match bits {
        0 => 0,
        128.. => u128::MAX,
        _ => (1u128 << bits) - 1,
    }
}

#[inline(always)]
fn mul_wide_u128(a: u128, b: u128) -> (u128, u128) {
    let (lo, hi) = a.carrying_mul(b, 0);
    (hi, lo)
}

fn add_u256_u128(hi: u128, lo: u128, add: u128) -> Option<(u128, u128)> {
    let (lo, carry) = lo.overflowing_add(add);
    Some((hi.checked_add(carry as u128)?, lo))
}

fn add_u256(ahi: u128, alo: u128, bhi: u128, blo: u128) -> Option<(u128, u128)> {
    let (lo, carry) = alo.overflowing_add(blo);
    let hi = ahi.checked_add(bhi)?.checked_add(carry as u128)?;
    Some((hi, lo))
}

fn sub_u256(ahi: u128, alo: u128, bhi: u128, blo: u128) -> Option<(u128, u128)> {
    let (lo, borrow) = alo.overflowing_sub(blo);
    let hi = ahi.checked_sub(bhi)?.checked_sub(borrow as u128)?;
    Some((hi, lo))
}

fn sub_u256_u128(hi: u128, lo: u128, sub: u128) -> Option<(u128, u128)> {
    sub_u256(hi, lo, 0, sub)
}

/// (x*y) ± z as u256; error if the result does not fit u128 (high limb non-zero).
fn mul_xy_addsub_z_fit_u128(
    x: u128,
    y: u128,
    z: u128,
    add_z: bool,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    let (hi, lo) = mul_wide_u128(x, y);
    let (hi, lo) = if add_z {
        add_u256_u128(hi, lo, z).ok_or_else(|| err())?
    } else {
        sub_u256_u128(hi, lo, z).ok_or_else(|| err())?
    };
    if hi != 0 {
        return Err(err());
    }
    Ok(lo)
}

/// Truncating ((x*y) ± z) / div; div must be non-zero.
fn mul_xy_addsub_z_div_u128(
    x: u128,
    y: u128,
    z: u128,
    div: u128,
    add_z: bool,
    round: FinRoundPolicy,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    let (hi, lo) = mul_wide_u128(x, y);
    let (hi, lo) = if add_z {
        add_u256_u128(hi, lo, z).ok_or_else(|| err())?
    } else {
        sub_u256_u128(hi, lo, z).ok_or_else(|| err())?
    };
    div_u256_by_u128_with_round(hi, lo, div, round, err)
}

fn div_u256_by_u128_to_u128(hi: u128, lo: u128, d: u128) -> Option<(u128, u128)> {
    if d == 0 || hi >= d {
        return None;
    }
    let mut rem = hi;
    let mut quo = 0u128;
    for shift in (0..128).rev() {
        let carry = rem >> 127;
        rem = (rem << 1) | ((lo >> shift) & 1);
        if carry != 0 || rem >= d {
            rem = rem.wrapping_sub(d);
            quo |= 1u128 << shift;
        }
    }
    Some((quo, rem))
}

fn div_u256_by_u129_to_u128(
    n_hi: u128,
    n_lo: u128,
    d_hi: u128,
    d_lo: u128,
) -> Option<(u128, u128, u128)> {
    if d_hi > 1 || (d_hi == 0 && d_lo == 0) {
        return None;
    }
    if d_hi == 0 {
        let (quo, rem) = div_u256_by_u128_to_u128(n_hi, n_lo, d_lo)?;
        return Some((quo, 0, rem));
    }
    let mut quo = 0u128;
    let mut rem_hi = 0u128;
    let mut rem_lo = 0u128;
    for shift in (0..256).rev() {
        let next_bit = if shift >= 128 {
            (n_hi >> (shift - 128)) & 1
        } else {
            (n_lo >> shift) & 1
        };
        let carry = rem_lo >> 127;
        rem_hi = (rem_hi << 1) | carry;
        rem_lo = (rem_lo << 1) | next_bit;
        if cmp_u256(rem_hi, rem_lo, d_hi, d_lo).is_ge() {
            let (new_hi, new_lo) = sub_u256(rem_hi, rem_lo, d_hi, d_lo)?;
            rem_hi = new_hi;
            rem_lo = new_lo;
            if shift >= 128 {
                return None;
            }
            quo |= 1u128 << shift;
        }
    }
    Some((quo, rem_hi, rem_lo))
}

fn shr_u256_to_u128(hi: u128, lo: u128, shift: u32) -> Option<(u128, bool)> {
    match shift {
        0 => {
            if hi == 0 {
                Some((lo, false))
            } else {
                None
            }
        }
        1..=127 => {
            if hi >> shift != 0 {
                return None;
            }
            let out = (hi << (128 - shift)) | (lo >> shift);
            let dropped = lo & low_bits_mask(shift) != 0;
            Some((out, dropped))
        }
        128 => Some((hi, lo != 0)),
        129..=255 => {
            let rhs = shift - 128;
            let out = hi >> rhs;
            let dropped = lo != 0 || hi & low_bits_mask(rhs) != 0;
            Some((out, dropped))
        }
        _ => None,
    }
}

fn mul_u256_u128_to_u256_checked(hi: u128, lo: u128, mul: u128) -> Option<(u128, u128)> {
    let (lo_hi, lo_lo) = mul_wide_u128(lo, mul);
    let (hi_hi, hi_lo) = mul_wide_u128(hi, mul);
    if hi_hi != 0 {
        return None;
    }
    let out_hi = hi_lo.checked_add(lo_hi)?;
    Some((out_hi, lo_lo))
}

fn cmp_u256(ahi: u128, alo: u128, bhi: u128, blo: u128) -> std::cmp::Ordering {
    ahi.cmp(&bhi).then(alo.cmp(&blo))
}

fn isqrt_u256_floor(hi: u128, lo: u128) -> u128 {
    if hi == 0 {
        return lo.isqrt();
    }
    let mut out = 0u128;
    let mut bit = 1u128 << 127;
    while bit != 0 {
        let candidate = out | bit;
        let (sq_hi, sq_lo) = mul_wide_u128(candidate, candidate);
        if cmp_u256(sq_hi, sq_lo, hi, lo).is_le() {
            out = candidate;
        }
        bit >>= 1;
    }
    out
}

fn cast_uint_result1(sample: &Value, out: u128, op: &str, x: &Value) -> VmrtRes<Value> {
    cast_uint_like_sample(sample, out, || ItrErr::new(Arithmetic, &check_failed_tip1(op, x)))
}

fn cast_uint_result2(sample: &Value, out: u128, op: &str, x: &Value, y: &Value) -> VmrtRes<Value> {
    cast_uint_like_sample(sample, out, || ItrErr::new(Arithmetic, &check_failed_tip(op, x, y)))
}

fn cast_uint_result3(
    sample: &Value,
    out: u128,
    op: &str,
    x: &Value,
    y: &Value,
    z: &Value,
) -> VmrtRes<Value> {
    cast_uint_like_sample(sample, out, || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z)))
}

fn cast_uint_result4(
    sample: &Value,
    out: u128,
    op: &str,
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
) -> VmrtRes<Value> {
    cast_uint_like_sample(sample, out, || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w)))
}

fn round_half_up_div_u256_by_u128(
    hi: u128,
    lo: u128,
    d: u128,
    op: &str,
    x: &Value,
    y: &Value,
    z: &Value,
) -> VmrtRes<u128> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    let (mut quo, rem) = div_u256_by_u128_to_u128(hi, lo, d).ok_or_else(err)?;
    let threshold = half_up_round_u128_threshold(d);
    if rem >= threshold {
        quo = quo.checked_add(1).ok_or_else(err)?;
    }
    Ok(quo)
}

fn round_half_even_quot_u128(
    mut quo: u128,
    rem: u128,
    d: u128,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    if rem == 0 {
        return Ok(quo);
    }
    let cmp_half = if rem > u128::MAX / 2 {
        std::cmp::Ordering::Greater
    } else {
        rem.checked_mul(2).ok_or_else(&err)?.cmp(&d)
    };
    if cmp_half.is_gt() || (cmp_half.is_eq() && quo & 1 == 1) {
        quo = quo.checked_add(1).ok_or_else(err)?;
    }
    Ok(quo)
}

fn round_quot_u128_with_policy(
    mut quo: u128,
    rem: u128,
    d: u128,
    round: FinRoundPolicy,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    match round {
        FinRoundPolicy::Exact => {
            if rem != 0 {
                return Err(err());
            }
        }
        FinRoundPolicy::Floor => {}
        FinRoundPolicy::Ceil => {
            quo = ceil_quot_if_rem_u128(quo, rem, &err)?;
        }
        FinRoundPolicy::HalfUp => {
            if rem >= half_up_round_u128_threshold(d) {
                quo = quo.checked_add(1).ok_or_else(&err)?;
            }
        }
        FinRoundPolicy::HalfEven => {
            quo = round_half_even_quot_u128(quo, rem, d, &err)?;
        }
    }
    Ok(quo)
}

fn div_u256_by_u128_with_round(
    hi: u128,
    lo: u128,
    d: u128,
    round: FinRoundPolicy,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    let (quo, rem) = div_u256_by_u128_to_u128(hi, lo, d).ok_or_else(&err)?;
    round_quot_u128_with_policy(quo, rem, d, round, err)
}

fn div_u256_by_u129_with_round(
    hi: u128,
    lo: u128,
    d_hi: u128,
    d_lo: u128,
    round: FinRoundPolicy,
    half_even_parity_offset: u128,
    err: impl Fn() -> ItrErr,
) -> VmrtRes<u128> {
    let (mut quo, rem_hi, rem_lo) = div_u256_by_u129_to_u128(hi, lo, d_hi, d_lo).ok_or_else(&err)?;
    let has_rem = rem_hi != 0 || rem_lo != 0;
    match round {
        FinRoundPolicy::Exact => {
            if has_rem {
                return Err(err());
            }
        }
        FinRoundPolicy::Floor => {}
        FinRoundPolicy::Ceil => {
            if has_rem {
                quo = quo.checked_add(1).ok_or_else(&err)?;
            }
        }
        FinRoundPolicy::HalfUp => {
            let (dbl_hi, dbl_lo) = add_u256(rem_hi, rem_lo, rem_hi, rem_lo).ok_or_else(&err)?;
            if cmp_u256(dbl_hi, dbl_lo, d_hi, d_lo).is_ge() {
                quo = quo.checked_add(1).ok_or_else(&err)?;
            }
        }
        FinRoundPolicy::HalfEven => {
            let (dbl_hi, dbl_lo) = add_u256(rem_hi, rem_lo, rem_hi, rem_lo).ok_or_else(&err)?;
            let cmp_half = cmp_u256(dbl_hi, dbl_lo, d_hi, d_lo);
            let final_quo_is_odd = (quo & 1) != (half_even_parity_offset & 1);
            if cmp_half.is_gt() || (cmp_half.is_eq() && final_quo_is_odd) {
                quo = quo.checked_add(1).ok_or_else(&err)?;
            }
        }
    }
    Ok(quo)
}

fn mul_div_half_up(
    lhs: u128,
    rhs: u128,
    d: u128,
    op: &str,
    x: &Value,
    y: &Value,
    z: &Value,
) -> VmrtRes<u128> {
    let (hi, lo) = mul_wide_u128(lhs, rhs);
    round_half_up_div_u256_by_u128(hi, lo, d, op, x, y, z)
}

fn scaled_abs_diff_div_u128(x: u128, reference: u128, scale: u128) -> Option<(u128, u128)> {
    if reference == 0 || scale == 0 {
        return None;
    }
    let diff = x.abs_diff(reference);
    let (hi, lo) = mul_wide_u128(diff, scale);
    div_u256_by_u128_to_u128(hi, lo, reference)
}

fn saturating_uint_add(x: &Value, y: &Value) -> VmrtRes<Value> {
    Ok(match (x, y) {
        (U8(l), U8(r)) => Value::U8(l.saturating_add(*r)),
        (U16(l), U16(r)) => Value::U16(l.saturating_add(*r)),
        (U32(l), U32(r)) => Value::U32(l.saturating_add(*r)),
        (U64(l), U64(r)) => Value::U64(l.saturating_add(*r)),
        (U128(l), U128(r)) => Value::U128(l.saturating_add(*r)),
        (_, _) => {
            return itr_err_fmt!(
                Arithmetic,
                "cannot do arithmetic between {:?} and {:?}",
                x,
                y
            )
        }
    })
}

fn saturating_uint_sub(x: &Value, y: &Value) -> VmrtRes<Value> {
    Ok(match (x, y) {
        (U8(l), U8(r)) => Value::U8(l.saturating_sub(*r)),
        (U16(l), U16(r)) => Value::U16(l.saturating_sub(*r)),
        (U32(l), U32(r)) => Value::U32(l.saturating_sub(*r)),
        (U64(l), U64(r)) => Value::U64(l.saturating_sub(*r)),
        (U128(l), U128(r)) => Value::U128(l.saturating_sub(*r)),
        (_, _) => {
            return itr_err_fmt!(
                Arithmetic,
                "cannot do arithmetic between {:?} and {:?}",
                x,
                y
            )
        }
    })
}

fn absdiff_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    Ok(match (x, y) {
        (U8(l), U8(r)) => Value::U8(l.abs_diff(*r)),
        (U16(l), U16(r)) => Value::U16(l.abs_diff(*r)),
        (U32(l), U32(r)) => Value::U32(l.abs_diff(*r)),
        (U64(l), U64(r)) => Value::U64(l.abs_diff(*r)),
        (U128(l), U128(r)) => Value::U128(l.abs_diff(*r)),
        (_, _) => {
            return itr_err_fmt!(
                Arithmetic,
                "cannot do arithmetic between {:?} and {:?}",
                x,
                y
            )
        }
    })
}

fn sqrt_floor_checked(x: &Value) -> VmrtRes<Value> {
    let n = x.extract_u128()?;
    let out = n.isqrt();
    cast_uint_result1(x, out, "sqrt", x)
}

fn sqrt_up_checked(x: &Value) -> VmrtRes<Value> {
    let n = x.extract_u128()?;
    let err = || ItrErr::new(Arithmetic, &check_failed_tip1("sqrt_up", x));
    let f = n.isqrt();
    let out = if n <= 1 {
        n
    } else if f.checked_mul(f) == Some(n) {
        f
    } else {
        f.checked_add(1).ok_or_else(err)?
    };
    cast_uint_result1(x, out, "sqrt_up", x)
}

fn sqrtmul_with_round_checked(
    x: &Value,
    y: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip(op, x, y));
    let (hi, lo) = mul_wide_u128(x.extract_u128()?, y.extract_u128()?);
    let floor = isqrt_u256_floor(hi, lo);
    let out = match round {
        FinRoundPolicy::Floor => floor,
        FinRoundPolicy::Ceil => {
            let (sq_hi, sq_lo) = mul_wide_u128(floor, floor);
            if cmp_u256(sq_hi, sq_lo, hi, lo).is_eq() {
                floor
            } else {
                floor.checked_add(1).ok_or_else(err)?
            }
        }
        _ => return Err(err()),
    };
    cast_uint_result2(x, out, op, x, y)
}

fn quantize_with_round_checked(
    x: &Value,
    y: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip(op, x, y));
    let value = x.extract_u128()?;
    let step = require_nonzero_u128(y.extract_u128()?, err)?;
    let quo = value / step;
    let rem = value % step;
    let out = match round {
        FinRoundPolicy::Floor => quo.checked_mul(step).ok_or_else(err)?,
        FinRoundPolicy::Ceil if rem == 0 => value,
        FinRoundPolicy::Ceil => quo
            .checked_add(1)
            .and_then(|q| q.checked_mul(step))
            .ok_or_else(err)?,
        _ => return Err(err()),
    };
    cast_uint_result2(x, out, op, x, y)
}

fn addmod_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3("add_mod", x, y, z));
    let a = x.extract_u128()?;
    let b = y.extract_u128()?;
    let modu = require_nonzero_u128(z.extract_u128()?, err)?;
    let out = add_mod_u128(a, b, modu);
    cast_uint_result3(x, out, "add_mod", x, y, z)
}

fn mulmod_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3("mul_mod", x, y, z));
    let modu = require_nonzero_u128(z.extract_u128()?, err)?;
    let out = mul_mod_u128(x.extract_u128()?, y.extract_u128()?, modu);
    cast_uint_result3(x, out, "mul_mod", x, y, z)
}

fn muldiv_checked(x: &Value, y: &Value, z: &Value, op: &'static str) -> VmrtRes<Value> {
    muldiv_with_round_checked(x, y, z, FinRoundPolicy::Floor, op)
}

fn muldiv_up_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    muldiv_with_round_checked(x, y, z, FinRoundPolicy::Ceil, "mul_div_up")
}

fn muldiv_with_round_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    let div = require_nonzero_u128(z.extract_u128()?, err)?;
    let (hi, lo) = mul_wide_u128(x.extract_u128()?, y.extract_u128()?);
    let quo = div_u256_by_u128_with_round(hi, lo, div, round, err)?;
    cast_uint_result3(x, quo, op, x, y, z)
}

fn scaled_addsub_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    add_delta: bool,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    if !matches!(round, FinRoundPolicy::Floor | FinRoundPolicy::Ceil) {
        return Err(err());
    }
    let value = x.extract_u128()?;
    let rate = y.extract_u128()?;
    let scale = require_nonzero_u128(z.extract_u128()?, err)?;
    let (hi, lo) = mul_wide_u128(value, rate);
    let delta = div_u256_by_u128_with_round(hi, lo, scale, round, err)?;
    let out = if add_delta {
        value.checked_add(delta)
    } else {
        value.checked_sub(delta)
    }
    .ok_or_else(err)?;
    cast_uint_result3(x, out, op, x, y, z)
}

fn muldiv_den_addsub_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    add_to_den: bool,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    if !matches!(round, FinRoundPolicy::Floor | FinRoundPolicy::Ceil) {
        return Err(err());
    }
    let lhs = x.extract_u128()?;
    let rhs = y.extract_u128()?;
    let den_base = z.extract_u128()?;
    let (hi, lo) = mul_wide_u128(lhs, rhs);
    let quo = if add_to_den {
        let (den_hi, den_lo) = add_u256_u128(0, den_base, rhs).ok_or_else(err)?;
        div_u256_by_u129_with_round(hi, lo, den_hi, den_lo, round, 0, err)?
    } else {
        let den = den_base.checked_sub(rhs).ok_or_else(err)?;
        let den = require_nonzero_u128(den, err)?;
        div_u256_by_u128_with_round(hi, lo, den, round, err)?
    };
    cast_uint_result3(x, quo, op, x, y, z)
}

fn muladd_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3("mul_add", x, y, z));
    let lo = mul_xy_addsub_z_fit_u128(
        x.extract_u128()?,
        y.extract_u128()?,
        z.extract_u128()?,
        true,
        err,
    )?;
    cast_uint_result3(x, lo, "mul_add", x, y, z)
}

fn mul_shr_impl(x: &Value, y: &Value, z: &Value, op: &'static str, ceil_dropped: bool) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    let shift = z.extract_u128()?;
    let (hi, lo) = mul_wide_u128(x.extract_u128()?, y.extract_u128()?);
    let out = if shift >= 256 {
        if ceil_dropped && (hi != 0 || lo != 0) { 1 } else { 0 }
    } else {
        let (mut out, dropped) = shr_u256_to_u128(hi, lo, shift as u32).ok_or_else(err)?;
        if ceil_dropped && dropped {
            out = out.checked_add(1).ok_or_else(err)?;
        }
        out
    };
    cast_uint_result3(x, out, op, x, y, z)
}

fn rpow_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let op = "rpow_half_up";
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    let mut n = y.extract_u128()?;
    let base = z.extract_u128()?;
    if base == 0 {
        return Err(err());
    }
    if n == 0 {
        return cast_uint_result3(x, base, op, x, y, z);
    }
    let mut bas = x.extract_u128()?;
    let mut out = if n & 1 == 1 { bas } else { base };
    while n > 1 {
        n >>= 1;
        bas = mul_div_half_up(bas, bas, base, op, x, y, z)?;
        if n & 1 == 1 {
            out = mul_div_half_up(out, bas, base, op, x, y, z)?;
        }
    }
    cast_uint_result3(x, out, op, x, y, z)
}

fn clamp_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3("clamp", x, y, z));
    let xv = x.extract_u128()?;
    let lo = y.extract_u128()?;
    let hi = z.extract_u128()?;
    if lo > hi {
        return Err(err());
    }
    let out = xv.clamp(lo, hi);
    cast_uint_result3(x, out, "clamp", x, y, z)
}

fn satadd_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    saturating_uint_add(x, y)
}

fn satsub_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    saturating_uint_sub(x, y)
}

fn div_with_round_checked(
    x: &Value,
    y: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip(op, x, y));
    let div = require_nonzero_u128(y.extract_u128()?, err)?;
    let num = x.extract_u128()?;
    let quo = round_quot_u128_with_policy(num / div, num % div, div, round, err)?;
    cast_uint_result2(x, quo, op, x, y)
}

fn mulsub_checked(x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3("mul_sub", x, y, z));
    let lo = mul_xy_addsub_z_fit_u128(
        x.extract_u128()?,
        y.extract_u128()?,
        z.extract_u128()?,
        false,
        err,
    )?;
    cast_uint_result3(x, lo, "mul_sub", x, y, z)
}

fn muladddiv_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let div = require_nonzero_u128(w.extract_u128()?, err)?;
    let quo = mul_xy_addsub_z_div_u128(
        x.extract_u128()?,
        y.extract_u128()?,
        z.extract_u128()?,
        div,
        true,
        round,
        err,
    )?;
    cast_uint_result4(x, quo, op, x, y, z, w)
}

fn mulsubdiv_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let div = require_nonzero_u128(w.extract_u128()?, err)?;
    let quo = mul_xy_addsub_z_div_u128(
        x.extract_u128()?,
        y.extract_u128()?,
        z.extract_u128()?,
        div,
        false,
        round,
        err,
    )?;
    cast_uint_result4(x, quo, op, x, y, z, w)
}

fn mul3div_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let div = require_nonzero_u128(w.extract_u128()?, err)?;
    let (hi, lo) = mul_wide_u128(x.extract_u128()?, y.extract_u128()?);
    let (hi, lo) = mul_u256_u128_to_u256_checked(hi, lo, z.extract_u128()?).ok_or_else(err)?;
    let quo = div_u256_by_u128_with_round(hi, lo, div, round, err)?;
    cast_uint_result4(x, quo, op, x, y, z, w)
}

fn devscaled_with_round_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip3(op, x, y, z));
    let reference = y.extract_u128()?;
    let scale = z.extract_u128()?;
    let (quo, rem) = scaled_abs_diff_div_u128(x.extract_u128()?, reference, scale)
        .ok_or_else(&err)?;
    let out = round_quot_u128_with_policy(quo, rem, reference, round, err)?;
    cast_uint_result3(x, out, op, x, y, z)
}

fn withinbps_checked(x: &Value, y: &Value, z: &Value, w: &Value) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4("within_bps", x, y, z, w));
    let value = x.extract_u128()?;
    let reference = y.extract_u128()?;
    let tolerance = z.extract_u128()?;
    let scale = w.extract_u128()?;
    if reference == 0 || scale == 0 {
        return Err(err());
    }
    if tolerance > scale {
        return Err(err());
    }
    let diff = value.abs_diff(reference);
    let (lhs_hi, lhs_lo) = mul_wide_u128(diff, scale);
    let (rhs_hi, rhs_lo) = mul_wide_u128(reference, tolerance);
    Ok(Value::bool(cmp_u256(lhs_hi, lhs_lo, rhs_hi, rhs_lo).is_le()))
}

fn crossmul_pred_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    kernel: FinKernel,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let (lhs_hi, lhs_lo) = mul_wide_u128(x.extract_u128()?, y.extract_u128()?);
    let (rhs_hi, rhs_lo) = mul_wide_u128(z.extract_u128()?, w.extract_u128()?);
    let ord = cmp_u256(lhs_hi, lhs_lo, rhs_hi, rhs_lo);
    let out = match kernel {
        FinKernel::CrossLte => ord.is_le(),
        FinKernel::CrossGte => ord.is_ge(),
        FinKernel::CrossEq => ord.is_eq(),
        _ => return Err(err()),
    };
    Ok(Value::bool(out))
}

fn wavg2_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let lhs = x.extract_u128()?;
    let rhs = z.extract_u128()?;
    let wx = y.extract_u128()?;
    let wy = w.extract_u128()?;
    let (den_hi, den_lo) = add_u256_u128(0, wx, wy).ok_or_else(err)?;
    if den_hi == 0 && den_lo == 0 {
        return Err(err());
    }
    if lhs == rhs {
        return cast_uint_result4(x, lhs, op, x, y, z, w);
    }
    let (base, diff, diff_weight) = if lhs < rhs {
        (lhs, rhs - lhs, wy)
    } else {
        (rhs, lhs - rhs, wx)
    };
    let (part_hi, part_lo) = mul_wide_u128(diff, diff_weight);
    let part = div_u256_by_u129_with_round(part_hi, part_lo, den_hi, den_lo, round, base, err)?;
    let out = base.checked_add(part).ok_or_else(err)?;
    cast_uint_result4(x, out, op, x, y, z, w)
}

fn lerp_checked(
    x: &Value,
    y: &Value,
    z: &Value,
    w: &Value,
    round: FinRoundPolicy,
    op: &'static str,
) -> VmrtRes<Value> {
    let err = || ItrErr::new(Arithmetic, &check_failed_tip4(op, x, y, z, w));
    let start = x.extract_u128()?;
    let end = y.extract_u128()?;
    let weight = z.extract_u128()?;
    let base = w.extract_u128()?;
    if base == 0 || weight > base {
        return Err(err());
    }
    let left_weight = base - weight;
    let (lhs_hi, lhs_lo) = mul_wide_u128(start, left_weight);
    let (rhs_hi, rhs_lo) = mul_wide_u128(end, weight);
    let (num_hi, num_lo) = add_u256(lhs_hi, lhs_lo, rhs_hi, rhs_lo).ok_or_else(err)?;
    let quo = div_u256_by_u128_with_round(num_hi, num_lo, base, round, err)?;
    cast_uint_result4(x, quo, op, x, y, z, w)
}

fn invalid_fin_spec(spec: FinSpec) -> VmrtRes<Value> {
    itr_err_fmt!(
        InstParamsErr,
        "invalid fin spec {} ({:?}, round {:?})",
        spec.name,
        spec.kernel,
        spec.round
    )
}

fn fin2_checked(spec: FinSpec, x: &Value, y: &Value) -> VmrtRes<Value> {
    let round = spec.round_or_exact();
    match spec.kernel {
        FinKernel::Div => div_with_round_checked(x, y, round, spec.name),
        FinKernel::SqrtMul => sqrtmul_with_round_checked(x, y, round, spec.name),
        FinKernel::Quantize => quantize_with_round_checked(x, y, round, spec.name),
        FinKernel::SatAdd => satadd_checked(x, y),
        FinKernel::SatSub => satsub_checked(x, y),
        _ => invalid_fin_spec(spec),
    }
}

fn fin3_checked(spec: FinSpec, x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    let round = spec.round_or_exact();
    match spec.kernel {
        FinKernel::MulDiv | FinKernel::ScaledDiv => {
            muldiv_with_round_checked(x, y, z, round, spec.name)
        }
        FinKernel::DevScaled => devscaled_with_round_checked(x, y, z, round, spec.name),
        FinKernel::ScaledAdd => scaled_addsub_checked(x, y, z, true, round, spec.name),
        FinKernel::ScaledSub => scaled_addsub_checked(x, y, z, false, round, spec.name),
        FinKernel::MulShr if round == FinRoundPolicy::Floor => {
            mul_shr_impl(x, y, z, spec.name, false)
        }
        FinKernel::MulShr if round == FinRoundPolicy::Ceil => {
            mul_shr_impl(x, y, z, spec.name, true)
        }
        FinKernel::MulDivDenAdd => {
            muldiv_den_addsub_checked(x, y, z, true, round, spec.name)
        }
        FinKernel::MulDivDenSub => {
            muldiv_den_addsub_checked(x, y, z, false, round, spec.name)
        }
        _ => invalid_fin_spec(spec),
    }
}

fn fin4_checked(spec: FinSpec, x: &Value, y: &Value, z: &Value, w: &Value) -> VmrtRes<Value> {
    let round = spec.round_or_exact();
    match spec.kernel {
        FinKernel::MulAddDiv => muladddiv_checked(x, y, z, w, round, spec.name),
        FinKernel::MulSubDiv => mulsubdiv_checked(x, y, z, w, round, spec.name),
        FinKernel::Mul3Div => mul3div_checked(x, y, z, w, round, spec.name),
        FinKernel::Wavg2 => wavg2_checked(x, y, z, w, round, spec.name),
        FinKernel::Lerp => lerp_checked(x, y, z, w, round, spec.name),
        _ => invalid_fin_spec(spec),
    }
}

fn finp3_checked(spec: FinSpec, x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    match spec.kernel {
        FinKernel::AbsDiffLte => {
            let xv = x.extract_u128()?;
            let yv = y.extract_u128()?;
            let tol = z.extract_u128()?;
            Ok(Value::bool(xv.abs_diff(yv) <= tol))
        }
        _ => invalid_fin_spec(spec),
    }
}

fn finp4_checked(spec: FinSpec, x: &Value, y: &Value, z: &Value, w: &Value) -> VmrtRes<Value> {
    match spec.kernel {
        FinKernel::WithinBps => withinbps_checked(x, y, z, w),
        FinKernel::CrossLte | FinKernel::CrossGte | FinKernel::CrossEq => {
            crossmul_pred_checked(x, y, z, w, spec.kernel, spec.name)
        }
        _ => invalid_fin_spec(spec),
    }
}

fn finpow3_checked(spec: FinSpec, x: &Value, y: &Value, z: &Value) -> VmrtRes<Value> {
    match spec.kernel {
        FinKernel::RPow => rpow_checked(x, y, z),
        _ => invalid_fin_spec(spec),
    }
}

// the value is must within u32
fn pow_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    let exp_u32 = |n: u128| -> VmrtRes<u32> {
        u32::try_from(n).map_err(|_| ItrErr::new(Arithmetic, &check_failed_tip("pow", x, y)))
    };
    match (x, y) {
        (U8(l), U8(r)) => <u8>::checked_pow(*l, *r as u32).map(Value::U8),
        (U16(l), U16(r)) => <u16>::checked_pow(*l, *r as u32).map(Value::U16),
        (U32(l), U32(r)) => <u32>::checked_pow(*l, *r).map(Value::U32),
        (U64(l), U64(r)) => <u64>::checked_pow(*l, exp_u32(*r as u128)?).map(Value::U64),
        (U128(l), U128(r)) => <u128>::checked_pow(*l, exp_u32(*r)?).map(Value::U128),
        (_, _) => {
            return itr_err_fmt!(
                Arithmetic,
                "cannot do pow arithmetic between {:?} and {:?}",
                x,
                y
            );
        }
    }
    .ok_or_else(|| ItrErr::new(Arithmetic, &check_failed_tip("pow", x, y)))
}

fn max_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    let a = x.extract_u128()?;
    let b = y.extract_u128()?;
    Ok(maybe!(a > b, x.clone(), y.clone()))
}

fn min_checked(x: &Value, y: &Value) -> VmrtRes<Value> {
    let a = x.extract_u128()?;
    let b = y.extract_u128()?;
    Ok(maybe!(a < b, x.clone(), y.clone()))
}

fn unary_inc(x: &mut Value, n: u8) -> VmrtErr {
    if !x.is_uint() {
        return itr_err_fmt!(Arithmetic, "cannot do arithmetic with {:?}", x);
    }
    x.inc(n)
        .map_err(|ItrErr(_, msg)| ItrErr::new(Arithmetic, &msg))
}

fn unary_dec(x: &mut Value, n: u8) -> VmrtErr {
    if !x.is_uint() {
        return itr_err_fmt!(Arithmetic, "cannot do arithmetic with {:?}", x);
    }
    x.dec(n)
        .map_err(|ItrErr(_, msg)| ItrErr::new(Arithmetic, &msg))
}

#[cfg(test)]
mod shift_u64_tests {
    use super::*;
    use std::collections::HashSet;

    fn fin_dispatch_for_test(spec: FinSpec, args: &[Value]) -> VmrtRes<Value> {
        match spec.family {
            Bytecode::FIN2 => fin2_checked(spec, &args[0], &args[1]),
            Bytecode::FIN3 => fin3_checked(spec, &args[0], &args[1], &args[2]),
            Bytecode::FIN4 => fin4_checked(spec, &args[0], &args[1], &args[2], &args[3]),
            Bytecode::FINP3 => finp3_checked(spec, &args[0], &args[1], &args[2]),
            Bytecode::FINP4 => finp4_checked(spec, &args[0], &args[1], &args[2], &args[3]),
            Bytecode::FINPOW3 => finpow3_checked(spec, &args[0], &args[1], &args[2]),
            _ => invalid_fin_spec(spec),
        }
    }

    fn fin_call_by_name(name: &'static str, args: Vec<Value>) -> VmrtRes<Value> {
        let spec = fin_source_call_spec(name).unwrap().unwrap();
        assert_eq!(
            spec.argc().unwrap() as usize,
            args.len(),
            "test case argc mismatch for {}",
            name
        );
        fin_dispatch_for_test(spec, &args)
    }

    #[test]
    fn bit_shl_u64_rejects_shift_count_over_u32() {
        let r = Value::U64((u32::MAX as u64) + 1);
        assert!(bit_shl(&Value::U64(1), &r).is_err());
    }

    #[test]
    fn bit_shr_u64_rejects_shift_count_over_u32() {
        let r = Value::U64((u32::MAX as u64) + 1);
        assert!(bit_shr(&Value::U64(1), &r).is_err());
    }

    #[test]
    fn direct_ceil_math_ops_match_fin_rounding() {
        assert_eq!(
            div_up_checked(&Value::U64(10), &Value::U64(3)).unwrap(),
            fin_call_by_name("div_ceil", vec![Value::U64(10), Value::U64(3)]).unwrap()
        );
        assert_eq!(
            div_exact_op_checked(&Value::U64(12), &Value::U64(3)).unwrap(),
            fin_call_by_name("div_exact", vec![Value::U64(12), Value::U64(3)]).unwrap()
        );
        assert!(div_exact_op_checked(&Value::U64(10), &Value::U64(3)).is_err());
        assert_eq!(
            muldiv_up_checked(&Value::U64(10), &Value::U64(10), &Value::U64(6)).unwrap(),
            fin_call_by_name(
                "mul_div_ceil",
                vec![Value::U64(10), Value::U64(10), Value::U64(6)]
            )
            .unwrap()
        );
        assert_eq!(
            fin_call_by_name("sat_add", vec![Value::U8(u8::MAX), Value::U8(1)]).unwrap(),
            Value::U8(u8::MAX)
        );
        assert_eq!(
            fin_call_by_name("sat_sub", vec![Value::U64(0), Value::U64(1)]).unwrap(),
            Value::U64(0)
        );
    }

    #[test]
    fn logic_and_or_auto_cast_to_bool() {
        assert_eq!(
            lgc_and(&Value::U8(2), &Value::Bytes(vec![])).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            lgc_or(&Value::U8(2), &Value::Bytes(vec![])).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn nil_and_bool_false_are_falsy_but_not_equal() {
        assert!(lgc_equal(&Value::Nil, &Value::Bool(false)).is_err());
        assert!(!Value::Nil.extract_bool().unwrap());
        assert!(!Value::Bool(false).extract_bool().unwrap());
        assert!(Value::U8(2).extract_bool().unwrap());
        assert!(!Value::Bytes(vec![]).extract_bool().unwrap());
    }

    #[test]
    fn eq_neq_uint_operands_use_numeric_compare() {
        assert_eq!(
            lgc_equal(&Value::U8(1), &Value::U16(1)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_not_equal(&Value::U32(5), &Value::U8(4)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_not_equal(&Value::U128(0), &Value::U8(0)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn eq_neq_reject_different_non_uint_types() {
        let adr = field::Address::from_readable("1MzNY1oA3kfgYi75zquj3SRUPYztzXHzK9").unwrap();
        assert!(lgc_equal(&Value::U16(1), &Value::Bytes(vec![1])).is_err());
        assert!(lgc_equal(&Value::U8(1), &Value::Bool(true)).is_err());
        assert!(lgc_equal(&Value::Nil, &Value::Bool(false)).is_err());
        assert!(lgc_not_equal(&Value::Address(adr), &Value::Bytes(vec![])).is_err());
    }

    #[test]
    fn eq_non_uint_same_type_compare_by_type_rules() {
        assert_eq!(
            lgc_equal(&Value::Bytes(vec![0, 1]), &Value::Bytes(vec![1])).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            lgc_equal(&Value::Bytes(vec![1]), &Value::Bytes(vec![1])).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Bool(true), &Value::Bool(true)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Nil, &Value::Nil).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn ordered_compare_requires_uint_operands() {
        assert_eq!(
            lgc_less(&Value::U8(1), &Value::U16(2)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_greater_equal(&Value::U32(2), &Value::U8(2)).unwrap(),
            Value::Bool(true)
        );
        assert!(lgc_less(&Value::Bytes(vec![1]), &Value::U16(2)).is_err());
        assert!(lgc_greater_equal(&Value::Bool(false), &Value::U8(1)).is_err());
        assert!(lgc_less_equal(&Value::Nil, &Value::U8(0)).is_err());
        assert!(lgc_greater(&Value::Bytes(vec![2]), &Value::Bytes(vec![1])).is_err());
    }

    #[test]
    fn pow_supports_u64_u128_and_rejects_exp_over_u32() {
        assert_eq!(
            pow_checked(&Value::U64(2), &Value::U64(10)).unwrap(),
            Value::U64(1024)
        );
        assert_eq!(
            pow_checked(&Value::U128(2), &Value::U128(10)).unwrap(),
            Value::U128(1024)
        );
        let over = Value::U128((u32::MAX as u128) + 1);
        assert!(pow_checked(&Value::U128(2), &over).is_err());
    }

    #[test]
    fn max_min_mixed_uint_widths_return_normalized_type() {
        let mut lhs_max = Value::U8(255);
        let mut rhs_max = Value::U128(254);
        locop_arithmetic(&mut lhs_max, &mut rhs_max, max_checked).unwrap();
        assert_eq!(lhs_max, Value::U128(255));

        let mut lhs_min = Value::U16(1);
        let mut rhs_min = Value::U64(2);
        locop_arithmetic(&mut lhs_min, &mut rhs_min, min_checked).unwrap();
        assert_eq!(lhs_min, Value::U64(1));
    }

    #[test]
    fn tuple_eq_uses_content_semantics() {
        let shared = TupleItem::new(vec![Value::U8(7), Value::Bytes(vec![1, 2, 3])]).unwrap();
        let same = shared.clone();
        let rebuilt = TupleItem::new(vec![Value::U8(7), Value::Bytes(vec![1, 2, 3])]).unwrap();
        let diff = TupleItem::new(vec![Value::U8(7), Value::Bytes(vec![1, 2, 4])]).unwrap();

        assert_eq!(
            lgc_equal(&Value::Tuple(shared.clone()), &Value::Tuple(same)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Tuple(shared.clone()), &Value::Tuple(rebuilt)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Tuple(shared.clone()), &Value::Tuple(diff)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn compo_eq_uses_content_semantics() {
        let c = CompoItem::list(VecDeque::from([Value::U8(1), Value::Bytes(vec![2, 3])])).unwrap();
        let same = c.clone();
        let copied = CompoItem::list(VecDeque::from([Value::U8(1), Value::Bytes(vec![2, 3])])).unwrap();
        let diff = CompoItem::list(VecDeque::from([Value::U8(1), Value::Bytes(vec![2, 4])])).unwrap();

        assert_eq!(
            lgc_equal(&Value::Compo(c.clone()), &Value::Compo(same)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Compo(c.clone()), &Value::Compo(copied.clone())).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            lgc_equal(&Value::Compo(c), &Value::Compo(diff)).unwrap(),
            Value::Bool(false)
        );
        assert!(lgc_equal(&Value::Compo(copied.clone()), &Value::Nil).is_err());
        assert!(lgc_not_equal(&Value::Compo(copied), &Value::Nil).is_err());
    }

    #[test]
    fn handle_eq_is_rejected() {
        let lhs = Value::handle(7u32);
        let rhs = lhs.clone();
        assert!(lgc_equal(&lhs, &rhs).is_err());
        assert!(lgc_not_equal(&lhs, &rhs).is_err());
    }

    #[test]
    fn finance_rounding_and_saturating_ops_work() {
        assert_eq!(
            div_with_round_checked(
                &Value::U64(10),
                &Value::U64(3),
                FinRoundPolicy::Ceil,
                "div_ceil",
            )
            .unwrap(),
            Value::U64(4)
        );
        assert_eq!(
            div_with_round_checked(
                &Value::U64(10),
                &Value::U64(6),
                FinRoundPolicy::HalfUp,
                "div_half_up",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            satadd_checked(&Value::U8(u8::MAX), &Value::U8(1)).unwrap(),
            Value::U8(u8::MAX)
        );
        assert_eq!(
            satsub_checked(&Value::U16(0), &Value::U16(7)).unwrap(),
            Value::U16(0)
        );
        assert_eq!(
            absdiff_checked(&Value::U64(3), &Value::U64(11)).unwrap(),
            Value::U64(8)
        );
    }

    #[test]
    fn finance_mul_variants_work() {
        assert_eq!(
            mulsub_checked(&Value::U64(9), &Value::U64(8), &Value::U64(6)).unwrap(),
            Value::U64(66)
        );
        assert_eq!(
            muldiv_with_round_checked(
                &Value::U64(5),
                &Value::U64(10),
                &Value::U64(6),
                FinRoundPolicy::HalfUp,
                "mul_div_half_up",
            )
            .unwrap(),
            Value::U64(8)
        );
        assert_eq!(
            muladddiv_checked(
                &Value::U64(2),
                &Value::U64(3),
                &Value::U64(4),
                &Value::U64(5),
                FinRoundPolicy::Floor,
                "mul_add_div_floor",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            mulsubdiv_checked(
                &Value::U64(9),
                &Value::U64(8),
                &Value::U64(6),
                &Value::U64(3),
                FinRoundPolicy::Floor,
                "mul_sub_div_floor",
            )
            .unwrap(),
            Value::U64(22)
        );
        assert_eq!(
            mul3div_checked(
                &Value::U64(3),
                &Value::U64(4),
                &Value::U64(5),
                &Value::U64(6),
                FinRoundPolicy::Floor,
                "mul3_div_floor",
            )
            .unwrap(),
            Value::U64(10)
        );
    }

    #[test]
    fn fin4_rounding_modes_work() {
        assert_eq!(
            muladddiv_checked(
                &Value::U64(2),
                &Value::U64(3),
                &Value::U64(4),
                &Value::U64(5),
                FinRoundPolicy::Ceil,
                "mul_add_div_ceil",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            muladddiv_checked(
                &Value::U64(2),
                &Value::U64(3),
                &Value::U64(4),
                &Value::U64(5),
                FinRoundPolicy::HalfUp,
                "mul_add_div_half_up",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert!(muladddiv_checked(
            &Value::U64(2),
            &Value::U64(3),
            &Value::U64(4),
            &Value::U64(6),
            FinRoundPolicy::Exact,
            "mul_add_div_exact",
        )
        .is_err());
        assert_eq!(
            muladddiv_checked(
                &Value::U64(2),
                &Value::U64(3),
                &Value::U64(4),
                &Value::U64(2),
                FinRoundPolicy::Exact,
                "mul_add_div_exact",
            )
            .unwrap(),
            Value::U64(5)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(100),
                &Value::U64(1),
                FinRoundPolicy::HalfUp,
                "wavg2_half_up",
            )
            .unwrap(),
            Value::U64(101)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(0),
                &Value::U64(1),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::Ceil,
                "lerp_ceil",
            )
            .unwrap(),
            Value::U64(1)
        );
    }

    #[test]
    fn finance_half_even_rounding_modes_work() {
        assert_eq!(
            div_with_round_checked(
                &Value::U64(5),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "div_half_even",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            div_with_round_checked(
                &Value::U64(7),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "div_half_even",
            )
            .unwrap(),
            Value::U64(4)
        );
        assert_eq!(
            muldiv_with_round_checked(
                &Value::U64(5),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "mul_div_half_even",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            muldiv_with_round_checked(
                &Value::U64(7),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "mul_div_half_even",
            )
            .unwrap(),
            Value::U64(4)
        );
        assert_eq!(
            devscaled_with_round_checked(
                &Value::U64(125),
                &Value::U64(100),
                &Value::U64(10),
                FinRoundPolicy::HalfEven,
                "dev_scaled_half_even",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            devscaled_with_round_checked(
                &Value::U64(135),
                &Value::U64(100),
                &Value::U64(10),
                FinRoundPolicy::HalfEven,
                "dev_scaled_half_even",
            )
            .unwrap(),
            Value::U64(4)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(0),
                &Value::U64(1),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "lerp_half_even",
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(1),
                &Value::U64(2),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::HalfEven,
                "lerp_half_even",
            )
            .unwrap(),
            Value::U64(2)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U64(1),
                &Value::U64(1),
                &Value::U64(2),
                &Value::U64(1),
                FinRoundPolicy::HalfEven,
                "wavg2_half_even",
            )
            .unwrap(),
            Value::U64(2)
        );
    }

    #[test]
    fn finance_guard_and_curve_ops_work() {
        assert_eq!(
            devscaled_with_round_checked(
                &Value::U64(10100),
                &Value::U64(10000),
                &Value::U64(10000),
                FinRoundPolicy::Ceil,
                "dev_scaled_ceil",
            )
            .unwrap(),
            Value::U64(100)
        );
        assert_eq!(
            devscaled_with_round_checked(
                &Value::U64(10001),
                &Value::U64(10000),
                &Value::U64(3),
                FinRoundPolicy::Floor,
                "dev_scaled_floor",
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            devscaled_with_round_checked(
                &Value::U64(10001),
                &Value::U64(10000),
                &Value::U64(3),
                FinRoundPolicy::Ceil,
                "dev_scaled_ceil",
            )
            .unwrap(),
            Value::U64(1)
        );
        assert_eq!(
            withinbps_checked(
                &Value::U64(10050),
                &Value::U64(10000),
                &Value::U64(60),
                &Value::U64(10000),
            )
            .unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            withinbps_checked(
                &Value::U64(10061),
                &Value::U64(10000),
                &Value::U64(60),
                &Value::U64(10000),
            )
            .unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U64(100),
                &Value::U64(1),
                &Value::U64(300),
                &Value::U64(3),
                FinRoundPolicy::Floor,
                "wavg2_floor",
            )
            .unwrap(),
            Value::U64(250)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(100),
                &Value::U64(1),
                FinRoundPolicy::Floor,
                "wavg2_floor",
            )
            .unwrap(),
            Value::U64(100)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U64(7),
                &Value::U128(u128::MAX),
                &Value::U64(7),
                &Value::U64(1),
                FinRoundPolicy::Floor,
                "wavg2_floor",
            )
            .unwrap(),
            Value::U64(7)
        );
        assert_eq!(
            wavg2_checked(
                &Value::U128(10),
                &Value::U128(u128::MAX),
                &Value::U128(14),
                &Value::U64(2),
                FinRoundPolicy::Floor,
                "wavg2_floor",
            )
            .unwrap(),
            Value::U128(10)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(100),
                &Value::U64(200),
                &Value::U64(1),
                &Value::U64(4),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(125)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(200),
                &Value::U64(100),
                &Value::U64(1),
                &Value::U64(4),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(175)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(0),
                &Value::U64(1),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(1),
                &Value::U64(0),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(101),
                &Value::U64(100),
                &Value::U64(1),
                &Value::U64(2),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            wavg2_checked(
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(100),
                &Value::U64(1),
                FinRoundPolicy::Floor,
                "wavg2_floor",
            )
                .unwrap()
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(7),
                &Value::U64(9),
                &Value::U64(0),
                &Value::U64(5),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(7)
        );
        assert_eq!(
            lerp_checked(
                &Value::U64(7),
                &Value::U64(9),
                &Value::U64(5),
                &Value::U64(5),
                FinRoundPolicy::Floor,
                "lerp_floor",
            )
            .unwrap(),
            Value::U64(9)
        );
    }

    #[test]
    fn finance_guard_and_curve_ops_reject_invalid_params() {
        assert!(devscaled_with_round_checked(
            &Value::U64(1),
            &Value::U64(0),
            &Value::U64(10),
            FinRoundPolicy::Ceil,
            "dev_scaled_ceil",
        )
        .is_err());
        assert!(devscaled_with_round_checked(
            &Value::U64(1),
            &Value::U64(1),
            &Value::U64(0),
            FinRoundPolicy::Ceil,
            "dev_scaled_ceil",
        )
        .is_err());
        assert!(devscaled_with_round_checked(
            &Value::U64(1),
            &Value::U64(0),
            &Value::U64(10),
            FinRoundPolicy::Floor,
            "dev_scaled_floor",
        )
        .is_err());
        assert!(devscaled_with_round_checked(
            &Value::U64(1),
            &Value::U64(1),
            &Value::U64(0),
            FinRoundPolicy::Floor,
            "dev_scaled_floor",
        )
        .is_err());
        assert!(withinbps_checked(&Value::U64(1), &Value::U64(0), &Value::U64(1), &Value::U64(10)).is_err());
        assert!(withinbps_checked(&Value::U64(1), &Value::U64(1), &Value::U64(1), &Value::U64(0)).is_err());
        assert!(lerp_checked(
            &Value::U64(1),
            &Value::U64(2),
            &Value::U64(1),
            &Value::U64(0),
            FinRoundPolicy::Floor,
            "lerp_floor",
        )
        .is_err());
        assert!(lerp_checked(
            &Value::U64(1),
            &Value::U64(2),
            &Value::U64(3),
            &Value::U64(2),
            FinRoundPolicy::Floor,
            "lerp_floor",
        )
        .is_err());
    }

    #[test]
    fn within_bps_rejects_tolerance_over_scale() {
        assert!(withinbps_checked(
            &Value::U64(1),
            &Value::U64(1),
            &Value::U64(11),
            &Value::U64(10)
        )
        .is_err());
    }

    #[test]
    fn mul_shr_large_shift_has_defined_zero_or_one_semantics() {
        assert_eq!(
            mul_shr_impl(
                &Value::U64(3),
                &Value::U64(5),
                &Value::U16(256),
                "mul_shr_floor",
                false
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            mul_shr_impl(
                &Value::U64(3),
                &Value::U64(5),
                &Value::U16(300),
                "mul_shr_floor",
                false
            )
            .unwrap(),
            Value::U64(0)
        );
        assert_eq!(
            mul_shr_impl(&Value::U64(3), &Value::U64(5), &Value::U16(256), "mul_shr_ceil", true)
                .unwrap(),
            Value::U64(1)
        );
        assert_eq!(
            mul_shr_impl(&Value::U64(0), &Value::U64(5), &Value::U16(300), "mul_shr_ceil", true)
                .unwrap(),
            Value::U64(0)
        );
    }

    #[test]
    fn dev_scaled_matches_within_bps_thresholds() {
        let x = Value::U64(10001);
        let reference = Value::U64(10000);
        let scale = Value::U64(3);
        let tol_ok = Value::U64(1);
        let tol_bad = Value::U64(0);
        assert_eq!(
            devscaled_with_round_checked(&x, &reference, &scale, FinRoundPolicy::Ceil, "dev_scaled_ceil")
                .unwrap(),
            Value::U64(1)
        );
        assert_eq!(
            withinbps_checked(&x, &reference, &tol_ok, &scale).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            withinbps_checked(&x, &reference, &tol_bad, &scale).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn fin2_sqrt_mul_and_quantize_work() {
        assert_eq!(
            sqrtmul_with_round_checked(
                &Value::U64(2),
                &Value::U64(8),
                FinRoundPolicy::Floor,
                "sqrt_mul_floor",
            )
            .unwrap(),
            Value::U64(4)
        );
        assert_eq!(
            sqrtmul_with_round_checked(
                &Value::U64(2),
                &Value::U64(3),
                FinRoundPolicy::Floor,
                "sqrt_mul_floor",
            )
            .unwrap(),
            Value::U64(2)
        );
        let sqrt_ceil_spec = fin_source_call_spec("sqrt_mul_ceil").unwrap().unwrap();
        assert_eq!(
            fin2_checked(sqrt_ceil_spec, &Value::U64(2), &Value::U64(3)).unwrap(),
            Value::U64(3)
        );
        assert_eq!(
            sqrtmul_with_round_checked(
                &Value::U128(u128::MAX),
                &Value::U128(u128::MAX),
                FinRoundPolicy::Floor,
                "sqrt_mul_floor",
            )
            .unwrap(),
            Value::U128(u128::MAX)
        );
        assert_eq!(
            quantize_with_round_checked(
                &Value::U64(101),
                &Value::U64(10),
                FinRoundPolicy::Floor,
                "quantize_floor",
            )
            .unwrap(),
            Value::U64(100)
        );
        let quantize_ceil_spec = fin_source_call_spec("quantize_ceil").unwrap().unwrap();
        assert_eq!(
            fin2_checked(quantize_ceil_spec, &Value::U64(101), &Value::U64(10)).unwrap(),
            Value::U64(110)
        );
        assert!(quantize_with_round_checked(
            &Value::U64(101),
            &Value::U64(0),
            FinRoundPolicy::Floor,
            "quantize_floor",
        )
        .is_err());
        assert!(quantize_with_round_checked(
            &Value::U128(u128::MAX),
            &Value::U128(2),
            FinRoundPolicy::Ceil,
            "quantize_ceil",
        )
        .is_err());
    }

    #[test]
    fn fin3_scaled_add_sub_work() {
        assert_eq!(
            scaled_addsub_checked(
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(2),
                true,
                FinRoundPolicy::Floor,
                "scaled_add_floor",
            )
            .unwrap(),
            Value::U64(151)
        );
        let scaled_add_ceil = fin_source_call_spec("scaled_add_ceil").unwrap().unwrap();
        assert_eq!(
            fin3_checked(
                scaled_add_ceil,
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(2),
            )
            .unwrap(),
            Value::U64(152)
        );
        assert_eq!(
            scaled_addsub_checked(
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(2),
                false,
                FinRoundPolicy::Floor,
                "scaled_sub_floor",
            )
            .unwrap(),
            Value::U64(51)
        );
        let scaled_sub_ceil = fin_source_call_spec("scaled_sub_ceil").unwrap().unwrap();
        assert_eq!(
            fin3_checked(
                scaled_sub_ceil,
                &Value::U64(101),
                &Value::U64(1),
                &Value::U64(2),
            )
            .unwrap(),
            Value::U64(50)
        );
        assert!(scaled_addsub_checked(
            &Value::U64(1),
            &Value::U64(1),
            &Value::U64(0),
            true,
            FinRoundPolicy::Floor,
            "scaled_add_floor",
        )
        .is_err());
        assert!(scaled_addsub_checked(
            &Value::U64(1),
            &Value::U64(3),
            &Value::U64(2),
            false,
            FinRoundPolicy::Ceil,
            "scaled_sub_ceil",
        )
        .is_err());
    }

    #[test]
    fn fin3_mul_div_den_add_sub_work() {
        assert_eq!(
            muldiv_den_addsub_checked(
                &Value::U64(100),
                &Value::U64(10),
                &Value::U64(90),
                true,
                FinRoundPolicy::Floor,
                "mul_div_den_add_floor",
            )
            .unwrap(),
            Value::U64(10)
        );
        let den_add_ceil = fin_source_call_spec("mul_div_den_add_ceil").unwrap().unwrap();
        assert_eq!(
            fin3_checked(
                den_add_ceil,
                &Value::U64(10),
                &Value::U64(10),
                &Value::U64(3),
            )
            .unwrap(),
            Value::U64(8)
        );
        assert_eq!(
            muldiv_den_addsub_checked(
                &Value::U64(100),
                &Value::U64(10),
                &Value::U64(110),
                false,
                FinRoundPolicy::Floor,
                "mul_div_den_sub_floor",
            )
            .unwrap(),
            Value::U64(10)
        );
        let den_sub_ceil = fin_source_call_spec("mul_div_den_sub_ceil").unwrap().unwrap();
        assert_eq!(
            fin3_checked(
                den_sub_ceil,
                &Value::U64(10),
                &Value::U64(10),
                &Value::U64(23),
            )
            .unwrap(),
            Value::U64(8)
        );
        assert!(muldiv_den_addsub_checked(
            &Value::U64(10),
            &Value::U64(10),
            &Value::U64(10),
            false,
            FinRoundPolicy::Floor,
            "mul_div_den_sub_floor",
        )
        .is_err());
        assert!(muldiv_den_addsub_checked(
            &Value::U64(10),
            &Value::U64(11),
            &Value::U64(10),
            false,
            FinRoundPolicy::Floor,
            "mul_div_den_sub_floor",
        )
        .is_err());
    }

    #[test]
    fn finp4_cross_mul_predicates_use_wide_products() {
        let max = Value::U128(u128::MAX);
        let almost = Value::U128(u128::MAX - 1);
        assert_eq!(
            crossmul_pred_checked(&max, &almost, &max, &max, FinKernel::CrossLte, "cross_lte").unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            crossmul_pred_checked(&max, &max, &max, &almost, FinKernel::CrossGte, "cross_gte").unwrap(),
            Value::Bool(true)
        );
        let cross_eq = fin_source_call_spec("cross_eq").unwrap().unwrap();
        assert_eq!(
            finp4_checked(cross_eq, &max, &almost, &max, &max).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            finp4_checked(cross_eq, &max, &almost, &max, &almost).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn finp3_abs_diff_lte_works() {
        let spec = fin_source_call_spec("abs_diff_lte").unwrap().unwrap();
        assert_eq!(
            finp3_checked(spec, &Value::U64(105), &Value::U64(100), &Value::U64(5)).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            finp3_checked(spec, &Value::U64(106), &Value::U64(100), &Value::U64(5)).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn every_fin_registry_entry_has_a_runtime_behavior_case() {
        macro_rules! case_u {
            ($name:literal, [$($arg:expr),* $(,)?], $expected:expr) => {
                (
                    $name,
                    vec![$(Value::U128($arg)),*],
                    Value::U128($expected),
                )
            };
        }
        macro_rules! case_b {
            ($name:literal, [$($arg:expr),* $(,)?], $expected:expr) => {
                (
                    $name,
                    vec![$(Value::U128($arg)),*],
                    Value::Bool($expected),
                )
            };
        }

        let cases = vec![
            case_u!("div_exact", [12, 3], 4),
            case_u!("div_floor", [10, 3], 3),
            case_u!("div_ceil", [10, 3], 4),
            case_u!("div_half_up", [10, 6], 2),
            case_u!("div_half_even", [5, 2], 2),
            case_u!("sqrt_mul_floor", [2, 3], 2),
            case_u!("sqrt_mul_ceil", [2, 3], 3),
            case_u!("quantize_floor", [101, 10], 100),
            case_u!("quantize_ceil", [101, 10], 110),
            case_u!("sat_add", [u128::MAX, 1], u128::MAX),
            case_u!("sat_sub", [0, 1], 0),
            case_u!("mul_div_exact", [6, 7, 3], 14),
            case_u!("mul_div_floor", [5, 10, 6], 8),
            case_u!("mul_div_ceil", [5, 10, 6], 9),
            case_u!("mul_div_half_up", [5, 1, 2], 3),
            case_u!("mul_div_half_even", [5, 1, 2], 2),
            case_u!("dev_scaled_floor", [10001, 10000, 3], 0),
            case_u!("dev_scaled_ceil", [10001, 10000, 3], 1),
            case_u!("dev_scaled_half_even", [125, 100, 10], 2),
            case_u!("scaled_div_floor", [5, 1, 2], 2),
            case_u!("scaled_div_ceil", [5, 1, 2], 3),
            case_u!("scaled_div_half_up", [5, 1, 2], 3),
            case_u!("scaled_div_half_even", [5, 1, 2], 2),
            case_u!("mul_shr_floor", [15, 1, 1], 7),
            case_u!("mul_shr_ceil", [15, 1, 1], 8),
            case_u!("scaled_add_floor", [101, 1, 2], 151),
            case_u!("scaled_add_ceil", [101, 1, 2], 152),
            case_u!("scaled_sub_floor", [101, 1, 2], 51),
            case_u!("scaled_sub_ceil", [101, 1, 2], 50),
            case_u!("mul_div_den_add_floor", [100, 10, 90], 10),
            case_u!("mul_div_den_add_ceil", [10, 10, 3], 8),
            case_u!("mul_div_den_sub_floor", [100, 10, 110], 10),
            case_u!("mul_div_den_sub_ceil", [10, 10, 23], 8),
            case_u!("mul_add_div_exact", [2, 3, 4, 2], 5),
            case_u!("mul_add_div_floor", [2, 3, 4, 6], 1),
            case_u!("mul_add_div_ceil", [2, 3, 4, 6], 2),
            case_u!("mul_add_div_half_up", [2, 3, 4, 6], 2),
            case_u!("mul_add_div_half_even", [2, 3, 4, 4], 2),
            case_u!("mul_sub_div_exact", [9, 8, 6, 3], 22),
            case_u!("mul_sub_div_floor", [9, 8, 6, 5], 13),
            case_u!("mul_sub_div_ceil", [9, 8, 6, 5], 14),
            case_u!("mul_sub_div_half_up", [9, 8, 6, 4], 17),
            case_u!("mul_sub_div_half_even", [9, 8, 6, 4], 16),
            case_u!("mul3_div_exact", [3, 4, 5, 6], 10),
            case_u!("mul3_div_floor", [3, 4, 5, 7], 8),
            case_u!("mul3_div_ceil", [3, 4, 5, 7], 9),
            case_u!("mul3_div_half_up", [3, 4, 5, 8], 8),
            case_u!("mul3_div_half_even", [13, 4, 1, 8], 6),
            case_u!("wavg2_exact", [100, 1, 300, 3], 250),
            case_u!("wavg2_floor", [101, 1, 100, 1], 100),
            case_u!("wavg2_ceil", [101, 1, 100, 1], 101),
            case_u!("wavg2_half_up", [101, 1, 100, 1], 101),
            case_u!("wavg2_half_even", [1, 1, 2, 1], 2),
            case_u!("lerp_exact", [100, 200, 1, 4], 125),
            case_u!("lerp_floor", [101, 100, 1, 2], 100),
            case_u!("lerp_ceil", [101, 100, 1, 2], 101),
            case_u!("lerp_half_up", [101, 100, 1, 2], 101),
            case_u!("lerp_half_even", [0, 1, 1, 2], 0),
            case_b!("abs_diff_lte", [105, 100, 5], true),
            case_b!("within_bps", [10050, 10000, 60, 10000], true),
            case_b!("cross_lte", [2, 3, 3, 2], true),
            case_b!("cross_gte", [2, 3, 3, 2], true),
            case_b!("cross_eq", [2, 3, 3, 2], true),
            case_u!("rpow_half_up", [105, 2, 100], 110),
        ];

        let mut covered = HashSet::new();
        for (name, args, expected) in cases {
            let got = fin_call_by_name(name, args).unwrap();
            assert_eq!(got, expected, "FIN behavior case failed for {}", name);
            assert!(covered.insert(name), "duplicate FIN behavior case {}", name);
        }

        for spec in fin_specs() {
            assert!(
                covered.contains(spec.name),
                "missing runtime behavior case for FIN spec {}",
                spec.name
            );
        }
        assert_eq!(
            covered.len(),
            fin_specs().len(),
            "FIN behavior case count must match registry"
        );
    }

    #[test]
    fn fin_exact_rounding_rejects_any_remainder() {
        let cases = [
            ("div_exact", vec![Value::U128(10), Value::U128(3)]),
            ("mul_div_exact", vec![Value::U128(5), Value::U128(10), Value::U128(6)]),
            (
                "mul_add_div_exact",
                vec![Value::U128(2), Value::U128(3), Value::U128(4), Value::U128(6)],
            ),
            (
                "mul_sub_div_exact",
                vec![Value::U128(9), Value::U128(8), Value::U128(6), Value::U128(5)],
            ),
            (
                "mul3_div_exact",
                vec![Value::U128(3), Value::U128(4), Value::U128(5), Value::U128(7)],
            ),
            (
                "wavg2_exact",
                vec![Value::U128(1), Value::U128(1), Value::U128(2), Value::U128(1)],
            ),
            (
                "lerp_exact",
                vec![Value::U128(0), Value::U128(1), Value::U128(1), Value::U128(2)],
            ),
        ];

        for (name, args) in cases {
            assert!(
                fin_call_by_name(name, args).is_err(),
                "{} must reject non-exact division",
                name
            );
        }
    }

    #[test]
    fn fin_rejects_zero_denominators_and_invalid_parameters() {
        let invalid = [
            ("div_floor", vec![Value::U128(1), Value::U128(0)]),
            ("quantize_floor", vec![Value::U128(1), Value::U128(0)]),
            ("mul_div_floor", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
            ("scaled_div_floor", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
            ("dev_scaled_floor", vec![Value::U128(1), Value::U128(0), Value::U128(1)]),
            ("dev_scaled_floor", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
            ("scaled_add_floor", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
            ("scaled_sub_floor", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
            ("mul_div_den_sub_floor", vec![Value::U128(1), Value::U128(1), Value::U128(1)]),
            (
                "mul_add_div_floor",
                vec![Value::U128(1), Value::U128(1), Value::U128(1), Value::U128(0)],
            ),
            (
                "mul_sub_div_floor",
                vec![Value::U128(1), Value::U128(1), Value::U128(1), Value::U128(0)],
            ),
            (
                "mul3_div_floor",
                vec![Value::U128(1), Value::U128(1), Value::U128(1), Value::U128(0)],
            ),
            (
                "wavg2_floor",
                vec![Value::U128(1), Value::U128(0), Value::U128(2), Value::U128(0)],
            ),
            (
                "lerp_floor",
                vec![Value::U128(1), Value::U128(2), Value::U128(1), Value::U128(0)],
            ),
            (
                "lerp_floor",
                vec![Value::U128(1), Value::U128(2), Value::U128(3), Value::U128(2)],
            ),
            (
                "within_bps",
                vec![Value::U128(1), Value::U128(0), Value::U128(1), Value::U128(10)],
            ),
            (
                "within_bps",
                vec![Value::U128(1), Value::U128(1), Value::U128(1), Value::U128(0)],
            ),
            (
                "within_bps",
                vec![Value::U128(1), Value::U128(1), Value::U128(11), Value::U128(10)],
            ),
            ("rpow_half_up", vec![Value::U128(1), Value::U128(1), Value::U128(0)]),
        ];

        for (name, args) in invalid {
            assert!(
                fin_call_by_name(name, args).is_err(),
                "{} must reject invalid parameters",
                name
            );
        }
    }

    #[test]
    fn fin_u128_wide_arithmetic_boundaries_are_checked() {
        let max = Value::U128(u128::MAX);
        let almost = Value::U128(u128::MAX - 1);

        assert_eq!(
            fin_call_by_name("mul_div_floor", vec![max.clone(), max.clone(), max.clone()]).unwrap(),
            max
        );
        assert!(
            fin_call_by_name("mul_div_ceil", vec![max.clone(), max.clone(), almost]).is_err(),
            "quotient above u128::MAX must be rejected"
        );
        assert!(
            fin_call_by_name(
                "mul3_div_floor",
                vec![max.clone(), max.clone(), Value::U128(2), max.clone()],
            )
            .is_err(),
            "3-factor product above u256 must be rejected"
        );
        assert!(
            fin_call_by_name("mul_add_div_floor", vec![max.clone(), max, Value::U128(1), Value::U128(1)])
                .is_err(),
            "unrepresentable divided result must be rejected"
        );
    }

    #[test]
    fn fin_mul_shr_boundary_shifts_cover_high_limb_and_round_up() {
        let max = Value::U128(u128::MAX);
        assert_eq!(
            fin_call_by_name("mul_shr_floor", vec![max.clone(), max.clone(), Value::U128(128)]).unwrap(),
            Value::U128(u128::MAX - 1)
        );
        assert_eq!(
            fin_call_by_name("mul_shr_ceil", vec![max.clone(), max, Value::U128(128)]).unwrap(),
            Value::U128(u128::MAX)
        );
        assert_eq!(
            fin_call_by_name("mul_shr_floor", vec![Value::U128(1), Value::U128(1), Value::U128(255)]).unwrap(),
            Value::U128(0)
        );
        assert_eq!(
            fin_call_by_name("mul_shr_ceil", vec![Value::U128(1), Value::U128(1), Value::U128(255)]).unwrap(),
            Value::U128(1)
        );
    }

    #[test]
    fn fin_rounding_half_even_ties_round_to_even_across_kernels() {
        assert_eq!(
            fin_call_by_name("div_half_even", vec![Value::U128(5), Value::U128(2)]).unwrap(),
            Value::U128(2)
        );
        assert_eq!(
            fin_call_by_name("div_half_even", vec![Value::U128(7), Value::U128(2)]).unwrap(),
            Value::U128(4)
        );
        assert_eq!(
            fin_call_by_name("mul_add_div_half_even", vec![
                Value::U128(2),
                Value::U128(3),
                Value::U128(4),
                Value::U128(4),
            ])
            .unwrap(),
            Value::U128(2)
        );
        assert_eq!(
            fin_call_by_name("mul_sub_div_half_even", vec![
                Value::U128(9),
                Value::U128(8),
                Value::U128(6),
                Value::U128(4),
            ])
            .unwrap(),
            Value::U128(16)
        );
        assert_eq!(
            fin_call_by_name("wavg2_half_even", vec![
                Value::U128(0),
                Value::U128(1),
                Value::U128(1),
                Value::U128(1),
            ])
            .unwrap(),
            Value::U128(0)
        );
        assert_eq!(
            fin_call_by_name("wavg2_half_even", vec![
                Value::U128(1),
                Value::U128(1),
                Value::U128(2),
                Value::U128(1),
            ])
            .unwrap(),
            Value::U128(2)
        );
    }

    #[test]
    fn fin_rpow_half_up_matches_reference_edges() {
        assert_eq!(
            fin_call_by_name("rpow_half_up", vec![Value::U128(123), Value::U128(0), Value::U128(100)]).unwrap(),
            Value::U128(100)
        );
        assert_eq!(
            fin_call_by_name("rpow_half_up", vec![Value::U128(123), Value::U128(1), Value::U128(100)]).unwrap(),
            Value::U128(123)
        );
        assert_eq!(
            fin_call_by_name("rpow_half_up", vec![Value::U128(105), Value::U128(2), Value::U128(100)]).unwrap(),
            Value::U128(110)
        );
        assert_eq!(
            fin_call_by_name("rpow_half_up", vec![Value::U128(105), Value::U128(3), Value::U128(100)]).unwrap(),
            Value::U128(116)
        );
        assert!(
            fin_call_by_name("rpow_half_up", vec![
                Value::U128(u128::MAX),
                Value::U128(2),
                Value::U128(1),
            ])
            .is_err(),
            "rpow must reject intermediate quotient overflow"
        );
    }

    #[test]
    fn fin_lookup_rejects_unknown_family_id() {
        assert!(fin_spec_lookup(Bytecode::FINPOW3, 31).is_err());
        assert!(fin_spec_lookup(Bytecode::FIN3, 31).is_err());
    }
}
