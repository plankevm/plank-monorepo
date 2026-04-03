use alloy_primitives::{I256, U256, U512};

#[cfg(test)]
mod tests;

pub fn add(a: U256, b: U256) -> U256 {
    a.wrapping_add(b)
}

pub fn mul(a: U256, b: U256) -> U256 {
    a.wrapping_mul(b)
}

pub fn sub(a: U256, b: U256) -> U256 {
    a.wrapping_sub(b)
}

pub fn div(a: U256, b: U256) -> U256 {
    a.checked_div(b).unwrap_or(U256::ZERO)
}

pub fn sdiv(a: U256, b: U256) -> U256 {
    let sa = I256::from_raw(a);
    let sb = I256::from_raw(b);
    if sb == I256::ZERO { U256::ZERO } else { sa.wrapping_div(sb).into_raw() }
}

pub fn r#mod(a: U256, b: U256) -> U256 {
    a.checked_rem(b).unwrap_or(U256::ZERO)
}

pub fn smod(a: U256, b: U256) -> U256 {
    let sa = I256::from_raw(a);
    let sb = I256::from_raw(b);
    sa.checked_rem(sb).unwrap_or(I256::ZERO).into_raw()
}

pub fn addmod(a: U256, b: U256, n: U256) -> U256 {
    if n.is_zero() {
        return U256::ZERO;
    }
    U256::from(U512::from(a).wrapping_add(U512::from(b)) % U512::from(n))
}

pub fn mulmod(a: U256, b: U256, n: U256) -> U256 {
    if n.is_zero() {
        return U256::ZERO;
    }
    U256::from(U512::from(a).wrapping_mul(U512::from(b)) % U512::from(n))
}

pub fn exp(base: U256, exponent: U256) -> U256 {
    base.wrapping_pow(exponent)
}

pub fn signextend(b: U256, x: U256) -> U256 {
    let Some(b) = usize::try_from(b).ok().filter(|b| *b < 32) else {
        return x;
    };
    let sign_bit_pos = (b + 1) * 8 - 1;
    if x.bit(sign_bit_pos) {
        x | (U256::MAX << (sign_bit_pos + 1))
    } else {
        x & ((U256::ONE << (sign_bit_pos + 1)) - U256::ONE)
    }
}

pub fn lt(a: U256, b: U256) -> bool {
    a < b
}

pub fn gt(a: U256, b: U256) -> bool {
    a > b
}

pub fn slt(a: U256, b: U256) -> bool {
    I256::from_raw(a) < I256::from_raw(b)
}

pub fn sgt(a: U256, b: U256) -> bool {
    I256::from_raw(a) > I256::from_raw(b)
}

pub fn eq(a: U256, b: U256) -> bool {
    a == b
}

pub fn iszero(a: U256) -> bool {
    a.is_zero()
}

pub fn and(a: U256, b: U256) -> U256 {
    a & b
}

pub fn or(a: U256, b: U256) -> U256 {
    a | b
}

pub fn xor(a: U256, b: U256) -> U256 {
    a ^ b
}

pub fn not(a: U256) -> U256 {
    !a
}

pub fn byte(i: U256, x: U256) -> U256 {
    let Ok(i) = usize::try_from(i) else {
        return U256::ZERO;
    };
    let byte = x.to_be_bytes::<32>().get(i).copied();
    U256::from(byte.unwrap_or(0))
}

pub fn shl(shift: U256, value: U256) -> U256 {
    value << shift
}

pub fn shr(shift: U256, value: U256) -> U256 {
    value >> shift
}

pub fn sar(shift: U256, value: U256) -> U256 {
    let value = I256::from_raw(value);
    if let Ok(shift) = shift.try_into() {
        value.asr(shift).into_raw()
    } else if value.is_negative() {
        I256::MINUS_ONE.into_raw()
    } else {
        U256::ZERO
    }
}
