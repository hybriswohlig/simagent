//! The criteria kit of step 11 (ADR-0020 section 8, spec 8): the statistics that P3, P4,
//! P5, P9 and F11 read from a town's export, and their report (`cargo xtask criteria
//! report`). Only the village check over the seeds waits for its user.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the village check over the seeds (stats::village): plan slice 11"
    )
)]

pub mod export;
pub mod report;
pub mod stats;
pub mod units;
pub mod write;
