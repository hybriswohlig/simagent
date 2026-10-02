//! SimAgent simulation core.
//!
//! The core owns all gameplay and the clock. It has no std, no wall clock, no threads,
//! no files and no engine types; the CI builds it for `wasm32v1-none` to prove that
//! (ADR-0000). Time only advances through the scheduler (ADR-0002), randomness only
//! through addressed draws (ADR-0001), nonlinear math only through `sim_math`
//! (ADR-0003). The modules arrive in the order of HANDOFF §9.

#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod body_contract;
pub mod canonical;
pub mod data_id;
pub mod finite;
pub mod genome;
pub mod hash;
pub mod hex_u64;
pub mod ids;
pub mod input;
pub mod lineage;
pub mod log;
pub mod params;
pub mod rng;
mod state;
pub mod time;
pub mod trace;
pub mod version;
pub mod world;
