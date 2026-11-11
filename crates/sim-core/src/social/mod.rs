//! The social layer of phase 3 (ADR-0030, ADR-0031): its data, the memory of a person in a
//! moved-in household, that person's bonds, and their beliefs and deeds. The choice reads the
//! bonds for company and avoid. Talks are the pure rules in `talk` and the run in `state::talk`.
//! The belief numbers are already part of the data so that the data hash moved once.

pub mod belief;
pub mod bond;
pub mod data;
pub mod memory;
pub mod state;
pub mod talk;
