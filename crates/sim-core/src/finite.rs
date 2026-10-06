//! Floats in serialized core types (ADR-0003 section 4): finite, and never the negative
//! zero. A NaN would carry platform bits into a hash, serde_json would write NaN and
//! infinity as `null`, and a `-0.0` that a save turned into `+0.0` would make the loaded
//! run differ from the uninterrupted one.

use core::cmp::Ordering;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A float that is neither NaN nor infinite and not `-0.0`. It has no arithmetic: code
/// computes in `f64` and stores the checked result.
#[derive(Clone, Copy, Debug)]
pub struct FiniteF64(f64);

/// The value was NaN or infinite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonFinite;

impl FiniteF64 {
    /// NaN and both infinities are `Err`; `-0.0` becomes `+0.0`.
    pub fn try_new(x: f64) -> Result<FiniteF64, NonFinite> {
        if !x.is_finite() {
            return Err(NonFinite);
        }
        // Adding `+0.0` turns `-0.0` into `+0.0` and leaves every other value as it is.
        Ok(FiniteF64(x + 0.0))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for FiniteF64 {
    fn eq(&self, other: &FiniteF64) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for FiniteF64 {}

impl PartialOrd for FiniteF64 {
    fn partial_cmp(&self, other: &FiniteF64) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The total order of the floats; without NaN and `-0.0` it is the order of the numbers.
impl Ord for FiniteF64 {
    fn cmp(&self, other: &FiniteF64) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// As the plain `f64`: eight bytes in postcard, a number in JSON.
impl Serialize for FiniteF64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}

/// Rejects what `try_new` would not have stored: non-finite values and `-0.0`. A correct
/// save holds neither.
impl<'de> Deserialize<'de> for FiniteF64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FiniteF64, D::Error> {
        let x = f64::deserialize(deserializer)?;
        if !x.is_finite() || (x == 0.0 && x.is_sign_negative()) {
            return Err(D::Error::custom(
                "finite_f64: expected a finite number other than -0.0",
            ));
        }
        Ok(FiniteF64(x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::vec::Vec;

    fn finite(x: f64) -> FiniteF64 {
        FiniteF64::try_new(x).expect("finite")
    }

    /// ADR-0003 `rejects_nan_and_infinity`.
    #[test]
    fn rejects_nan_and_infinity() {
        let other_nan = f64::from_bits(0xfff8_0000_0000_0001);
        assert!(other_nan.is_nan());
        for x in [
            f64::NAN,
            -f64::NAN,
            other_nan,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(FiniteF64::try_new(x).err(), Some(NonFinite), "{x}");
        }
        for x in [
            0.0,
            1.5,
            -1.5,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            5e-324,
            -5e-324,
        ] {
            assert_eq!(finite(x).get().to_bits(), x.to_bits(), "{x}");
        }
    }

    /// ADR-0003 `negative_zero_becomes_positive`: canonical when built, so before every
    /// hash and save.
    #[test]
    fn negative_zero_becomes_positive() {
        assert_eq!((-0.0_f64).to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(finite(-0.0).get().to_bits(), 0);
        assert_eq!(finite(0.0).get().to_bits(), 0);
        assert_eq!(finite(-0.0), finite(0.0));
        // A product that underflows to the negative zero.
        assert_eq!(finite(-5e-324 * 0.5).get().to_bits(), 0);
    }

    fn postcard_of(x: f64) -> Vec<u8> {
        postcard::to_allocvec(&x).expect("encodes")
    }

    /// ADR-0003 `deserialize_rejects_non_finite_and_negative_zero`.
    #[test]
    fn deserialize_rejects_non_finite_and_negative_zero() {
        for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
            let bytes = postcard_of(x);
            assert!(postcard::from_bytes::<FiniteF64>(&bytes).is_err(), "{x}");
        }
        for text in ["-0.0", "-0", "null", "\"1.0\"", "1e999", "NaN"] {
            assert!(serde_json::from_str::<FiniteF64>(text).is_err(), "{text}");
        }
        // What it accepts, it writes back bit for bit: eight bytes, little endian.
        for x in [0.0, 0.02, -1.5, f64::MAX, 5e-324] {
            let bytes = postcard_of(x);
            assert_eq!(bytes, x.to_le_bytes());
            let back: FiniteF64 = postcard::from_bytes(&bytes).expect("decodes");
            assert_eq!(back.get().to_bits(), x.to_bits());
            assert_eq!(postcard::to_allocvec(&back).expect("encodes"), bytes);
            let text = serde_json::to_string(&finite(x)).expect("json");
            assert_eq!(text, serde_json::to_string(&x).expect("json"));
            let back: FiniteF64 = serde_json::from_str(&text).expect("parses");
            assert_eq!(back.get().to_bits(), x.to_bits());
        }
        assert_eq!(serde_json::to_string(&finite(0.02)).expect("json"), "0.02");
        // A whole number in JSON is a float here.
        let whole: FiniteF64 = serde_json::from_str("3").expect("parses");
        assert_eq!(whole.get().to_bits(), 3.0_f64.to_bits());
    }

    /// ADR-0003 `ord_matches_total_cmp`: sortable and usable as a map key.
    #[test]
    fn ord_matches_total_cmp() {
        let raw = [
            1.0,
            -1.0,
            0.0,
            f64::MAX,
            f64::MIN,
            f64::MIN_POSITIVE,
            -f64::MIN_POSITIVE,
            5e-324,
            0.3,
            0.1 + 0.2,
        ];
        let mut values: Vec<FiniteF64> = raw.iter().map(|&x| finite(x)).collect();
        values.sort();
        let mut expected = raw;
        expected.sort_by(f64::total_cmp);
        let sorted: Vec<u64> = values.iter().map(|v| v.get().to_bits()).collect();
        let wanted: Vec<u64> = expected.iter().map(|x| x.to_bits()).collect();
        assert_eq!(sorted, wanted);
        for a in &values {
            for b in &values {
                assert_eq!(a.cmp(b), a.get().total_cmp(&b.get()));
                assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
                assert_eq!(a == b, a.get().to_bits() == b.get().to_bits());
            }
        }
        assert!(finite(0.3) < finite(0.1 + 0.2), "neighbours are told apart");
        assert!(finite(-1.0) < finite(0.0) && finite(0.0) < finite(5e-324));
    }

    proptest! {
        /// ADR-0003 `try_new_on_any_bits`: `Err` exactly for NaN and the infinities,
        /// otherwise never `-0.0`.
        #[test]
        fn try_new_on_any_bits(bits in any::<u64>()) {
            let x = f64::from_bits(bits);
            match FiniteF64::try_new(x) {
                Err(NonFinite) => prop_assert!(x.is_nan() || x.is_infinite()),
                Ok(value) => {
                    prop_assert!(x.is_finite());
                    prop_assert_ne!(value.get().to_bits(), 0x8000_0000_0000_0000_u64);
                    if bits == 0x8000_0000_0000_0000 {
                        prop_assert_eq!(value.get().to_bits(), 0);
                    } else {
                        prop_assert_eq!(value.get().to_bits(), bits);
                    }
                }
            }
        }
    }
}
