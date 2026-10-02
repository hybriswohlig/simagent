//! The body contract (ADR-0005): the visible body values of a sim at one moment. The
//! types come from `contracts/body.json` by way of `cargo xtask codegen`; the core never
//! reads that file at run time. `BodyParams` are no state: they are not hashed and not
//! saved.

mod generated;

pub use generated::{BodyParamKey, BodyParamSpec, BodyParams, CONTRACT_VERSION, SOURCE_HASH};

/// Why the contract does not accept a value for a parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyParamError {
    /// The value is NaN or infinite.
    NotFinite(BodyParamKey),
    /// The value lies outside the range of the parameter.
    OutOfRange(BodyParamKey),
}

impl BodyParams {
    /// These values with the parameter `key` set to `value`: only a finite value inside
    /// the range of the contract (both ends inside) is accepted.
    pub fn with(mut self, key: BodyParamKey, value: f64) -> Result<BodyParams, BodyParamError> {
        let spec = key.spec();
        if !value.is_finite() {
            return Err(BodyParamError::NotFinite(key));
        }
        if value < spec.min || value > spec.max {
            return Err(BodyParamError::OutOfRange(key));
        }
        *self.slot(key) = value;
        Ok(self)
    }
}
