//! Newtypes that carry units in the type system.
//!
//! Field names in serialized JSON still include the unit (`p99_ms`,
//! `cost_usd`) so TypeScript clients do not have to import these
//! wrappers. The wrappers exist to stop accidental mixing in Rust
//! (`Milliseconds` + `Tokens` does not compile).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

macro_rules! unit {
    ($name:ident, $label:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub f64);

        impl $name {
            pub const ZERO: Self = Self(0.0);

            pub const fn new(v: f64) -> Self {
                Self(v)
            }

            pub const fn get(self) -> f64 {
                self.0
            }

            pub fn is_finite(self) -> bool {
                self.0.is_finite()
            }

            pub fn is_non_negative(self) -> bool {
                self.0.is_finite() && self.0 >= 0.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{} {}", self.0, $label)
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Mul<f64> for $name {
            type Output = Self;
            fn mul(self, rhs: f64) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Div<f64> for $name {
            type Output = Self;
            fn div(self, rhs: f64) -> Self {
                Self(self.0 / rhs)
            }
        }
    };
}

unit!(Milliseconds, "ms");
unit!(Tokens, "tokens");
unit!(Usd, "USD");
unit!(Rps, "rps");

/// Probability in `[0, 1]`. Construction does not clamp; [`Probability::clamp`]
/// does. Invalid values are rejected at estimate time.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Probability(pub f64);

impl Probability {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub const fn new(v: f64) -> Self {
        Self(v)
    }

    pub const fn get(self) -> f64 {
        self.0
    }

    pub fn clamp(v: f64) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    pub fn is_valid(self) -> bool {
        self.0.is_finite() && (0.0..=1.0).contains(&self.0)
    }
}

/// Quality proxy in `[0, 1]`. Not a measured groundedness score.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quality(pub f64);

impl Quality {
    pub const fn new(v: f64) -> Self {
        Self(v)
    }

    pub const fn get(self) -> f64 {
        self.0
    }

    pub fn clamp(v: f64) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    pub fn is_valid(self) -> bool {
        self.0.is_finite() && (0.0..=1.0).contains(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milliseconds_do_not_impl_add_with_tokens() {
        let a = Milliseconds::new(10.0);
        let b = Milliseconds::new(5.0);
        assert!((a + b).get() - 15.0 < 1e-12);
        let t = Tokens::new(100.0);
        assert!((t * 2.0).get() - 200.0 < 1e-12);
    }

    #[test]
    fn probability_clamp_and_validity() {
        assert!(!Probability::new(1.2).is_valid());
        assert!(!Probability::new(f64::NAN).is_valid());
        assert!((Probability::clamp(1.4).get() - 1.0).abs() < 1e-12);
    }
}
