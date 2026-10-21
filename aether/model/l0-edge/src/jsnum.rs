//! Integer emulation of the JavaScript number arithmetic the upstream pipeline uses, so ports
//! reproduce upstream results bit for bit, including where binary rounding differs from exact.

use core::cmp::Ordering;
use core::ops::{Add, Div, Mul};

const MANT_BITS: u32 = 53;

/// A non-negative finite IEEE-754 binary64 value, held exactly as `mant * 2^exp`.
///
/// `mant` is 0 (the value zero) or lies in `2^52..2^53`; subnormals are outside the domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dbl {
    mant: u64,
    exp: i32,
}

fn bit_len(n: u128) -> u32 {
    128 - n.leading_zeros()
}

/// Rounds `q * 2^exp` (plus a positive amount below one unit of `q` when `sticky`) to 53 bits,
/// ties to even. `sticky` must be false when `q` has 53 bits or fewer.
fn round_to_double(q: u128, sticky: bool, exp: i32) -> Dbl {
    if q == 0 {
        return Dbl::ZERO;
    }
    let len = bit_len(q);
    if len <= MANT_BITS {
        let shift = MANT_BITS - len;
        return Dbl {
            mant: (q << shift) as u64,
            exp: exp - shift as i32,
        };
    }
    let shift = len - MANT_BITS;
    let mut mant = q >> shift;
    let rest = q & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    if rest > half || (rest == half && (sticky || mant & 1 == 1)) {
        mant += 1;
    }
    let mut exp = exp + shift as i32;
    if mant == 1u128 << MANT_BITS {
        mant >>= 1;
        exp += 1;
    }
    Dbl {
        mant: mant as u64,
        exp,
    }
}

/// Correctly rounded `num / den * 2^exp`; `den` must be non-zero.
fn ratio(num: u64, den: u64, exp: i32) -> Dbl {
    assert!(den != 0, "division by zero");
    if num == 0 {
        return Dbl::ZERO;
    }
    let (num, den) = (u128::from(num), u128::from(den));
    // At least 55 quotient bits keep the rounding bit inside the quotient.
    let shift = (56 + bit_len(den)).saturating_sub(bit_len(num));
    let scaled = num << shift;
    round_to_double(scaled / den, scaled % den != 0, exp - shift as i32)
}

impl Dbl {
    /// The value zero.
    pub const ZERO: Dbl = Dbl { mant: 0, exp: 0 };

    /// The double nearest `n` (exact below 2^53).
    pub fn from_u64(n: u64) -> Dbl {
        round_to_double(u128::from(n), false, 0)
    }

    /// The double nearest `num / den`, as JavaScript's `num / den` on integer operands below 2^53.
    pub fn from_ratio(num: u64, den: u64) -> Dbl {
        ratio(num, den, 0)
    }

    /// The double JavaScript parses from the decimal `units / 10^digits` (e.g. `+x.toFixed(4)`).
    pub fn decimal(units: u64, digits: u32) -> Dbl {
        ratio(units, 10u64.pow(digits), 0)
    }

    /// True for the value zero.
    pub fn is_zero(self) -> bool {
        self.mant == 0
    }

    /// The IEEE-754 binary64 bit pattern, for comparing against a JavaScript `Float64Array`.
    pub fn to_bits(self) -> u64 {
        if self.is_zero() {
            return 0;
        }
        let biased = self.exp + 52 + 1023;
        assert!(
            (1..2047).contains(&biased),
            "value outside the normal range"
        );
        ((biased as u64) << 52) | (self.mant & ((1 << 52) - 1))
    }

    /// JavaScript `Math.round(x)` (nearest integer, ties toward +infinity).
    pub fn js_round(self) -> u64 {
        self.round_scaled(1)
    }

    /// JavaScript `x.toFixed(digits)` as its integer count of `10^-digits` units (digits <= 4).
    pub fn to_fixed_units(self, digits: u32) -> u64 {
        assert!(digits <= 4, "toFixed digits above 4 are outside the domain");
        self.round_scaled(10u64.pow(digits))
    }

