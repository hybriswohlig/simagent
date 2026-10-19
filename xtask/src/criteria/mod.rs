//! The criteria kit of step 11 (ADR-0020 section 8, spec 8): the statistics that P3, P4,
//! P5, P9 and F11 read from a town's export, their report (`cargo xtask criteria report`),
//! the criteria town and the villages of P4.

pub mod export;
pub mod report;
pub mod stats;
pub mod town;
pub mod units;
pub mod village;
pub mod write;
