//! The social layer of phase 3 (ADR-0030, ADR-0031): its data, the memory of a person in a
//! moved-in household, and that person's bonds. The choice reads those bonds for company and
//! avoid. Talks are the pure rules in `talk` and the run in `state::talk`. Beliefs come with
//! their slice; their values are already part of the data so that the data hash moves once.

pub mod bond;
pub mod data;
pub mod memory;
pub mod state;
pub mod talk;