    /// `floor(x * scale + 1/2)`, exact.
    fn round_scaled(self, scale: u64) -> u64 {
        if self.is_zero() {
            return 0;
        }
        let scaled = u128::from(self.mant) * u128::from(scale);
        if self.exp >= 0 {
            assert!(self.exp <= 60, "value outside the u64 domain");
            let value = scaled << self.exp as u32;
            return u64::try_from(value).expect("value outside the u64 domain");
        }
        let k = self.exp.unsigned_abs();
        // scaled < 2^67, so for k >= 68 the value is below one half.
        if k >= 68 {
            return 0;
        }
        ((2 * scaled + (1u128 << k)) >> (k + 1)) as u64
    }
}

impl Mul for Dbl {
    type Output = Dbl;

    /// JavaScript `a * b`.
    fn mul(self, other: Dbl) -> Dbl {
        if self.is_zero() || other.is_zero() {
            return Dbl::ZERO;
        }
        let product = u128::from(self.mant) * u128::from(other.mant);
        round_to_double(product, false, self.exp + other.exp)
    }
}

impl Add for Dbl {
    type Output = Dbl;

    /// JavaScript `a + b`.
    fn add(self, other: Dbl) -> Dbl {
        if self.is_zero() {
            return other;
        }
        if other.is_zero() {
            return self;
        }
        let (big, small) = if self.exp >= other.exp {
            (self, other)
        } else {
            (other, self)
        };
        let gap = (big.exp - small.exp) as u32;
        // Past 70 bits the smaller addend is below half a unit of the larger and cannot round.
        if gap > 70 {
            return big;
        }
        let sum = (u128::from(big.mant) << gap) + u128::from(small.mant);
        round_to_double(sum, false, small.exp)
    }
}

impl Div for Dbl {
    type Output = Dbl;

    /// JavaScript `a / b`; `b` must be non-zero.
    fn div(self, other: Dbl) -> Dbl {
        assert!(!other.is_zero(), "division by zero");
        ratio(self.mant, other.mant, self.exp - other.exp)
    }
}

impl PartialOrd for Dbl {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Dbl {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_zero(), other.is_zero()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.exp.cmp(&other.exp).then(self.mant.cmp(&other.mant)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reproduces_the_javascript_half_way_misses() {
        // JS: Math.round(23 / 40 * 100) === 57 although 23 / 40 * 100 is exactly 57.5.
        let x = Dbl::from_ratio(23, 40) * Dbl::from_u64(100);
        assert_eq!(x.js_round(), 57);
        // JS: Math.round(201 / 400 * 1000) === 502 although the exact value is 502.5.
        let x = Dbl::from_ratio(201, 400) * Dbl::from_u64(1000);
        assert_eq!(x.js_round(), 502);
        // JS: Math.round(1 / 8 * 100) === 13 (12.5 is exact in binary, ties go up).
        assert_eq!((Dbl::from_ratio(1, 8) * Dbl::from_u64(100)).js_round(), 13);
    }

    #[test]
    fn decimal_arithmetic_matches_javascript() {
        // JS: 0.1 + 0.2 === 0.30000000000000004, so (0.1 + 0.2).toFixed(4) === "0.3000".
        let sum = Dbl::decimal(1, 1) + Dbl::decimal(2, 1);
        assert_ne!(sum, Dbl::decimal(3, 1));
        assert_eq!(sum.to_fixed_units(4), 3000);
        // JS: (1.005).toFixed(2) === "1.00" because 1.005 is stored below 1.005.
        assert_eq!(Dbl::decimal(1005, 3).to_fixed_units(2), 100);
        // JS: 0.3 / 0.1 === 2.9999999999999996.
        let q = Dbl::decimal(3, 1) / Dbl::decimal(1, 1);
        assert!(q < Dbl::from_u64(3));
        assert_eq!(q.js_round(), 3);
    }

    #[test]
    fn integers_round_trip_and_order() {
        for n in [0u64, 1, 2, 3, 99, 1 << 40, 1 << 52, (1 << 53) - 1] {
            assert_eq!(Dbl::from_u64(n).js_round(), n);
        }
        for n in [0u64, 1, 99, 1 << 40] {
            assert_eq!(Dbl::from_u64(n).to_fixed_units(4), n * 10_000);
        }
        assert!(Dbl::ZERO < Dbl::decimal(1, 4));
        assert!(Dbl::decimal(7199, 4) < Dbl::decimal(72, 2));
        assert_eq!(Dbl::decimal(7200, 4), Dbl::decimal(72, 2));
    }
}
