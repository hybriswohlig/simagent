//! Inputs (ADR-0004 section 4): the only way from outside into the core, applied by
//! `World::submit` at event boundaries and recorded in the input log. The JSON form is
//! the command schema (B15): externally tagged, snake_case, unknown fields rejected.
//! Variants arrive with their first handler and only go to the end.

use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::ids::InputSeq;
use crate::time::SimTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    /// The resolved scenario; only as the first input.
    Start(Start),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    #[serde(with = "crate::hex_u64")]
    pub world_seed: u64,
    #[serde(with = "crate::hex_u64")]
    pub founder_tag: u64,
    pub founders: Vec<Founder>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Founder {
    pub born: SimTime,
}

/// Where an accepted input sits: `t` is the last closed second, `seq` its position in
/// the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStamp {
    pub t: SimTime,
    pub seq: InputSeq,
}

/// Stable reason codes for a rejected input (no text, B16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectCode {
    AlreadyStarted,
}

/// A rejected input changes nothing and takes no position in the input log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRejected {
    pub code: RejectCode,
}
