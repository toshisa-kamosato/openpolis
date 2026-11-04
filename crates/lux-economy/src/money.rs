// SPDX-License-Identifier: GPL-2.0-or-later
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// An amount of LUX in centi-LUX (1 LUX = 100). Never negative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Amount(pub u64);

impl Amount {
    pub const ZERO: Amount = Amount(0);

    pub const fn lux(whole: u64) -> Amount {
        Amount(whole * 100)
    }
    pub const fn centi(c: u64) -> Amount {
        Amount(c)
    }
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
    pub fn saturating_sub(self, o: Amount) -> Amount {
        Amount(self.0.saturating_sub(o.0))
    }
    /// `self × bps / 10 000`, rounded half up (used for commissions).
    pub fn bps_round(self, bps: u32) -> Amount {
        Amount(((self.0 as u128 * bps as u128 + 5_000) / 10_000) as u64)
    }
    /// `self × bps / 10 000`, rounded down (used for taxes and shares: never takes more than due).
    pub fn bps_floor(self, bps: u32) -> Amount {
        Amount((self.0 as u128 * bps as u128 / 10_000) as u64)
    }
    /// Parses "12", "12.3" or "12.34".
    pub fn parse(s: &str) -> Option<Amount> {
        let (whole, frac) = match s.split_once('.') {
            Some((w, f)) => (w, f),
            None => (s, ""),
        };
        if frac.len() > 2 || whole.is_empty() && frac.is_empty() {
            return None;
        }
        let w: u64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
        let f: u64 = match frac.len() {
            0 => 0,
            1 => frac.parse::<u64>().ok()? * 10,
            _ => frac.parse().ok()?,
        };
        w.checked_mul(100)?.checked_add(f).map(Amount)
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

impl Add for Amount {
    type Output = Amount;
    fn add(self, o: Amount) -> Amount {
        Amount(self.0.checked_add(o.0).expect("amount overflow"))
    }
}
impl Sub for Amount {
    type Output = Amount;
    fn sub(self, o: Amount) -> Amount {
        Amount(self.0.checked_sub(o.0).expect("amount underflow"))
    }
}
impl AddAssign for Amount {
    fn add_assign(&mut self, o: Amount) {
        *self = *self + o;
    }
}
impl SubAssign for Amount {
    fn sub_assign(&mut self, o: Amount) {
        *self = *self - o;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse() {
        assert_eq!(Amount(123_456).to_string(), "1234.56");
        assert_eq!(Amount(5).to_string(), "0.05");
        assert_eq!(Amount::parse("12.3"), Some(Amount(1230)));
        assert_eq!(Amount::parse("0.05"), Some(Amount(5)));
        assert_eq!(Amount::parse("7"), Some(Amount(700)));
        assert_eq!(Amount::parse("1.234"), None);
        assert_eq!(Amount::parse(""), None);
    }

    #[test]
    fn commission_rounding() {
        // 2.5 % of 97.56 = 2.439 -> 2.44 ; of 97.57 = 2.43925 -> 2.44 ; of 0.01 = 0.00025 -> 0.00
        assert_eq!(Amount(9756).bps_round(250), Amount(244));
        assert_eq!(Amount(9757).bps_round(250), Amount(244));
        assert_eq!(Amount(1).bps_round(250), Amount(0));
        assert_eq!(Amount(9756).bps_floor(250), Amount(243));
    }
}
