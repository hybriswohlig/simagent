//! The criteria kit of step 11 (ADR-0020 section 8, spec 8): the statistics that P3, P4,
//! P5, P9 and F11 read from a town's export. The report (`cargo xtask criteria`) is their
//! user; until it arrives only the tests call them.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the criteria report is the user (step 11, plan slice 4b)"
    )
)]

pub mod stats;
